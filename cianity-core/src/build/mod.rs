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
/// # Errors
///
/// Returns `Err` if the source has parse errors.
pub fn render_to_string(source: &str, target: Target) -> anyhow::Result<String> {
    match target {
        Target::Gitlab => gitlab::render_source(source),
    }
}
