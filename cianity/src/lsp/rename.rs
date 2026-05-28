use ciane::{
    ast::{AstNode, Attr, AttrValue, HasAttrList, HasName, Root, Stage},
    parser::Parse,
    syntax::{SyntaxKind, SyntaxNode, SyntaxToken},
};
use rowan::TextRange;
use tower_lsp_server::ls_types::{PrepareRenameResponse, TextEdit};

use super::util::{range_to_lsp, token_at};

/// Returns the rename range and placeholder if the position is renameable.
#[must_use]
pub(super) fn prepare(parse: &Parse, source: &str, offset: usize) -> Option<PrepareRenameResponse> {
    let token = token_at(&parse.syntax(), offset)?;
    let range = rename_range(&token)?;
    Some(PrepareRenameResponse::RangeWithPlaceholder {
        range: range_to_lsp(source, range),
        placeholder: token.text().to_owned(),
    })
}

/// Returns all text edits to rename the symbol at `offset` to `new_name`.
#[must_use]
pub(super) fn edits_for(
    parse: &Parse,
    source: &str,
    offset: usize,
    new_name: &str,
) -> Option<Vec<TextEdit>> {
    let root_node = parse.syntax();
    let token = token_at(&root_node, offset)?;
    rename_range(&token)?;
    let root = Root::cast(root_node)?;
    collect_edits(&token, &root, source, new_name)
}

/// Returns the text range of the renameable symbol at `token`, or `None`.
fn rename_range(token: &SyntaxToken) -> Option<TextRange> {
    if token.kind() == SyntaxKind::Ident {
        let parent = token.parent()?;
        if parent.kind() == SyntaxKind::Name {
            let owner = parent.parent()?;
            if matches!(
                owner.kind(),
                SyntaxKind::Stage | SyntaxKind::Job | SyntaxKind::TemplateDef
            ) {
                return Some(token.text_range());
            }
        }
        if parent.kind() == SyntaxKind::Ref && is_dependency_ref(&parent) {
            return Some(token.text_range());
        }
    }
    if token.kind() == SyntaxKind::BareValue {
        let attr_key = token
            .parent()
            .and_then(AttrValue::cast)
            .and_then(|av| Attr::cast(av.syntax().parent()?))
            .and_then(|a| a.key_text());
        if attr_key.as_deref() == Some("inherit") && !token.text().contains('/') {
            return Some(token.text_range());
        }
    }
    None
}

fn is_dependency_ref(ref_node: &SyntaxNode) -> bool {
    let attr_key = ref_node
        .parent()
        .filter(|n| n.kind() == SyntaxKind::RefList)
        .and_then(|rl| rl.parent())
        .and_then(AttrValue::cast)
        .and_then(|av| Attr::cast(av.syntax().parent()?))
        .and_then(|a| a.key_text());
    attr_key.as_deref() == Some("dependencies")
}

fn collect_edits(
    token: &SyntaxToken,
    root: &Root,
    source: &str,
    new_name: &str,
) -> Option<Vec<TextEdit>> {
    if token.kind() == SyntaxKind::Ident {
        let parent = token.parent()?;
        if parent.kind() == SyntaxKind::Name {
            let owner = parent.parent()?;
            return match owner.kind() {
                SyntaxKind::Stage => Some(edits_rename_stage(token, root, source, new_name)),
                SyntaxKind::Job => edits_rename_job(token, root, source, new_name),
                SyntaxKind::TemplateDef => edits_rename_template(token, source, new_name),
                _ => None,
            };
        }
        if parent.kind() == SyntaxKind::Ref {
            return edits_rename_dep_ref(token, &parent, root, source, new_name);
        }
    }
    if token.kind() == SyntaxKind::BareValue {
        return edits_rename_inherit_ref(token, source, new_name);
    }
    None
}

fn edits_rename_stage(
    name_token: &SyntaxToken,
    root: &Root,
    source: &str,
    new_name: &str,
) -> Vec<TextEdit> {
    let old_name = name_token.text();
    let mut edits = vec![make_edit(source, name_token.text_range(), new_name)];
    for tok in dep_ref_stage_tokens(root, old_name) {
        edits.push(make_edit(source, tok.text_range(), new_name));
    }
    edits
}

fn edits_rename_job(
    name_token: &SyntaxToken,
    root: &Root,
    source: &str,
    new_name: &str,
) -> Option<Vec<TextEdit>> {
    let old_name = name_token.text();
    // name_token → Name → Job → StageBody → Stage
    let stage = name_token
        .parent()
        .and_then(|n| n.parent())
        .and_then(|n| n.ancestors().find_map(Stage::cast))?;
    let stage_name = stage.name()?;
    let mut edits = vec![make_edit(source, name_token.text_range(), new_name)];
    for tok in dep_ref_job_tokens(root, stage_name.as_str(), old_name) {
        edits.push(make_edit(source, tok.text_range(), new_name));
    }
    Some(edits)
}

fn edits_rename_template(
    name_token: &SyntaxToken,
    source: &str,
    new_name: &str,
) -> Option<Vec<TextEdit>> {
    let old_name = name_token.text();
    // name_token → Name → TemplateDef → StageBody → Stage
    let stage = name_token
        .parent()
        .and_then(|n| n.parent())
        .and_then(|n| n.ancestors().find_map(Stage::cast))?;
    let mut edits = vec![make_edit(source, name_token.text_range(), new_name)];
    for tok in inherit_tokens_matching(&stage, old_name) {
        edits.push(make_edit(source, tok.text_range(), new_name));
    }
    Some(edits)
}

