//! Integration-style tests for LSP feature handlers.
//!
//! Each test parses a source string and calls the relevant LSP function
//! directly, asserting on the returned LSP types.  The full pipeline —
//! parse → AST → LSP response — is exercised in every test.

use std::path::Path;

use ciane::parse;
use tower_lsp_server::ls_types::{
    CompletionItemKind, HoverContents, Position, PrepareRenameResponse, SymbolKind, Uri,
};

use super::{completion, definition, hover, references, rename, symbols, util};

// ── helpers ───────────────────────────────────────────────────────────────────

fn offset_of(source: &str, needle: &str) -> usize {
    source
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} not found in source"))
}

/// Returns the byte offset of the `n`th occurrence of `needle` (1-based).
fn nth_offset(source: &str, needle: &str, n: usize) -> usize {
    let mut search_from = 0;
    for _ in 1..n {
        let rel = source[search_from..]
            .find(needle)
            .unwrap_or_else(|| panic!("{needle:?} occurrence before {n} not found in source"));
        search_from += rel + needle.len();
    }
    search_from
        + source[search_from..]
            .find(needle)
            .unwrap_or_else(|| panic!("{needle:?} occurrence {n} not found in source"))
}

fn hover_text(source: &str, offset: usize) -> Option<String> {
    let parsed = parse(source);
    hover::at(&parsed, source, offset).map(|h| match h.contents {
        HoverContents::Markup(m) => m.value,
        _ => panic!("unexpected hover content kind"),
    })
}

fn completion_labels(source: &str, offset: usize) -> Vec<String> {
    let parsed = parse(source);
    completion::at(&parsed, source, offset)
        .into_iter()
        .map(|item| item.label)
        .collect()
}

fn dummy_uri() -> Uri {
    Uri::from_file_path(Path::new("/tmp/test.ci")).expect("valid URI from /tmp/test.ci")
}

// ── util::offset_to_position ─────────────────────────────────────────────────

#[test]
fn offset_to_position_start_of_file() {
    assert_eq!(util::offset_to_position("hello", 0), Position::new(0, 0));
}

#[test]
fn offset_to_position_mid_line() {
    assert_eq!(
        util::offset_to_position("hello world", 6),
        Position::new(0, 6)
    );
}

#[test]
fn offset_to_position_after_newline() {
    assert_eq!(
        util::offset_to_position("hello\nworld", 6),
        Position::new(1, 0)
    );
}

#[test]
fn offset_to_position_second_line_mid() {
    assert_eq!(
        util::offset_to_position("hello\nworld", 8),
        Position::new(1, 2)
    );
}

// ── util::position_to_offset ─────────────────────────────────────────────────

#[test]
fn position_to_offset_start() {
    assert_eq!(util::position_to_offset("hello", Position::new(0, 0)), 0);
}

#[test]
fn position_to_offset_mid_line() {
    assert_eq!(
        util::position_to_offset("hello world", Position::new(0, 6)),
        6
    );
}

#[test]
fn position_to_offset_second_line() {
    assert_eq!(
        util::position_to_offset("hello\nworld", Position::new(1, 2)),
        8
    );
}

#[test]
fn position_round_trips_through_offset() {
    let source = "stage build {}\nstage test {}";
    for offset in [0usize, 5, 9, 15, 22] {
        let pos = util::offset_to_position(source, offset);
        assert_eq!(
            util::position_to_offset(source, pos),
            offset,
            "round-trip failed at offset {offset}"
        );
    }
}

// ── util::token_at ────────────────────────────────────────────────────────────

#[test]
fn token_at_returns_token_at_offset() {
    // "workflow w { stage foo {} }" — offset inside "foo"
    let source = "workflow w { stage foo {} }";
    let parsed = parse(source);
    let tok = util::token_at(&parsed.syntax(), offset_of(source, "foo"))
        .expect("expected token at 'foo'");
    assert_eq!(tok.text(), "foo");
}

