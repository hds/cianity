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
    /// CI variables passed directly to this job (key-value pairs).
    pub variables: Vec<(String, String)>,
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

    /// `true` if this job depends on another job in its own stage, meaning it
    /// must be ordered after that job rather than running concurrently.
    #[must_use]
    pub fn has_same_stage_dependency(&self) -> bool {
        self.needs.iter().any(|r| r.stage == self.stage)
    }
}

#[derive(Debug, Clone)]
pub struct JobRef {
    pub stage: String,
    pub job: String,
}

/// A job dependency that can't be satisfied: on itself, on a job that doesn't
/// exist, on a job in a later stage, or part of a cycle between jobs in the
/// same stage.
#[derive(Debug)]
pub struct DependencyError {
    /// Stage of the job that declares (or inherits) the dependency.
    pub stage: String,
    /// Name of the job that declares (or inherits) the dependency.
    pub job: String,
    pub message: String,
}

/// Find all dependencies in `workflow` which can't be satisfied.
///
/// Dependencies are checked on the lowered workflow so that those inherited
/// from templates are included. References which aren't in `stage.job` form
/// never reach the IR; they're reported by `ciane` validation.
#[must_use]
pub fn dependency_errors(workflow: &Workflow) -> Vec<DependencyError> {
    let mut errors = Vec::new();

    for (stage_idx, stage) in workflow.stages.iter().enumerate() {
        for job in &stage.jobs {
            for dep in &job.needs {
                let dep_stage_idx = workflow.stages.iter().position(|s| s.name == dep.stage);
                let message = if dep.stage == job.stage && dep.job == job.name {
                    format!("job `{}` depends on itself", job.full_name())
                } else if dep_stage_idx.is_none() {
                    format!(
                        "job `{}` depends on `{}.{}`, but there is no stage `{}`",
                        job.full_name(),
                        dep.stage,
                        dep.job,
                        dep.stage
                    )
                } else if workflow.job(&dep.stage, &dep.job).is_none() {
                    format!(
                        "job `{}` depends on `{}.{}`, but stage `{}` has no job `{}`",
                        job.full_name(),
                        dep.stage,
                        dep.job,
                        dep.stage,
                        dep.job
                    )
                } else if dep_stage_idx.is_some_and(|dep_idx| dep_idx > stage_idx) {
                    format!(
                        "job `{}` depends on `{}.{}` in later stage `{}`; dependencies must be \
                         on jobs in the same or an earlier stage",
                        job.full_name(),
                        dep.stage,
                        dep.job,
                        dep.stage
                    )
                } else {
                    continue;
                };
                errors.push(DependencyError {
                    stage: job.stage.clone(),
                    job: job.name.clone(),
                    message,
                });
            }
        }
        same_stage_cycle_errors(stage, &mut errors);
    }

    errors
}

/// Report each cycle formed by dependencies between jobs within `stage`.
///
/// Self-dependencies are excluded, as they're reported separately.
fn same_stage_cycle_errors(stage: &Stage, errors: &mut Vec<DependencyError>) {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Visit {
        Unvisited,
        InProgress,
        Done,
    }

    fn visit(
        idx: usize,
        stage: &Stage,
        visits: &mut [Visit],
        path: &mut Vec<usize>,
        errors: &mut Vec<DependencyError>,
    ) {
        visits[idx] = Visit::InProgress;
        path.push(idx);
        let job = &stage.jobs[idx];
        for dep in &job.needs {
            if dep.stage != stage.name || dep.job == job.name {
                continue;
            }
            let Some(dep_idx) = stage.jobs.iter().position(|j| j.name == dep.job) else {
                continue;
            };
            match visits[dep_idx] {
                Visit::Unvisited => visit(dep_idx, stage, visits, path, errors),
                Visit::InProgress => {
                    let start = path
                        .iter()
                        .position(|&i| i == dep_idx)
                        .expect("in-progress job is on the current path");
                    let mut names: Vec<&str> = path[start..]
                        .iter()
                        .map(|&i| stage.jobs[i].name.as_str())
                        .collect();
                    names.push(&stage.jobs[dep_idx].name);
                    errors.push(DependencyError {
                        stage: stage.name.clone(),
                        job: stage.jobs[dep_idx].name.clone(),
                        message: format!(
                            "dependency cycle in stage `{}`: {}",
                            stage.name,
                            names.join(" -> ")
                        ),
                    });
                }
                Visit::Done => {}
            }
        }
        path.pop();
        visits[idx] = Visit::Done;
    }

    let mut visits = vec![Visit::Unvisited; stage.jobs.len()];
    let mut path = Vec::new();
    for idx in 0..stage.jobs.len() {
        if visits[idx] == Visit::Unvisited {
            visit(idx, stage, &mut visits, &mut path, errors);
        }
    }
}

