//! Integration tests for `ciane::validation::validate`.
//!
//! Each test parses valid source (no parse errors) and then runs semantic
//! validation, checking that the expected diagnostics are produced.

use ciane::{
    ast::{AstNode, Root},
    error::{Diagnostic, Severity},
    parse,
    validation::validate,
};

// ── helpers ───────────────────────────────────────────────────────────────────

fn run(src: &str) -> Vec<Diagnostic> {
    let result = parse(src);
    let root = Root::cast(result.syntax()).expect("parse always produces a Root node");
    validate(&root)
}

fn assert_no_diagnostics(src: &str) {
    let diags = run(src);
    assert!(diags.is_empty(), "expected no diagnostics, got: {diags:?}");
}

fn assert_has_diagnostic(src: &str, severity: Severity, needle: &str) {
    let diags = run(src);
    let found = diags
        .iter()
        .any(|d| d.severity == severity && d.message.contains(needle));
    assert!(
        found,
        "expected a {severity:?} containing {needle:?}, got: {diags:?}"
    );
}

fn assert_diagnostic_count(src: &str, severity: Severity, expected: usize) {
    let diags = run(src);
    let count = diags.iter().filter(|d| d.severity == severity).count();
    assert_eq!(
        count, expected,
        "expected {expected} {severity:?} diagnostic(s), got: {diags:?}"
    );
}

// ── valid: no diagnostics expected ───────────────────────────────────────────

#[test]
fn valid_unique_stage_names() {
    assert_no_diagnostics(
        r"
stage build {
    job compile { cargo build }
}

stage test {
    job run { cargo test }
}
",
    );
}

#[test]
fn valid_unique_job_names_in_stage() {
    assert_no_diagnostics(
        r"
stage build {
    job compile { cargo build }
    job lint    { cargo clippy }
}
",
    );
}

#[test]
fn valid_steps_with_inherit() {
    // `steps` is valid when the job has an `inherit` attribute.
    assert_no_diagnostics(
        r"
stage test {
    template base [
        step setup { echo setup }
        step run   { cargo test }
    ]

    job full(inherit = base) [
        steps
    ]
}
",
    );
}

#[test]
fn valid_inherit_existing_template() {
    // `inherit` naming an actual template in the same stage produces no warning.
    assert_no_diagnostics(
        r"
stage test {
    template base [
        step run { cargo test }
    ]

    job smoke(inherit = base) [
        step run { cargo test -- smoke }
    ]
}
",
    );
}

#[test]
fn valid_inherit_cross_workflow_ref() {
    // A cross-workflow reference (`ns/template`) is never warned about, even if
    // no matching local template exists.
    assert_no_diagnostics(
        r"
stage test {
    job full(inherit = upstream/base) [
        steps
    ]
}
",
    );
}

#[test]
fn valid_workflow_import_complete() {
    assert_no_diagnostics(
        r"
use {
    workflow(location = ./shared.ci, name = shared)
}

stage build {
    job compile { cargo build }
}
",
    );
}

#[test]
fn valid_same_job_names_across_stages() {
    // Duplicate names across different stages are not an error.
    assert_no_diagnostics(
        r"
stage build {
    job compile { cargo build }
}

stage test {
    job compile { cargo test }
}
",
    );
}

// ── invalid: expected diagnostics ────────────────────────────────────────────

#[test]
fn error_duplicate_stage_names() {
    assert_has_diagnostic(
        r"
stage build {
    job compile { cargo build }
}

stage build {
    job run { cargo test }
}
",
        Severity::Error,
        "duplicate stage name",
    );
}

#[test]
fn error_duplicate_job_names_in_stage() {
    assert_has_diagnostic(
        r"
stage build {
    job compile { cargo build }
    job compile { cargo build --release }
}
",
        Severity::Error,
        "duplicate job name",
    );
}

#[test]
fn error_job_and_template_share_name() {
    // A template whose name matches a job in the same stage is a duplicate.
    assert_has_diagnostic(
        r"
stage build {
    job common { cargo build }

    template common [
        step run { cargo build }
    ]
}
",
        Severity::Error,
        "duplicate name",
    );
}

#[test]
fn error_steps_without_inherit() {
    // `steps` requires the job to have an `inherit` attribute.
    assert_has_diagnostic(
        r"
stage build {
    job compile [
        steps,
        step extra { echo extra }
    ]
}
",
        Severity::Error,
        "steps` can only be used in a job that has an `inherit` attribute",
    );
}

#[test]
fn warning_inherit_references_unknown_template() {
    // Inheriting from a name that isn't a template in this stage is a warning.
    assert_has_diagnostic(
        r"
stage build {
    job compile(inherit = nonexistent) [
        step run { cargo build }
    ]
}
",
        Severity::Warning,
        "no template with that name is defined in this stage",
    );
}

#[test]
fn warning_inherit_unknown_template_no_false_positive_for_cross_workflow() {
    // A slash-qualified ref must not produce a warning even though no matching
    // local template exists.
    let diags = run(r"
stage build {
    job compile(inherit = upstream/base) [
        step run { cargo build }
    ]
}
");
    let has_unknown_warning = diags.iter().any(|d| {
        d.severity == Severity::Warning && d.message.contains("no template with that name")
    });
    assert!(
        !has_unknown_warning,
        "cross-workflow `inherit` should not produce an unknown-template warning"
    );
}

#[test]
fn error_workflow_import_missing_location() {
    assert_has_diagnostic(
        r"
use {
    workflow(name = shared)
}

stage build {
    job compile { cargo build }
}
",
        Severity::Error,
        "missing the `location` attribute",
    );
}

#[test]
fn error_workflow_import_missing_name() {
    assert_has_diagnostic(
        r"
use {
    workflow(location = ./shared.ci)
}

stage build {
    job compile { cargo build }
}
",
        Severity::Error,
        "missing the `name` attribute",
    );
}

#[test]
fn error_workflow_import_missing_both_attrs() {
    // Both `location` and `name` are required — two errors when both are absent.
    assert_diagnostic_count(
        r"
use {
    workflow()
}

stage build {
    job compile { cargo build }
}
",
        Severity::Error,
        2,
    );
}