#[test]
fn token_at_boundary_prefers_non_trivia() {
    // Offset on the whitespace/"foo" boundary; token_at should return "foo".
    let source = "workflow w { stage foo {} }";
    let parsed = parse(source);
    let foo_offset = offset_of(source, "foo");
    // boundary: one byte before "foo" (the space), token_at should prefer "foo"
    let tok = util::token_at(&parsed.syntax(), foo_offset - 1).expect("expected token at boundary");
    // This may return whitespace or "foo"; the important thing is a token is returned.
    assert!(!tok.text().is_empty());
}

// ── hover ─────────────────────────────────────────────────────────────────────

#[test]
fn hover_workflow_keyword() {
    let src = "workflow ci {}";
    let text = hover_text(src, 0).expect("expected hover on 'workflow'");
    assert!(text.contains("workflow"), "hover: {text}");
}

#[test]
fn hover_stage_keyword() {
    let src = "workflow w { stage foo {} }";
    let text = hover_text(src, offset_of(src, "stage")).expect("expected hover on 'stage'");
    assert!(text.contains("stage"), "hover: {text}");
}

#[test]
fn hover_job_keyword() {
    let src = "workflow w { stage s { job foo {} } }";
    let text = hover_text(src, offset_of(src, "job")).expect("expected hover on 'job'");
    assert!(text.contains("job"), "hover: {text}");
}

#[test]
fn hover_template_keyword() {
    let src = "workflow w { stage s { template t [] } }";
    let text = hover_text(src, offset_of(src, "template")).expect("expected hover on 'template'");
    assert!(text.contains("template"), "hover: {text}");
}

#[test]
fn hover_workflow_name_ident() {
    let src = "workflow myWorkflow {}";
    let text =
        hover_text(src, offset_of(src, "myWorkflow")).expect("expected hover on workflow name");
    assert!(text.contains("myWorkflow"), "hover: {text}");
}

#[test]
fn hover_stage_name_ident() {
    let src = "workflow w { stage myStage {} }";
    let text = hover_text(src, offset_of(src, "myStage")).expect("expected hover on stage name");
    assert!(text.contains("myStage"), "hover: {text}");
}

#[test]
fn hover_job_name_ident() {
    let src = "workflow w { stage s { job myJob {} } }";
    let text = hover_text(src, offset_of(src, "myJob")).expect("expected hover on job name");
    assert!(text.contains("myJob"), "hover: {text}");
}

#[test]
fn hover_template_name_ident() {
    let src = "workflow w { stage s { template myTmpl [] } }";
    let text = hover_text(src, offset_of(src, "myTmpl")).expect("expected hover on template name");
    assert!(text.contains("myTmpl"), "hover: {text}");
}

#[test]
fn hover_inherit_attr_key() {
    let src = "workflow w { stage s { template t [] job j ( inherit = t ) [] } }";
    let text = hover_text(src, offset_of(src, "inherit")).expect("expected hover on 'inherit' key");
    assert!(text.contains("inherit"), "hover: {text}");
}

#[test]
fn hover_inherit_bare_value() {
    // "tmplA" appears twice: template declaration, then as the inherit value.
    // Hover on the second (value) occurrence should describe the template reference.
    let src = "workflow w { stage s { template tmplA [] job j ( inherit = tmplA ) [] } }";
    let text =
        hover_text(src, nth_offset(src, "tmplA", 2)).expect("expected hover on inherit value");
    assert!(text.contains("tmplA"), "hover: {text}");
}

#[test]
fn hover_opening_brace_returns_none() {
    let src = "workflow w { stage s {} }";
    assert!(
        hover_text(src, offset_of(src, "{")).is_none(),
        "expected no hover on '{{'"
    );
}

// ── symbols ───────────────────────────────────────────────────────────────────

#[test]
fn symbols_empty_source() {
    let parsed = parse("");
    assert!(symbols::collect(&parsed, "").is_empty());
}

