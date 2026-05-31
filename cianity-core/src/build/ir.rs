use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ciane::{
    ast::{self, AstNode, HasAttrList, HasName, JobBodySteps, Root},
    parse,
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

#[derive(Debug, Clone)]
pub struct JobRef {
    pub stage: String,
    pub job: String,
}

// ─── Lowering ────────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct TemplateData {
    steps: Vec<(String, String)>,
    image: Option<String>,
    needs: Vec<JobRef>,
}

/// Lower a parsed `Root` AST node into the rich IR `Workflow`.
///
/// Templates are resolved and inlined into their jobs; the returned `Workflow`
/// contains only concrete jobs.
#[must_use]
pub fn lower(root: &Root) -> Workflow {
    let root_templates = collect_root_templates(root);
    let mut stages = Vec::new();

    for stage in root.stages() {
        let stage_name = stage.name().map_or_else(String::new, |s| s.to_string());
        let Some(body) = stage.body() else {
            continue;
        };

        let stage_templates = collect_local_templates(&body);
        let mut jobs = Vec::new();

        for job in body.jobs() {
            let job_name = job.name().map_or_else(String::new, |s| s.to_string());
            let (mut image, inherit, mut needs) = parse_job_attrs(&job);

            // Stage-local templates shadow root-level templates.
            let template_data = inherit
                .as_deref()
                .filter(|t| !t.contains('/'))
                .and_then(|t| stage_templates.get(t).or_else(|| root_templates.get(t)));

            if image.is_none() {
                image = template_data.and_then(|td| td.image.clone());
            }
            if needs.is_empty()
                && let Some(td) = template_data
            {
                needs.clone_from(&td.needs);
            }

            let empty: Vec<(String, String)> = Vec::new();
            let template_steps = template_data.map_or(empty.as_slice(), |td| td.steps.as_slice());

            let script = job_script(&job, template_steps);

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

/// Lower a parsed `Root` into a `Workflow`, resolving cross-file template
/// references via the `use {}` import map.
///
/// Paths in `use {}` blocks are resolved relative to `path`'s parent directory.
///
/// Cross-file reference formats:
/// - `ns/tmpl` — top-level template in the imported file
/// - `ns/stage.tmpl` — template inside `stage` in the imported file
///
/// # Errors
///
/// Returns `Err` if a referenced import file cannot be read, or if the named
/// template is not found in that file.
pub fn lower_with_path(root: &Root, path: &Path) -> anyhow::Result<Workflow> {
    let base = path.parent().unwrap_or(Path::new("."));
    let import_map = build_import_map(root, base);
    let root_templates = collect_root_templates(root);
    let mut stages = Vec::new();

    for stage in root.stages() {
        let stage_name = stage.name().map_or_else(String::new, |s| s.to_string());
        let Some(body) = stage.body() else {
            continue;
        };

        let stage_templates = collect_local_templates(&body);
        let mut jobs = Vec::new();

        for job in body.jobs() {
            let job_name = job.name().map_or_else(String::new, |s| s.to_string());
            let (mut image, inherit, mut needs) = parse_job_attrs(&job);

            let template_data: TemplateData = if let Some(name) = inherit.as_deref() {
                if let Some((import_name, template_ref)) = name.split_once('/') {
                    let file_path = import_map
                        .get(import_name)
                        .ok_or_else(|| anyhow::anyhow!("unknown import `{import_name}`"))?;
                    // `ns/tmpl` → top-level; `ns/stage.tmpl` → stage-local
                    if let Some((sname, tname)) = template_ref.split_once('.') {
                        load_cross_file_stage_template(file_path, sname, tname)?
                    } else {
                        load_cross_file_top_level_template(file_path, template_ref)?
                    }
                } else {
                    // Stage-local shadows root-level.
                    stage_templates
                        .get(name)
                        .or_else(|| root_templates.get(name))
                        .cloned()
                        .unwrap_or_default()
                }
            } else {
                TemplateData::default()
            };

            if image.is_none() {
                image.clone_from(&template_data.image);
            }
            if needs.is_empty() {
                needs.clone_from(&template_data.needs);
            }

            let script = job_script(&job, &template_data.steps);

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

    Ok(Workflow { stages })
}

// ─── helpers ─────────────────────────────────────────────────────────────────

fn collect_root_templates(root: &Root) -> HashMap<String, TemplateData> {
    root.templates()
        .filter_map(|t| {
            let name = t.name()?.to_string();
            Some((name, template_data_from_ast(&t)))
        })
        .collect()
}

fn collect_local_templates(body: &ast::StageBody) -> HashMap<String, TemplateData> {
    body.templates()
        .filter_map(|t| {
            let name = t.name()?.to_string();
            Some((name, template_data_from_ast(&t)))
        })
        .collect()
}

fn template_data_from_ast(tmpl: &ast::TemplateDef) -> TemplateData {
    let steps = tmpl
        .body()
        .map_or_else(Vec::new, |b| collect_template_steps(&b));
    let mut image = None;
    let mut needs = Vec::new();
    if let Some(al) = tmpl.attr_list() {
        for attr in al.attrs() {
            match attr.key_text().as_deref() {
                Some("image") => image = attr.value_text().map(|s| s.to_string()),
                Some("dependencies") => {
                    if let Some(val) = attr.value() {
                        needs = refs_from_attr_value(&val);
                    }
                }
                _ => {}
            }
        }
    }
    TemplateData {
        steps,
        image,
        needs,
    }
}

fn parse_job_attrs(job: &ast::Job) -> (Option<String>, Option<String>, Vec<JobRef>) {
    let mut image = None;
    let mut inherit = None;
    let mut needs = Vec::new();
    if let Some(al) = job.attr_list() {
        for attr in al.attrs() {
            match attr.key_text().as_deref() {
                Some("image") => image = attr.value_text().map(|s| s.to_string()),
                Some("inherit") => inherit = attr.value_text().map(|s| s.to_string()),
                Some("dependencies") => {
                    if let Some(val) = attr.value() {
                        needs = refs_from_attr_value(&val);
                    }
                }
                _ => {}
            }
        }
    }
    (image, inherit, needs)
}

fn refs_from_attr_value(val: &ast::AttrValue) -> Vec<JobRef> {
    val.ref_list()
        .map(|rl| {
            rl.refs()
                .filter_map(|r| {
                    let text = r.text();
                    let (s, j) = text.split_once('.')?;
                    Some(JobRef {
                        stage: s.to_string(),
                        job: j.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn job_script(job: &ast::Job, template_steps: &[(String, String)]) -> Vec<String> {
    if let Some(inline) = job.inline_body() {
        inline
            .shell_text()
            .map(|s| vec![dedent(&s)])
            .unwrap_or_default()
    } else if let Some(steps_body) = job.steps_body() {
        resolve_steps(&steps_body, template_steps)
    } else {
        Vec::new()
    }
}

fn build_import_map(root: &Root, base: &Path) -> HashMap<String, PathBuf> {
    let mut map = HashMap::new();
    for ub in root.use_blocks() {
        for imp in ub.imports() {
            if let Some((name, loc)) = imp.name().zip(imp.location()) {
                map.insert(name.to_string(), base.join(loc.as_str()));
            }
        }
    }
    map
}

fn load_cross_file_top_level_template(
    path: &Path,
    template_name: &str,
) -> anyhow::Result<TemplateData> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?;
    let result = parse(&source);
    let root =
        Root::cast(result.syntax()).ok_or_else(|| anyhow::anyhow!("internal: no Root node"))?;
    for tmpl in root.templates() {
        if tmpl.name().as_deref() == Some(template_name) {
            return Ok(template_data_from_ast(&tmpl));
        }
    }
    anyhow::bail!(
        "top-level template `{template_name}` not found in `{}`",
        path.display()
    )
}

fn load_cross_file_stage_template(
    path: &Path,
    stage_name: &str,
    template_name: &str,
) -> anyhow::Result<TemplateData> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?;
    let result = parse(&source);
    let root =
        Root::cast(result.syntax()).ok_or_else(|| anyhow::anyhow!("internal: no Root node"))?;
    for stage in root.stages() {
        if stage.name().as_deref() != Some(stage_name) {
            continue;
        }
        if let Some(body) = stage.body() {
            for tmpl in body.templates() {
                if tmpl.name().as_deref() == Some(template_name) {
                    return Ok(template_data_from_ast(&tmpl));
                }
            }
        }
    }
    anyhow::bail!(
        "template `{template_name}` not found in stage `{stage_name}` of `{}`",
        path.display()
    )
}

fn collect_template_steps(body: &JobBodySteps) -> Vec<(String, String)> {
    body.steps()
        .filter_map(|s| {
            let name = s.name()?.to_string();
            let shell = s.shell_text()?;
            Some((name, dedent(&shell)))
        })
        .collect()
}

fn resolve_steps(body: &JobBodySteps, template_steps: &[(String, String)]) -> Vec<String> {
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