// ─── Lowering ────────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct TemplateData {
    steps: Vec<(String, String)>,
    image: Option<String>,
    needs: Vec<JobRef>,
    artifacts: Vec<String>,
    env: Vec<String>,
    variables: Vec<(String, String)>,
    /// Variable names to remove from any base that this data is merged onto.
    /// Always empty in fully-resolved `TemplateData`; populated only in `raw.own`.
    unset_variables: Vec<String>,
}

/// Raw template data before inheritance is resolved: own attributes plus
/// the list of local template names to inherit from (in order, last wins).
struct RawTemplateEntry {
    own: TemplateData,
    inherit_names: Vec<String>,
}

/// Merge two variable lists: overlay entries override same-named base entries;
/// base entries not present in the overlay are kept.
fn merge_variables(
    base: Vec<(String, String)>,
    overlay: Vec<(String, String)>,
) -> Vec<(String, String)> {
    let mut result = base;
    for (key, value) in overlay {
        match result.iter_mut().find(|(k, _)| k == &key) {
            Some((_, existing)) => *existing = value,
            None => result.push((key, value)),
        }
    }
    result
}

/// Merge `overlay` on top of `base`.
///
/// - Steps: overlay steps replace same-named base steps; new overlay steps are appended.
/// - Image: overlay image wins if present, otherwise base image.
/// - Needs: overlay needs win if non-empty, otherwise base needs.
/// - Artifacts: base and overlay artifacts are concatenated (both kept).
/// - Env: overlay env wins if non-empty, otherwise base env.
/// - Variables: overlay `unset_variables` are removed first, then overlay entries
///   override same-named base entries; new overlay entries are appended.
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
    let mut base_vars = base.variables;
    for key in &overlay.unset_variables {
        base_vars.retain(|(k, _)| k != key);
    }
    let variables = merge_variables(base_vars, overlay.variables);
    TemplateData {
        steps,
        image,
        needs,
        artifacts,
        env,
        variables,
        unset_variables: Vec::new(), // unsets are consumed during merge
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
    let steps = if let Some(inline) = tmpl.inline_body() {
        let step_name = tmpl.name().map_or_else(String::new, |n| n.to_string());
        inline
            .shell_text()
            .map(|s| vec![(step_name, dedent(&s))])
            .unwrap_or_default()
    } else {
        tmpl.body()
            .map_or_else(Vec::new, |b| collect_template_steps(&b))
    };
    let mut image = None;
    let mut needs = Vec::new();
    let mut inherit_names = Vec::new();
    let mut artifacts = Vec::new();
    let mut env = Vec::new();
    let mut variables = Vec::new();
    let mut unset_variables = Vec::new();
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
                Some("variables") => {
                    if let Some(val) = attr.value() {
                        (variables, unset_variables) = vars_and_unsets_from_attr_value(&val);
                    }
                }
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
            variables,
            unset_variables,
        },
        inherit_names,
    )
}

fn template_data_from_ast(tmpl: &ast::TemplateDef) -> TemplateData {
    raw_template_data_from_ast(tmpl).0
}

/// Identifies a template within a file: the stage it is defined in, or `None`
/// for a top-level one.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct TemplateKey {
    stage: Option<String>,
    name: String,
}

/// Every template in a file, with its `inherit` chain already applied.
struct TemplateScope {
    resolved: HashMap<TemplateKey, TemplateData>,
}

impl TemplateScope {
    /// Resolve every template in the file, top-level and stage-level alike.
    fn collect(root: &Root) -> Self {
        let mut raw: HashMap<TemplateKey, RawTemplateEntry> = HashMap::new();
        let mut add = |stage: Option<String>, tmpl: &ast::TemplateDef| {
            if let Some(name) = tmpl.name() {
                let (own, inherit_names) = raw_template_data_from_ast(tmpl);
                raw.insert(
                    TemplateKey {
                        stage,
                        name: name.to_string(),
                    },
                    RawTemplateEntry { own, inherit_names },
                );
            }
        };

        for tmpl in root.templates() {
            add(None, &tmpl);
        }
        for stage in root.stages() {
            let Some(stage_name) = stage.name() else {
                continue;
            };
            let Some(body) = stage.body() else {
                continue;
            };
            for tmpl in body.templates() {
                add(Some(stage_name.to_string()), &tmpl);
            }
        }

        let mut resolved = HashMap::new();
        let mut stack = Vec::new();
        for key in raw.keys() {
            resolve_key(key, &raw, &mut resolved, &mut stack);
        }
        Self { resolved }
    }

