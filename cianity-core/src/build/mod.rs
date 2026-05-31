mod gitlab;
pub mod ir;

use std::path::{Path, PathBuf};

/// Target CI system.
#[derive(Clone, Copy)]
pub enum Target {
    Gitlab,
}

/// Build a `ciane` source file into a CI-system configuration file.
///
/// `output` overrides the default output path chosen by the target writer.
/// Returns the path of the generated output file.
///
/// # Errors
///
/// Returns `Err` if the source file cannot be read or parsed, or if the output
/// cannot be written.
pub fn run(path: &Path, target: Target, output: Option<&Path>) -> anyhow::Result<PathBuf> {
    match target {
        Target::Gitlab => gitlab::run(path, output),
    }
}

/// Render a `ciane` source string into a CI-system configuration string
/// without any file I/O.
///
/// Cross-file template references (`inherit = name/template`) are not resolved;
/// those jobs produce empty scripts. Use [`render_path_to_string`] when the
/// source is read from a file and cross-file resolution is required.
///
/// # Errors
///
/// Returns `Err` if the source has parse errors.
pub fn render_to_string(source: &str, target: Target) -> anyhow::Result<String> {
    match target {
        Target::Gitlab => gitlab::render_source(source),
    }
}

/// Read a `ciane` source file and render it into a CI-system configuration
/// string, resolving any cross-file template references.
///
/// # Errors
///
/// Returns `Err` if the file cannot be read, has parse errors, or a
/// cross-file template reference cannot be resolved.
pub fn render_path_to_string(path: &Path, target: Target) -> anyhow::Result<String> {
    match target {
        Target::Gitlab => gitlab::render_path(path),
    }
}
