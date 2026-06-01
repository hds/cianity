use crate::syntax::SyntaxKind;

use super::Parser;

/// Parse the root of a `ciane` file.
///
/// ```text
/// root = workflow_def*
/// ```
pub(super) fn root(p: &mut Parser<'_>) {
    p.start_root_node();

    while !p.at(SyntaxKind::Eof) {
        if p.at(SyntaxKind::KwWorkflow) {
            workflow_def(p);
        } else {
            p.error_bump("expected `workflow` or end of file");
        }
    }

    p.finish_node();
}

// ─── workflow def ─────────────────────────────────────────────────────────────

fn workflow_def(p: &mut Parser<'_>) {
    p.start_node(SyntaxKind::WorkflowDef);
    p.bump(); // `workflow`
    name(p);
    if p.at(SyntaxKind::LParen) {
        attr_list_body(p);
    }
    workflow_body(p);
    p.finish_node();
}

fn workflow_body(p: &mut Parser<'_>) {
    p.start_node(SyntaxKind::WorkflowBody);
    let braced = p.at(SyntaxKind::LBrace);
    if braced {
        p.bump(); // `{`
    }
    loop {
        if p.at(SyntaxKind::Eof) {
            break;
        }
        if braced && p.at(SyntaxKind::RBrace) {
            break;
        }
        match p.current() {
            SyntaxKind::KwUse => use_decl(p),
            SyntaxKind::KwStage => stage(p),
            SyntaxKind::KwTemplate => template_def(p),
            SyntaxKind::KwDefaults => {
                p.start_node(SyntaxKind::AttrList);
                p.bump(); // `defaults`
                if p.at(SyntaxKind::LParen) {
                    attr_list_body(p);
                }
                p.finish_node();
            }
            _ => p.error_bump("expected `use`, `stage`, `template`, or end of workflow"),
        }
    }
    if braced {
        p.expect(SyntaxKind::RBrace);
    }
    p.finish_node();
}

// ─── use decl ────────────────────────────────────────────────────────────────

fn use_decl(p: &mut Parser<'_>) {
    p.start_node(SyntaxKind::UseDecl);
    p.bump(); // `use`
    name(p);
    if p.at(SyntaxKind::LParen) {
        attr_list_body(p);
    }
    p.finish_node();
}

// ─── attributes ──────────────────────────────────────────────────────────────

fn attr_list_body(p: &mut Parser<'_>) {
    p.start_node(SyntaxKind::AttrList);
    p.expect(SyntaxKind::LParen);
    while !p.at(SyntaxKind::RParen) && !p.at(SyntaxKind::Eof) {
        if p.at(SyntaxKind::Ident) || p.at_keyword() {
            attr(p);
        } else if p.at(SyntaxKind::Comma) {
            p.bump();
        } else {
            p.error_bump("expected attribute name or `)`");
        }
    }
    p.expect(SyntaxKind::RParen);
    p.finish_node();
}

fn attr(p: &mut Parser<'_>) {
    p.start_node(SyntaxKind::Attr);
    p.bump(); // attribute name
    p.expect(SyntaxKind::Eq);
    attr_value(p);
    p.eat_optional(SyntaxKind::Comma);
    p.finish_node();
}

fn attr_value(p: &mut Parser<'_>) {
    p.start_node(SyntaxKind::AttrValue);
    if p.at(SyntaxKind::LBracket) {
        ref_list(p);
    } else if p.at(SyntaxKind::BareValue) {
        p.bump();
    } else {
        p.error("expected attribute value");
    }
    p.finish_node();
}

// ─── references ──────────────────────────────────────────────────────────────

fn ref_list(p: &mut Parser<'_>) {
    p.start_node(SyntaxKind::RefList);
    p.expect(SyntaxKind::LBracket);
    while !p.at(SyntaxKind::RBracket) && !p.at(SyntaxKind::Eof) {
        if p.at(SyntaxKind::Ident) {
            reference(p);
            p.eat_optional(SyntaxKind::Comma);
        } else if p.at(SyntaxKind::Comma) {
            p.bump();
        } else {
            p.error_bump("expected identifier in reference list");
        }
    }
    p.expect(SyntaxKind::RBracket);
    p.finish_node();
}

fn reference(p: &mut Parser<'_>) {
    p.start_node(SyntaxKind::Ref);
    p.expect(SyntaxKind::Ident);
    loop {
        if p.at(SyntaxKind::Dot) || p.at(SyntaxKind::Slash) {
            p.bump(); // `.` or `/`
            if p.at(SyntaxKind::Ident) {
                p.bump();
            } else {
                p.error("expected identifier after `.` or `/`");
                break;
            }
        } else {
            break;
        }
    }
    p.finish_node();
}