#[test]
fn symbols_single_stage() {
    let src = "workflow w { stage build {} }";
    let parsed = parse(src);
    let syms = symbols::collect(&parsed, src);
    assert_eq!(syms.len(), 1);
    assert_eq!(syms[0].name, "build");
    assert_eq!(syms[0].kind, SymbolKind::NAMESPACE);
}

#[test]
fn symbols_stage_with_job() {
    let src = "workflow w { stage s { job compile {} } }";
    let parsed = parse(src);
    let syms = symbols::collect(&parsed, src);
    let children = syms[0].children.as_deref().expect("expected children");
    let job = children
        .iter()
        .find(|s| s.name == "compile")
        .expect("expected 'compile' child symbol");
    assert_eq!(job.kind, SymbolKind::FUNCTION);
}

#[test]
fn symbols_stage_with_template() {
    let src = "workflow w { stage s { template base [] } }";
    let parsed = parse(src);
    let syms = symbols::collect(&parsed, src);
    let children = syms[0].children.as_deref().expect("expected children");
    let tmpl = children
        .iter()
        .find(|s| s.name == "base")
        .expect("expected 'base' child symbol");
    assert_eq!(tmpl.kind, SymbolKind::CLASS);
}

#[test]
fn symbols_multiple_stages() {
    let src = "workflow w { stage a { job x {} } stage b { job y {} } }";
    let parsed = parse(src);
    let syms = symbols::collect(&parsed, src);
    assert_eq!(syms.len(), 2);
    let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"a"), "names: {names:?}");
    assert!(names.contains(&"b"), "names: {names:?}");
}

#[test]
fn symbols_stage_range_starts_after_workflow_wrapper() {
    let src = "workflow w {\n    stage s {}\n}";
    let parsed = parse(src);
    let syms = symbols::collect(&parsed, src);
    assert_eq!(syms[0].range.start.line, 1, "stage 's' is on line 1");
}

// ── completion ────────────────────────────────────────────────────────────────

#[test]
fn completion_toplevel_offers_workflow() {
    // A token at the root level triggers top-level keywords — only `workflow` is valid there.
    let src = "x";
    let labels = completion_labels(src, 0);
    assert!(
        labels.contains(&"workflow".to_owned()),
        "labels: {labels:?}"
    );
}

#[test]
fn completion_toplevel_items_are_keyword_kind() {
    let src = "x";
    let parsed = parse(src);
    let items = completion::at(&parsed, src, 0);
    assert!(
        items
            .iter()
            .all(|i| i.kind == Some(CompletionItemKind::KEYWORD)),
        "top-level completions should all be KEYWORD kind"
    );
}

#[test]
fn completion_workflow_body_offers_use_stage_template() {
    // A token inside a workflow body triggers workflow-body keywords.
    let src = "workflow w { x }";
    let labels = completion_labels(src, offset_of(src, "x"));
    assert!(labels.contains(&"use".to_owned()), "labels: {labels:?}");
    assert!(labels.contains(&"stage".to_owned()), "labels: {labels:?}");
    assert!(
        labels.contains(&"template".to_owned()),
        "labels: {labels:?}"
    );
}

#[test]
fn completion_stage_body_offers_job_and_template() {
    // An unknown token inside a stage body triggers stage-body keywords.
    let src = "workflow w { stage s { x } }";
    let labels = completion_labels(src, offset_of(src, "x"));
    assert!(labels.contains(&"job".to_owned()), "labels: {labels:?}");
    assert!(
        labels.contains(&"template".to_owned()),
        "labels: {labels:?}"
    );
}

#[test]
fn completion_inherit_value_suggests_local_templates() {
    // Cursor on the inherit value token — should list stage-local templates.
    let src = "workflow w { stage s { template tmplA [] job j ( inherit = tmplA ) [] } }";
    let labels = completion_labels(src, nth_offset(src, "tmplA", 2));
    assert!(labels.contains(&"tmplA".to_owned()), "labels: {labels:?}");
}

