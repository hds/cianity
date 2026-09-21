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
    body: TemplateBody,
    inherit_names: Vec<String>,
}

/// How a template's body determines its steps.
enum TemplateBody {
    /// No body at all: the steps it inherits are kept as they are.
    Inherited,
    /// A single inline body, which replaces anything inherited.
    Inline(String),
    /// A step list, resolved against the inherited steps like a job body.
    Steps(JobBodySteps),
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

fn raw_template_data_from_ast(
    tmpl: &ast::TemplateDef,
) -> (TemplateData, TemplateBody, Vec<String>) {
    let body = if let Some(inline) = tmpl.inline_body() {
        inline.shell_text().map_or(TemplateBody::Inherited, |s| {
            TemplateBody::Inline(dedent(&s))
        })
    } else {
        tmpl.body()
            .map_or(TemplateBody::Inherited, TemplateBody::Steps)
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
            steps: Vec::new(),
            image,
            needs,
            artifacts,
            env,
            variables,
            unset_variables,
        },
        body,
        inherit_names,
    )
}

/// Identifies a template within a file: the stage it is defined in, or `None`
/// for a top-level one.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct TemplateKey {
    stage: Option<String>,
    name: String,
}

/// Something a workflow refers to that couldn't be resolved while lowering.
#[derive(Debug)]
pub struct LowerError {
    pub site: ErrorSite,
    pub message: String,
}

/// Where a [`LowerError`] was written, so that it can be pointed at in the
/// source.
#[derive(Debug, PartialEq, Eq)]
pub enum ErrorSite {
    /// An `inherit` reference in the file being lowered, as written.
    InheritRef(String),
    /// Somewhere inside a file reached through a `use` import.
    Import(PathBuf),
    /// A bare `step` reference in a job body.
    StepRef {
        stage: String,
        job: String,
        step: String,
    },
    /// A bare `step` reference in a template body.
    TemplateStepRef {
        /// The stage the template is defined in; `None` at the top level.
        stage: Option<String>,
        template: String,
        step: String,
    },
}

/// The templates and imports of one file.
#[derive(Default)]
struct FileTemplates {
    raw: HashMap<TemplateKey, RawTemplateEntry>,
    imports: HashMap<String, PathBuf>,
}