// ─── stage ───────────────────────────────────────────────────────────────────

fn stage(p: &mut Parser<'_>) {
    p.start_node(SyntaxKind::Stage);
    p.bump(); // `stage`
    name(p);
    if p.at(SyntaxKind::LParen) {
        attr_list_body(p);
    }
    stage_body(p);
    p.finish_node();
}

fn stage_body(p: &mut Parser<'_>) {
    p.start_node(SyntaxKind::StageBody);
    p.expect(SyntaxKind::LBrace);
    while !p.at(SyntaxKind::RBrace) && !p.at(SyntaxKind::Eof) {
        match p.current() {
            SyntaxKind::KwJob => job(p),
            SyntaxKind::KwTemplate => template_def(p),
            _ => p.error_bump("expected `job` or `template`"),
        }
    }
    p.expect(SyntaxKind::RBrace);
    p.finish_node();
}

// ─── job / template ──────────────────────────────────────────────────────────

fn job(p: &mut Parser<'_>) {
    p.start_node(SyntaxKind::Job);
    p.bump(); // `job`
    name(p);
    if p.at(SyntaxKind::LParen) {
        attr_list_body(p);
    }
    match p.current() {
        SyntaxKind::LBrace => job_body_inline(p),
        SyntaxKind::LBracket => job_body_steps(p),
        _ => p.error_bump("expected `{` or `[` for job body"),
    }
    p.finish_node();
}

fn template_def(p: &mut Parser<'_>) {
    p.start_node(SyntaxKind::TemplateDef);
    p.bump(); // `template`
    name(p);
    if p.at(SyntaxKind::LParen) {
        attr_list_body(p);
    }
    if p.at(SyntaxKind::LBracket) {
        job_body_steps(p);
    }
    p.finish_node();
}

fn job_body_inline(p: &mut Parser<'_>) {
    p.start_node(SyntaxKind::JobBodyInline);
    p.signal_shell();
    p.bump(); // `{` — lexer transitions to shell mode immediately after
    if p.at(SyntaxKind::ShellBody) {
        p.bump();
    }
    p.expect(SyntaxKind::RBrace);
    p.finish_node();
}

fn job_body_steps(p: &mut Parser<'_>) {
    p.start_node(SyntaxKind::JobBodySteps);
    p.expect(SyntaxKind::LBracket);
    while !p.at(SyntaxKind::RBracket) && !p.at(SyntaxKind::Eof) {
        match p.current() {
            SyntaxKind::KwSteps => steps_keyword(p),
            SyntaxKind::KwStep => step_item(p),
            SyntaxKind::Comma => {
                p.bump();
            }
            _ => p.error_bump("expected `step`, `steps`, or `]`"),
        }
    }
    p.expect(SyntaxKind::RBracket);
    p.finish_node();
}

// ─── step items ──────────────────────────────────────────────────────────────

/// Parse a step item inside a step list.
///
/// This covers both a full step (`step name { body }`) and a bare step
/// reference (`step name ,` or `step name`).  Both are emitted as a `Step`
/// node; the absence of a `JobBodyInline` child distinguishes the reference
/// form at the AST level.
fn step_item(p: &mut Parser<'_>) {
    p.start_node(SyntaxKind::Step);
    p.bump(); // `step`
    name(p);
    match p.current() {
        SyntaxKind::LBrace => {
            p.signal_shell();
            p.bump(); // `{`
            if p.at(SyntaxKind::ShellBody) {
                p.bump();
            }
            p.expect(SyntaxKind::RBrace);
        }
        SyntaxKind::Comma => {
            // Bare step reference: `step name,`
            p.bump();
        }
        SyntaxKind::RBracket
        | SyntaxKind::KwStep
        | SyntaxKind::KwSteps
        | SyntaxKind::KwTemplate => {
            // Bare step reference without trailing comma (last in list).
        }
        _ => {
            p.error("expected `{` for step body or `,` for step reference");
        }
    }
    p.finish_node();
}

fn steps_keyword(p: &mut Parser<'_>) {
    p.start_node(SyntaxKind::StepsKeyword);
    p.bump(); // `steps`
    p.eat_optional(SyntaxKind::Comma);
    p.finish_node();
}

// ─── helpers ─────────────────────────────────────────────────────────────────

fn name(p: &mut Parser<'_>) {
    p.start_node(SyntaxKind::Name);
    if p.at(SyntaxKind::Ident) {
        p.bump();
    } else {
        p.error("expected identifier for name");
    }
    p.finish_node();
}
