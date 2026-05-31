use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ariadne::{Color, Label, Report, ReportKind, Source};
use ciane::{
    ast::{AstNode, HasAttrList, HasName, Root},
    error::{Diagnostic, Severity},
    parse,
    validation::validate,
};

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
    let source =
        std::fs::read_to_string(path).map_err(|e| anyhow::anyhow!("cannot read file: {e}"))?;
    let filename = path.display().to_string();

    let result = parse(&source);
    let mut has_error = false;

    for err in result.errors() {
        has_error = true;
        print_diagnostic(
            &filename,
            &source,
            &Diagnostic {
                severity: Severity::Error,
                message: err.message.clone(),
                span: err.span.clone(),
            },
        );
    }

    if let Some(root) = Root::cast(result.syntax()) {
        for diag in validate(&root) {
            if diag.severity == Severity::Error {
                has_error = true;
            }
            print_diagnostic(&filename, &source, &diag);
        }
        check_cross_file_inherits(&root, path, &filename, &source, &mut has_error);
    }

    if has_error {
        anyhow::bail!("found errors in {filename}");
    }

    Ok(())
}

fn check_cross_file_inherits(
    root: &Root,
    path: &Path,
    filename: &str,
    source: &str,
    has_error: &mut bool,
) {
    let base = path.parent().unwrap_or(Path::new("."));
    let imports = build_import_map(root, base);

    for stage in root.stages() {
        let Some(body) = stage.body() else { continue };
        for job in body.jobs() {
            let Some(al) = job.attr_list() else { continue };
            for attr in al.attrs() {
                if attr.key_text().as_deref() != Some("inherit") {
                    continue;
                }
                let Some(value) = attr.value_text() else {
                    continue;
                };
                let Some((import_name, template_ref)) = value.split_once('/') else {
                    continue;
                };
                let span = {
                    let r = attr.syntax().text_range();
                    usize::from(r.start())..usize::from(r.end())
                };
                if let Some(file_path) = imports.get(import_name) {
                    if file_path.exists() {
                        // `ns/tmpl` → top-level template; `ns/stage.tmpl` → stage-local
                        let result =
                            if let Some((stage_name, tmpl_name)) = template_ref.split_once('.') {
                                template_exists_in_stage(file_path, stage_name, tmpl_name)
                            } else {
                                template_exists_at_top_level(file_path, template_ref)
                            };
                        match result {
                            Ok(true) => {}
                            Ok(false) => {
                                *has_error = true;
                                print_diagnostic(
                                    filename,
                                    source,
                                    &Diagnostic {
                                        severity: Severity::Error,
                                        message: format!(
                                            "template `{template_ref}` not found \
                                             in import `{import_name}`"
                                        ),
                                        span,
                                    },
                                );
                            }
                            Err(e) => {
                                *has_error = true;
                                print_diagnostic(
                                    filename,
                                    source,
                                    &Diagnostic {
                                        severity: Severity::Error,
                                        message: format!(
                                            "failed to read import `{import_name}`: {e}"
                                        ),
                                        span,
                                    },
                                );
                            }
                        }
                    } else {
                        *has_error = true;
                        print_diagnostic(
                            filename,
                            source,
                            &Diagnostic {
                                severity: Severity::Error,
                                message: format!(
                                    "import `{import_name}` references `{}`, \
                                     but that file does not exist",
                                    file_path.display()
                                ),
                                span,
                            },
                        );
                    }
                } else {
                    *has_error = true;
                    print_diagnostic(
                        filename,
                        source,
                        &Diagnostic {
                            severity: Severity::Error,
                            message: format!(
                                "inherit references import `{import_name}`, but no such \
                                 import exists in the `use` block"
                            ),
                            span,
                        },
                    );
                }
            }
        }
    }
}

fn build_import_map(root: &Root, base: &Path) -> HashMap<String, PathBuf> {
    let mut map = HashMap::new();
    for ub in root.use_blocks() {
        for imp in ub.imports() {
            if let Some((name, loc)) = imp.name().zip(imp.location()) {
                map.insert(name.to_string(), base.join(loc.as_str()));
            }
        }
    }
    map
}

/// Check whether a top-level (outside any stage) template with `template_name`
/// exists in `path`.
fn template_exists_at_top_level(path: &Path, template_name: &str) -> anyhow::Result<bool> {
    let source =
        std::fs::read_to_string(path).map_err(|e| anyhow::anyhow!("cannot read file: {e}"))?;
    let result = parse(&source);
    let root =
        Root::cast(result.syntax()).ok_or_else(|| anyhow::anyhow!("internal: no Root node"))?;
    Ok(root
        .templates()
        .any(|t| t.name().as_deref() == Some(template_name)))
}

/// Check whether a template named `template_name` exists inside stage
/// `stage_name` in `path`.
fn template_exists_in_stage(
    path: &Path,
    stage_name: &str,
    template_name: &str,
) -> anyhow::Result<bool> {
    let source =
        std::fs::read_to_string(path).map_err(|e| anyhow::anyhow!("cannot read file: {e}"))?;
    let result = parse(&source);
    let root =
        Root::cast(result.syntax()).ok_or_else(|| anyhow::anyhow!("internal: no Root node"))?;
    Ok(root
        .stages()
        .find(|s| s.name().as_deref() == Some(stage_name))
        .and_then(|s| s.body())
        .is_some_and(|b| {
            b.templates()
                .any(|t| t.name().as_deref() == Some(template_name))
        }))
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
