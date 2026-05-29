use std::path::{Path, PathBuf};

use ciane::{
    ast::{AstNode, Root},
    parse,
};

/// Resolve the workflow root file from explicit args or by discovery.
///
/// At most one of `file` and `workspace` may be `Some`.
///
/// # Errors
///
/// Returns `Err` if both `file` and `workspace` are `Some`, if the workspace
/// directory contains no `workflow.ci`, or if discovery from the process cwd
/// finds no `workflow.ci` in any ancestor directory.
pub fn resolve_root(file: Option<&Path>, workspace: Option<&Path>) -> anyhow::Result<PathBuf> {
    match (file, workspace) {
        (Some(_), Some(_)) => {
            anyhow::bail!("cannot specify both a workflow file and --workspace")
        }
        (Some(f), None) => Ok(f.to_path_buf()),
        (None, Some(dir)) => find_in_dir(dir),
        (None, None) => discover(),
    }
}

/// Return the paths of `.ci` files referenced via `use {}` blocks in `root`.
///
/// Paths are resolved relative to `root`'s parent directory. Files whose
/// `location` path does not exist on disk are silently skipped.
///
/// # Errors
///
/// Returns `Err` if `root` cannot be read.
pub fn referenced_files(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let source = std::fs::read_to_string(root)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", root.display()))?;

    let result = parse(&source);
    let ast_root = Root::cast(result.syntax())
        .ok_or_else(|| anyhow::anyhow!("internal error: parse produced no Root node"))?;

    let base = root.parent().unwrap_or(Path::new("."));

    let mut paths: Vec<PathBuf> = Vec::new();
    for use_block in ast_root.use_blocks() {
        for import in use_block.imports() {
            if let Some(loc) = import.location() {
                let path = base.join(loc.as_str());
                if path.exists() {
                    paths.push(path);
                }
            }
        }
    }

    Ok(paths)
}

/// Walk up from `start`, returning the first `workflow.ci` or `.workflow.ci`
/// found. Prefers `workflow.ci`; warns if both are present.
///
/// # Errors
///
/// Returns `Err` if no `workflow.ci` or `.workflow.ci` is found in `start` or
/// any of its ancestors.
pub fn discover_from(start: &Path) -> anyhow::Result<PathBuf> {
    let mut dir = start;
    loop {
        let primary = dir.join("workflow.ci");
        let hidden = dir.join(".workflow.ci");

        if primary.exists() || hidden.exists() {
            return find_in_dir(dir);
        }

        match dir.parent() {
            Some(parent) => dir = parent,
            None => anyhow::bail!(
                "no workflow.ci found in {} or any parent directory",
                start.display()
            ),
        }
    }
}

fn find_in_dir(dir: &Path) -> anyhow::Result<PathBuf> {
    let primary = dir.join("workflow.ci");
    let hidden = dir.join(".workflow.ci");

    match (primary.exists(), hidden.exists()) {
        (true, true) => {
            eprintln!(
                "warning: ignoring {} because {} is present",
                hidden.display(),
                primary.display()
            );
            Ok(primary)
        }
        (true, false) => Ok(primary),
        (false, true) => Ok(hidden),
        (false, false) => anyhow::bail!("no workflow.ci found in {}", dir.display()),
    }
}

fn discover() -> anyhow::Result<PathBuf> {
    let cwd = std::env::current_dir()
        .map_err(|e| anyhow::anyhow!("cannot determine current directory: {e}"))?;
    discover_from(&cwd)
}