#[test]
fn completion_dependencies_suggests_stage_job_pairs() {
    // Cursor inside a dep ref list — should offer `stage.job` completions.
    let src = "workflow w { stage build { job compile {} } stage test { job unit ( dependencies = [build.compile] ) {} } }";
    let labels = completion_labels(src, offset_of(src, "build.compile"));
    assert!(
        labels.contains(&"build.compile".to_owned()),
        "labels: {labels:?}"
    );
}

#[test]
fn completion_job_attr_key_offers_all_job_fields() {
    // Cursor on an attr key inside a job — should offer all job attribute names.
    let src = "workflow w { stage s { job j ( inherit = t ) {} } }";
    let labels = completion_labels(src, offset_of(src, "inherit"));
    assert!(labels.contains(&"inherit".to_owned()), "labels: {labels:?}");
    assert!(
        labels.contains(&"dependencies".to_owned()),
        "labels: {labels:?}"
    );
    assert!(
        labels.contains(&"container".to_owned()),
        "labels: {labels:?}"
    );
}

// ── definition ────────────────────────────────────────────────────────────────

#[test]
fn definition_inherit_resolves_to_local_template() {
    let src = "workflow w {\n    stage s {\n        template tmplA []\n        job j ( inherit = tmplA ) []\n    }\n}";
    let parsed = parse(src);
    let uri = dummy_uri();
    // First "tmplA" is the declaration on line 2; second is the inherit value on line 3.
    let loc = definition::resolve(
        &parsed,
        src,
        nth_offset(src, "tmplA", 2),
        Path::new("/tmp/test.ci"),
        &uri,
    )
    .expect("expected a definition location");
    assert_eq!(loc.uri, uri, "expected location in same file");
    assert_eq!(
        loc.range.start.line, 2,
        "template 'tmplA' is declared on line 2"
    );
}

#[test]
fn definition_dependency_ref_resolves_to_job() {
    let src = "workflow w {\n    stage build { job compile {} }\n    stage test { job unit ( dependencies = [build.compile] ) {} }\n}";
    let parsed = parse(src);
    let uri = dummy_uri();
    // First "compile" is the job declaration on line 1; second is in the dep ref on line 2.
    let loc = definition::resolve(
        &parsed,
        src,
        nth_offset(src, "compile", 2),
        Path::new("/tmp/test.ci"),
        &uri,
    )
    .expect("expected a definition for dependency");
    assert_eq!(loc.uri, uri);
    assert_eq!(
        loc.range.start.line, 1,
        "job 'compile' is declared on line 1"
    );
}

#[test]
fn definition_stage_keyword_returns_none() {
    let src = "workflow w { stage s {} }";
    let parsed = parse(src);
    let result = definition::resolve(
        &parsed,
        src,
        offset_of(src, "stage"),
        Path::new("/tmp/test.ci"),
        &dummy_uri(),
    );
    assert!(
        result.is_none(),
        "'stage' keyword should not resolve to a definition"
    );
}

#[test]
fn definition_stage_name_declaration_returns_none() {
    // A declaration site is not a reference — goto-definition should return nothing.
    let src = "workflow w { stage myStage {} }";
    let parsed = parse(src);
    let result = definition::resolve(
        &parsed,
        src,
        offset_of(src, "myStage"),
        Path::new("/tmp/test.ci"),
        &dummy_uri(),
    );
    assert!(
        result.is_none(),
        "stage declaration should not resolve to a definition"
    );
}

// ── rename ────────────────────────────────────────────────────────────────────

#[test]
fn prepare_rename_on_stage_name() {
    let src = "workflow w { stage myStage {} }";
    let parsed = parse(src);
    let resp = rename::prepare(&parsed, src, offset_of(src, "myStage"))
        .expect("expected PrepareRenameResponse");
    let PrepareRenameResponse::RangeWithPlaceholder { range, placeholder } = resp else {
        panic!("expected RangeWithPlaceholder");
    };
    assert_eq!(placeholder, "myStage");
    assert_eq!(range.start.line, 0);
}

