use std::path::{Path, PathBuf};

use crate::Target;

/// Build a `ciane` workflow file into CI-system YAML.
pub fn build(path: &Path, target: Target, output: Option<&Path>) -> anyhow::Result<()> {
    let core_target = match target {
        Target::Gitlab => cianity_core::build::Target::Gitlab,
        Target::Github => anyhow::bail!("GitHub Actions target is not yet implemented"),
    };
    let out_path = cianity_core::build::run(path, core_target, output)?;
    println!("wrote {}", out_path.display());
    Ok(())
}

/// Check a `ciane` workflow file for errors.
pub fn check(path: &Path) -> anyhow::Result<()> {
    cianity_core::check::run(path)
}

/// Format one or more `ciane` source files (or check that they are already formatted).
///
/// All files are processed even if one fails; returns `Err` if any file could
/// not be formatted.
pub fn format(paths: &[PathBuf], check_only: bool) -> anyhow::Result<()> {
    let mut had_error = false;
    for path in paths {
        if let Err(e) = cianity_core::format::run(path, check_only) {
            eprintln!("error: {e}");
            had_error = true;
        }
    }
    if had_error {
        anyhow::bail!("one or more files had format errors");
    }

    Ok(())
}
