use std::ops::Range;
use std::path::Path;

use ariadne::{Color, Label, Report, ReportKind, Source};
use ciane::{
    ast::{AstNode, Attr, AttrList, HasAttrList, HasName, Root},
    error::{Diagnostic, Severity},
    parse,
    validation::validate,
};

use crate::build::ir::{ErrorSite, dependency_errors, lower_with_path_partial};

/// Read, parse, and validate a `ciane` source file.
///
/// Diagnostics are printed to stderr using `ariadne`.  Returns `Ok(())` if
/// there are no errors (warnings are allowed).
///
/// # Errors
///
/// Returns `Err` if the file cannot be read, or if any error-level diagnostic
/// is produced during parsing or semantic validation.
pub fn run(path: &Path) -> anyhow::Result<()> {
    let source = read_source(path)?;
    let filename = path.display().to_string();

    let mut has_error = false;
    for diag in collect_diagnostics(path, &source) {
        if diag.severity == Severity::Error {
            has_error = true;
        }
        print_diagnostic(&filename, &source, &diag);
    }

    if has_error {
        anyhow::bail!("found errors in {filename}");
    }

    Ok(())
}

/// Read, parse, and validate a `ciane` source file, returning the diagnostics
/// that [`run`] would print.
///
/// # Errors
///
/// Returns `Err` if the file cannot be read.
pub fn diagnostics(path: &Path) -> anyhow::Result<Vec<Diagnostic>> {
    let source = read_source(path)?;
    Ok(collect_diagnostics(path, &source))
}

fn read_source(path: &Path) -> anyhow::Result<String> {
    std::fs::read_to_string(path).map_err(|e| anyhow::anyhow!("cannot read file: {e}"))
}

fn collect_diagnostics(path: &Path, source: &str) -> Vec<Diagnostic> {
    let result = parse(source);
    let mut diagnostics: Vec<Diagnostic> = result
        .errors()
        .iter()
        .map(|err| Diagnostic {
            severity: Severity::Error,
            message: err.message.clone(),
            span: err.span.clone(),
        })
        .collect();

    if let Some(root) = Root::cast(result.syntax()) {
        diagnostics.extend(validate(&root));
        // Error recovery can leave jobs and templates out of the tree, which
        // would make references to them look like they don't exist.
        if result.errors().is_empty() {
            check_resolved_workflow(&root, path, &mut diagnostics);
        }
    }

    diagnostics
}

/// Report template references that can't be resolved and dependencies that
/// can't be satisfied.
///
/// Both are checked on the lowered workflow, so that templates inherited from
/// other files — and the `inherit` chains inside those files — are included.
fn check_resolved_workflow(root: &Root, path: &Path, diagnostics: &mut Vec<Diagnostic>) {
    let (workflow, template_errors) = lower_with_path_partial(root, path);

    for err in template_errors {
        let span = match &err.site {
            ErrorSite::InheritRef(reference) => inherit_ref_span(root, reference),
            ErrorSite::Import(file) => import_span(root, path, file),
            ErrorSite::StepRef { stage, job, step } => step_ref_span(root, stage, job, step),
        };
        diagnostics.push(Diagnostic {
            severity: Severity::Error,
            message: err.message,
            span,
        });
    }

    for err in dependency_errors(&workflow) {
        diagnostics.push(Diagnostic {
            severity: Severity::Error,
            message: err.message,
            span: job_dependency_span(root, &err.stage, &err.job),
        });
    }
}

/// The span of the `inherit` attribute that names `reference`.
fn inherit_ref_span(root: &Root, reference: &str) -> Range<usize> {
    inherit_attrs(root)
        .find(|attr| inherit_names_of(attr).iter().any(|name| name == reference))
        .map_or(0..0, |attr| span_of(attr.syntax()))
}

/// The span of a bare `step` reference in a job body.
fn step_ref_span(root: &Root, stage_name: &str, job_name: &str, step: &str) -> Range<usize> {
    root.stages()
        .find(|s| s.name().as_deref() == Some(stage_name))
        .and_then(|s| s.body())
        .and_then(|b| b.jobs().find(|j| j.name().as_deref() == Some(job_name)))
        .and_then(|job| job.steps_body())
        .and_then(|body| body.steps().find(|s| s.name().as_deref() == Some(step)))
        .map_or(0..0, |step| span_of(step.syntax()))
}

/// The span of the `use` import that brings in `file`.
fn import_span(root: &Root, path: &Path, file: &Path) -> Range<usize> {
    let base = path.parent().unwrap_or(Path::new("."));
    root.use_decls()
        .find(|imp| {
            imp.path()
                .is_some_and(|loc| base.join(loc.as_str()) == file)
        })
        .map_or(0..0, |imp| span_of(imp.syntax()))
}

fn span_of(node: &ciane::syntax::SyntaxNode) -> Range<usize> {
    let range = node.text_range();
    usize::from(range.start())..usize::from(range.end())
}

/// Every `inherit` attribute in the file, on templates and jobs alike.
fn inherit_attrs(root: &Root) -> impl Iterator<Item = Attr> + '_ {
    let top_level = root.templates().filter_map(|t| t.attr_list());
    let in_stages = root.stages().filter_map(|s| s.body()).flat_map(|body| {
        let jobs: Vec<AttrList> = body.jobs().filter_map(|j| j.attr_list()).collect();
        let templates: Vec<AttrList> = body.templates().filter_map(|t| t.attr_list()).collect();
        jobs.into_iter().chain(templates)
    });
    top_level
        .chain(in_stages)
        .flat_map(|list| list.attrs().collect::<Vec<_>>())
        .filter(|attr| attr.key_text().as_deref() == Some("inherit"))
}

/// The template names an `inherit` attribute lists.
fn inherit_names_of(attr: &Attr) -> Vec<String> {
    attr.value_text().map_or_else(
        || {
            attr.value()
                .and_then(|value| value.ref_list())
                .map(|list| list.refs().map(|r| r.text()).collect())
                .unwrap_or_default()
        },
        |value| vec![value.to_string()],
    )
}

/// The span of the job's `dependencies` attribute, or its `inherit` attribute
/// if the dependencies come from a template, falling back to the whole job.
fn job_dependency_span(root: &Root, stage_name: &str, job_name: &str) -> Range<usize> {
    let Some(job) = root
        .stages()
        .find(|s| s.name().as_deref() == Some(stage_name))
        .and_then(|s| s.body())
        .and_then(|b| b.jobs().find(|j| j.name().as_deref() == Some(job_name)))
    else {
        return 0..0;
    };
    let attr_named = |key: &str| {
        job.attr_list()?
            .attrs()
            .find(|a| a.key_text().as_deref() == Some(key))
    };
    let range = attr_named("dependencies")
        .or_else(|| attr_named("inherit"))
        .map_or_else(|| job.syntax().text_range(), |a| a.syntax().text_range());
    usize::from(range.start())..usize::from(range.end())
}

fn print_diagnostic(filename: &str, source: &str, diag: &Diagnostic) {
    let kind = match diag.severity {
        Severity::Error => ReportKind::Error,
        Severity::Warning => ReportKind::Warning,
    };
    let color = match diag.severity {
        Severity::Error => Color::Red,
        Severity::Warning => Color::Yellow,
    };

    Report::build(kind, (filename, diag.span.clone()))
        .with_message(&diag.message)
        .with_label(Label::new((filename, diag.span.clone())).with_color(color))
        .finish()
        .eprint((filename, Source::from(source)))
        .expect("failed to write diagnostic");
}
