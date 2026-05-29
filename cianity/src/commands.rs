use std::path::{Path, PathBuf};

use cianity_core::workspace;

use crate::Target;

/// Build a `ciane` workflow file into CI-system YAML.
pub fn build(
    file: Option<&Path>,
    target: Target,
    output: Option<&Path>,
    workspace: Option<&Path>,
) -> anyhow::Result<()> {
    let core_target = match target {
        Target::Gitlab => cianity_core::build::Target::Gitlab,
        Target::Github => anyhow::bail!("GitHub Actions target is not yet implemented"),
    };
    let root = workspace::resolve_root(file, workspace)?;
    let out_path = cianity_core::build::run(&root, core_target, output)?;
    println!("wrote {}", out_path.display());
    Ok(())
}

/// Check a `ciane` workflow file (and any referenced files in workspace mode).
pub fn check(file: Option<&Path>, workspace: Option<&Path>) -> anyhow::Result<()> {
    let root = workspace::resolve_root(file, workspace)?;
    let mut had_error = false;

    if let Err(e) = cianity_core::check::run(&root) {
        eprintln!("error: {e}");
        had_error = true;
    }

    let refs = workspace::referenced_files(&root)?;
    for path in &refs {
        if let Err(e) = cianity_core::check::run(path) {
            eprintln!("error: {e}");
            had_error = true;
        }
    }

    if had_error {
        anyhow::bail!("one or more files had errors");
    }
    Ok(())
}

/// Format one or more `ciane` source files, or the workspace root and its
/// referenced files when no explicit paths are given.
pub fn format(files: &[PathBuf], check_only: bool, workspace: Option<&Path>) -> anyhow::Result<()> {
    let paths: Vec<PathBuf> = if files.is_empty() {
        let root = workspace::resolve_root(None, workspace)?;
        let mut all = vec![root.clone()];
        all.extend(workspace::referenced_files(&root)?);
        all
    } else {
        files.to_vec()
    };

    let mut had_error = false;
    for path in &paths {
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
