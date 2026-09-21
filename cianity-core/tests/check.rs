use std::path::{Path, PathBuf};

use ciane::error::Severity;
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
fn valid_template_inline_body() {
    assert_check_passes("template_inline_body");
}

#[test]
fn valid_cross_file_stage_template() {
    assert_check_passes("cross_file_stage_template");
}

#[test]
fn valid_variables_basic() {
    assert_check_passes("variables_basic");
}

#[test]
fn valid_variables_from_template() {
    assert_check_passes("variables_from_template");
}

#[test]
fn valid_variables_unset() {
    assert_check_passes("variables_unset");
}

#[test]
fn valid_cross_file_template_on_template() {
    assert_check_passes("cross_file_template_on_template");
}

#[test]
fn valid_dependency_same_stage() {
    assert_check_passes("dependency_same_stage");
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

#[test]
fn invalid_inherit_cross_file_missing_in_list() {
    assert_check_fails("inherit_cross_file_missing_in_list");
}

#[test]
fn invalid_inherit_cross_file_missing_on_template() {
    assert_check_fails("inherit_cross_file_missing_on_template");
}

#[test]
fn invalid_template_step_reference_unknown() {
    assert_check_fails("template_step_reference_unknown");
}

#[test]
fn invalid_template_steps_without_inherit() {
    assert_check_fails("template_steps_without_inherit");
}

#[test]
fn invalid_step_reference_unknown() {
    assert_check_fails("step_reference_unknown");
}

#[test]
fn invalid_step_reference_without_inherit() {
    assert_check_fails("step_reference_without_inherit");
}

#[test]
fn invalid_inherit_unknown_template_inline_body() {
    assert_check_fails("inherit_unknown_template_inline_body");
}

#[test]
fn invalid_inherit_unknown_template() {
    assert_check_fails("inherit_unknown_template");
}

#[test]
fn invalid_inherit_unknown_stage() {
    assert_check_fails("inherit_unknown_stage");
}

#[test]
fn invalid_dependency_later_stage() {
    assert_check_fails("dependency_later_stage");
}

#[test]
fn invalid_dependency_later_stage_from_template() {
    assert_check_fails("dependency_later_stage_from_template");
}

#[test]
fn invalid_dependency_unknown_stage() {
    assert_check_fails("dependency_unknown_stage");
}

#[test]
fn invalid_dependency_unknown_job() {
    assert_check_fails("dependency_unknown_job");
}

#[test]
fn invalid_dependency_missing_stage_prefix() {
    assert_check_fails("dependency_missing_stage_prefix");
}

#[test]
fn invalid_dependencies_not_a_list() {
    assert_check_fails("dependencies_not_a_list");
}

#[test]
fn invalid_dependency_on_self() {
    assert_check_fails("dependency_on_self");
}

#[test]
fn invalid_dependency_cycle() {
    assert_check_fails("dependency_cycle");
}

// ── reporting all errors at once ──────────────────────────────────────────────

/// Messages of all error-level diagnostics for an invalid fixture.
fn error_messages(name: &str) -> Vec<String> {
    let path = fixture_path("invalid", name);
    check::diagnostics(&path)
        .unwrap_or_else(|e| panic!("cannot check {name}.ci: {e}"))
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.message)
        .collect()
}

#[test]
fn reports_all_errors_in_one_run() {
    let messages = error_messages("multiple_errors");
    let expected = [
        "unknown attribute `foo` on job",
        "dependency `compile` must be written as `stage.job`",
        "dependency `build.compile.extra` must be written as `stage.job`",
        "inherit references import `missing`, but no such import exists",
        "job `build.lint` depends on `build.compiel`, but stage `build` has no job `compiel`",
        "job `build.lint` depends on `test.unit` in later stage `test`",
    ];
    for needle in expected {
        let count = messages.iter().filter(|m| m.contains(needle)).count();
        assert_eq!(
            count, 1,
            "expected exactly one error containing {needle:?}, got: {messages:#?}"
        );
    }
    assert_eq!(
        messages.len(),
        expected.len(),
        "unexpected extra errors: {messages:#?}"
    );
}

#[test]
fn parse_errors_suppress_dependency_errors() {
    // Error recovery can drop jobs from the tree, so dependency checks on a
    // file with parse errors could report jobs as missing when they aren't.
    let messages = error_messages("parse_error_with_dependency_error");
    assert!(!messages.is_empty(), "expected parse errors, got none");
    assert!(
        messages.iter().all(|m| !m.contains("depends on")),
        "dependency errors should not be reported alongside parse errors: {messages:#?}"
    );
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
fn workspace_invalid_deep_reference() {
    assert_workspace_fails("invalid_deep_reference");
}

#[test]
fn workspace_import_loop_is_an_error() {
    assert_workspace_fails("import_loop");
}

#[test]
fn invalid_import_loop_self() {
    assert_check_fails("import_loop_self");
}

#[test]
fn workspace_invalid_referenced_parse_error() {
    assert_workspace_fails("invalid_referenced_parse_error");
}

#[test]
fn workspace_invalid_referenced_validation() {
    assert_workspace_fails("invalid_referenced_validation");
}
