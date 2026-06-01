use crate::{
    ast::{
        AstNode, Attr, AttrList, AttrValue, HasAttrList, HasName, Job, JobBodyInline, JobBodySteps,
        Ref, RefList, Root, Stage, StageBody, Step, StepsKeyword, TemplateDef, UseDecl,
        WorkflowBody, WorkflowDef,
    },
    syntax::SyntaxKind,
};

use super::FormatError;

pub(super) struct Printer {
    output: String,
    indent: usize,
}

const INDENT: &str = "    ";

impl Printer {
    pub(super) fn new() -> Self {
        Self {
            output: String::new(),
            indent: 0,
        }
    }

    pub(super) fn print_root(mut self, root: &Root) -> Result<String, FormatError> {
        let mut had_output = false;

        for child in root.syntax().children() {
            if let Some(wdef) = WorkflowDef::cast(child) {
                if had_output {
                    self.push_str("\n\n");
                }
                self.print_workflow_def(&wdef)?;
                had_output = true;
            }
        }

        self.push('\n');
        Ok(self.output)
    }

    // ── workflow def ─────────────────────────────────────────────────────────

    fn print_workflow_def(&mut self, wdef: &WorkflowDef) -> Result<(), FormatError> {
        self.push_str("workflow ");
        self.push_str(wdef.name().as_deref().unwrap_or(""));
        if let Some(attrs) = wdef.attr_list() {
            self.print_attr_list(&attrs);
        }
        let Some(body) = wdef.body() else {
            return Ok(());
        };
        if body
            .syntax()
            .children()
            .any(|n| n.kind() == SyntaxKind::AttrList)
        {
            return Err(FormatError::DefaultsBlockUnsupported);
        }
        if body.is_braced() {
            self.push_str(" {");
            self.print_workflow_body_braced(&body);
            self.push('\n');
            self.push_indent();
            self.push_str("}");
        } else {
            self.print_workflow_body_unbraced(&body);
        }
        Ok(())
    }

    fn print_workflow_body_braced(&mut self, body: &WorkflowBody) {
        self.indent += 1;
        let mut first = true;
        for child in body.syntax().children() {
            match child.kind() {
                SyntaxKind::UseDecl => {
                    if let Some(use_decl) = UseDecl::cast(child) {
                        if first {
                            first = false;
                        } else {
                            self.push('\n');
                        }
                        self.push('\n');
                        self.push_indent();
                        self.print_use_decl(&use_decl);
                    }
                }
                SyntaxKind::Stage => {
                    if let Some(stage) = Stage::cast(child) {
                        if first {
                            first = false;
                        } else {
                            self.push('\n');
                        }
                        self.push('\n');
                        self.push_indent();
                        self.print_stage(&stage);
                    }
                }
                SyntaxKind::TemplateDef => {
                    if let Some(tmpl) = TemplateDef::cast(child) {
                        if first {
                            first = false;
                        } else {
                            self.push('\n');
                        }
                        self.push('\n');
                        self.push_indent();
                        self.print_template(&tmpl);
                    }
                }
                _ => {}
            }
        }
        self.indent -= 1;
    }

    fn print_workflow_body_unbraced(&mut self, body: &WorkflowBody) {
        for child in body.syntax().children() {
            match child.kind() {
                SyntaxKind::UseDecl => {
                    if let Some(use_decl) = UseDecl::cast(child) {
                        self.push_str("\n\n");
                        self.print_use_decl(&use_decl);
                    }
                }
                SyntaxKind::Stage => {
                    if let Some(stage) = Stage::cast(child) {
                        self.push_str("\n\n");
                        self.print_stage(&stage);
                    }
                }
                SyntaxKind::TemplateDef => {
                    if let Some(tmpl) = TemplateDef::cast(child) {
                        self.push_str("\n\n");
                        self.print_template(&tmpl);
                    }
                }
                _ => {}
            }
        }
    }

    // ── use decl ─────────────────────────────────────────────────────────────

    fn print_use_decl(&mut self, decl: &UseDecl) {
        self.push_str("use ");
        self.push_str(decl.name().as_deref().unwrap_or(""));
        if let Some(attrs) = decl.attr_list() {
            self.print_attr_list(&attrs);
        }
    }

    // ── stage ─────────────────────────────────────────────────────────────────

    fn print_stage(&mut self, stage: &Stage) {
        self.push_str("stage ");
        self.push_str(stage.name().as_deref().unwrap_or(""));
        if let Some(attrs) = stage.attr_list() {
            self.print_attr_list(&attrs);
        }
        self.push_str(" {");
        if let Some(body) = stage.body() {
            self.print_stage_body(&body);
        }
        self.push('\n');
        self.push_indent();
        self.push_str("}");
    }

    fn print_stage_body(&mut self, body: &StageBody) {
        self.indent += 1;
        let mut first = true;
        for child in body.syntax().children() {
            match child.kind() {
                SyntaxKind::Job => {
                    if let Some(job) = Job::cast(child) {
                        if first {
                            first = false;
                        } else {
                            self.push('\n');
                        }
                        self.push('\n');
                        self.push_indent();
                        self.print_job(&job);
                    }
                }
                SyntaxKind::TemplateDef => {
                    if let Some(tmpl) = TemplateDef::cast(child) {
                        if first {
                            first = false;
                        } else {
                            self.push('\n');
                        }
                        self.push('\n');
                        self.push_indent();
                        self.print_template(&tmpl);
                    }
                }
                _ => {}
            }
        }
        self.indent -= 1;
    }

