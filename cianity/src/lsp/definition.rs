use ciane::{
    ast::{AstNode, Attr, AttrValue, HasName, Root, Stage},
    parser::Parse,
    syntax::{SyntaxKind, SyntaxNode, SyntaxToken},
};
use tower_lsp_server::ls_types::Range;

use super::util::{range_to_lsp, token_at};

#[must_use]
pub(super) fn resolve(parse: &Parse, source: &str, offset: usize) -> Option<Range> {
    let root_node = parse.syntax();
    let token = token_at(&root_node, offset)?;
    let root = Root::cast(root_node)?;
    resolve_inherit(&token, source).or_else(|| resolve_dependency(&token, &root, source))
}

fn resolve_inherit(token: &SyntaxToken, source: &str) -> Option<Range> {
    if token.kind() != SyntaxKind::BareValue {
        return None;
    }
    let attr_value = AttrValue::cast(token.parent()?)?;
    let attr = Attr::cast(attr_value.syntax().parent()?)?;
    if attr.key_text().as_deref() != Some("inherit") {
        return None;
    }
    let template_name = attr.value_text()?;
    // Strip optional namespace prefix: "ns/tmpl" → "tmpl"
    let local = template_name
        .split('/')
        .next_back()
        .unwrap_or(template_name.as_str());
    let stage = ancestor_stage(attr.syntax())?;
    let tmpl = stage
        .body()?
        .templates()
        .find(|t| t.name().as_deref() == Some(local))?;
    Some(range_to_lsp(source, tmpl.name_token()?.text_range()))
}

fn resolve_dependency(token: &SyntaxToken, root: &Root, source: &str) -> Option<Range> {
    if token.kind() != SyntaxKind::Ident {
        return None;
    }
    let ref_node = token.parent()?;
    if ref_node.kind() != SyntaxKind::Ref {
        return None;
    }
    let ref_list = ref_node.parent()?;
    if ref_list.kind() != SyntaxKind::RefList {
        return None;
    }
    let attr_value = AttrValue::cast(ref_list.parent()?)?;
    let attr = Attr::cast(attr_value.syntax().parent()?)?;
    if attr.key_text().as_deref() != Some("dependencies") {
        return None;
    }
    let ref_text = ref_node.text().to_string();
    let (stage_name, job_name) = ref_text.split_once('.')?;
    let stage = root
        .stages()
        .find(|s| s.name().as_deref() == Some(stage_name))?;
    let job = stage
        .body()?
        .jobs()
        .find(|j| j.name().as_deref() == Some(job_name))?;
    Some(range_to_lsp(source, job.name_token()?.text_range()))
}

fn ancestor_stage(node: &SyntaxNode) -> Option<Stage> {
    node.ancestors().find_map(Stage::cast)
}
