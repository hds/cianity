use std::collections::HashSet;

use smol_str::SmolStr;

use crate::{
    ast::{
        AstNode, Attr, HasAttrList, HasName, Root, Stage, TemplateDef, UseDecl, WorkflowBody,
        WorkflowDef,
    },
    error::{Diagnostic, Severity},
};

const VALID_STRATEGIES: &[&str] = &[
    "default_branch_and_reviews",
    "default_branch",
    "reviews",
    "none",
];

const VALID_WORKFLOW_ATTRS: &[&str] = &["strategy"];
const VALID_STAGE_ATTRS: &[&str] = &["dependencies"];
const VALID_JOB_ATTRS: &[&str] = &["image", "inherit", "dependencies"];
const VALID_TEMPLATE_ATTRS: &[&str] = &["image", "inherit", "dependencies"];
const VALID_USE_ATTRS: &[&str] = &["path"];

fn check_unknown_attrs<N: HasAttrList>(
    node: &N,
    owner: &str,
    valid_keys: &[&str],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(al) = node.attr_list() else {
        return;
    };
    for attr in al.attrs() {
        let Some(key) = attr.key_text() else {
            continue;
        };
        if !valid_keys.contains(&key.as_str()) {
            let listed = valid_keys
                .iter()
                .map(|k| format!("`{k}`"))
                .collect::<Vec<_>>()
                .join(", ");
            diagnostics.push(Diagnostic {
                severity: Severity::Error,
                message: format!(
                    "unknown attribute `{key}` on {owner}; valid attributes are {listed}"
                ),
                span: span_of(attr.syntax()),
            });
        }
    }
}

fn inherit_names_from_attr(attr: &Attr) -> Vec<SmolStr> {
    attr.value_text().map_or_else(
        || {
            attr.value()
                .and_then(|av| av.ref_list())
                .map(|rl| rl.refs().map(|r| SmolStr::new(r.text())).collect())
                .unwrap_or_default()
        },
        |v| vec![v],
    )
}

/// Collect all semantic diagnostics from a parsed `Root` node.
pub(super) fn check_root(root: &Root, diagnostics: &mut Vec<Diagnostic>) {
    for workflow in root.workflow_defs() {
        check_workflow_def(&workflow, diagnostics);
    }
    for use_decl in root.use_decls() {
        check_use_decl_attrs(&use_decl, diagnostics);
    }
}

fn check_workflow_def(workflow: &WorkflowDef, diagnostics: &mut Vec<Diagnostic>) {
    check_unknown_attrs(workflow, "workflow", VALID_WORKFLOW_ATTRS, diagnostics);
    check_workflow_strategy(workflow, diagnostics);
    let Some(body) = workflow.body() else {
        return;
    };
    check_duplicate_workflow_stage_names(&body, diagnostics);
    check_duplicate_workflow_template_names(&body, diagnostics);
    let root_template_names: HashSet<SmolStr> = body.templates().filter_map(|t| t.name()).collect();
    for tmpl in body.templates() {
        check_unknown_attrs(&tmpl, "template", VALID_TEMPLATE_ATTRS, diagnostics);
        check_template_inherit(&tmpl, &root_template_names, diagnostics);
    }
    for stage in body.stages() {
        check_stage(&stage, &root_template_names, diagnostics);
    }
}

fn check_workflow_strategy(workflow: &WorkflowDef, diagnostics: &mut Vec<Diagnostic>) {
    let Some(al) = workflow.attr_list() else {
        return;
    };
    for attr in al.attrs() {
        if attr.key_text().as_deref() != Some("strategy") {
            continue;
        }
        if let Some(value) = attr.value_text()
            && !VALID_STRATEGIES.contains(&value.as_str())
        {
            diagnostics.push(Diagnostic {
                severity: Severity::Error,
                message: format!(
                    "invalid strategy `{value}`; expected one of \
                     `default_branch_and_reviews`, `default_branch`, `reviews`, or `none`"
                ),
                span: span_of(attr.syntax()),
            });
        }
    }
}

fn check_duplicate_workflow_stage_names(body: &WorkflowBody, diagnostics: &mut Vec<Diagnostic>) {
    let mut seen: HashSet<SmolStr> = HashSet::new();
    for stage in body.stages() {
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

fn check_duplicate_workflow_template_names(body: &WorkflowBody, diagnostics: &mut Vec<Diagnostic>) {
    let mut seen: HashSet<SmolStr> = HashSet::new();
    for tmpl in body.templates() {
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
    check_unknown_attrs(stage, "stage", VALID_STAGE_ATTRS, diagnostics);

    let Some(body) = stage.body() else {
        return;
    };

    let stage_template_names: HashSet<SmolStr> =
        body.templates().filter_map(|t| t.name()).collect();

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
        check_unknown_attrs(&job, "job", VALID_JOB_ATTRS, diagnostics);
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
        check_unknown_attrs(&tmpl, "template", VALID_TEMPLATE_ATTRS, diagnostics);
        check_template_inherit(&tmpl, &all_template_names, diagnostics);
    }
}

fn check_template_inherit(
    tmpl: &TemplateDef,
    template_names: &HashSet<SmolStr>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(al) = tmpl.attr_list() else { return };
    for attr in al.attrs() {
        if attr.key_text().as_deref() != Some("inherit") {
            continue;
        }
        for name in inherit_names_from_attr(&attr) {
            if !name.contains('/') && !template_names.contains(&name) {
                diagnostics.push(Diagnostic {
                    severity: Severity::Warning,
                    message: format!(
                        "template inherits from `{name}`, but no template with that name \
                         is defined in this scope"
                    ),
                    span: span_of(attr.syntax()),
                });
            }
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
            for name in inherit_names_from_attr(&attr) {
                if !name.contains('/') && !template_names.contains(&name) {
                    diagnostics.push(Diagnostic {
                        severity: Severity::Warning,
                        message: format!(
                            "job inherits from `{name}`, but no template with that name \
                             is defined in this stage or at the top level"
                        ),
                        span: span_of(attr.syntax()),
                    });
                }
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

fn check_use_decl_attrs(decl: &UseDecl, diagnostics: &mut Vec<Diagnostic>) {
    check_unknown_attrs(decl, "use import", VALID_USE_ATTRS, diagnostics);
    if decl.path().is_none() {
        diagnostics.push(Diagnostic {
            severity: Severity::Error,
            message: "`use` import is missing the `path` attribute".to_owned(),
            span: span_of(decl.syntax()),
        });
    }
}

fn span_of(node: &crate::syntax::SyntaxNode) -> std::ops::Range<usize> {
    let range = node.text_range();
    usize::from(range.start())..usize::from(range.end())
}
