use ciane::{
    ast::{AstNode, Attr, AttrValue, HasAttrList, HasName, Root, Stage},
    parser::Parse,
    syntax::{SyntaxKind, SyntaxNode, SyntaxToken},
};
use tower_lsp_server::ls_types::Range;

use super::util::{range_to_lsp, token_at};

/// Returns all reference locations for the job or template at `offset`.
#[must_use]
pub(super) fn find(
    parse: &Parse,
    source: &str,
    offset: usize,
    include_declaration: bool,
) -> Option<Vec<Range>> {
    let root_node = parse.syntax();
    let token = token_at(&root_node, offset)?;
    let root = Root::cast(root_node)?;

    if token.kind() == SyntaxKind::Ident {
        let parent = token.parent()?;
        if parent.kind() == SyntaxKind::Name {
            let owner = parent.parent()?;
            return match owner.kind() {
                SyntaxKind::TemplateDef => {
                    let stage = owner.ancestors().find_map(Stage::cast)?;
                    let tmpl_name = token.text();
                    let decl = include_declaration.then(|| token.clone());
                    Some(template_refs(&stage, tmpl_name, decl, source))
                }
                SyntaxKind::Job => {
                    let stage = owner.ancestors().find_map(Stage::cast)?;
                    let stage_name = stage.name()?;
                    let job_name = token.text();
                    let decl = include_declaration.then(|| token.clone());
                    Some(job_refs(&root, stage_name.as_str(), job_name, decl, source))
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
            return Some(job_refs(&root, stage_name, job_name, decl, source));
        }
    }

    if token.kind() == SyntaxKind::BareValue {
        let attr_key = token
            .parent()
            .and_then(AttrValue::cast)
            .and_then(|av| Attr::cast(av.syntax().parent()?))
            .and_then(|a| a.key_text());
        if attr_key.as_deref() == Some("inherit") && !token.text().contains('/') {
            let tmpl_name = token.text();
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
            return Some(template_refs(&stage, tmpl_name, decl, source));
        }
    }

    None
}

fn template_refs(
    stage: &Stage,
    tmpl_name: &str,
    decl_token: Option<SyntaxToken>,
    source: &str,
) -> Vec<Range> {
    let mut ranges = Vec::new();
    if let Some(tok) = decl_token {
        ranges.push(range_to_lsp(source, tok.text_range()));
    }
    for tok in inherit_tokens_matching(stage, tmpl_name) {
        ranges.push(range_to_lsp(source, tok.text_range()));
    }
    ranges
}

fn job_refs(
    root: &Root,
    stage_name: &str,
    job_name: &str,
    decl_token: Option<SyntaxToken>,
    source: &str,
) -> Vec<Range> {
    let mut ranges = Vec::new();
    if let Some(tok) = decl_token {
        ranges.push(range_to_lsp(source, tok.text_range()));
    }
    for tok in dep_ref_job_tokens(root, stage_name, job_name) {
        ranges.push(range_to_lsp(source, tok.text_range()));
    }
    ranges
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
    }
    tokens
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
