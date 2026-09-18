use std::collections::{HashMap, HashSet};

use rowan::NodeOrToken;
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
const VALID_JOB_ATTRS: &[&str] = &["image", "inherit", "dependencies", "variables"];
const VALID_TEMPLATE_ATTRS: &[&str] = &["image", "inherit", "dependencies", "variables"];
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
    // Templates per stage, so that `inherit = stage.template` can be checked.
    let stage_templates: HashMap<SmolStr, HashSet<SmolStr>> = body
        .stages()
        .filter_map(|stage| {
            let names = stage
                .body()
                .map(|b| b.templates().filter_map(|t| t.name()).collect())
                .unwrap_or_default();
            Some((stage.name()?, names))
        })
        .collect();
    for tmpl in body.templates() {
        check_unknown_attrs(&tmpl, "template", VALID_TEMPLATE_ATTRS, diagnostics);
        check_dependencies_attr(&tmpl, diagnostics);
        check_template_inherit(&tmpl, &root_template_names, &stage_templates, diagnostics);
        check_template_steps(&tmpl, diagnostics);
    }
    for stage in body.stages() {
        check_stage(&stage, &root_template_names, &stage_templates, diagnostics);
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
    stage_templates: &HashMap<SmolStr, HashSet<SmolStr>>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    check_unknown_attrs(stage, "stage", VALID_STAGE_ATTRS, diagnostics);
    check_dependencies_attr(stage, diagnostics);

    let Some(body) = stage.body() else {
        return;
    };

    let stage_template_names: HashSet<SmolStr> = stage
        .name()
        .and_then(|name| stage_templates.get(&name).cloned())
        .unwrap_or_default();

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
        check_dependencies_attr(&job, diagnostics);
        check_job_steps(&job, &all_template_names, stage_templates, diagnostics);
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
        check_dependencies_attr(&tmpl, diagnostics);
        check_template_inherit(&tmpl, &all_template_names, stage_templates, diagnostics);
        check_template_steps(&tmpl, diagnostics);
    }
}

fn check_template_steps(tmpl: &TemplateDef, diagnostics: &mut Vec<Diagnostic>) {
    let Some(body) = tmpl.body() else {
        return;
    };
    let has_inherit = tmpl.attr_list().is_some_and(|al| {
        al.attrs()
            .any(|a| a.key_text().as_deref() == Some("inherit"))
    });
    if !has_inherit {
        check_step_reuse(&body, "template", diagnostics);
    }
}

/// `steps` and bare `step` references reuse steps from an inherited template,
/// so they only make sense where there is something to inherit from.
fn check_step_reuse(
    body: &crate::ast::JobBodySteps,
    owner: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(kw) = body.steps_keywords().next() {
        diagnostics.push(Diagnostic {
            severity: Severity::Error,
            message: format!(
                "`steps` can only be used in a {owner} that has an `inherit` attribute"
            ),
            span: span_of(kw.syntax()),
        });
    }

    // A step with no body reuses a step from an inherited template.
    for step in body.steps().filter(|s| s.shell_text().is_none()) {
        let Some(name) = step.name() else {
            continue;
        };
        diagnostics.push(Diagnostic {
            severity: Severity::Error,
            message: format!(
                "`step {name}` without a body can only be used in a {owner} that has an \
                 `inherit` attribute"
            ),
            span: span_of(step.syntax()),
        });
    }
}

/// Check that a `dependencies` attribute is a list of `stage.job` references.
///
/// Whether the referenced jobs exist can only be known once templates have
/// been resolved, so that's checked when the workflow is lowered.
fn check_dependencies_attr<N: HasAttrList>(node: &N, diagnostics: &mut Vec<Diagnostic>) {
    let Some(al) = node.attr_list() else {
        return;
    };
    for attr in al.attrs() {
        if attr.key_text().as_deref() != Some("dependencies") {
            continue;
        }
        let Some(value) = attr.value() else {
            continue;
        };
        let Some(ref_list) = value.ref_list() else {
            // A missing value is already reported as a parse error.
            if value.bare_text().is_some() {
                diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    message: "`dependencies` must be a list of jobs, e.g. `[ stage.job ]`"
                        .to_owned(),
                    span: span_of(attr.syntax()),
                });
            }
            continue;
        };
        for dep in ref_list.refs() {
            if dep.stage_job().is_none() {
                diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    message: format!("dependency `{}` must be written as `stage.job`", dep.text()),
                    span: span_without_trivia(dep.syntax()),
                });
            }
        }
    }
}

fn check_template_inherit(
    tmpl: &TemplateDef,
    template_names: &HashSet<SmolStr>,
    stage_templates: &HashMap<SmolStr, HashSet<SmolStr>>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(al) = tmpl.attr_list() else { return };
    for attr in al.attrs() {
        if attr.key_text().as_deref() != Some("inherit") {
            continue;
        }
        for name in inherit_names_from_attr(&attr) {
            check_inherit_name(
                &name,
                "template",
                "this scope",
                template_names,
                stage_templates,
                &attr,
                diagnostics,
            );
        }
    }
}

/// Report an `inherit` name that doesn't name a template which exists.
///
/// `stage.name` names a template in that stage; an unqualified name is looked
/// up in `unqualified`, the templates usable without a prefix from here.
/// Cross-file references are checked when the workflow is built.
fn check_inherit_name(
    name: &SmolStr,
    owner: &str,
    scope: &str,
    unqualified: &HashSet<SmolStr>,
    stage_templates: &HashMap<SmolStr, HashSet<SmolStr>>,
    attr: &Attr,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if name.contains('/') {
        return;
    }
    let message = if let Some((stage_name, template_name)) = name.split_once('.') {
        match stage_templates.get(stage_name) {
            None => format!("{owner} inherits from `{name}`, but there is no stage `{stage_name}`"),
            Some(names) if !names.contains(template_name) => format!(
                "{owner} inherits from `{name}`, but stage `{stage_name}` has no template \
                 `{template_name}`"
            ),
            Some(_) => return,
        }
    } else if unqualified.contains(name) {
        return;
    } else {
        format!(
            "{owner} inherits from `{name}`, but no template with that name is defined in \
             {scope}"
        )
    };
    diagnostics.push(Diagnostic {
        severity: Severity::Error,
        message,
        span: span_of(attr.syntax()),
    });
}

fn check_job_steps(
    job: &crate::ast::Job,
    template_names: &HashSet<SmolStr>,
    stage_templates: &HashMap<SmolStr, HashSet<SmolStr>>,
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
                check_inherit_name(
                    &name,
                    "job",
                    "this stage or at the top level",
                    template_names,
                    stage_templates,
                    &attr,
                    diagnostics,
                );
            }
        }
    }

    if !has_inherit {
        check_step_reuse(&body, "job", diagnostics);
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

/// The span of `node`, excluding any leading or trailing trivia.
fn span_without_trivia(node: &crate::syntax::SyntaxNode) -> std::ops::Range<usize> {
    let mut tokens = node
        .descendants_with_tokens()
        .filter_map(NodeOrToken::into_token)
        .filter(|t| !t.kind().is_trivia());
    let Some(first) = tokens.next() else {
        return span_of(node);
    };
    let end = tokens
        .last()
        .unwrap_or_else(|| first.clone())
        .text_range()
        .end();
    usize::from(first.text_range().start())..usize::from(end)
}
