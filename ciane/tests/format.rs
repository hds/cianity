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
    let src = "workflow w { stage build { } }";
    let out = format(src);
    assert_eq!(out, "workflow w {\n    stage build {\n    }\n}\n");
    check_idempotent(src);
}

#[test]
fn stage_with_inline_job() {
    let src = "workflow w { stage build { job compile { cargo build } } }";
    let out = format(src);
    assert_eq!(
        out,
        "workflow w {\n    stage build {\n        job compile { cargo build }\n    }\n}\n"
    );
    check_idempotent(src);
}

#[test]
fn stage_with_attr_list() {
    let src = "workflow w { stage test(dependencies=[build.compile]){ job run { cargo test } } }";
    let out = format(src);
    assert_eq!(
        out,
        "workflow w {\n    stage test ( dependencies = [ build.compile ] ) {\n        job run { cargo test }\n    }\n}\n"
    );
    check_idempotent(src);
}

#[test]
fn use_decl() {
    let src = "workflow w { use a ( path = ./a.ci ) }";
    let out = format(src);
    assert_eq!(out, "workflow w {\n    use a ( path = ./a.ci )\n}\n");
    check_idempotent(src);
}

#[test]
fn use_decl_then_stage() {
    let src = "workflow w { use a ( path = ./a.ci ) stage build { job compile { cargo build } } }";
    let out = format(src);
    assert!(
        out.starts_with("workflow w {\n    use a"),
        "should start with use decl"
    );
    assert!(
        out.contains("\n\n    stage build"),
        "blank line before stage"
    );
    check_idempotent(src);
}

#[test]
fn multiple_stages() {
    let src = "workflow w { stage a { job j { echo } } stage b { job k { echo } } }";
    let out = format(src);
    assert!(out.contains("\n\n    stage b"), "blank line between stages");
    check_idempotent(src);
}

#[test]
fn template_and_steps_job() {
    let src = "workflow w { stage s { template t [ step a { cmd } step b { cmd2 } ] job j(inherit=t) [ steps ] } }";
    let out = format(src);
    assert!(out.contains("template t ["));
    assert!(out.contains("steps,"));
    check_idempotent(src);
}

#[test]
fn step_reference() {
    let src = "workflow w { stage s { job j(inherit=t) [ step a, step b { run } ] } }";
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
    let src = "workflow w { stage s(dependencies=[a.b, c.d]) { job j { echo } } }";
    let out = format(src);
    assert!(out.contains("[ a.b, c.d ]"), "ref list preserved");
    check_idempotent(src);
}

// ── top-level templates ───────────────────────────────────────────────────────

#[test]
fn top_level_template_only() {
    let src = "workflow w { template base [ step run { cargo test } ] }";
    let out = format(src);
    assert!(
        out.starts_with("workflow w {\n    template base ["),
        "should start with workflow then template"
    );
    assert!(out.contains("step run { cargo test }"));
    check_idempotent(src);
}

#[test]
fn top_level_template_then_stage() {
    let src = "workflow w { template base [ step run { cargo test } ] stage test { job unit { cargo test } } }";
    let out = format(src);
    assert!(out.starts_with("workflow w {\n    template base ["));
    assert!(
        out.contains("\n\n    stage test"),
        "blank line between template and stage"
    );
    check_idempotent(src);
}

#[test]
fn stage_then_top_level_template() {
    let src = "workflow w { stage build { job compile { cargo build } } template post [ step done { echo ok } ] }";
    let out = format(src);
    assert!(out.starts_with("workflow w {\n    stage build {"));
    assert!(
        out.contains("\n\n    template post ["),
        "blank line between stage and template"
    );
    check_idempotent(src);
}

#[test]
fn use_decl_then_top_level_template_then_stage() {
    let src = "workflow w { use a ( path = ./a.ci ) template t [ step s { cmd } ] stage b { job j { echo } } }";
    let out = format(src);
    assert!(out.starts_with("workflow w {\n    use a"));
    assert!(out.contains("\n\n    template t ["));
    assert!(out.contains("\n\n    stage b {"));
    check_idempotent(src);
}

// ── return annotation ─────────────────────────────────────────────────────────

#[test]
fn job_with_return_annotation_paths() {
    let src = "workflow w { stage build { job compile { cargo build } -> [dist/, **/*.so] } }";
    let out = format(src);
    assert!(
        out.contains("{ cargo build } -> [ dist/, **/*.so ]"),
        "return annotation preserved: {out}"
    );
    check_idempotent(src);
}

#[test]
fn job_with_return_annotation_env() {
    let src = "workflow w { stage build { job compile { cargo build } -> [$RELEASE, $TARGET] } }";
    let out = format(src);
    assert!(
        out.contains("} -> [ $RELEASE, $TARGET ]"),
        "env annotation preserved: {out}"
    );
    check_idempotent(src);
}

#[test]
fn job_with_return_annotation_mixed() {
    let src = "workflow w { stage build { job compile { cargo build } -> [dist/, $RELEASE] } }";
    let out = format(src);
    assert!(
        out.contains("} -> [ dist/, $RELEASE ]"),
        "mixed annotation preserved: {out}"
    );
    check_idempotent(src);
}

#[test]
fn template_with_return_annotation() {
    let src = "workflow w { stage build { template base [ step build { cargo build } ] -> [dist/] job compile (inherit = base) [ steps, ] -> [target/release/app] } }";
    let out = format(src);
    assert!(
        out.contains("] -> [ dist/ ]"),
        "template return annotation preserved: {out}"
    );
    assert!(
        out.contains("] -> [ target/release/app ]"),
        "job return annotation preserved: {out}"
    );
    check_idempotent(src);
}

#[test]
fn fixture_artifacts_merged_idempotent() {
    let src = include_str!("../../cianity-core/tests/fixtures/build/artifacts_merged.ci");
    check_idempotent(src);
}

// ── error cases ───────────────────────────────────────────────────────────────

#[test]
fn defaults_block_returns_error() {
    let src = "workflow w { defaults(runner=docker) }";
    let result = parse(src);
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
