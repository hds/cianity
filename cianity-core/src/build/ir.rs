use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ciane::{
    ast::{self, AstNode, HasAttrList, HasName, JobBodySteps, Root},
    parse,
    syntax::SyntaxKind,
};

// ─── IR types ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WorkflowStrategy {
    #[default]
    None,
    DefaultBranch,
    Reviews,
    DefaultBranchAndReviews,
}

#[derive(Debug)]
pub struct Workflow {
    pub stages: Vec<Stage>,
    pub strategy: WorkflowStrategy,
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
    pub artifacts: Vec<String>,
    /// Names of environment variables this job exports to downstream jobs (no `$` prefix).
    pub env: Vec<String>,
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
    artifacts: Vec<String>,
    env: Vec<String>,
}

/// Raw template data before inheritance is resolved: own attributes plus
/// the list of local template names to inherit from (in order, last wins).
struct RawTemplateEntry {
    own: TemplateData,
    inherit_names: Vec<String>,
}

/// Merge `overlay` on top of `base`.
///
/// - Steps: overlay steps replace same-named base steps; new overlay steps are appended.
/// - Image: overlay image wins if present, otherwise base image.
/// - Needs: overlay needs win if non-empty, otherwise base needs.
/// - Artifacts: base and overlay artifacts are concatenated (both kept).
/// - Env: overlay env wins if non-empty, otherwise base env.
fn merge_template_data(base: TemplateData, overlay: TemplateData) -> TemplateData {
    let mut steps = base.steps;
    for (name, shell) in overlay.steps {
        match steps.iter_mut().find(|(n, _)| n == &name) {
            Some((_, existing)) => *existing = shell,
            None => steps.push((name, shell)),
        }
    }
    let image = overlay.image.or(base.image);
    let needs = if overlay.needs.is_empty() {
        base.needs
    } else {
        overlay.needs
    };
    let mut artifacts = base.artifacts;
    artifacts.extend(overlay.artifacts);
    let env = if overlay.env.is_empty() {
        base.env
    } else {
        overlay.env
    };
    TemplateData {
        steps,
        image,
        needs,
        artifacts,
        env,
    }
}

/// Split a return-annotation list into artifact paths and exported env var names.
///
/// Items starting with `$` are env var names (the `$` is stripped); all other
/// items are treated as artifact paths or globs.
fn split_return_annotation(ra: &ast::ReturnAnnotation) -> (Vec<String>, Vec<String>) {
    let mut artifacts = Vec::new();
    let mut env = Vec::new();
    if let Some(pl) = ra.path_list() {
        for item in pl.items() {
            if let Some(text) = item.path_text() {
                if let Some(name) = text.strip_prefix('$') {
                    env.push(name.to_string());
                } else {
                    artifacts.push(text.to_string());
                }
            }
        }
    }
    (artifacts, env)
}

fn inherit_names_from_attr(attr: &ast::Attr) -> Vec<String> {
    attr.value_text().map_or_else(
        || {
            attr.value()
                .and_then(|av| av.ref_list())
                .map(|rl| rl.refs().map(|r| r.text()).collect())
                .unwrap_or_default()
        },
        |v| vec![v.to_string()],
    )
}

fn raw_template_data_from_ast(tmpl: &ast::TemplateDef) -> (TemplateData, Vec<String>) {
    let steps = tmpl
        .body()
        .map_or_else(Vec::new, |b| collect_template_steps(&b));
    let mut image = None;
    let mut needs = Vec::new();
    let mut inherit_names = Vec::new();
    let mut artifacts = Vec::new();
    let mut env = Vec::new();
    if let Some(al) = tmpl.attr_list() {
        for attr in al.attrs() {
            match attr.key_text().as_deref() {
                Some("image") => image = attr.value_text().map(|s| s.to_string()),
                Some("dependencies") => {
                    if let Some(val) = attr.value() {
                        needs = refs_from_attr_value(&val);
                    }
                }
                Some("inherit") => inherit_names = inherit_names_from_attr(&attr),
                _ => {}
            }
        }
    }
    if let Some(ra) = tmpl.return_annotation() {
        let (ra_artifacts, ra_env) = split_return_annotation(&ra);
        if !ra_artifacts.is_empty() {
            artifacts = ra_artifacts;
        }
        if !ra_env.is_empty() {
            env = ra_env;
        }
    }
    (
        TemplateData {
            steps,
            image,
            needs,
            artifacts,
            env,
        },
        inherit_names,
    )
}

fn template_data_from_ast(tmpl: &ast::TemplateDef) -> TemplateData {
    raw_template_data_from_ast(tmpl).0
}

