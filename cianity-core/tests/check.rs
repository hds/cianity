use std::path::{Path, PathBuf};

use cianity_core::{check, workspace};

// ── helpers ───────────────────────────────────────────────────────────────────

fn fixture_path(subdir: &str, name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(subdir)
        .join(format!("{name}.ci"))
}

fn assert_check_passes(name: &str) {
    let path = fixture_path("valid", name);
    if let Err(e) = check::run(&path) {
        panic!("{name}.ci should pass check: {e}");
    }
}

fn assert_check_fails(name: &str) {
    let path = fixture_path("invalid", name);
    assert!(
        check::run(&path).is_err(),
        "{name}.ci should fail check but passed"
    );
}

// ── valid fixtures ────────────────────────────────────────────────────────────

#[test]
fn valid_simple_stage() {
    assert_check_passes("simple_stage");
}

#[test]
fn valid_multiline_job() {
    assert_check_passes("multiline_job");
}

#[test]
fn valid_template_and_inherit() {
    assert_check_passes("template_and_inherit");
}

#[test]
fn valid_workflow_import() {
    assert_check_passes("workflow_import");
}

#[test]
fn valid_cross_file_inherit() {
    assert_check_passes("cross_file_inherit");
}

#[test]
fn valid_top_level_template() {
    assert_check_passes("top_level_template");
}

#[test]
fn valid_cross_file_stage_template() {
    assert_check_passes("cross_file_stage_template");
}

// ── invalid fixtures ──────────────────────────────────────────────────────────

#[test]
fn invalid_duplicate_stage_names() {
    assert_check_fails("duplicate_stage_names");
}

#[test]
fn invalid_parse_error_missing_stage_name() {
    assert_check_fails("parse_error_missing_stage_name");
}

#[test]
fn invalid_steps_without_inherit() {
    assert_check_fails("steps_without_inherit");
}

#[test]
fn invalid_workflow_missing_location() {
    assert_check_fails("workflow_missing_location");
}

#[test]
fn invalid_inherit_no_use_entry() {
    assert_check_fails("inherit_no_use_entry");
}

#[test]
fn invalid_inherit_use_file_missing() {
    assert_check_fails("inherit_use_file_missing");
}

#[test]
fn invalid_inherit_template_not_in_file() {
    assert_check_fails("inherit_template_not_in_file");
}

#[test]
fn invalid_inherit_stage_template_not_found() {
    assert_check_fails("inherit_stage_template_not_found");
}

// ── workspace check ───────────────────────────────────────────────────────────
//
// These tests replicate the `commands::check` flow — run check on the root
// workflow.ci then run check on every file listed in its `use {}` block.

fn workspace_fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/workspace")
        .join(name)
        .join("workflow.ci")
}

fn run_workspace_check(root: &Path) -> anyhow::Result<()> {
    let mut had_error = false;
    if check::run(root).is_err() {
        had_error = true;
    }
    let refs = workspace::referenced_files(root)?;
    for path in &refs {
        if check::run(path).is_err() {
            had_error = true;
        }
    }
    if had_error {
        anyhow::bail!("workspace had errors");
    }
    Ok(())
}

fn assert_workspace_passes(name: &str) {
    let root = workspace_fixture(name);
    if let Err(e) = run_workspace_check(&root) {
        panic!("{name} workspace should pass check: {e}");
    }
}

fn assert_workspace_fails(name: &str) {
    let root = workspace_fixture(name);
    assert!(
        run_workspace_check(&root).is_err(),
        "{name} workspace should fail check but passed"
    );
}

#[test]
fn workspace_valid_root_and_referenced() {
    assert_workspace_passes("valid_root_and_referenced");
}

#[test]
fn workspace_invalid_referenced_parse_error() {
    assert_workspace_fails("invalid_referenced_parse_error");
}

#[test]
fn workspace_invalid_referenced_validation() {
    assert_workspace_fails("invalid_referenced_validation");
}
