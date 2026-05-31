//! Integration tests for `ciane::parse`.
//!
//! The valid-keyword tests each exercise one grammar keyword and assert that
//! `ciane::parse` produces no errors.  The invalid tests feed malformed input
//! and assert that the expected parse-error message is present.

use ciane::parse;

// ── helpers ───────────────────────────────────────────────────────────────────

fn assert_no_errors(src: &str) {
    let result = parse(src);
    assert!(
        result.is_ok(),
        "expected no parse errors, got: {:?}",
        result.errors()
    );
}

fn assert_has_error(src: &str, needle: &str) {
    let result = parse(src);
    assert!(
        !result.is_ok(),
        "expected a parse error but parsing succeeded"
    );
    let found = result.errors().iter().any(|e| e.message.contains(needle));
    assert!(
        found,
        "expected an error containing {needle:?}, got: {:?}",
        result.errors()
    );
}

// ── valid: one test per keyword ───────────────────────────────────────────────

#[test]
fn kw_use() {
    assert_no_errors(
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
fn kw_workflow() {
    // `workflow` is only valid inside a `use` block.
    assert_no_errors(
        r"
use {
    workflow(
        location = ./templates/base.ci,
        name = base,
    )
}

stage check {
    job lint { cargo clippy }
}
",
    );
}

#[test]
fn kw_stage() {
    assert_no_errors(
        r"
stage build {
    job compile { cargo build }
}
",
    );
}

#[test]
fn kw_job() {
    // Inline job body: `job name { shell }`.
    assert_no_errors(
        r"
stage build {
    job compile { cargo build --release }
}
",
    );
}

#[test]
fn kw_step() {
    // Step with a shell body inside a step-list job.
    assert_no_errors(
        r"
stage build {
    job compile [
        step setup { rustup update }
        step build { cargo build }
    ]
}
",
    );
}

#[test]
fn kw_template() {
    assert_no_errors(
        r"
stage test {
    template base_steps [
        step setup { echo setup }
        step run   { cargo test }
    ]

    job unit_tests(inherit = base_steps) [
        steps
    ]
}
",
    );
}

#[test]
fn template_attrs_and_body() {
    // A template may have both attributes and a body.
    assert_no_errors(
        r"
stage test {
    template base ( image = rust:latest ) [
        step run { cargo test }
    ]

    job unit ( inherit = base ) [
        steps
    ]
}
",
    );
}

#[test]
fn kw_steps() {
    // `steps` inherits all template steps into a job.
    assert_no_errors(
        r"
stage test {
    template common [
        step setup  { echo setup }
        step verify { echo verify }
    ]

    job full(inherit = common) [
        steps,
        step extra { echo extra }
    ]
}
",
    );
}

#[test]
fn kw_defaults() {
    assert_no_errors(
        r"
defaults(image = rust:1.82.0)

stage build {
    job compile { cargo build }
}
",
    );
}

// ── valid: additional scenarios ───────────────────────────────────────────────

#[test]
fn step_reference_bare() {
    // `step name,` refers to a template step without overriding its body.
    assert_no_errors(
        r"
stage test {
    template shared [
        step prepare { echo prepare }
        step run     { cargo test }
    ]

    job smoke(inherit = shared) [
        step prepare,
        step run { cargo test -- smoke }
    ]
}
",
    );
}

#[test]
fn stage_with_dependency_ref_list() {
    // Exercises `BareValue`, `RefList`, and dotted `Ref`.
    assert_no_errors(
        r"
stage build {
    job compile { cargo build }
}

stage test (
    image = rust:1.82.0,
    dependencies = [build.compile],
) {
    job run { cargo test }
}
",
    );
}

#[test]
fn job_with_attrs() {
    assert_no_errors(
        r"
stage build {
    job release (
        image = rust:1.82.0,
        timeout = 30m,
    ) {
        cargo build --release
    }
}
",
    );
}

#[test]
fn shell_body_nested_braces() {
    // `${VAR}` inside a shell body must not confuse the lexer's brace tracking.
    assert_no_errors(
        r#"
stage deploy {
    job run {
        echo "user=${USER}"
        export TAG=${GIT_SHA}
    }
}
"#,
    );
}

#[test]
fn multiple_stages() {
    assert_no_errors(
        r"
stage setup {
    job prepare { echo setup }
}

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
fn comprehensive_example() {
    // Covers: use/workflow, stage attrs, inline job, step-list job,
    // template, step references, `steps` keyword, and cross-workflow refs.
    assert_no_errors(
        r#"
use {
    workflow(
        location = other/dir/templates.ci
        name = good_defaults
    )
}

stage setup {
    job prepare_credentials {
        echo "USER=user" > credentials.txt
        echo "PASS=$(echo $SECRET | base64)" >> credentials.txt
    }
}

stage build (
    image = rust:1.94.0,
) {
    job build_debug (
        artifacts = ./target/debug,
    ) {
        cargo build
    }

    job build_release (
        artifacts = ./target/release,
    ) {
        cargo build --release
    }
}

stage test (
    image = rust:1.94.0,
    dependencies = [build.build_debug],
) {
    template extra_tests [
        step download_artifacts {
            curl example.com/my/artifacts.gzip
        },
        step unpack {
            gunzip artifacts.gzip
        },
        step run_test {
            cd artifacts
            ./run_test.sh
        }
    ]

    job smoke(inherit = extra_tests) [
        step download_artifacts,
        step unpack,
        step run_test {
            cd artifacts
            ./run_smoke.sh
        }
    ]

    job full(inherit = good_defaults/extra_tests) [
        steps
    ]
}
"#,
    );
}

// ── invalid: parse errors ─────────────────────────────────────────────────────

#[test]
fn error_unexpected_token_at_root() {
    // A bare identifier at the top level is not a valid item.
    assert_has_error("foo", "expected `stage` or end of file");
}

#[test]
fn error_use_missing_brace() {
    // `use` must be followed by `{`.
    assert_has_error(
        r"
use stage build {
    job compile { cargo build }
}
",
        "expected LBrace",
    );
}

#[test]
fn error_use_block_non_workflow_item() {
    // Only `workflow` is valid inside a `use` block.
    assert_has_error(
        r"
use {
    stage
}
",
        "expected `workflow`",
    );
}

#[test]
fn error_workflow_import_missing_paren() {
    // `workflow` must be followed by `(`.
    assert_has_error(
        r"
use {
    workflow location = ./foo.ci, name = foo
}
",
        "expected `(` after `workflow`",
    );
}

#[test]
fn error_stage_missing_name() {
    // `stage` must be followed by an identifier.
    assert_has_error(
        r"
stage {
    job compile { cargo build }
}
",
        "expected identifier for name",
    );
}

#[test]
fn error_stage_missing_body() {
    // A stage with a name but no `{` body.
    assert_has_error("stage build", "expected LBrace");
}

#[test]
fn error_job_missing_name() {
    assert_has_error(
        r"
stage build {
    job { cargo build }
}
",
        "expected identifier for name",
    );
}

#[test]
fn error_job_missing_body() {
    // `job name` without `{` or `[` following.
    assert_has_error(
        r"
stage build {
    job compile
}
",
        "expected `{` or `[` for job body",
    );
}

#[test]
fn error_step_missing_name() {
    assert_has_error(
        r"
stage build {
    job compile [
        step { cargo build }
    ]
}
",
        "expected identifier for name",
    );
}

#[test]
fn template_attrs_only() {
    // A template may have attributes but no body.
    assert_no_errors(
        r"
stage build {
    template base ( image = rust:latest )

    job compile ( inherit = base ) { cargo build }
}
",
    );
}

#[test]
fn error_template_brace_body_not_valid() {
    // `template` bodies use `[…]`, not `{…}`.  A `{` is not consumed by
    // template_def (which makes body optional) and surfaces as an unexpected
    // token in the enclosing stage_body.
    assert_has_error(
        r"
stage build {
    template base {
        step compile { cargo build }
    }
}
",
        "expected `job` or `template`",
    );
}

#[test]
fn error_ref_list_unexpected_token() {
    // A reference list must contain identifiers, not bare punctuation.
    assert_has_error(
        r"
stage test (
    dependencies = [.invalid],
) {
    job run { cargo test }
}
",
        "expected identifier in reference list",
    );
}

#[test]
fn error_stage_body_unexpected_keyword() {
    // Inside a stage body only `job` and `template` are valid.
    assert_has_error(
        r"
stage build {
    step compile { cargo build }
}
",
        "expected `job` or `template`",
    );
}
