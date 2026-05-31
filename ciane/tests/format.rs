use ciane::{
    ast::{AstNode, Root},
    formatter::{self, FormatError},
    parse,
};

fn format(src: &str) -> String {
    let result = parse(src);
    assert!(
        result.errors().is_empty(),
        "unexpected parse errors: {:?}",
        result.errors()
    );
    let root = Root::cast(result.syntax()).expect("Root node");
    formatter::format(&root).expect("format succeeded")
}

fn check_idempotent(src: &str) {
    let first = format(src);
    let second = format(&first);
    assert_eq!(first, second, "formatter is not idempotent");
}

// ── basic constructs ──────────────────────────────────────────────────────────

#[test]
fn empty_stage() {
    let src = "stage build { }";
    let out = format(src);
    assert_eq!(out, "stage build {\n}\n");
    check_idempotent(src);
}

#[test]
fn stage_with_inline_job() {
    let src = "stage build { job compile { cargo build } }";
    let out = format(src);
    assert_eq!(out, "stage build {\n    job compile { cargo build }\n}\n");
    check_idempotent(src);
}

#[test]
fn stage_with_attr_list() {
    let src = "stage test(dependencies=[build.compile]){ job run { cargo test } }";
    let out = format(src);
    assert_eq!(
        out,
        "stage test ( dependencies = [build.compile] ) {\n    job run { cargo test }\n}\n"
    );
    check_idempotent(src);
}

#[test]
fn use_block() {
    let src = "use { workflow(location=./a.ci,name=a) }";
    let out = format(src);
    assert_eq!(
        out,
        "use {\n    workflow (\n        location = ./a.ci,\n        name = a,\n    )\n}\n"
    );
    check_idempotent(src);
}

#[test]
fn use_block_then_stage() {
    let src =
        "use { workflow(location=./a.ci,name=a) } stage build { job compile { cargo build } }";
    let out = format(src);
    assert!(out.starts_with("use {"), "should start with use block");
    assert!(out.contains("\n\nstage build"), "blank line before stage");
    check_idempotent(src);
}

#[test]
fn multiple_stages() {
    let src = "stage a { job j { echo } } stage b { job k { echo } }";
    let out = format(src);
    assert!(out.contains("\n\nstage b"), "blank line between stages");
    check_idempotent(src);
}

#[test]
fn template_and_steps_job() {
    let src = r#"stage s {
    template t [ step a { cmd } step b { cmd2 } ]
    job j(inherit=t) [ steps ]
}"#;
    let out = format(src);
    assert!(out.contains("template t ["));
    assert!(out.contains("steps,"));
    check_idempotent(src);
}

#[test]
fn step_reference() {
    let src = "stage s { job j(inherit=t) [ step a, step b { run } ] }";
    let out = format(src);
    assert!(out.contains("step a,"), "bare step ref has trailing comma");
    assert!(
        out.contains("step b { run }"),
        "step with body is single-line"
    );
    check_idempotent(src);
}

#[test]
fn ref_list_attr_value() {
    let src = "stage s(dependencies=[a.b, c.d]) { job j { echo } }";
    let out = format(src);
    assert!(out.contains("[a.b, c.d]"), "ref list preserved");
    check_idempotent(src);
}

// ── top-level templates ───────────────────────────────────────────────────────

#[test]
fn top_level_template_only() {
    let src = "template base [ step run { cargo test } ]";
    let out = format(src);
    assert!(
        out.starts_with("template base ["),
        "should start with template"
    );
    assert!(out.contains("step run { cargo test }"));
    check_idempotent(src);
}

#[test]
fn top_level_template_then_stage() {
    let src = "template base [ step run { cargo test } ] stage test { job unit { cargo test } }";
    let out = format(src);
    assert!(out.starts_with("template base ["));
    assert!(
        out.contains("\n\nstage test"),
        "blank line between template and stage"
    );
    check_idempotent(src);
}

#[test]
fn stage_then_top_level_template() {
    let src = "stage build { job compile { cargo build } } template post [ step done { echo ok } ]";
    let out = format(src);
    assert!(out.starts_with("stage build {"));
    assert!(
        out.contains("\n\ntemplate post ["),
        "blank line between stage and template"
    );
    check_idempotent(src);
}

#[test]
fn use_block_then_top_level_template_then_stage() {
    let src = "use { workflow(location=./a.ci,name=a) } template t [ step s { cmd } ] stage b { job j { echo } }";
    let out = format(src);
    assert!(out.starts_with("use {"));
    assert!(out.contains("\n\ntemplate t ["));
    assert!(out.contains("\n\nstage b {"));
    check_idempotent(src);
}

// ── error cases ───────────────────────────────────────────────────────────────

#[test]
fn defaults_block_returns_error() {
    let src = "defaults(runner=docker)";
    let result = parse(src);
    // Defaults parse without errors (the parser accepts the syntax)
    if result.errors().is_empty() {
        let root = Root::cast(result.syntax()).expect("Root node");
        assert_eq!(
            formatter::format(&root),
            Err(FormatError::DefaultsBlockUnsupported)
        );
    }
}

// ── fixture round-trips ───────────────────────────────────────────────────────

#[test]
fn fixture_workflow_import_idempotent() {
    let src = include_str!("../../cianity-core/tests/fixtures/valid/workflow_import.ci");
    check_idempotent(src);
}

#[test]
fn fixture_template_and_inherit_idempotent() {
    let src = include_str!("../../cianity-core/tests/fixtures/valid/template_and_inherit.ci");
    check_idempotent(src);
}

#[test]
fn fixture_simple_stage_idempotent() {
    let src = include_str!("../../cianity-core/tests/fixtures/valid/simple_stage.ci");
    check_idempotent(src);
}

#[test]
fn fixture_multiline_job_idempotent() {
    let src = include_str!("../../cianity-core/tests/fixtures/valid/multiline_job.ci");
    check_idempotent(src);
}
