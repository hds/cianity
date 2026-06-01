use ciane::{
    ast::{AstNode, Attr, AttrValue},
    parser::Parse,
    syntax::{SyntaxKind, SyntaxToken},
};
use tower_lsp_server::ls_types::{Hover, HoverContents, MarkupContent, MarkupKind};

use super::util::{range_to_lsp, token_at};

#[must_use]
pub(super) fn at(parse: &Parse, source: &str, offset: usize) -> Option<Hover> {
    let root = parse.syntax();
    let token = token_at(&root, offset)?;
    let content = hover_content(&token)?;
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: content,
        }),
        range: Some(range_to_lsp(source, token.text_range())),
    })
}

fn hover_content(token: &SyntaxToken) -> Option<String> {
    match token.kind() {
        SyntaxKind::KwStage => Some("**stage** — groups jobs and templates".into()),
        SyntaxKind::KwJob => Some("**job** — a CI job that runs shell commands or steps".into()),
        SyntaxKind::KwStep => Some("**step** — a named shell command within a job".into()),
        SyntaxKind::KwSteps => {
            Some("**steps** — inherit all steps from the parent template".into())
        }
        SyntaxKind::KwTemplate => Some("**template** — a reusable sequence of steps".into()),
        SyntaxKind::KwUse => Some("**use** — imports external workflows".into()),
        SyntaxKind::KwWorkflow => Some("**workflow** — declares a CI workflow".into()),
        SyntaxKind::Ident => hover_for_ident(token),
        SyntaxKind::BareValue => hover_for_bare_value(token),
        _ => None,
    }
}

fn hover_for_ident(token: &SyntaxToken) -> Option<String> {
    let parent = token.parent()?;
    match parent.kind() {
        SyntaxKind::Name => {
            let owner = parent.parent()?;
            match owner.kind() {
                SyntaxKind::WorkflowDef => Some(format!("workflow `{}`", token.text())),
                SyntaxKind::Stage => Some(format!("stage `{}`", token.text())),
                SyntaxKind::Job => Some(format!("job `{}`", token.text())),
                SyntaxKind::TemplateDef => Some(format!("template `{}`", token.text())),
                SyntaxKind::Step => Some(format!("step `{}`", token.text())),
                SyntaxKind::UseDecl => Some(format!("import alias `{}`", token.text())),
                _ => None,
            }
        }
        SyntaxKind::Attr => attr_key_doc(token.text()).map(str::to_owned),
        _ => None,
    }
}

fn hover_for_bare_value(token: &SyntaxToken) -> Option<String> {
    let attr_value = AttrValue::cast(token.parent()?)?;
    let attr = Attr::cast(attr_value.syntax().parent()?)?;
    let key = attr.key_text()?;
    let value = token.text();
    match key.as_str() {
        "inherit" => Some(format!("inherits steps from template `{value}`")),
        "image" => Some(format!("runs in container image `{value}`")),
        "path" => Some(format!("imports workflow from `{value}`")),
        "strategy" => Some(format!("workflow strategy: `{value}`")),
        _ => None,
    }
}

fn attr_key_doc(key: &str) -> Option<&'static str> {
    match key {
        "inherit" => Some("**inherit** — template whose steps this job inherits"),
        "dependencies" => Some("**dependencies** — jobs that must complete before this one"),
        "image" => Some("**image** — Docker image to run the job in"),
        "path" => Some("**path** — path to the imported workflow file"),
        "strategy" => Some("**strategy** — when this workflow runs"),
        _ => None,
    }
}
