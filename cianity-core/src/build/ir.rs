use std::collections::HashMap;

use ciane::{
    ast::{self, AstNode, HasAttrList, HasName, JobBodySteps, Root},
    syntax::SyntaxKind,
};

// ─── IR types ────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct Workflow {
    pub stages: Vec<Stage>,
}

impl Workflow {
    /// Look up a job by stage name and job name.
    #[must_use]
    pub fn job(&self, stage: &str, name: &str) -> Option<&Job> {
        self.stages
            .iter()
            .find(|s| s.name == stage)?
            .jobs
            .iter()
            .find(|j| j.name == name)
    }
}

#[derive(Debug)]
pub struct Stage {
    pub name: String,
    pub jobs: Vec<Job>,
}

#[derive(Debug)]
pub struct Job {
    pub name: String,
    pub stage: String,
    pub image: Option<String>,
    /// Fully resolved script lines, with any inherited template steps inlined.
    pub script: Vec<String>,
    pub needs: Vec<JobRef>,
}

impl Job {
    /// The canonical `stage.job` identifier used in GitLab CI job names and
    /// cross-job references.
    #[must_use]
    pub fn full_name(&self) -> String {
        format!("{}.{}", self.stage, self.name)
    }

    /// Resolve the dependency job references against the workflow, returning
    /// the concrete `Job` items this job depends on.
    #[must_use]
    pub fn dependency_jobs<'w>(&self, workflow: &'w Workflow) -> Vec<&'w Job> {
        self.needs
            .iter()
            .filter_map(|r| workflow.job(&r.stage, &r.job))
            .collect()
    }
}

#[derive(Debug)]
pub struct JobRef {
    pub stage: String,
    pub job: String,
}

// ─── Lowering ────────────────────────────────────────────────────────────────

type TemplateSteps = Vec<(String, String)>;

/// Lower a parsed `Root` AST node into the rich IR `Workflow`.
///
/// Templates are resolved and inlined into their jobs; the returned `Workflow`
/// contains only concrete jobs.
#[must_use]
pub fn lower(root: &Root) -> Workflow {
    let mut stages = Vec::new();

    for stage in root.stages() {
        let stage_name = stage.name().map_or_else(String::new, |s| s.to_string());

        let Some(body) = stage.body() else {
            continue;
        };

        let templates: HashMap<String, TemplateSteps> = body
            .templates()
            .filter_map(|t| {
                let name = t.name()?.to_string();
                let steps = t
                    .body()
                    .map_or_else(Vec::new, |b| collect_template_steps(&b));
                Some((name, steps))
            })
            .collect();

        let mut jobs = Vec::new();
        for job in body.jobs() {
            let job_name = job.name().map_or_else(String::new, |s| s.to_string());

            let mut image: Option<String> = None;
            let mut inherit: Option<String> = None;
            let mut needs: Vec<JobRef> = Vec::new();

            if let Some(attr_list) = job.attr_list() {
                for attr in attr_list.attrs() {
                    match attr.key_text().as_deref() {
                        Some("image") => {
                            image = attr.value_text().map(|s| s.to_string());
                        }
                        Some("inherit") => {
                            inherit = attr.value_text().map(|s| s.to_string());
                        }
                        Some("dependencies") => {
                            if let Some(val) = attr.value()
                                && let Some(ref_list) = val.ref_list()
                            {
                                for r in ref_list.refs() {
                                    let text = r.text();
                                    if let Some((s, j)) = text.split_once('.') {
                                        needs.push(JobRef {
                                            stage: s.to_string(),
                                            job: j.to_string(),
                                        });
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }

            let empty: TemplateSteps = Vec::new();
            let template_steps: &TemplateSteps = inherit
                .as_deref()
                .filter(|t| !t.contains('/'))
                .and_then(|t| templates.get(t))
                .unwrap_or(&empty);

            let script = if let Some(inline) = job.inline_body() {
                inline
                    .shell_text()
                    .map(|s| vec![dedent(&s)])
                    .unwrap_or_default()
            } else if let Some(steps_body) = job.steps_body() {
                resolve_steps(&steps_body, template_steps)
            } else {
                Vec::new()
            };

            jobs.push(Job {
                name: job_name,
                stage: stage_name.clone(),
                image,
                script,
                needs,
            });
        }

        stages.push(Stage {
            name: stage_name,
            jobs,
        });
    }

    Workflow { stages }
}

fn collect_template_steps(body: &JobBodySteps) -> TemplateSteps {
    body.steps()
        .filter_map(|s| {
            let name = s.name()?.to_string();
            let shell = s.shell_text()?;
            Some((name, dedent(&shell)))
        })
        .collect()
}

fn resolve_steps(body: &JobBodySteps, template_steps: &TemplateSteps) -> Vec<String> {
    // Names of all steps explicitly listed in this job body (both full steps
    // and bare references).  These are skipped when `steps` is expanded so the
    // same step does not appear twice.
    let explicit_names: std::collections::HashSet<String> = body
        .steps()
        .filter_map(|s| s.name().map(|n| n.to_string()))
        .collect();

    let mut script = Vec::new();

    for child in body.syntax().children() {
        match child.kind() {
            SyntaxKind::Step => {
                if let Some(step) = ast::Step::cast(child) {
                    if let Some(shell) = step.shell_text() {
                        // Full step with an explicit body.
                        script.push(dedent(&shell));
                    } else if let Some(name) = step.name() {
                        // Bare step reference: inline the named template step.
                        if let Some((_, shell)) = template_steps
                            .iter()
                            .find(|(n, _)| n.as_str() == name.as_str())
                        {
                            script.push(shell.clone());
                        }
                    }
                }
            }
            SyntaxKind::StepsKeyword => {
                // Expand all template steps not already covered by explicit
                // steps in this job.
                for (name, shell) in template_steps {
                    if !explicit_names.contains(name) {
                        script.push(shell.clone());
                    }
                }
            }
            _ => {}
        }
    }

    script
}

fn dedent(s: &str) -> String {
    let lines: Vec<&str> = s.lines().collect();
    let min_indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|l| {
            if l.len() >= min_indent {
                &l[min_indent..]
            } else {
                l.trim_start()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}
