use std::path::Path;

use crate::Target;

/// Build a `ciane` workflow file into CI-system YAML.
pub fn build(_path: &Path, _target: Target) -> anyhow::Result<()> {
    anyhow::bail!("the `build` command is not yet implemented")
}

/// Check a `ciane` workflow file for errors.
pub fn check(path: &Path) -> anyhow::Result<()> {
    cianity_core::check::run(path)
}

/// Format a `ciane` source file (or check that it is already formatted).
pub fn format(_path: &Path, _check_only: bool) -> anyhow::Result<()> {
    anyhow::bail!("the `format` command is not yet implemented")
}