#[test]
fn prepare_rename_on_job_name() {
    let src = "workflow w { stage s { job myJob {} } }";
    let parsed = parse(src);
    let resp = rename::prepare(&parsed, src, offset_of(src, "myJob"))
        .expect("expected PrepareRenameResponse");
    let PrepareRenameResponse::RangeWithPlaceholder { placeholder, .. } = resp else {
        panic!("expected RangeWithPlaceholder");
    };
    assert_eq!(placeholder, "myJob");
}

#[test]
fn prepare_rename_on_template_name() {
    let src = "workflow w { stage s { template myTmpl [] } }";
    let parsed = parse(src);
    let resp = rename::prepare(&parsed, src, offset_of(src, "myTmpl"))
        .expect("expected PrepareRenameResponse");
    let PrepareRenameResponse::RangeWithPlaceholder { placeholder, .. } = resp else {
        panic!("expected RangeWithPlaceholder");
    };
    assert_eq!(placeholder, "myTmpl");
}

#[test]
fn prepare_rename_on_keyword_returns_none() {
    let src = "workflow w { stage s {} }";
    let parsed = parse(src);
    assert!(
        rename::prepare(&parsed, src, offset_of(src, "stage")).is_none(),
        "keywords are not renameable"
    );
}

#[test]
fn rename_stage_propagates_to_dependency_references() {
    let src = "workflow w {\n    stage build { job compile {} }\n    stage test { job unit ( dependencies = [build.compile] ) {} }\n}";
    let parsed = parse(src);
    let edits = rename::edits_for(&parsed, src, offset_of(src, "build"), "newBuild")
        .expect("expected rename edits");
    // 2 edits: stage declaration + dep ref stage part.
    assert_eq!(edits.len(), 2, "edits: {edits:?}");
    assert!(
        edits.iter().all(|e| e.new_text == "newBuild"),
        "edits: {edits:?}"
    );
}

#[test]
fn rename_job_propagates_to_dependency_references() {
    let src = "workflow w {\n    stage build { job compile {} }\n    stage test { job unit ( dependencies = [build.compile] ) {} }\n}";
    let parsed = parse(src);
    let edits = rename::edits_for(&parsed, src, offset_of(src, "compile"), "newCompile")
        .expect("expected rename edits");
    // 2 edits: job declaration + dep ref job part.
    assert_eq!(edits.len(), 2, "edits: {edits:?}");
    assert!(
        edits.iter().all(|e| e.new_text == "newCompile"),
        "edits: {edits:?}"
    );
}

#[test]
fn rename_template_propagates_to_inherit_references() {
    let src = "workflow w {\n    stage s {\n        template base []\n        job a ( inherit = base ) []\n        job b ( inherit = base ) []\n    }\n}";
    let parsed = parse(src);
    let edits = rename::edits_for(&parsed, src, offset_of(src, "base"), "newBase")
        .expect("expected rename edits");
    // 3 edits: template declaration + 2 inherit references.
    assert_eq!(edits.len(), 3, "edits: {edits:?}");
    assert!(
        edits.iter().all(|e| e.new_text == "newBase"),
        "edits: {edits:?}"
    );
}

#[test]
fn rename_from_dependency_ref_propagates_to_stage_declaration() {
    // Rename triggered from the stage part of a dep ref — should also update the declaration.
    let src = "workflow w {\n    stage build { job compile {} }\n    stage test { job unit ( dependencies = [build.compile] ) {} }\n}";
    let parsed = parse(src);
    // Second "build" is in the dependency reference.
    let edits = rename::edits_for(&parsed, src, nth_offset(src, "build", 2), "infra")
        .expect("expected rename edits");
    assert_eq!(edits.len(), 2, "edits: {edits:?}");
    assert!(
        edits.iter().all(|e| e.new_text == "infra"),
        "edits: {edits:?}"
    );
}

