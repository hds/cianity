use std::path::Path;

use ariadne::{Color, Label, Report, ReportKind, Source};
use ciane::{
    ast::{AstNode, Root},
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
    }

    if has_error {
        anyhow::bail!("found errors in {filename}");
    }

    Ok(())
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
