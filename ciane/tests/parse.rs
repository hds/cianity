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

// ── valid: workflow wrapper ───────────────────────────────────────────────────

#[test]
fn kw_workflow() {
    assert_no_errors(
        r"
workflow ci {
    stage build {
        job compile { cargo build }
    }
}
",
    );
}

#[test]
fn workflow_implicit_body() {
    assert_no_errors(
        r"
workflow ci

stage build {
    job compile { cargo build }
}
",
    );
}

#[test]
fn workflow_with_strategy_attr() {
    assert_no_errors(
        r"
workflow ci (strategy = default_branch) {
    stage build {
        job compile { cargo build }
    }
}
",
    );
}

#[test]
fn workflow_all_strategy_values() {
    for s in &[
        "default_branch_and_reviews",
        "default_branch",
        "reviews",
        "none",
    ] {
        assert_no_errors(&format!(
            "workflow ci (strategy = {s}) {{ stage build {{ job j {{ x }} }} }}"
        ));
    }
}

// ── valid: one test per keyword ───────────────────────────────────────────────

#[test]
fn kw_use() {
    assert_no_errors(
        r"
workflow ci {
    use shared ( path = ./shared.ci )

    stage build {
        job compile { cargo build }
    }
}
",
    );
}

#[test]
fn kw_use_inline() {
    assert_no_errors(
        r"
workflow ci {
    use base ( path = ./templates/base.ci )

    stage check {
        job lint { cargo clippy }
    }
}
",
    );
}

#[test]
fn kw_stage() {
    assert_no_errors(
        r"
workflow ci {
    stage build {
        job compile { cargo build }
    }
}
",
    );
}

#[test]
fn kw_job() {
    assert_no_errors(
        r"
workflow ci {
    stage build {
        job compile { cargo build --release }
    }
}
",
    );
}

#[test]
fn kw_step() {
    assert_no_errors(
        r"
workflow ci {
    stage build {
        job compile [
            step setup { rustup update }
            step build { cargo build }
        ]
    }
}
",
    );
}

#[test]
fn kw_template() {
    assert_no_errors(
        r"
workflow ci {
    stage test {
        template base_steps [
            step setup { echo setup }
            step run   { cargo test }
        ]

        job unit_tests(inherit = base_steps) [
            steps
        ]
    }
}
",
    );
}

#[test]
fn template_attrs_and_body() {
    assert_no_errors(
        r"
workflow ci {
    stage test {
        template base ( image = rust:latest ) [
            step run { cargo test }
        ]

        job unit ( inherit = base ) [
            steps
        ]
    }
}
",
    );
}

#[test]
fn kw_steps() {
    assert_no_errors(
        r"
workflow ci {
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
}
",
    );
}

#[test]
fn kw_defaults() {
    assert_no_errors(
        r"
workflow ci {
    defaults(image = rust:1.82.0)

    stage build {
        job compile { cargo build }
    }
}
",
    );
}

// ── valid: additional scenarios ───────────────────────────────────────────────

#[test]
fn step_reference_bare() {
    assert_no_errors(
        r"
workflow ci {
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
}
",
    );
}

#[test]
fn stage_with_dependency_ref_list() {
    assert_no_errors(
        r"
workflow ci {
    stage build {
        job compile { cargo build }
    }

    stage test (
        image = rust:1.82.0,
        dependencies = [build.compile],
    ) {
        job run { cargo test }
    }
}
",
    );
}

#[test]
fn job_with_attrs() {
    assert_no_errors(
        r"
workflow ci {
    stage build {
        job release (
            image = rust:1.82.0,
            timeout = 30m,
        ) {
            cargo build --release
        }
    }
}
",
    );
}

#[test]
fn shell_body_nested_braces() {
    assert_no_errors(
        r#"
workflow ci {
    stage deploy {
        job run {
            echo "user=${USER}"
            export TAG=${GIT_SHA}
        }
    }
}
"#,
    );
}