fn edits_rename_inherit_ref(
    token: &SyntaxToken,
    source: &str,
    new_name: &str,
) -> Option<Vec<TextEdit>> {
    let tmpl_name = token.text();
    let stage = token
        .parent()
        .and_then(|n| n.ancestors().find_map(Stage::cast))?;
    let mut edits = Vec::new();
    // Include the declaration if it exists in this stage
    if let Some(body) = stage.body()
        && let Some(tmpl) = body
            .templates()
            .find(|t| t.name().as_deref() == Some(tmpl_name))
        && let Some(tok) = tmpl.name_token()
    {
        edits.push(make_edit(source, tok.text_range(), new_name));
    }
    // Include all inherit references in this stage
    for tok in inherit_tokens_matching(&stage, tmpl_name) {
        edits.push(make_edit(source, tok.text_range(), new_name));
    }
    Some(edits)
}

fn edits_rename_dep_ref(
    token: &SyntaxToken,
    ref_node: &SyntaxNode,
    root: &Root,
    source: &str,
    new_name: &str,
) -> Option<Vec<TextEdit>> {
    let (first, second) = ref_idents_of(ref_node);
    let first = first?;
    // If cursor is on the second ident, we're renaming the job
    if let Some(second) = second
        && second.text_range() == token.text_range()
    {
        let stage_name = first.text();
        let old_job_name = token.text();
        let stage = root
            .stages()
            .find(|s| s.name().as_deref() == Some(stage_name))?;
        let job = stage
            .body()?
            .jobs()
            .find(|j| j.name().as_deref() == Some(old_job_name))?;
        let mut edits = Vec::new();
        if let Some(tok) = job.name_token() {
            edits.push(make_edit(source, tok.text_range(), new_name));
        }
        for tok in dep_ref_job_tokens(root, stage_name, old_job_name) {
            edits.push(make_edit(source, tok.text_range(), new_name));
        }
        return Some(edits);
    }
    // Cursor is on the stage (first) ident — rename the stage
    let old_stage_name = first.text();
    let stage_decl = root
        .stages()
        .find(|s| s.name().as_deref() == Some(old_stage_name))?;
    let mut edits = Vec::new();
    if let Some(tok) = stage_decl.name_token() {
        edits.push(make_edit(source, tok.text_range(), new_name));
    }
    for tok in dep_ref_stage_tokens(root, old_stage_name) {
        edits.push(make_edit(source, tok.text_range(), new_name));
    }
    Some(edits)
}

fn dep_ref_stage_tokens(root: &Root, stage_name: &str) -> Vec<SyntaxToken> {
    all_dep_ref_pairs(root)
        .into_iter()
        .filter_map(|(first, _)| {
            if first.text() == stage_name {
                Some(first)
            } else {
                None
            }
        })
        .collect()
}

fn dep_ref_job_tokens(root: &Root, stage_name: &str, job_name: &str) -> Vec<SyntaxToken> {
    all_dep_ref_pairs(root)
        .into_iter()
        .filter_map(|(first, second)| {
            if first.text() == stage_name {
                second.filter(|t| t.text() == job_name)
            } else {
                None
            }
        })
        .collect()
}

fn inherit_tokens_matching(stage: &Stage, tmpl_name: &str) -> Vec<SyntaxToken> {
    let mut tokens = Vec::new();
    let Some(body) = stage.body() else {
        return tokens;
    };
    for job in body.jobs() {
        let Some(al) = job.attr_list() else {
            continue;
        };
        for attr in al.attrs() {
            if attr.key_text().as_deref() != Some("inherit") {
                continue;
            }
            let Some(av) = attr.value() else {
                continue;
            };
            if let Some(tok) = av
                .syntax()
                .children_with_tokens()
                .find_map(|e| e.into_token().filter(|t| t.kind() == SyntaxKind::BareValue))
                && tok.text() == tmpl_name
            {
                tokens.push(tok);
            }
        }
    }
    tokens
}

fn all_dep_ref_pairs(root: &Root) -> Vec<(SyntaxToken, Option<SyntaxToken>)> {
    let mut pairs = Vec::new();
    for stage in root.stages() {
        let Some(body) = stage.body() else {
            continue;
        };
        for job in body.jobs() {
            let Some(al) = job.attr_list() else {
                continue;
            };
            for attr in al.attrs() {
                if attr.key_text().as_deref() != Some("dependencies") {
                    continue;
                }
                let Some(av) = attr.value() else {
                    continue;
                };
                let Some(rl) = av.ref_list() else {
                    continue;
                };
                for r in rl.refs() {
                    let (first, second) = ref_idents_of(r.syntax());
                    if let Some(f) = first {
                        pairs.push((f, second));
                    }
                }
            }
        }
    }
    pairs
}

fn ref_idents_of(ref_node: &SyntaxNode) -> (Option<SyntaxToken>, Option<SyntaxToken>) {
    let mut idents = ref_node
        .children_with_tokens()
        .filter_map(|e| e.into_token().filter(|t| t.kind() == SyntaxKind::Ident));
    (idents.next(), idents.next())
}

fn make_edit(source: &str, range: TextRange, new_text: &str) -> TextEdit {
    TextEdit {
        range: range_to_lsp(source, range),
        new_text: new_text.to_owned(),
    }
}
