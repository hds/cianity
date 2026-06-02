//! Snapshot tests for `cianity_core::build`.
//!
//! Each fixture has a `.ci` source file and a corresponding `.gitlab-ci.yml`
//! snapshot in `tests/fixtures/build/`.  Tests call `render_path_to_string`
//! so that cross-file template references are resolved correctly.
//!
//! To regenerate a snapshot after an intentional change to the renderer, run:
//!
//! ```text
//! cianity build <fixture>.ci -t gitlab -o <fixture>.gitlab-ci.yml
//! ```
//!
//! ## Coverage map
//!
//! | Fixture | Features exercised |
//! |---|---|
//! | `simple_stage` | basic inline job; plain job name |
//! | `multiline_job` | multi-line `- \|` block scalar; bare step reference; image |
//! | `template_and_inherit` | `steps` keyword expansion; step body override; unquoted `'` in script |
//! | `workflow_import` | `use {}` block handled gracefully; stage-level attr ignored |
//! | `job_dependencies` | `dependencies:` list from job attr; `dependencies: []` for jobs without deps; image; unquoted `--`/`-D` dashes in script |
//! | `name_conflict` | `stage.job` qualified names when the same job name appears in multiple stages; qualified names in `dependencies:` |
//! | `cross_file_inherit` | `steps` expansion and step override from a template defined in another file |
//! | `template_attrs_inherit` | template `image` attr propagates to inheriting jobs; job attr overrides template |
//! | `top_level_template_inherit` | job inherits from a top-level template defined outside any stage |
//! | `cross_file_stage_template` | `ns/stage.tmpl` syntax resolves a template inside a named stage in another file |
//! | `template_deps_inherit` | template `dependencies` attr propagates to inheriting job; job `dependencies` overrides template |
//! | `artifacts_basic` | `artifacts` list with globs on a job; paths appear in `artifacts.paths:` |
//! | `artifacts_from_template` | template return annotation artifacts propagate to inheriting job |
//! | `artifacts_merged` | template and job both declare artifacts; all paths appear in the output |
//! | `return_annotation_paths` | `->` syntax for artifact paths |
//! | `return_annotation_env` | `->` syntax for env vars only; emits `variables:` |
//! | `return_annotation_both` | `->` syntax for both paths and env vars |
//! | `return_annotation_template` | `->` on template propagates paths and env to inheriting job |

use std::path::{Path, PathBuf};

use cianity_core::build;

// ── helpers ───────────────────────────────────────────────────────────────────

fn ci_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/build")
        .join(format!("{name}.ci"))
}

fn snapshot_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/build")
        .join(format!("{name}.gitlab-ci.yml"))
}

fn assert_gitlab_snapshot(name: &str) {
    let ci = ci_path(name);
    let snap = snapshot_path(name);
    let expected = std::fs::read_to_string(&snap)
        .unwrap_or_else(|e| panic!("cannot read snapshot {}: {e}", snap.display()));
    let actual = build::render_path_to_string(&ci, build::Target::Gitlab)
        .unwrap_or_else(|e| panic!("{name}.ci build failed: {e}"));
    assert_eq!(actual, expected, "snapshot mismatch for {name}.ci");
}

// ── snapshot tests ────────────────────────────────────────────────────────────

#[test]
fn build_simple_stage() {
    assert_gitlab_snapshot("simple_stage");
}

#[test]
fn build_multiline_job() {
    assert_gitlab_snapshot("multiline_job");
}

#[test]
fn build_template_and_inherit() {
    assert_gitlab_snapshot("template_and_inherit");
}

#[test]
fn build_workflow_import() {
    assert_gitlab_snapshot("workflow_import");
}

#[test]
fn build_job_dependencies() {
    assert_gitlab_snapshot("job_dependencies");
}

#[test]
fn build_name_conflict() {
    assert_gitlab_snapshot("name_conflict");
}

#[test]
fn build_cross_file_inherit() {
    assert_gitlab_snapshot("cross_file_inherit");
}

#[test]
fn build_template_attrs_inherit() {
    assert_gitlab_snapshot("template_attrs_inherit");
}

#[test]
fn build_top_level_template_inherit() {
    assert_gitlab_snapshot("top_level_template_inherit");
}

#[test]
fn build_stage_template_shadows_root() {
    assert_gitlab_snapshot("stage_template_shadows_root");
}

#[test]
fn build_cross_file_stage_template() {
    assert_gitlab_snapshot("cross_file_stage_template");
}

#[test]
fn build_template_deps_inherit() {
    assert_gitlab_snapshot("template_deps_inherit");
}

// ── workflow strategy tests ───────────────────────────────────────────────────

#[test]
fn build_strategy_default_branch() {
    assert_gitlab_snapshot("strategy_default_branch");
}

#[test]
fn build_strategy_reviews() {
    assert_gitlab_snapshot("strategy_reviews");
}

#[test]
fn build_strategy_default_branch_and_reviews() {
    assert_gitlab_snapshot("strategy_default_branch_and_reviews");
}

#[test]
fn build_strategy_none() {
    assert_gitlab_snapshot("strategy_none");
}

#[test]
fn build_template_inherit() {
    assert_gitlab_snapshot("template_inherit");
}

#[test]
fn build_multi_inherit() {
    assert_gitlab_snapshot("multi_inherit");
}

#[test]
fn build_artifacts_basic() {
    assert_gitlab_snapshot("artifacts_basic");
}

#[test]
fn build_artifacts_from_template() {
    assert_gitlab_snapshot("artifacts_from_template");
}

#[test]
fn build_artifacts_merged() {
    assert_gitlab_snapshot("artifacts_merged");
}

#[test]
fn build_return_annotation_paths() {
    assert_gitlab_snapshot("return_annotation_paths");
}

#[test]
fn build_return_annotation_env() {
    assert_gitlab_snapshot("return_annotation_env");
}

#[test]
fn build_return_annotation_both() {
    assert_gitlab_snapshot("return_annotation_both");
}

#[test]
fn build_return_annotation_template() {
    assert_gitlab_snapshot("return_annotation_template");
}

// ── error cases ───────────────────────────────────────────────────────────────

#[test]
fn build_fails_on_parse_errors() {
    let err = build::render_to_string("stage {", build::Target::Gitlab)
        .expect_err("a file with parse errors should not produce output");
    assert!(
        err.to_string().contains("parse errors"),
        "unexpected error message: {err}"
    );
}

#[test]
fn build_fails_on_unknown_job_attr() {
    let err = build::render_to_string(
        "workflow ci { stage build { job compile (foo = bar) { cargo build } } }",
        build::Target::Gitlab,
    )
    .expect_err("a file with an unknown job attr should not produce output");
    assert!(
        err.to_string().contains("validation errors"),
        "unexpected error message: {err}"
    );
}

#[test]
fn build_fails_on_unknown_workflow_attr() {
    let err = build::render_to_string(
        "workflow ci (foo = bar) { stage build { job compile { cargo build } } }",
        build::Target::Gitlab,
    )
    .expect_err("a file with an unknown workflow attr should not produce output");
    assert!(
        err.to_string().contains("validation errors"),
        "unexpected error message: {err}"
    );
}
