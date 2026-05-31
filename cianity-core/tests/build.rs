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
//! | `job_dependencies` | `needs:` section; image; unquoted `--`/`-D` dashes in script |
//! | `name_conflict` | `stage.job` qualified names when the same job name appears in multiple stages; qualified names in `needs:` |
//! | `cross_file_inherit` | `steps` expansion and step override from a template defined in another file |

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
