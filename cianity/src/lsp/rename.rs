use std::path::Path;

use ciane::{
    ast::{AstNode, Attr, AttrValue, HasAttrList, HasName, Root, Stage},
    parser::Parse,
    syntax::{SyntaxKind, SyntaxNode, SyntaxToken},
};
use rowan::TextRange;
use tower_lsp_server::ls_types::{PrepareRenameResponse, TextEdit};

use super::templates::{self, TemplateFile};
use super::util::{range_to_lsp, token_at};

/// Imports are resolved only to tell a local template reference from one in
/// another file, which rename doesn't touch, so no real path is needed.
fn local_path() -> &'static Path {
    Path::new(".")
}

/// Returns the rename range and placeholder if the position is renameable.
#[must_use]
pub(super) fn prepare(parse: &Parse, source: &str, offset: usize) -> Option<PrepareRenameResponse> {
    let root_node = parse.syntax();
    let token = token_at(&root_node, offset)?;
    let root = Root::cast(root_node)?;
    let range = rename_range(&token, &root)?;
    let placeholder = source
        .get(usize::from(range.start())..usize::from(range.end()))
        .unwrap_or_else(|| token.text())
        .to_owned();
    Some(PrepareRenameResponse::RangeWithPlaceholder {
        range: range_to_lsp(source, range),
        placeholder,
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
    let root = Root::cast(root_node)?;
    rename_range(&token, &root)?;
    collect_edits(&token, &root, source, new_name)
}

/// Returns the text range of the renameable symbol at `token`, or `None`.
fn rename_range(token: &SyntaxToken, root: &Root) -> Option<TextRange> {
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
    // Only the name part of an `inherit` reference is renamed, and only when
    // the template is in this file: an edit can't reach another one.
    let template_ref = templates::ref_at(token, root, local_path())?;
    if template_ref.id.file == TemplateFile::Current {
        return Some(template_ref.name_range);
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
                SyntaxKind::TemplateDef => edits_rename_template(token, root, source, new_name),
                _ => None,
            };
        }
        if parent.kind() == SyntaxKind::Ref && is_dependency_ref(&parent) {
            return edits_rename_dep_ref(token, &parent, root, source, new_name);
        }
    }
    edits_rename_inherit_ref(token, root, source, new_name)
}

/// Rename the template a reference names, updating its declaration and every
/// other reference to it in this file.
fn edits_rename_inherit_ref(
    token: &SyntaxToken,
    root: &Root,
    source: &str,
    new_name: &str,
) -> Option<Vec<TextEdit>> {
    let id = templates::ref_at(token, root, local_path())?.id;
    if id.file != TemplateFile::Current {
        return None;
    }
    Some(edits_for_template(&id, root, source, new_name))
}

/// The declaration of `id` plus every reference to it in this file.
fn edits_for_template(
    id: &templates::TemplateId,
    root: &Root,
    source: &str,
    new_name: &str,
) -> Vec<TextEdit> {
    let mut edits = Vec::new();
    if let Some(def) = templates::find_def(id, root, source)
        && def.path.is_none()
    {
        edits.push(make_edit(source, def.name_range, new_name));
    }
    for template_ref in templates::all_refs(root, local_path()) {
        if &template_ref.id == id {
            edits.push(make_edit(source, template_ref.name_range, new_name));
        }
    }
    edits
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
    // `inherit = <stage>.template` references name the stage too.
    for template_ref in templates::all_refs(root, local_path()) {
        if template_ref.id.file == TemplateFile::Current
            && template_ref.id.stage.as_deref() == Some(old_name)
            && let Some(range) = template_ref.stage_range
        {
            edits.push(make_edit(source, range, new_name));
        }
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
    root: &Root,
    source: &str,
    new_name: &str,
) -> Option<Vec<TextEdit>> {
    let id = templates::def_at(name_token)?;
    Some(edits_for_template(&id, root, source, new_name))
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