    // ── job / template ───────────────────────────────────────────────────────

    fn print_job(&mut self, job: &Job) {
        self.push_str("job ");
        self.push_str(job.name().as_deref().unwrap_or(""));
        if let Some(attrs) = job.attr_list() {
            self.print_attr_list(&attrs);
        }
        if let Some(body) = job.inline_body() {
            self.push(' ');
            self.print_job_body_inline(&body);
        } else if let Some(body) = job.steps_body() {
            self.push(' ');
            self.print_job_body_steps(&body);
        }
    }

    fn print_template(&mut self, tmpl: &TemplateDef) {
        self.push_str("template ");
        self.push_str(tmpl.name().as_deref().unwrap_or(""));
        if let Some(attrs) = tmpl.attr_list() {
            self.print_attr_list(&attrs);
        }
        if let Some(body) = tmpl.body() {
            self.push(' ');
            self.print_job_body_steps(&body);
        }
    }

    // ── job bodies ───────────────────────────────────────────────────────────

    fn print_job_body_inline(&mut self, body: &JobBodyInline) {
        let shell = body.shell_text().unwrap_or_default();
        self.print_brace_body(&shell);
    }

    fn print_job_body_steps(&mut self, body: &JobBodySteps) {
        self.push('[');
        self.indent += 1;
        for child in body.syntax().children() {
            match child.kind() {
                SyntaxKind::Step => {
                    if let Some(step) = Step::cast(child) {
                        self.push('\n');
                        self.push_indent();
                        self.print_step(&step);
                    }
                }
                SyntaxKind::StepsKeyword => {
                    if let Some(kw) = StepsKeyword::cast(child) {
                        self.push('\n');
                        self.push_indent();
                        self.print_steps_keyword(&kw);
                    }
                }
                _ => {}
            }
        }
        self.indent -= 1;
        self.push('\n');
        self.push_indent();
        self.push(']');
    }

    // ── steps ────────────────────────────────────────────────────────────────

    fn print_step(&mut self, step: &Step) {
        self.push_str("step ");
        self.push_str(step.name().as_deref().unwrap_or(""));
        if step.has_body() {
            let shell = step.shell_text().unwrap_or_default();
            self.push(' ');
            self.print_brace_body(&shell);
        } else {
            self.push(',');
        }
    }

    /// Emit a `{ ... }` shell body, choosing single- or multi-line layout.
    ///
    /// Single-line (`{ cmd }`) when the body has at most one non-empty command.
    /// Multi-line (`{\n    cmd\n    cmd\n}`) when there are two or more.
    fn print_brace_body(&mut self, shell: &str) {
        let lines: Vec<&str> = shell
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();

        if lines.is_empty() {
            self.push_str("{}");
        } else if lines.len() == 1 {
            self.push_str("{ ");
            self.push_str(lines[0]);
            self.push_str(" }");
        } else {
            self.push('{');
            self.indent += 1;
            for line in &lines {
                self.push('\n');
                self.push_indent();
                self.push_str(line);
            }
            self.indent -= 1;
            self.push('\n');
            self.push_indent();
            self.push('}');
        }
    }

    fn print_steps_keyword(&mut self, _kw: &StepsKeyword) {
        self.push_str("steps,");
    }

    // ── attribute lists ──────────────────────────────────────────────────────

    /// Chooses inline or multiline layout based on attribute count.
    fn print_attr_list(&mut self, attrs: &AttrList) {
        if attrs.attrs().count() == 1 {
            self.print_attr_list_inline(attrs);
        } else {
            self.print_attr_list_multiline(attrs);
        }
    }

    fn print_attr_list_inline(&mut self, attrs: &AttrList) {
        self.push_str(" ( ");
        let mut first = true;
        for attr in attrs.attrs() {
            if first {
                first = false;
            } else {
                self.push_str(", ");
            }
            self.print_attr(&attr);
        }
        self.push_str(" )");
    }

    fn print_attr_list_multiline(&mut self, attrs: &AttrList) {
        self.push_str(" (");
        self.indent += 1;
        for attr in attrs.attrs() {
            self.push('\n');
            self.push_indent();
            self.print_attr(&attr);
            self.push(',');
        }
        self.indent -= 1;
        self.push('\n');
        self.push_indent();
        self.push(')');
    }

    fn print_attr(&mut self, attr: &Attr) {
        self.push_str(attr.key_text().as_deref().unwrap_or(""));
        self.push_str(" = ");
        if let Some(value) = attr.value() {
            self.print_attr_value(&value);
        }
    }

    fn print_attr_value(&mut self, value: &AttrValue) {
        if let Some(text) = value.bare_text() {
            self.push_str(&text);
        } else if let Some(ref_list) = value.ref_list() {
            self.print_ref_list(&ref_list);
        }
    }

    fn print_ref_list(&mut self, list: &RefList) {
        self.push('[');
        let mut first = true;
        for r in list.refs() {
            if first {
                first = false;
            } else {
                self.push_str(", ");
            }
            self.print_ref(&r);
        }
        self.push(']');
    }

    fn print_ref(&mut self, r: &Ref) {
        self.push_str(&r.text());
    }

    // ── output helpers ───────────────────────────────────────────────────────

    fn push(&mut self, c: char) {
        self.output.push(c);
    }

    fn push_str(&mut self, s: &str) {
        self.output.push_str(s);
    }

    fn push_indent(&mut self) {
        for _ in 0..self.indent {
            self.output.push_str(INDENT);
        }
    }
}
