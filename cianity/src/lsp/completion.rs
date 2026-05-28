use ciane::{
    ast::{AstNode, Attr, AttrValue, HasName, Root, Stage},
    parser::Parse,
    syntax::{SyntaxKind, SyntaxNode, SyntaxToken},
};
use tower_lsp_server::ls_types::{CompletionItem, CompletionItemKind};

use super::util::token_at;

#[must_use]
pub(super) fn at(parse: &Parse, _source: &str, offset: usize) -> Vec<CompletionItem> {
    let root_node = parse.syntax();
    let Some(token) = token_at(&root_node, offset) else {
        return Vec::new();
    };
    completion_for_token(&token, parse)
}

fn completion_for_token(token: &SyntaxToken, parse: &Parse) -> Vec<CompletionItem> {
    for node in std::iter::successors(token.parent(), SyntaxNode::parent) {
        match node.kind() {
            SyntaxKind::RefList => {
                return ref_list_completions(&node, parse);
            }
            SyntaxKind::AttrValue => {
                let key = attrvalue_key(&node);
                return value_completions(key.as_deref(), &node, parse);
            }
            SyntaxKind::Attr => {
                let Some(attr) = Attr::cast(node.clone()) else {
                    return Vec::new();
                };
                let on_key = attr
                    .key_token()
                    .is_some_and(|k| k.text_range() == token.text_range());
                if on_key || attr.value().is_none() {
                    // If cursor is past the `=`, complete the value
                    let past_eq = attr.key_token().is_some_and(|k| {
                        usize::from(k.text_range().end()) < usize::from(token.text_range().start())
                    });
                    if past_eq {
                        let key = attr.key_text().map(|s| s.to_string());
                        return value_completions(key.as_deref(), &node, parse);
                    }
                    let owner = node.parent().and_then(|p| p.parent()).map(|n| n.kind());
                    return attr_name_completions(owner);
                }
                let key = attr.key_text().map(|s| s.to_string());
                return value_completions(key.as_deref(), &node, parse);
            }
            SyntaxKind::AttrList => {
                let owner = node.parent().map(|n| n.kind());
                return attr_name_completions(owner);
            }
            SyntaxKind::Job
            | SyntaxKind::TemplateDef
            | SyntaxKind::Step
            | SyntaxKind::JobBodyInline
            | SyntaxKind::JobBodySteps => {
                return Vec::new();
            }
            SyntaxKind::StageBody => {
                return stage_body_keywords();
            }
            SyntaxKind::Root => {
                return toplevel_keywords();
            }
            _ => {}
        }
    }
    Vec::new()
}

fn ref_list_completions(ref_list_node: &SyntaxNode, parse: &Parse) -> Vec<CompletionItem> {
    let key = ref_list_node
        .parent()
        .and_then(AttrValue::cast)
        .and_then(|av| Attr::cast(av.syntax().parent()?))
        .and_then(|a| a.key_text());
    if key.as_deref() == Some("dependencies") {
        return dependency_completions(parse);
    }
    Vec::new()
}

fn attrvalue_key(attr_value_node: &SyntaxNode) -> Option<String> {
    Attr::cast(attr_value_node.parent()?)?
        .key_text()
        .map(|s| s.to_string())
}

fn value_completions(key: Option<&str>, node: &SyntaxNode, parse: &Parse) -> Vec<CompletionItem> {
    match key {
        Some("inherit") => inherit_completions(node, parse),
        _ => Vec::new(),
    }
}

fn inherit_completions(from_node: &SyntaxNode, parse: &Parse) -> Vec<CompletionItem> {
    let Some(stage) = from_node.ancestors().find_map(Stage::cast) else {
        return all_template_completions(parse);
    };
    let Some(body) = stage.body() else {
        return Vec::new();
    };
    body.templates()
        .filter_map(|t| t.name())
        .map(|n| template_item(n.as_str()))
        .collect()
}

fn all_template_completions(parse: &Parse) -> Vec<CompletionItem> {
    let Some(root) = Root::cast(parse.syntax()) else {
        return Vec::new();
    };
    let mut items = Vec::new();
    for stage in root.stages() {
        let Some(body) = stage.body() else {
            continue;
        };
        let names: Vec<_> = body.templates().filter_map(|t| t.name()).collect();
        for name in names {
            items.push(template_item(name.as_str()));
        }
    }
    items
}

fn dependency_completions(parse: &Parse) -> Vec<CompletionItem> {
    let Some(root) = Root::cast(parse.syntax()) else {
        return Vec::new();
    };
    let mut items = Vec::new();
    for stage in root.stages() {
        let Some(stage_name) = stage.name() else {
            continue;
        };
        let Some(body) = stage.body() else {
            continue;
        };
        let job_names: Vec<_> = body.jobs().filter_map(|j| j.name()).collect();
        for job_name in job_names {
            items.push(CompletionItem {
                label: format!("{stage_name}.{job_name}"),
                kind: Some(CompletionItemKind::FUNCTION),
                ..CompletionItem::default()
            });
        }
    }
    items
}

fn attr_name_completions(owner: Option<SyntaxKind>) -> Vec<CompletionItem> {
    let names: &[&str] = match owner {
        Some(SyntaxKind::Job) => &["inherit", "dependencies", "container"],
        Some(SyntaxKind::WorkflowImport) => &["location", "name"],
        _ => &[],
    };
    names.iter().map(|&n| field_item(n)).collect()
}

fn stage_body_keywords() -> Vec<CompletionItem> {
    vec![keyword_item("job"), keyword_item("template")]
}

fn toplevel_keywords() -> Vec<CompletionItem> {
    vec![keyword_item("stage"), keyword_item("use")]
}

fn keyword_item(label: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_owned(),
        kind: Some(CompletionItemKind::KEYWORD),
        ..CompletionItem::default()
    }
}

fn field_item(label: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_owned(),
        kind: Some(CompletionItemKind::FIELD),
        ..CompletionItem::default()
    }
}

fn template_item(label: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_owned(),
        kind: Some(CompletionItemKind::CLASS),
        ..CompletionItem::default()
    }
}