    /// The template an `inherit` name written in `context_stage` refers to.
    fn get(&self, name: &str, context_stage: Option<&str>) -> Option<&TemplateData> {
        let key = template_key(name, context_stage, |key| self.resolved.contains_key(key))?;
        self.resolved.get(&key)
    }
}

/// The template an `inherit` name refers to from `context_stage`, or `None`
/// for a cross-file reference, which this can't resolve.
///
/// A plain name is the stage's own template when it has one, and a top-level
/// template otherwise. `stage.name` names a template in another stage.
fn template_key(
    name: &str,
    context_stage: Option<&str>,
    exists: impl Fn(&TemplateKey) -> bool,
) -> Option<TemplateKey> {
    if name.contains('/') {
        return None;
    }
    if let Some((stage_name, template_name)) = name.split_once('.') {
        return Some(TemplateKey {
            stage: Some(stage_name.to_owned()),
            name: template_name.to_owned(),
        });
    }
    let in_stage = TemplateKey {
        stage: context_stage.map(ToOwned::to_owned),
        name: name.to_owned(),
    };
    if context_stage.is_some() && exists(&in_stage) {
        return Some(in_stage);
    }
    Some(TemplateKey {
        stage: None,
        name: name.to_owned(),
    })
}

/// Resolve one template, following its `inherit` chain.
///
/// `resolved` memoises this pass and `stack` is the current resolution path,
/// used to break inheritance cycles.
fn resolve_key(
    key: &TemplateKey,
    raw: &HashMap<TemplateKey, RawTemplateEntry>,
    resolved: &mut HashMap<TemplateKey, TemplateData>,
    stack: &mut Vec<TemplateKey>,
) -> TemplateData {
    if let Some(data) = resolved.get(key) {
        return data.clone();
    }
    let Some(entry) = raw.get(key) else {
        return TemplateData::default();
    };
    if stack.contains(key) {
        // Circular inheritance — break the cycle by contributing nothing.
        return TemplateData::default();
    }
    stack.push(key.clone());
    let mut merged = TemplateData::default();
    for name in &entry.inherit_names {
        // Cross-file refs are not resolved inside template inheritance chains.
        if let Some(parent) = template_key(name, key.stage.as_deref(), |k| raw.contains_key(k)) {
            let data = resolve_key(&parent, raw, resolved, stack);
            merged = merge_template_data(merged, data);
        }
    }
    merged = merge_template_data(merged, entry.own.clone());
    stack.pop();
    resolved.insert(key.clone(), merged.clone());
    merged
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
    let templates = TemplateScope::collect(root);
    let mut stages = Vec::new();

    for stage in root.stages() {
        let stage_name = stage.name().map_or_else(String::new, |s| s.to_string());
        let Some(body) = stage.body() else {
            continue;
        };

        let mut jobs = Vec::new();

        for job in body.jobs() {
            let job_name = job.name().map_or_else(String::new, |s| s.to_string());
            let JobAttrs {
                mut image,
                inherit_names,
                mut needs,
                mut artifacts,
                mut env,
                mut variables,
                unset_variables,
            } = parse_job_attrs(&job);

            let mut template_data = TemplateData::default();
            for name in &inherit_names {
                if name.contains('/') {
                    continue; // No path context; cross-file refs produce empty scripts.
                }
                if let Some(td) = templates.get(name, Some(&stage_name)) {
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
            let mut base_vars = template_data.variables.clone();
            for key in &unset_variables {
                base_vars.retain(|(k, _)| k != key);
            }
            variables = merge_variables(base_vars, variables);

            let script = job_script(&job, &template_data.steps);

            jobs.push(Job {
                name: job_name,
                stage: stage_name.clone(),
                image,
                script,
                needs,
                artifacts,
                env,
                variables,
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
    let (workflow, errors) = lower_with_path_partial(root, path);
    match errors.into_iter().next() {
        Some(err) => Err(err),
        None => Ok(workflow),
    }
}

/// Lower a parsed `Root` into a `Workflow` like [`lower_with_path`], but
/// without stopping at cross-file templates which can't be resolved.
///
/// Each such template contributes nothing to the jobs inheriting from it, and
/// the reason it couldn't be resolved is returned alongside the workflow. This
/// allows the rest of the workflow to be checked in the same pass.
#[must_use]
pub fn lower_with_path_partial(root: &Root, path: &Path) -> (Workflow, Vec<anyhow::Error>) {
    let mut errors = Vec::new();
    let strategy = strategy_from_root(root);
    let base = path.parent().unwrap_or(Path::new("."));
    let import_map = build_import_map(root, base);
    let templates = TemplateScope::collect(root);
    let mut stages = Vec::new();

    for stage in root.stages() {
        let stage_name = stage.name().map_or_else(String::new, |s| s.to_string());
        let Some(body) = stage.body() else {
            continue;
        };

        let mut jobs = Vec::new();

        for job in body.jobs() {
            let job_name = job.name().map_or_else(String::new, |s| s.to_string());
            let JobAttrs {
                mut image,
                inherit_names,
                mut needs,
                mut artifacts,
                mut env,
                mut variables,
                unset_variables,
            } = parse_job_attrs(&job);

            let mut template_data = TemplateData::default();
            for name in &inherit_names {
                let td = if let Some((import_name, template_ref)) = name.split_once('/') {
                    import_map
                        .get(import_name)
                        .ok_or_else(|| anyhow::anyhow!("unknown import `{import_name}`"))
                        .and_then(|file_path| {
                            if let Some((sname, tname)) = template_ref.split_once('.') {
                                load_cross_file_stage_template(file_path, sname, tname)
                            } else {
                                load_cross_file_top_level_template(file_path, template_ref)
                            }
                        })
                        .unwrap_or_else(|err| {
                            errors.push(err);
                            TemplateData::default()
                        })
                } else {
                    templates
                        .get(name, Some(&stage_name))
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
            let mut base_vars = template_data.variables.clone();
            for key in &unset_variables {
                base_vars.retain(|(k, _)| k != key);
            }
            variables = merge_variables(base_vars, variables);

            let script = job_script(&job, &template_data.steps);

            jobs.push(Job {
                name: job_name,
                stage: stage_name.clone(),
                image,
                script,
                needs,
                artifacts,
                env,
                variables,
            });
        }

        stages.push(Stage {
            name: stage_name,
            jobs,
        });
    }

    (Workflow { stages, strategy }, errors)
}

struct JobAttrs {
    image: Option<String>,
    inherit_names: Vec<String>,
    needs: Vec<JobRef>,
    artifacts: Vec<String>,
    env: Vec<String>,
    variables: Vec<(String, String)>,
    unset_variables: Vec<String>,
}

// ─── helpers ─────────────────────────────────────────────────────────────────

fn parse_job_attrs(job: &ast::Job) -> JobAttrs {
    let mut image = None;
    let mut inherit_names = Vec::new();
    let mut needs = Vec::new();
    let mut artifacts = Vec::new();
    let mut env = Vec::new();
    let mut variables = Vec::new();
    let mut unset_variables = Vec::new();
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
                Some("variables") => {
                    if let Some(val) = attr.value() {
                        (variables, unset_variables) = vars_and_unsets_from_attr_value(&val);
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
        variables,
        unset_variables,
    }
}

/// Job references from a `dependencies` value.
///
/// References that aren't in `stage.job` form are skipped; they're reported by
/// `ciane` validation, and resolving them here would report them a second
/// time as missing jobs.
fn refs_from_attr_value(val: &ast::AttrValue) -> Vec<JobRef> {
    val.ref_list()
        .map(|rl| {
            rl.refs()
                .filter_map(|r| {
                    let (stage, job) = r.stage_job()?;
                    Some(JobRef { stage, job })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn vars_and_unsets_from_attr_value(val: &ast::AttrValue) -> (Vec<(String, String)>, Vec<String>) {
    let Some(vl) = val.var_list() else {
        return (Vec::new(), Vec::new());
    };
    let vars = vl
        .entries()
        .filter_map(|e| Some((e.key_text()?.to_string(), e.value_text()?.to_string())))
        .collect();
    let unsets = vl
        .unsets()
        .filter_map(|u| Some(u.key_text()?.to_string()))
        .collect();
    (vars, unsets)
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
