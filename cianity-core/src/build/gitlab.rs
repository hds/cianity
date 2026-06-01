use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use ciane::{
    ast::{AstNode, Root},
    parse,
};

use super::ir::{self, Job, JobRef, Workflow, WorkflowStrategy};

/// Parse `source` and render it as a GitLab CI YAML string.
///
/// # Errors
///
/// Returns `Err` if the source has parse errors.
pub(super) fn render_source(source: &str) -> anyhow::Result<String> {
    let result = parse(source);
    if !result.errors().is_empty() {
        anyhow::bail!("file has parse errors");
    }

    let root = Root::cast(result.syntax())
        .ok_or_else(|| anyhow::anyhow!("internal error: parse produced no Root node"))?;

    Ok(render(&ir::lower(&root)))
}

/// Read a `ciane` source file and render it as a GitLab CI YAML string,
/// resolving any cross-file template references.
///
/// # Errors
///
/// Returns `Err` if the file cannot be read, has parse errors, or a
/// cross-file template reference cannot be resolved.
pub(super) fn render_path(path: &Path) -> anyhow::Result<String> {
    let source =
        std::fs::read_to_string(path).map_err(|e| anyhow::anyhow!("cannot read file: {e}"))?;
    let result = parse(&source);
    if !result.errors().is_empty() {
        anyhow::bail!("file has parse errors");
    }
    let root = Root::cast(result.syntax())
        .ok_or_else(|| anyhow::anyhow!("internal error: parse produced no Root node"))?;
    Ok(render(&ir::lower_with_path(&root, path)?))
}

/// Read a `ciane` source file and write an equivalent `.gitlab-ci.yml` to the
/// same directory.
///
/// # Errors
///
/// Returns `Err` if the file cannot be read, has parse errors, a cross-file
/// template reference cannot be resolved, or the output cannot be written.
pub(super) fn run(path: &Path, output: Option<&Path>) -> anyhow::Result<PathBuf> {
    let yaml =
        render_path(path).map_err(|e| anyhow::anyhow!("cannot build {}: {e}", path.display()))?;

    let out_path = output.map_or_else(|| path.with_file_name(".gitlab-ci.yml"), Path::to_path_buf);
    std::fs::write(&out_path, yaml)
        .map_err(|e| anyhow::anyhow!("cannot write {}: {e}", out_path.display()))?;

    Ok(out_path)
}

fn render(workflow: &Workflow) -> String {
    let mut out = String::new();
    let conflicts = conflicting_job_names(workflow);

    out.push_str("stages:\n");
    for stage in &workflow.stages {
        let _ = writeln!(out, "  - {}", stage.name);
    }

    for stage in &workflow.stages {
        for job in &stage.jobs {
            out.push('\n');
            render_job(&mut out, job, &conflicts, workflow.strategy);
        }
    }

    out
}

/// Returns the set of job names that appear in more than one stage and
/// therefore need the `stage.job` prefix to be unambiguous in GitLab CI.
fn conflicting_job_names(workflow: &Workflow) -> HashSet<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut conflicts: HashSet<String> = HashSet::new();
    for stage in &workflow.stages {
        for job in &stage.jobs {
            if !seen.insert(job.name.clone()) {
                conflicts.insert(job.name.clone());
            }
        }
    }
    conflicts
}

fn job_name(stage: &str, name: &str, conflicts: &HashSet<String>) -> String {
    if conflicts.contains(name) {
        format!("{stage}.{name}")
    } else {
        name.to_owned()
    }
}

fn dep_name(dep: &JobRef, conflicts: &HashSet<String>) -> String {
    job_name(&dep.stage, &dep.job, conflicts)
}

fn render_job(
    out: &mut String,
    job: &Job,
    conflicts: &HashSet<String>,
    strategy: WorkflowStrategy,
) {
    let _ = writeln!(out, "{}:", job_name(&job.stage, &job.name, conflicts));
    let _ = writeln!(out, "  stage: {}", job.stage);

    if let Some(image) = &job.image {
        let _ = writeln!(out, "  image: {}", yaml_scalar(image));
    }

    if !job.needs.is_empty() {
        out.push_str("  needs:\n");
        for need in &job.needs {
            let _ = writeln!(
                out,
                "    - job: {}",
                yaml_scalar(&dep_name(need, conflicts))
            );
        }
    }

    let rules = strategy_rules(strategy);
    if !rules.is_empty() {
        out.push_str("  rules:\n");
        for rule in rules {
            let _ = writeln!(out, "    - if: {rule}");
        }
    }

    out.push_str("  script:\n");
    if job.script.is_empty() {
        out.push_str("    - \"\"\n");
    } else {
        for cmd in &job.script {
            write_script_item(out, cmd);
        }
    }
}

fn strategy_rules(strategy: WorkflowStrategy) -> &'static [&'static str] {
    match strategy {
        WorkflowStrategy::None => &[],
        WorkflowStrategy::DefaultBranch => &["$CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH"],
        WorkflowStrategy::Reviews => &["$CI_MERGE_REQUEST_IID"],
        WorkflowStrategy::DefaultBranchAndReviews => &[
            "$CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH",
            "$CI_MERGE_REQUEST_IID",
        ],
    }
}

fn write_script_item(out: &mut String, cmd: &str) {
    if cmd.contains('\n') {
        out.push_str("    - |\n");
        for line in cmd.lines() {
            let _ = writeln!(out, "      {line}");
        }
    } else {
        let _ = writeln!(out, "    - {cmd}");
    }
}

fn yaml_scalar(s: &str) -> String {
    if needs_quoting(s) {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_owned()
    }
}

fn needs_quoting(s: &str) -> bool {
    s.is_empty()
        || s.contains(|c: char| {
            matches!(
                c,
                ':' | '#' | '{' | '}' | '[' | ']' | '*' | '&' | '!' | '|' | '>' | '\'' | '"' | '`'
            )
        })
        || matches!(s.chars().next(), Some('%' | '@'))
        || matches!(s, "true" | "false" | "null" | "yes" | "no" | "on" | "off")
}
