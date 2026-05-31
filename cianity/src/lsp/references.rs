use std::path::Path;

use ciane::{
    ast::{AstNode, Attr, AttrValue, HasAttrList, HasName, Root, Stage, WorkflowImport},
    parse,
    parser::Parse,
    syntax::{SyntaxKind, SyntaxNode, SyntaxToken},
};
use tower_lsp_server::ls_types::{Location, Uri};

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
    match token.kind() {
        SyntaxKind::Ident => find_ident(&token, &root, source, include_declaration, current_uri),
        SyntaxKind::BareValue => find_bare_value(
            &token,
            &root,
            source,
            include_declaration,
            file_path,
            current_uri,
        ),
        _ => None,
    }
}

fn find_ident(
    token: &SyntaxToken,
    root: &Root,
    source: &str,
    include_declaration: bool,
    current_uri: &Uri,
) -> Option<Vec<Location>> {
    let parent = token.parent()?;
    if parent.kind() == SyntaxKind::Name {
        let owner = parent.parent()?;
        return match owner.kind() {
            SyntaxKind::TemplateDef => {
                let stage = owner.ancestors().find_map(Stage::cast)?;
                let tmpl_name = token.text();
                let decl = include_declaration.then(|| token.clone());
                Some(template_refs(&stage, tmpl_name, decl, source, current_uri))
            }
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

fn find_bare_value(
    token: &SyntaxToken,
    root: &Root,
    source: &str,
    include_declaration: bool,
    file_path: &Path,
    current_uri: &Uri,
) -> Option<Vec<Location>> {
    let attr_key = token
        .parent()
        .and_then(AttrValue::cast)
        .and_then(|av| Attr::cast(av.syntax().parent()?))
        .and_then(|a| a.key_text());
    if attr_key.as_deref() == Some("inherit") {
        let raw = token.text();
        if let Some((import_name, template_name)) = raw.split_once('/') {
            let mut locations = Vec::new();
            if include_declaration
                && let Some(loc) =
                    cross_file_template_location(root, import_name, template_name, file_path)
            {
                locations.push(loc);
            }
            for tok in inherit_tokens_matching_root(root, raw) {
                locations.push(Location {
                    uri: current_uri.clone(),
                    range: range_to_lsp(source, tok.text_range()),
                });
            }
            return Some(locations);
        }
        let tmpl_name = raw;
        let stage = token.parent()?.ancestors().find_map(Stage::cast)?;
        let decl = if include_declaration {
            stage
                .body()
                .and_then(|b| {
                    b.templates()
                        .find(|t| t.name().as_deref() == Some(tmpl_name))
                })
                .and_then(|t| t.name_token())
        } else {
            None
        };
        return Some(template_refs(&stage, tmpl_name, decl, source, current_uri));
    }

    // `name` attr value inside a WorkflowImport: find all `inherit = importname/...` usages.
    let is_use_import_name = token
        .parent()
        .and_then(AttrValue::cast)
        .and_then(|av| Attr::cast(av.syntax().parent()?))
        .filter(|a| a.key_text().as_deref() == Some("name"))
        .and_then(|a| WorkflowImport::cast(a.syntax().parent()?.parent()?))
        .is_some();
    if is_use_import_name {
        let import_name = token.text();
        let prefix = format!("{import_name}/");
        let mut locations = Vec::new();
        if include_declaration {
            locations.push(Location {
                uri: current_uri.clone(),
                range: range_to_lsp(source, token.text_range()),
            });
        }
        for tok in inherit_tokens_with_ns_prefix(root, &prefix) {
            locations.push(Location {
                uri: current_uri.clone(),
                range: range_to_lsp(source, tok.text_range()),
            });
        }
        return Some(locations);
    }

    None
}

// ─── cross-document helpers ───────────────────────────────────────────────────

/// Returns the template name if `offset` lands on a `TemplateDef` name token.
pub(super) fn template_def_at(parse: &Parse, offset: usize) -> Option<String> {
    let root_node = parse.syntax();
    let token = token_at(&root_node, offset)?;
    if token.kind() != SyntaxKind::Ident {
        return None;
    }
    let parent = token.parent()?;
    if parent.kind() != SyntaxKind::Name {
        return None;
    }
    if parent.parent()?.kind() != SyntaxKind::TemplateDef {
        return None;
    }
    Some(token.text().to_string())
}

/// Search `doc` for all `inherit = X/template_name` tokens where import `X`
/// resolves (via its `location` attribute) to `defined_in`.
pub(super) fn cross_doc_template_refs(
    parse: &Parse,
    source: &str,
    file_path: &Path,
    uri: &Uri,
    template_name: &str,
    defined_in: &Path,
) -> Vec<Location> {
    let root_node = parse.syntax();
    let Some(root) = Root::cast(root_node) else {
        return Vec::new();
    };
    let base = file_path.parent().unwrap_or(Path::new("."));
    let Ok(defined_in_canon) = defined_in.canonicalize() else {
        return Vec::new();
    };

    // Collect all import names in this document whose location resolves to `defined_in`.
    let import_names: Vec<String> = root
        .use_blocks()
        .flat_map(|ub| ub.imports().collect::<Vec<_>>())
        .filter_map(|imp| {
            let loc = imp.location()?;
            let canon = base.join(loc.as_str()).canonicalize().ok()?;
            if canon == defined_in_canon {
                Some(imp.name()?.to_string())
            } else {
                None
            }
        })
        .collect();

    if import_names.is_empty() {
        return Vec::new();
    }

    let mut locs = Vec::new();
    for import_name in import_names {
        let full_ref = format!("{import_name}/{template_name}");
        for tok in inherit_tokens_matching_root(&root, &full_ref) {
            locs.push(Location {
                uri: uri.clone(),
                range: range_to_lsp(source, tok.text_range()),
            });
        }
    }
    locs
}

// ─── location helpers ─────────────────────────────────────────────────────────

fn template_refs(
    stage: &Stage,
    tmpl_name: &str,
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
    for tok in inherit_tokens_matching(stage, tmpl_name) {
        locs.push(Location {
            uri: current_uri.clone(),
            range: range_to_lsp(source, tok.text_range()),
        });
    }
    locs
}

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

// ─── cross-file resolution ───────────────────────────────────────────────────

fn cross_file_template_location(
    root: &Root,
    import_name: &str,
    template_name: &str,
    file_path: &Path,
) -> Option<Location> {
    let base = file_path.parent().unwrap_or(Path::new("."));
    let import_path = {
        let mut found = None;
        'search: for ub in root.use_blocks() {
            for imp in ub.imports() {
                if imp.name().as_deref() == Some(import_name)
                    && let Some(loc) = imp.location()
                {
                    found = Some(base.join(loc.as_str()));
                    break 'search;
                }
            }
        }
        found?
    };
    let target_source = std::fs::read_to_string(&import_path).ok()?;
    let target_parse = parse(&target_source);
    let target_root = Root::cast(target_parse.syntax())?;
    let tmpl = target_root.stages().find_map(|s| {
        s.body()?
            .templates()
            .find(|t| t.name().as_deref() == Some(template_name))
    })?;
    let range = range_to_lsp(&target_source, tmpl.name_token()?.text_range());
    let uri = Uri::from_file_path(&import_path)?;
    Some(Location { uri, range })
}

// ─── token collectors ─────────────────────────────────────────────────────────

fn inherit_tokens_matching_root(root: &Root, tmpl_name: &str) -> Vec<SyntaxToken> {
    root.stages()
        .flat_map(|s| inherit_tokens_matching(&s, tmpl_name))
        .collect()
}

fn inherit_tokens_with_ns_prefix(root: &Root, prefix: &str) -> Vec<SyntaxToken> {
    let mut tokens = Vec::new();
    for stage in root.stages() {
        let Some(body) = stage.body() else {
            continue;
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
                    && tok.text().starts_with(prefix)
                {
                    tokens.push(tok);
                }
            }
        }
    }
    tokens
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
