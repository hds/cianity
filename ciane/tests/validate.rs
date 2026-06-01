//! Integration tests for `ciane::validation::validate`.

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

// ── valid: workflow strategy ──────────────────────────────────────────────────

#[test]
fn valid_strategy_default_branch_and_reviews() {
    assert_no_diagnostics(
        r"
workflow ci (strategy = default_branch_and_reviews) {
    stage build { job compile { cargo build } }
}
",
    );
}

#[test]
fn valid_strategy_default_branch() {
    assert_no_diagnostics(
        r"
workflow ci (strategy = default_branch) {
    stage build { job compile { cargo build } }
}
",
    );
}

#[test]
fn valid_strategy_reviews() {
    assert_no_diagnostics(
        r"
workflow ci (strategy = reviews) {
    stage build { job compile { cargo build } }
}
",
    );
}

#[test]
fn valid_strategy_none() {
    assert_no_diagnostics(
        r"
workflow ci (strategy = none) {
    stage build { job compile { cargo build } }
}
",
    );
}

#[test]
fn valid_no_strategy_attr() {
    assert_no_diagnostics(
        r"
workflow ci {
    stage build { job compile { cargo build } }
}
",
    );
}

// ── valid: no diagnostics expected ───────────────────────────────────────────

#[test]
fn valid_unique_stage_names() {
    assert_no_diagnostics(
        r"
workflow ci {
    stage build {
        job compile { cargo build }
    }

    stage test {
        job run { cargo test }
    }
}
",
    );
}

#[test]
fn valid_unique_job_names_in_stage() {
    assert_no_diagnostics(
        r"
workflow ci {
    stage build {
        job compile { cargo build }
        job lint    { cargo clippy }
    }
}
",
    );
}

#[test]
fn valid_steps_with_inherit() {
    assert_no_diagnostics(
        r"
workflow ci {
    stage test {
        template base [
            step setup { echo setup }
            step run   { cargo test }
        ]

        job full(inherit = base) [
            steps
        ]
    }
}
",
    );
}

#[test]
fn valid_inherit_existing_template() {
    assert_no_diagnostics(
        r"
workflow ci {
    stage test {
        template base [
            step run { cargo test }
        ]

        job smoke(inherit = base) [
            step run { cargo test -- smoke }
        ]
    }
}
",
    );
}

#[test]
fn valid_inherit_cross_workflow_ref() {
    assert_no_diagnostics(
        r"
workflow ci {
    stage test {
        job full(inherit = upstream/base) [
            steps
        ]
    }
}
",
    );
}

#[test]
fn valid_workflow_import_complete() {
    assert_no_diagnostics(
        r"
workflow ci {
    use {
        workflow(location = ./shared.ci, name = shared)
    }

    stage build {
        job compile { cargo build }
    }
}
",
    );
}

#[test]
fn valid_same_job_names_across_stages() {
    assert_no_diagnostics(
        r"
workflow ci {
    stage build {
        job compile { cargo build }
    }

    stage test {
        job compile { cargo test }
    }
}
",
    );
}

#[test]
fn valid_inherit_top_level_template() {
    assert_no_diagnostics(
        r"
workflow ci {
    template base [
        step run { cargo test }
    ]

    stage test {
        job smoke ( inherit = base ) [
            step run { cargo test -- smoke }
        ]
    }
}
",
    );
}

#[test]
fn valid_inherit_top_level_template_across_stages() {
    assert_no_diagnostics(
        r"
workflow ci {
    template shared [
        step build { cargo build }
        step test { cargo test }
    ]

    stage a {
        job job_a ( inherit = shared ) [ steps, ]
    }

    stage b {
        job job_b ( inherit = shared ) [ steps, ]
    }
}
",
    );
}

#[test]
fn valid_stage_template_shadows_root_template() {
    assert_no_diagnostics(
        r"
workflow ci {
    template base [
        step run { echo root }
    ]

    stage test {
        template base [
            step run { cargo test }
        ]

        job unit ( inherit = base ) [ steps, ]
    }
}
",
    );
}

#[test]
fn valid_stage_template_shadows_root_no_warning_for_root_only() {
    assert_no_diagnostics(
        r"
workflow ci {
    template shared [
        step run { cargo test }
    ]

    stage test {
        job unit ( inherit = shared ) [
            step run { cargo test -- unit }
        ]
    }
}
",
    );
}

#[test]
fn valid_shadowing_does_not_warn_when_both_exist() {
    assert_no_diagnostics(
        r"
workflow ci {
    template common [
        step run { echo root }
    ]

    stage a {
        template common [
            step run { echo stage_a }
        ]
        job job_a ( inherit = common ) [ steps, ]
    }

    stage b {
        job job_b ( inherit = common ) [ steps, ]
    }
}
",
    );
}

// ── invalid: workflow strategy ────────────────────────────────────────────────

#[test]
fn error_invalid_strategy() {
    assert_has_diagnostic(
        r"
workflow ci (strategy = weekly) {
    stage build { job compile { cargo build } }
}
",
        Severity::Error,
        "invalid strategy `weekly`",
    );
}

// ── invalid: expected diagnostics ────────────────────────────────────────────

#[test]
fn error_duplicate_top_level_template_names() {
    assert_has_diagnostic(
        r"
workflow ci {
    template base [
        step run { cargo test }
    ]

    template base [
        step run { cargo build }
    ]

    stage build {
        job compile { cargo build }
    }
}
",
        Severity::Error,
        "duplicate top-level template name",
    );
}

#[test]
fn error_duplicate_stage_names() {
    assert_has_diagnostic(
        r"
workflow ci {
    stage build {
        job compile { cargo build }
    }

    stage build {
        job run { cargo test }
    }
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
workflow ci {
    stage build {
        job compile { cargo build }
        job compile { cargo build --release }
    }
}
",
        Severity::Error,
        "duplicate job name",
    );
}

#[test]
fn error_job_and_template_share_name() {
    assert_has_diagnostic(
        r"
workflow ci {
    stage build {
        job common { cargo build }

        template common [
            step run { cargo build }
        ]
    }
}
",
        Severity::Error,
        "duplicate name",
    );
}

#[test]
fn error_steps_without_inherit() {
    assert_has_diagnostic(
        r"
workflow ci {
    stage build {
        job compile [
            steps,
            step extra { echo extra }
        ]
    }
}
",
        Severity::Error,
        "steps` can only be used in a job that has an `inherit` attribute",
    );
}

#[test]
fn warning_inherit_references_unknown_template() {
    assert_has_diagnostic(
        r"
workflow ci {
    stage build {
        job compile(inherit = nonexistent) [
            step run { cargo build }
        ]
    }
}
",
        Severity::Warning,
        "no template with that name is defined in this stage or at the top level",
    );
}

#[test]
fn warning_inherit_unknown_template_no_false_positive_for_cross_workflow() {
    let diags = run(r"
workflow ci {
    stage build {
        job compile(inherit = upstream/base) [
            step run { cargo build }
        ]
    }
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
workflow ci {
    use {
        workflow(name = shared)
    }

    stage build {
        job compile { cargo build }
    }
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
workflow ci {
    use {
        workflow(location = ./shared.ci)
    }

    stage build {
        job compile { cargo build }
    }
}
",
        Severity::Error,
        "missing the `name` attribute",
    );
}

#[test]
fn error_workflow_import_missing_both_attrs() {
    assert_diagnostic_count(
        r"
workflow ci {
    use {
        workflow()
    }

    stage build {
        job compile { cargo build }
    }
}
",
        Severity::Error,
        2,
    );
}