/// Resolve one template by name, following its `inherit` chain.
///
/// `local` contains raw (unresolved) templates at the current scope.
/// `parent` contains already-resolved templates from the enclosing scope
/// (e.g. root-level templates when resolving stage-level templates).
/// `resolved` is the memoisation cache for the current resolution pass.
/// `stack` tracks the current resolution path for cycle detection.
fn resolve_template(
    name: &str,
    local: &HashMap<String, RawTemplateEntry>,
    parent: &HashMap<String, TemplateData>,
    resolved: &mut HashMap<String, TemplateData>,
    stack: &mut Vec<String>,
) -> TemplateData {
    if let Some(data) = resolved.get(name) {
        return data.clone();
    }
    // Local scope shadows parent scope.
    let Some(raw) = local.get(name) else {
        return parent.get(name).cloned().unwrap_or_default();
    };
    if stack.iter().any(|n| n == name) {
        // Circular inheritance — break cycle by contributing nothing.
        return TemplateData::default();
    }
    stack.push(name.to_string());
    let mut merged = TemplateData::default();
    for parent_name in &raw.inherit_names {
        if parent_name.contains('/') {
            // Cross-file refs are not resolved inside template inheritance chains.
            continue;
        }
        let parent_data = resolve_template(parent_name, local, parent, resolved, stack);
        merged = merge_template_data(merged, parent_data);
    }
    merged = merge_template_data(merged, raw.own.clone());
    stack.pop();
    resolved.insert(name.to_string(), merged.clone());
    merged
}

/// Resolve all templates in `local`, using `parent` as the enclosing scope.
fn resolve_all(
    local: &HashMap<String, RawTemplateEntry>,
    parent: &HashMap<String, TemplateData>,
) -> HashMap<String, TemplateData> {
    let mut resolved = HashMap::new();
    let mut stack = Vec::new();
    for name in local.keys() {
        resolve_template(name, local, parent, &mut resolved, &mut stack);
    }
    resolved
}

fn strategy_from_root(root: &Root) -> WorkflowStrategy {
    root.workflow_defs()
        .next()
        .and_then(|wd| wd.strategy())
        .as_deref()
        .map(strategy_from_str)
        .unwrap_or_default()
}

fn strategy_from_str(s: &str) -> WorkflowStrategy {
    match s {
        "default_branch" => WorkflowStrategy::DefaultBranch,
        "reviews" => WorkflowStrategy::Reviews,
        "default_branch_and_reviews" => WorkflowStrategy::DefaultBranchAndReviews,
        _ => WorkflowStrategy::None,
    }
}

/// Lower a parsed `Root` AST node into the rich IR `Workflow`.
///
/// Templates are resolved and inlined into their jobs; the returned `Workflow`
/// contains only concrete jobs.
#[must_use]
pub fn lower(root: &Root) -> Workflow {
    let strategy = strategy_from_root(root);
    let root_templates = collect_root_templates(root);
    let mut stages = Vec::new();

    for stage in root.stages() {
        let stage_name = stage.name().map_or_else(String::new, |s| s.to_string());
        let Some(body) = stage.body() else {
            continue;
        };

        let stage_templates = collect_local_templates(&body, &root_templates);
        let mut jobs = Vec::new();

        for job in body.jobs() {
            let job_name = job.name().map_or_else(String::new, |s| s.to_string());
            let JobAttrs {
                mut image,
                inherit_names,
                mut needs,
                mut artifacts,
                mut env,
            } = parse_job_attrs(&job);

            let mut template_data = TemplateData::default();
            for name in &inherit_names {
                if name.contains('/') {
                    continue; // No path context; cross-file refs produce empty scripts.
                }
                if let Some(td) = stage_templates
                    .get(name)
                    .or_else(|| root_templates.get(name))
                {
                    template_data = merge_template_data(template_data, td.clone());
                }
            }

            if image.is_none() {
                image.clone_from(&template_data.image);
            }
            if needs.is_empty() {
                needs.clone_from(&template_data.needs);
            }
            let mut merged_artifacts = template_data.artifacts.clone();
            merged_artifacts.extend(artifacts);
            artifacts = merged_artifacts;
            if env.is_empty() {
                env.clone_from(&template_data.env);
            }

            let script = job_script(&job, &template_data.steps);

            jobs.push(Job {
                name: job_name,
                stage: stage_name.clone(),
                image,
                script,
                needs,
                artifacts,
                env,
            });
        }

        stages.push(Stage {
            name: stage_name,
            jobs,
        });
    }

    Workflow { stages, strategy }
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
    let strategy = strategy_from_root(root);
    let base = path.parent().unwrap_or(Path::new("."));
    let import_map = build_import_map(root, base);
    let root_templates = collect_root_templates(root);
    let mut stages = Vec::new();

    for stage in root.stages() {
        let stage_name = stage.name().map_or_else(String::new, |s| s.to_string());
        let Some(body) = stage.body() else {
            continue;
        };

        let stage_templates = collect_local_templates(&body, &root_templates);
        let mut jobs = Vec::new();

        for job in body.jobs() {
            let job_name = job.name().map_or_else(String::new, |s| s.to_string());
            let JobAttrs {
                mut image,
                inherit_names,
                mut needs,
                mut artifacts,
                mut env,
            } = parse_job_attrs(&job);

            let mut template_data = TemplateData::default();
            for name in &inherit_names {
                let td = if let Some((import_name, template_ref)) = name.split_once('/') {
                    let file_path = import_map
                        .get(import_name)
                        .ok_or_else(|| anyhow::anyhow!("unknown import `{import_name}`"))?;
                    if let Some((sname, tname)) = template_ref.split_once('.') {
                        load_cross_file_stage_template(file_path, sname, tname)?
                    } else {
                        load_cross_file_top_level_template(file_path, template_ref)?
                    }
                } else {
                    stage_templates
                        .get(name)
                        .or_else(|| root_templates.get(name))
                        .cloned()
                        .unwrap_or_default()
                };
                template_data = merge_template_data(template_data, td);
            }

            if image.is_none() {
                image.clone_from(&template_data.image);
            }
            if needs.is_empty() {
                needs.clone_from(&template_data.needs);
            }
            let mut merged_artifacts = template_data.artifacts.clone();
            merged_artifacts.extend(artifacts);
            artifacts = merged_artifacts;
            if env.is_empty() {
                env.clone_from(&template_data.env);
            }

            let script = job_script(&job, &template_data.steps);

            jobs.push(Job {
                name: job_name,
                stage: stage_name.clone(),
                image,
                script,
                needs,
                artifacts,
                env,
            });
        }

        stages.push(Stage {
            name: stage_name,
            jobs,
        });
    }

    Ok(Workflow { stages, strategy })
}

