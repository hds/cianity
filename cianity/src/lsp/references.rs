use std::path::Path;

use ciane::{
    ast::{AstNode, Attr, AttrValue, HasAttrList, HasName, Root, Stage},
    parser::Parse,
    syntax::{SyntaxKind, SyntaxNode, SyntaxToken},
};
use tower_lsp_server::ls_types::{Location, Uri};

use super::templates::{self, TemplateFile, TemplateId};
use super::util::{range_to_lsp, token_at};

/// Returns all reference locations for the job or template at `offset`.
#[must_use]
pub(super) fn find(
    parse: &Parse,
    source: &str,
    offset: usize,
    include_declaration: bool,
    file_path: &Path,
    current_uri: &Uri,
) -> Option<Vec<Location>> {
    let root_node = parse.syntax();
    let token = token_at(&root_node, offset)?;
    let root = Root::cast(root_node)?;
    template_locations(
        &token,
        &root,
        source,
        include_declaration,
        file_path,
        current_uri,
    )
    .or_else(|| {
        find_ident(
            &token,
            &root,
            source,
            include_declaration,
            file_path,
            current_uri,
        )
    })
}

/// References to the template declared or named at `token`, wherever in the
/// file they're written and however they're qualified.
fn template_locations(
    token: &SyntaxToken,
    root: &Root,
    source: &str,
    include_declaration: bool,
    file_path: &Path,
    current_uri: &Uri,
) -> Option<Vec<Location>> {
    let id = templates::def_at(token)
        .or_else(|| templates::ref_at(token, root, file_path).map(|r| r.id))?;

    let mut locations = Vec::new();
    if include_declaration && let Some(def) = templates::find_def(&id, root, source) {
        let uri = match &def.path {
            Some(path) => Uri::from_file_path(path)?,
            None => current_uri.clone(),
        };
        locations.push(Location {
            uri,
            range: range_to_lsp(&def.source, def.name_range),
        });
    }
    for template_ref in templates::all_refs(root, file_path) {
        if template_ref.id == id {
            locations.push(Location {
                uri: current_uri.clone(),
                range: range_to_lsp(source, template_ref.name_range),
            });
        }
    }
    Some(locations)
}

fn find_ident(
    token: &SyntaxToken,
    root: &Root,
    source: &str,
    include_declaration: bool,
    file_path: &Path,
    current_uri: &Uri,
) -> Option<Vec<Location>> {
    if token.kind() != SyntaxKind::Ident {
        return None;
    }
    let parent = token.parent()?;
    if parent.kind() == SyntaxKind::Name {
        let owner = parent.parent()?;
        return match owner.kind() {
            SyntaxKind::Job => {
                let stage = owner.ancestors().find_map(Stage::cast)?;
                let stage_name = stage.name()?;
                let job_name = token.text();
                let decl = include_declaration.then(|| token.clone());
                Some(job_refs(
                    root,
                    stage_name.as_str(),
                    job_name,
                    decl,
                    source,
                    current_uri,
                ))
            }
            SyntaxKind::UseDecl => Some(import_refs(
                token,
                root,
                source,
                include_declaration,
                file_path,
                current_uri,
            )),
            _ => None,
        };
    }
    if parent.kind() == SyntaxKind::Ref && is_dependency_ref(&parent) {
        let (first, second) = ref_idents_of(&parent);
        let first = first?;
        let second = second?;
        if second.text_range() != token.text_range() {
            return None; // cursor on stage ident, not job ident
        }
        let stage_name = first.text();
        let job_name = token.text();
        let decl = if include_declaration {
            root.stages()
                .find(|s| s.name().as_deref() == Some(stage_name))
                .and_then(|s| s.body())
                .and_then(|b| b.jobs().find(|j| j.name().as_deref() == Some(job_name)))
                .and_then(|j| j.name_token())
        } else {
            None
        };
        return Some(job_refs(
            root,
            stage_name,
            job_name,
            decl,
            source,
            current_uri,
        ));
    }
    None
}