impl FileTemplates {
    fn from_root(root: &Root, path: &Path) -> Self {
        let mut raw: HashMap<TemplateKey, RawTemplateEntry> = HashMap::new();
        let mut add = |stage: Option<String>, tmpl: &ast::TemplateDef| {
            if let Some(name) = tmpl.name() {
                let (own, body, inherit_names) = raw_template_data_from_ast(tmpl);
                raw.insert(
                    TemplateKey {
                        stage,
                        name: name.to_string(),
                    },
                    RawTemplateEntry {
                        own,
                        body,
                        inherit_names,
                    },
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

        let base = path.parent().unwrap_or(Path::new("."));
        let imports = root
            .use_decls()
            .filter_map(|imp| {
                Some((
                    imp.name()?.to_string(),
                    crate::workspace::normalize(&base.join(imp.path()?.as_str())),
                ))
            })
            .collect();

        Self { raw, imports }
    }
}

/// Resolves `inherit` references, reading imported files as it goes.
///
/// Templates are resolved in the file they are written in, so an imported
/// template's own `inherit` chain — including references through that file's
/// `use` imports — is applied before it reaches the job inheriting it.
struct TemplateResolver {
    root: PathBuf,
    files: HashMap<PathBuf, FileTemplates>,
    resolved: HashMap<(PathBuf, TemplateKey), TemplateData>,
    /// The resolution path, used to break inheritance cycles.
    stack: Vec<(PathBuf, TemplateKey)>,
    /// Whether references into other files may be followed.
    imports_allowed: bool,
    errors: Vec<LowerError>,
}

impl TemplateResolver {
    fn new(root: &Root, path: &Path, imports_allowed: bool) -> Self {
        let mut files = HashMap::new();
        files.insert(path.to_path_buf(), FileTemplates::from_root(root, path));
        Self {
            root: path.to_path_buf(),
            files,
            resolved: HashMap::new(),
            stack: Vec::new(),
            imports_allowed,
            errors: Vec::new(),
        }
    }

    /// What an `inherit` name written in `file` inside `context_stage`
    /// contributes to the thing inheriting it.
    fn resolve(&mut self, file: &Path, name: &str, context_stage: Option<&str>) -> TemplateData {
        let Some((target_file, key)) = self.target_of(file, name, context_stage) else {
            return TemplateData::default();
        };
        self.resolve_key(&target_file, &key)
    }

    /// The file and template an `inherit` name refers to, reporting an error
    /// if a cross-file reference can't be followed.
    fn target_of(
        &mut self,
        file: &Path,
        name: &str,
        context_stage: Option<&str>,
    ) -> Option<(PathBuf, TemplateKey)> {
        let Some((import_name, rest)) = name.split_once('/') else {
            return Some((
                file.to_path_buf(),
                self.local_key(file, name, context_stage),
            ));
        };
        if !self.imports_allowed {
            return None;
        }
        let Some(import_path) = self
            .files
            .get(file)
            .and_then(|f| f.imports.get(import_name))
            .cloned()
        else {
            self.error(
                file,
                name,
                format!(
                    "inherit references import `{import_name}`, but no such import exists in \
                     the `use` block"
                ),
            );
            return None;
        };
        if !self.load(&import_path) {
            // A missing file is reported for the `use` import itself.
            if import_path.exists() {
                self.error(
                    file,
                    name,
                    format!(
                        "import `{import_name}` references `{}`, but that file cannot be read",
                        import_path.display()
                    ),
                );
            }
            return None;
        }
        let key = match rest.split_once('.') {
            Some((stage_name, template_name)) => TemplateKey {
                stage: Some(stage_name.to_owned()),
                name: template_name.to_owned(),
            },
            None => TemplateKey {
                stage: None,
                name: rest.to_owned(),
            },
        };
        if !self.files[&import_path].raw.contains_key(&key) {
            let message = match &key.stage {
                Some(stage_name) => format!(
                    "template `{}` not found in stage `{stage_name}` of import `{import_name}`",
                    key.name
                ),
                None => format!(
                    "top-level template `{}` not found in import `{import_name}`",
                    key.name
                ),
            };
            self.error(file, name, message);
            return None;
        }
        Some((import_path, key))
    }

    /// An unqualified name is the stage's own template when it has one, and a
    /// top-level template otherwise. `stage.name` names another stage's.
    fn local_key(&self, file: &Path, name: &str, context_stage: Option<&str>) -> TemplateKey {
        if let Some((stage_name, template_name)) = name.split_once('.') {
            return TemplateKey {
                stage: Some(stage_name.to_owned()),
                name: template_name.to_owned(),
            };
        }
        let in_stage = TemplateKey {
            stage: context_stage.map(ToOwned::to_owned),
            name: name.to_owned(),
        };
        if context_stage.is_some()
            && self
                .files
                .get(file)
                .is_some_and(|f| f.raw.contains_key(&in_stage))
        {
            return in_stage;
        }
        TemplateKey {
            stage: None,
            name: name.to_owned(),
        }
    }

    /// Resolve one template, applying its own `inherit` chain first.
    ///
    /// A template that doesn't exist contributes nothing; within a file that
    /// is reported by `ciane` validation.
    fn resolve_key(&mut self, file: &Path, key: &TemplateKey) -> TemplateData {
        let id = (file.to_path_buf(), key.clone());
        if let Some(data) = self.resolved.get(&id) {
            return data.clone();
        }
        if self.stack.contains(&id) {
            // Circular inheritance — break the cycle by contributing nothing.
            return TemplateData::default();
        }
        let Some(entry) = self.files.get(file).and_then(|f| f.raw.get(key)) else {
            return TemplateData::default();
        };
        let own = entry.own.clone();
        let inherit_names = entry.inherit_names.clone();
        let body = match &entry.body {
            TemplateBody::Inherited => TemplateBody::Inherited,
            TemplateBody::Inline(shell) => TemplateBody::Inline(shell.clone()),
            TemplateBody::Steps(steps) => TemplateBody::Steps(steps.clone()),
        };

        self.stack.push(id.clone());
        let errors_before = self.errors.len();
        let mut merged = TemplateData::default();
        for parent in &inherit_names {
            let data = self.resolve(file, parent, key.stage.as_deref());
            merged = merge_template_data(merged, data);
        }
        let inherits_resolved = self.errors.len() == errors_before;
        let inherited_steps = merged.steps.clone();
        merged = merge_template_data(merged, own);

        // A template's body picks its steps the same way a job's does.
        merged.steps = match body {
            TemplateBody::Inherited => inherited_steps,
            TemplateBody::Inline(shell) => vec![(key.name.clone(), shell)],
            TemplateBody::Steps(body) => {
                let (steps, unresolved) = resolve_steps(&body, &inherited_steps);
                // A template with no `inherit` is reported by `ciane` validation,
                // and one whose parents failed to resolve has been reported already.
                if !inherit_names.is_empty() && inherits_resolved {
                    for step in unresolved {
                        self.step_error(file, key, &step);
                    }
                }
                steps
            }
        };
        self.stack.pop();

        self.resolved.insert(id, merged.clone());
        merged
    }

    /// Report a bare `step` reference in a template that names a step none of
    /// its inherited templates define.
    fn step_error(&mut self, file: &Path, key: &TemplateKey, step: &str) {
        let name = match &key.stage {
            Some(stage) => format!("{stage}.{}", key.name),
            None => key.name.clone(),
        };
        let message =
            format!("template `{name}` uses step `{step}`, but no template it inherits defines it");
        let error = if file == self.root {
            LowerError {
                site: ErrorSite::TemplateStepRef {
                    stage: key.stage.clone(),
                    template: key.name.clone(),
                    step: step.to_owned(),
                },
                message,
            }
        } else {
            LowerError {
                site: ErrorSite::Import(file.to_path_buf()),
                message: format!("in `{}`: {message}", file.display()),
            }
        };
        if !self
            .errors
            .iter()
            .any(|e| e.site == error.site && e.message == error.message)
        {
            self.errors.push(error);
        }
    }

    /// Resolve every template in the root file, so that problems in a
    /// template no job happens to inherit are reported too.
    fn resolve_root_templates(&mut self) {
        let root = self.root.clone();
        let keys: Vec<TemplateKey> = self
            .files
            .get(&root)
            .map(|f| f.raw.keys().cloned().collect())
            .unwrap_or_default();
        for key in keys {
            self.resolve_key(&root, &key);
        }
    }

    /// Read and parse an imported file, keeping it for later references.
    fn load(&mut self, path: &Path) -> bool {
        if self.files.contains_key(path) {
            return true;
        }
        let Ok(source) = std::fs::read_to_string(path) else {
            return false;
        };
        let Some(root) = Root::cast(parse(&source).syntax()) else {
            return false;
        };
        self.files
            .insert(path.to_path_buf(), FileTemplates::from_root(&root, path));
        true
    }

    fn error(&mut self, file: &Path, reference: &str, message: String) {
        let error = if file == self.root {
            LowerError {
                site: ErrorSite::InheritRef(reference.to_owned()),
                message,
            }
        } else {
            LowerError {
                site: ErrorSite::Import(file.to_path_buf()),
                message: format!("in `{}`: {message}", file.display()),
            }
        };
        // The same reference is resolved once per job that inherits it.
        if !self
            .errors
            .iter()
            .any(|e| e.site == error.site && e.message == error.message)
        {
            self.errors.push(error);
        }
    }
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
/// contains only concrete jobs. Cross-file references are not resolved, as
/// there is no file to resolve imports against; use [`lower_with_path`] when
/// the source came from a file.
#[must_use]
pub fn lower(root: &Root) -> Workflow {
    lower_partial(root).0
}

/// Lower a parsed `Root` like [`lower`], returning what couldn't be resolved
/// alongside the workflow.
#[must_use]
pub fn lower_partial(root: &Root) -> (Workflow, Vec<LowerError>) {
    lower_inner(root, Path::new(""), false)
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
        Some(err) => Err(anyhow::anyhow!(err.message)),
        None => Ok(workflow),
    }
}

/// Lower a parsed `Root` into a `Workflow` like [`lower_with_path`], but
/// without stopping at references which can't be resolved.
///
/// Each such reference contributes nothing to the job or template inheriting
/// it, and the reason is returned alongside the workflow. This allows the rest
/// of the workflow to be checked in the same pass.
#[must_use]
pub fn lower_with_path_partial(root: &Root, path: &Path) -> (Workflow, Vec<LowerError>) {
    lower_inner(root, path, true)
}

fn lower_inner(root: &Root, path: &Path, imports_allowed: bool) -> (Workflow, Vec<LowerError>) {
    let strategy = strategy_from_root(root);
    let mut resolver = TemplateResolver::new(root, path, imports_allowed);
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
            let errors_before = resolver.errors.len();
            for name in &inherit_names {
                let data = resolver.resolve(path, name, Some(&stage_name));
                template_data = merge_template_data(template_data, data);
            }
            let inherits_resolved = resolver.errors.len() == errors_before;

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

            let (script, unresolved_steps) = job_script(&job, &template_data.steps);
            // A job with no `inherit` is reported by `ciane` validation, and a
            // job whose templates failed to resolve has been reported already.
            if !inherit_names.is_empty() && inherits_resolved {
                for step in unresolved_steps {
                    resolver.errors.push(LowerError {
                        message: format!(
                            "job `{stage_name}.{job_name}` uses step `{step}`, but no template \
                             it inherits defines it"
                        ),
                        site: ErrorSite::StepRef {
                            stage: stage_name.clone(),
                            job: job_name.clone(),
                            step,
                        },
                    });
                }
            }

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

    resolver.resolve_root_templates();

    (Workflow { stages, strategy }, resolver.errors)
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

/// The job's script, along with the names of any bare `step` references that
/// no inherited template defines.
fn job_script(job: &ast::Job, template_steps: &[(String, String)]) -> (Vec<String>, Vec<String>) {
    if let Some(inline) = job.inline_body() {
        let script = inline
            .shell_text()
            .map(|s| vec![dedent(&s)])
            .unwrap_or_default();
        (script, Vec::new())
    } else if let Some(steps_body) = job.steps_body() {
        let (steps, unresolved) = resolve_steps(&steps_body, template_steps);
        (
            steps.into_iter().map(|(_, shell)| shell).collect(),
            unresolved,
        )
    } else {
        (Vec::new(), Vec::new())
    }
}

/// Resolve a step list against the steps it inherits.
///
/// Returns the steps, named so that whatever inherits them can refer to them
/// in turn, and the names of references no inherited template defines.
fn resolve_steps(
    body: &JobBodySteps,
    template_steps: &[(String, String)],
) -> (Vec<(String, String)>, Vec<String>) {
    // Names of all steps explicitly listed in this job body (both full steps
    // and bare references).  These are skipped when `steps` is expanded so the
    // same step does not appear twice.
    let explicit_names: std::collections::HashSet<String> = body
        .steps()
        .filter_map(|s| s.name().map(|n| n.to_string()))
        .collect();

    let mut script = Vec::new();
    let mut unresolved = Vec::new();

    for child in body.syntax().children() {
        match child.kind() {
            SyntaxKind::Step => {
                if let Some(step) = ast::Step::cast(child) {
                    let name = step.name().map_or_else(String::new, |n| n.to_string());
                    if let Some(shell) = step.shell_text() {
                        // Full step with an explicit body.
                        script.push((name, dedent(&shell)));
                    } else if step.name().is_some() {
                        // Bare step reference: inline the named template step.
                        if let Some((_, shell)) =
                            template_steps.iter().find(|(n, _)| n.as_str() == name)
                        {
                            script.push((name, shell.clone()));
                        } else {
                            unresolved.push(name);
                        }
                    }
                }
            }
            SyntaxKind::StepsKeyword => {
                // Expand all template steps not already covered by explicit
                // steps in this job.
                for (name, shell) in template_steps {
                    if !explicit_names.contains(name) {
                        script.push((name.clone(), shell.clone()));
                    }
                }
            }
            _ => {}
        }
    }

    (script, unresolved)
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