struct JobAttrs {
    image: Option<String>,
    inherit_names: Vec<String>,
    needs: Vec<JobRef>,
    artifacts: Vec<String>,
    env: Vec<String>,
}

// ─── helpers ─────────────────────────────────────────────────────────────────

fn collect_root_templates(root: &Root) -> HashMap<String, TemplateData> {
    let local: HashMap<String, RawTemplateEntry> = root
        .templates()
        .filter_map(|t| {
            let name = t.name()?.to_string();
            let (own, inherit_names) = raw_template_data_from_ast(&t);
            Some((name, RawTemplateEntry { own, inherit_names }))
        })
        .collect();
    resolve_all(&local, &HashMap::new())
}

fn collect_local_templates(
    body: &ast::StageBody,
    root_resolved: &HashMap<String, TemplateData>,
) -> HashMap<String, TemplateData> {
    let local: HashMap<String, RawTemplateEntry> = body
        .templates()
        .filter_map(|t| {
            let name = t.name()?.to_string();
            let (own, inherit_names) = raw_template_data_from_ast(&t);
            Some((name, RawTemplateEntry { own, inherit_names }))
        })
        .collect();
    resolve_all(&local, root_resolved)
}

fn parse_job_attrs(job: &ast::Job) -> JobAttrs {
    let mut image = None;
    let mut inherit_names = Vec::new();
    let mut needs = Vec::new();
    let mut artifacts = Vec::new();
    let mut env = Vec::new();
    if let Some(al) = job.attr_list() {
        for attr in al.attrs() {
            match attr.key_text().as_deref() {
                Some("image") => image = attr.value_text().map(|s| s.to_string()),
                Some("inherit") => inherit_names = inherit_names_from_attr(&attr),
                Some("dependencies") => {
                    if let Some(val) = attr.value() {
                        needs = refs_from_attr_value(&val);
                    }
                }
                _ => {}
            }
        }
    }
    if let Some(ra) = job.return_annotation() {
        let (ra_artifacts, ra_env) = split_return_annotation(&ra);
        if !ra_artifacts.is_empty() {
            artifacts = ra_artifacts;
        }
        if !ra_env.is_empty() {
            env = ra_env;
        }
    }
    JobAttrs {
        image,
        inherit_names,
        needs,
        artifacts,
        env,
    }
}

fn refs_from_attr_value(val: &ast::AttrValue) -> Vec<JobRef> {
    val.ref_list()
        .map(|rl| {
            rl.refs()
                .filter_map(|r| {
                    let text = r.text();
                    let (s, j) = text.trim().split_once('.')?;
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
    for imp in root.use_decls() {
        if let Some((name, path)) = imp.name().zip(imp.path()) {
            map.insert(name.to_string(), base.join(path.as_str()));
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