/// Every `inherit` reference that goes through the import named at `token`.
fn import_refs(
    token: &SyntaxToken,
    root: &Root,
    source: &str,
    include_declaration: bool,
    file_path: &Path,
    current_uri: &Uri,
) -> Vec<Location> {
    let import_name = token.text();
    let mut locations = Vec::new();
    if include_declaration {
        locations.push(Location {
            uri: current_uri.clone(),
            range: range_to_lsp(source, token.text_range()),
        });
    }
    for template_ref in templates::all_refs(root, file_path) {
        if template_ref.import.as_deref() == Some(import_name)
            && let Some(range) = template_ref.import_range
        {
            locations.push(Location {
                uri: current_uri.clone(),
                range: range_to_lsp(source, range),
            });
        }
    }
    locations
}

// ─── cross-document helpers ───────────────────────────────────────────────────

/// Returns the template if `offset` lands on a `TemplateDef` name token.
pub(super) fn template_def_at(parse: &Parse, offset: usize) -> Option<TemplateId> {
    let token = token_at(&parse.syntax(), offset)?;
    templates::def_at(&token)
}

/// Search `doc` for every `inherit` reference that resolves to `target`,
/// which is defined in `defined_in`.
pub(super) fn cross_doc_template_refs(
    parse: &Parse,
    source: &str,
    file_path: &Path,
    uri: &Uri,
    target: &TemplateId,
    defined_in: &Path,
) -> Vec<Location> {
    let Some(root) = Root::cast(parse.syntax()) else {
        return Vec::new();
    };
    let Ok(defined_in) = defined_in.canonicalize() else {
        return Vec::new();
    };

    templates::all_refs(&root, file_path)
        .into_iter()
        .filter(|template_ref| {
            let TemplateFile::Imported(path) = &template_ref.id.file else {
                return false;
            };
            path.canonicalize().is_ok_and(|path| path == defined_in)
                && template_ref.id.stage == target.stage
                && template_ref.id.name == target.name
        })
        .map(|template_ref| Location {
            uri: uri.clone(),
            range: range_to_lsp(source, template_ref.name_range),
        })
        .collect()
}

// ─── location helpers ─────────────────────────────────────────────────────────

fn job_refs(
    root: &Root,
    stage_name: &str,
    job_name: &str,
    decl_token: Option<SyntaxToken>,
    source: &str,
    current_uri: &Uri,
) -> Vec<Location> {
    let mut locs = Vec::new();
    if let Some(tok) = decl_token {
        locs.push(Location {
            uri: current_uri.clone(),
            range: range_to_lsp(source, tok.text_range()),
        });
    }
    for tok in dep_ref_job_tokens(root, stage_name, job_name) {
        locs.push(Location {
            uri: current_uri.clone(),
            range: range_to_lsp(source, tok.text_range()),
        });
    }
    locs
}

// ─── token collectors ─────────────────────────────────────────────────────────

fn dep_ref_job_tokens(root: &Root, stage_name: &str, job_name: &str) -> Vec<SyntaxToken> {
    let mut tokens = Vec::new();
    for stage in root.stages() {
        if let Some(al) = stage.attr_list() {
            collect_matching_dep_tokens(&al, stage_name, job_name, &mut tokens);
        }
        let Some(body) = stage.body() else {
            continue;
        };
        for job in body.jobs() {
            let Some(al) = job.attr_list() else {
                continue;
            };
            collect_matching_dep_tokens(&al, stage_name, job_name, &mut tokens);
        }
    }
    tokens
}

fn collect_matching_dep_tokens(
    al: &ciane::ast::AttrList,
    stage_name: &str,
    job_name: &str,
    tokens: &mut Vec<SyntaxToken>,
) {
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
            if let Some(f) = first
                && f.text() == stage_name
                && let Some(s) = second
                && s.text() == job_name
            {
                tokens.push(s);
            }
        }
    }
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

fn ref_idents_of(ref_node: &SyntaxNode) -> (Option<SyntaxToken>, Option<SyntaxToken>) {
    let mut idents = ref_node
        .children_with_tokens()
        .filter_map(|e| e.into_token().filter(|t| t.kind() == SyntaxKind::Ident));
    (idents.next(), idents.next())
}
