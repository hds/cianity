use std::collections::HashSet;

use smol_str::SmolStr;

use crate::{
    ast::{AstNode, HasAttrList, HasName, Root, Stage},
    error::{Diagnostic, Severity},
};

/// Collect all semantic diagnostics from a parsed `Root` node.
pub(super) fn check_root(root: &Root, diagnostics: &mut Vec<Diagnostic>) {
    check_duplicate_stage_names(root, diagnostics);
    check_duplicate_root_template_names(root, diagnostics);
    let root_template_names: HashSet<SmolStr> = root.templates().filter_map(|t| t.name()).collect();
    for stage in root.stages() {
        check_stage(&stage, &root_template_names, diagnostics);
    }
    for use_block in root.use_blocks() {
        for import in use_block.imports() {
            check_workflow_import_attrs(&import, diagnostics);
        }
    }
}

fn check_duplicate_stage_names(root: &Root, diagnostics: &mut Vec<Diagnostic>) {
    let mut seen: HashSet<SmolStr> = HashSet::new();
    for stage in root.stages() {
        if let Some(name) = stage.name()
            && !seen.insert(name.clone())
        {
            diagnostics.push(Diagnostic {
                severity: Severity::Error,
                message: format!("duplicate stage name `{name}`"),
                span: span_of(stage.syntax()),
            });
        }
    }
}

fn check_duplicate_root_template_names(root: &Root, diagnostics: &mut Vec<Diagnostic>) {
    let mut seen: HashSet<SmolStr> = HashSet::new();
    for tmpl in root.templates() {
        if let Some(name) = tmpl.name()
            && !seen.insert(name.clone())
        {
            diagnostics.push(Diagnostic {
                severity: Severity::Error,
                message: format!("duplicate top-level template name `{name}`"),
                span: span_of(tmpl.syntax()),
            });
        }
    }
}

fn check_stage(
    stage: &Stage,
    root_templates: &HashSet<SmolStr>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(body) = stage.body() else {
        return;
    };

    let stage_template_names: HashSet<SmolStr> =
        body.templates().filter_map(|t| t.name()).collect();

    // Both stage-local and root-level templates are in scope for `inherit`.
    let all_template_names: HashSet<SmolStr> = stage_template_names
        .iter()
        .chain(root_templates.iter())
        .cloned()
        .collect();

    let mut seen: HashSet<SmolStr> = HashSet::new();
    for job in body.jobs() {
        if let Some(name) = job.name()
            && !seen.insert(name.clone())
        {
            diagnostics.push(Diagnostic {
                severity: Severity::Error,
                message: format!("duplicate job name `{name}` in stage"),
                span: span_of(job.syntax()),
            });
        }
        check_job_steps(&job, &all_template_names, diagnostics);
    }
    for tmpl in body.templates() {
        if let Some(name) = tmpl.name()
            && !seen.insert(name.clone())
        {
            diagnostics.push(Diagnostic {
                severity: Severity::Error,
                message: format!("duplicate name `{name}` in stage"),
                span: span_of(tmpl.syntax()),
            });
        }
    }
}

fn check_job_steps(
    job: &crate::ast::Job,
    template_names: &HashSet<SmolStr>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(body) = job.steps_body() else {
        return;
    };

    let has_inherit = job.attr_list().is_some_and(|al| {
        al.attrs()
            .any(|a| a.key_text().as_deref() == Some("inherit"))
    });

    if has_inherit && let Some(al) = job.attr_list() {
        for attr in al.attrs() {
            if attr.key_text().as_deref() != Some("inherit") {
                continue;
            }
            if let Some(value) = attr.value_text()
                && !value.contains('/')
                && !template_names.contains(value.as_str())
            {
                diagnostics.push(Diagnostic {
                    severity: Severity::Warning,
                    message: format!(
                        "job inherits from `{value}`, but no template with that name \
                         is defined in this stage or at the top level"
                    ),
                    span: span_of(attr.syntax()),
                });
            }
        }
    }

    let steps_kw_count = body.steps_keywords().count();
    if steps_kw_count > 0
        && !has_inherit
        && let Some(kw) = body.steps_keywords().next()
    {
        diagnostics.push(Diagnostic {
            severity: Severity::Error,
            message: "`steps` can only be used in a job that has an `inherit` attribute".to_owned(),
            span: span_of(kw.syntax()),
        });
    }
}

fn check_workflow_import_attrs(
    import: &crate::ast::WorkflowImport,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if import.location().is_none() {
        diagnostics.push(Diagnostic {
            severity: Severity::Error,
            message: "`workflow` import is missing the `location` attribute".to_owned(),
            span: span_of(import.syntax()),
        });
    }
    if import.name().is_none() {
        diagnostics.push(Diagnostic {
            severity: Severity::Error,
            message: "`workflow` import is missing the `name` attribute".to_owned(),
            span: span_of(import.syntax()),
        });
    }
}

fn span_of(node: &crate::syntax::SyntaxNode) -> std::ops::Range<usize> {
    let range = node.text_range();
    usize::from(range.start())..usize::from(range.end())
}