#[test]
fn multiple_stages() {
    assert_no_errors(
        r"
workflow ci {
    stage setup {
        job prepare { echo setup }
    }

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
fn comprehensive_example() {
    assert_no_errors(
        r#"
workflow ci {
    use good_defaults ( path = other/dir/templates.ci )

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
}
"#,
    );
}

// ── valid: top-level templates ────────────────────────────────────────────────

#[test]
fn top_level_template_no_body() {
    assert_no_errors(
        r"
workflow ci {
    template base ( image = rust:latest )

    stage build {
        job compile ( inherit = base ) { cargo build }
    }
}
",
    );
}

#[test]
fn top_level_template_with_body() {
    assert_no_errors(
        r"
workflow ci {
    template common [
        step setup { echo setup }
        step run { cargo test }
    ]

    stage test {
        job unit ( inherit = common ) [
            steps,
        ]
    }
}
",
    );
}

#[test]
fn top_level_template_before_and_after_stage() {
    assert_no_errors(
        r"
workflow ci {
    template pre [
        step init { echo init }
    ]

    stage build {
        job compile { cargo build }
    }

    template post [
        step cleanup { echo done }
    ]

    stage test {
        job run ( inherit = pre ) [ steps, ]
    }
}
",
    );
}

#[test]
fn top_level_template_standalone() {
    assert_no_errors(
        r"
workflow ci {
    template base [
        step run { cargo test }
    ]
}
",
    );
}

// ── valid: multiple braced workflows ─────────────────────────────────────────

#[test]
fn multiple_braced_workflows() {
    assert_no_errors(
        r"
workflow ci {
    stage build {
        job compile { cargo build }
    }
}

workflow nightly {
    stage test {
        job run { cargo test }
    }
}
",
    );
}

// ── invalid: parse errors ─────────────────────────────────────────────────────

#[test]
fn error_unexpected_token_at_root() {
    assert_has_error("foo", "expected `workflow` or end of file");
}

#[test]
fn error_stage_at_root_without_workflow() {
    assert_has_error(
        "stage build { job compile { cargo build } }",
        "expected `workflow` or end of file",
    );
}

#[test]
fn error_workflow_missing_name() {
    assert_has_error(
        r"workflow { stage build { job compile { cargo build } } }",
        "expected identifier for name",
    );
}

#[test]
fn error_use_missing_name() {
    assert_has_error(
        r"
workflow ci {
    use ( path = ./foo.ci )
    stage build {
        job compile { cargo build }
    }
}
",
        "expected identifier for name",
    );
}

#[test]
fn error_stage_missing_name() {
    assert_has_error(
        r"
workflow ci {
    stage {
        job compile { cargo build }
    }
}
",
        "expected identifier for name",
    );
}

#[test]
fn error_stage_missing_body() {
    assert_has_error("workflow ci { stage build }", "expected LBrace");
}

#[test]
fn error_job_missing_name() {
    assert_has_error(
        r"
workflow ci {
    stage build {
        job { cargo build }
    }
}
",
        "expected identifier for name",
    );
}

#[test]
fn error_job_missing_body() {
    assert_has_error(
        r"
workflow ci {
    stage build {
        job compile
    }
}
",
        "expected `{` or `[` for job body",
    );
}

#[test]
fn error_step_missing_name() {
    assert_has_error(
        r"
workflow ci {
    stage build {
        job compile [
            step { cargo build }
        ]
    }
}
",
        "expected identifier for name",
    );
}

#[test]
fn template_attrs_only() {
    assert_no_errors(
        r"
workflow ci {
    stage build {
        template base ( image = rust:latest )

        job compile ( inherit = base ) { cargo build }
    }
}
",
    );
}

#[test]
fn error_template_brace_body_not_valid() {
    assert_has_error(
        r"
workflow ci {
    stage build {
        template base {
            step compile { cargo build }
        }
    }
}
",
        "expected `job` or `template`",
    );
}

#[test]
fn error_ref_list_unexpected_token() {
    assert_has_error(
        r"
workflow ci {
    stage test (
        dependencies = [.invalid],
    ) {
        job run { cargo test }
    }
}
",
        "expected identifier in reference list",
    );
}

#[test]
fn error_stage_body_unexpected_keyword() {
    assert_has_error(
        r"
workflow ci {
    stage build {
        step compile { cargo build }
    }
}
",
        "expected `job` or `template`",
    );
}
