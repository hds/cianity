use ciane::{
    ast::{AstNode, HasName, Job, Root, Stage, TemplateDef},
    parser::Parse,
};
use tower_lsp_server::ls_types::{DocumentSymbol, Range, SymbolKind};

use super::util::range_to_lsp;

#[must_use]
pub(super) fn collect(parse: &Parse, source: &str) -> Vec<DocumentSymbol> {
    let Some(root) = Root::cast(parse.syntax()) else {
        return Vec::new();
    };
    root.stages()
        .filter_map(|stage| stage_sym(&stage, source))
        .collect()
}

fn stage_sym(stage: &Stage, source: &str) -> Option<DocumentSymbol> {
    let name = stage.name()?.to_string();
    let range = range_to_lsp(source, stage.syntax().text_range());
    let sel = stage
        .name_token()
        .map_or(range, |t| range_to_lsp(source, t.text_range()));
    let children = stage.body().map_or_else(Vec::new, |body| {
        body.jobs()
            .filter_map(|j| job_sym(&j, source))
            .chain(body.templates().filter_map(|t| tmpl_sym(&t, source)))
            .collect()
    });
    Some(make_sym(
        name,
        SymbolKind::NAMESPACE,
        range,
        sel,
        Some(children),
    ))
}

fn job_sym(job: &Job, source: &str) -> Option<DocumentSymbol> {
    let name = job.name()?.to_string();
    let range = range_to_lsp(source, job.syntax().text_range());
    let sel = job
        .name_token()
        .map_or(range, |t| range_to_lsp(source, t.text_range()));
    Some(make_sym(name, SymbolKind::FUNCTION, range, sel, None))
}

fn tmpl_sym(tmpl: &TemplateDef, source: &str) -> Option<DocumentSymbol> {
    let name = tmpl.name()?.to_string();
    let range = range_to_lsp(source, tmpl.syntax().text_range());
    let sel = tmpl
        .name_token()
        .map_or(range, |t| range_to_lsp(source, t.text_range()));
    Some(make_sym(name, SymbolKind::CLASS, range, sel, None))
}

#[allow(deprecated)]
fn make_sym(
    name: String,
    kind: SymbolKind,
    range: Range,
    selection_range: Range,
    children: Option<Vec<DocumentSymbol>>,
) -> DocumentSymbol {
    DocumentSymbol {
        name,
        detail: None,
        kind,
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children,
    }
}