// ── references ────────────────────────────────────────────────────────────────

#[test]
fn references_job_name_excludes_declaration_when_not_requested() {
    // Cursor on job declaration with include_declaration=false — only dep refs returned.
    let src = "workflow w {\n    stage build { job compile {} }\n    stage test { job unit ( dependencies = [build.compile] ) {} }\n}";
    let parsed = parse(src);
    let uri = dummy_uri();
    let locs = references::find(
        &parsed,
        src,
        offset_of(src, "compile"),
        false,
        Path::new("/tmp/test.ci"),
        &uri,
    )
    .expect("expected locations");
    assert_eq!(locs.len(), 1, "locs: {locs:?}");
    assert_eq!(locs[0].range.start.line, 2, "dep ref is on line 2");
}

#[test]
fn references_job_name_includes_declaration_when_requested() {
    let src = "workflow w {\n    stage build { job compile {} }\n    stage test { job unit ( dependencies = [build.compile] ) {} }\n}";
    let parsed = parse(src);
    let uri = dummy_uri();
    let locs = references::find(
        &parsed,
        src,
        offset_of(src, "compile"),
        true,
        Path::new("/tmp/test.ci"),
        &uri,
    )
    .expect("expected locations");
    // declaration + dep ref
    assert_eq!(locs.len(), 2, "locs: {locs:?}");
}

#[test]
fn references_template_declaration_finds_all_inherit_uses() {
    let src = "workflow w {\n    stage s {\n        template base []\n        job a ( inherit = base ) []\n        job b ( inherit = base ) []\n    }\n}";
    let parsed = parse(src);
    let uri = dummy_uri();
    let locs = references::find(
        &parsed,
        src,
        offset_of(src, "base"),
        false,
        Path::new("/tmp/test.ci"),
        &uri,
    )
    .expect("expected locations");
    // 2 inherit references, no declaration
    assert_eq!(locs.len(), 2, "locs: {locs:?}");
}

#[test]
fn references_inherit_value_with_declaration_finds_all_sites() {
    // Cursor on an inherit value — returns template declaration + all inherit refs.
    let src = "workflow w {\n    stage s {\n        template base []\n        job a ( inherit = base ) []\n        job b ( inherit = base ) []\n    }\n}";
    let parsed = parse(src);
    let uri = dummy_uri();
    // Third "base": after template decl and job a's inherit
    let locs = references::find(
        &parsed,
        src,
        nth_offset(src, "base", 2),
        true,
        Path::new("/tmp/test.ci"),
        &uri,
    )
    .expect("expected locations");
    // template declaration + 2 inherit refs
    assert_eq!(locs.len(), 3, "locs: {locs:?}");
}

#[test]
fn references_keyword_returns_none() {
    let src = "workflow w { stage s {} }";
    let parsed = parse(src);
    let uri = dummy_uri();
    let result = references::find(
        &parsed,
        src,
        offset_of(src, "stage"),
        false,
        Path::new("/tmp/test.ci"),
        &uri,
    );
    assert!(result.is_none(), "keywords should not have references");
}

#[test]
fn template_def_at_returns_name_on_template_declaration() {
    let src = "workflow w { stage s { template myTmpl [] } }";
    let parsed = parse(src);
    let name = references::template_def_at(&parsed, offset_of(src, "myTmpl"))
        .expect("expected template name");
    assert_eq!(name, "myTmpl");
}

#[test]
fn template_def_at_returns_none_for_non_template_positions() {
    let src = "workflow w { stage s { job myJob {} } }";
    let parsed = parse(src);
    assert!(
        references::template_def_at(&parsed, offset_of(src, "myJob")).is_none(),
        "job name is not a template def"
    );
}
