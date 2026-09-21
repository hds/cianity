use std::collections::{HashSet, VecDeque};
use std::path::{Component, Path, PathBuf};

use ciane::{
    ast::{AstNode, HasName, Root},
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

/// Return the paths of every `.ci` file reachable from `root` through
/// `use {}` blocks, however deep the imports go.
///
/// Paths are resolved relative to the parent of the file the `use` is written
/// in. Files whose `path` does not exist on disk are silently skipped, and
/// each file is listed once, which also breaks import loops.
///
/// # Errors
///
/// Returns `Err` if `root` cannot be read.
pub fn referenced_files(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut queue: VecDeque<PathBuf> = VecDeque::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut files: Vec<PathBuf> = Vec::new();

    seen.insert(identity(root));
    queue.extend(direct_imports(root)?);

    while let Some(path) = queue.pop_front() {
        if !seen.insert(identity(&path)) {
            continue;
        }
        // A file that can't be read or parsed is reported when it is checked.
        queue.extend(direct_imports(&path).unwrap_or_default());
        files.push(path);
    }

    Ok(files)
}

/// Return the `use` imports of `root` whose file is not on disk, as pairs of
/// import name and the path it resolves to.
///
/// An import is checked whether or not anything inherits through it: it names
/// a file the workflow depends on either way.
#[must_use]
pub fn missing_imports(root: &Root, path: &Path) -> Vec<(String, PathBuf)> {
    let base = path.parent().unwrap_or(Path::new("."));
    root.use_decls()
        .filter_map(|use_decl| {
            let imported = normalize(&base.join(use_decl.path()?.as_str()));
            if imported.exists() {
                return None;
            }
            Some((use_decl.name()?.to_string(), imported))
        })
        .collect()
}

/// Return the import loops that `path` is responsible for reporting.
///
/// Each loop is the chain of files that closes it, starting and ending at
/// `path`, e.g. `[workflow.ci, shared.ci, workflow.ci]`.
///
/// Every file in a loop can see it, so a loop is reported by just one of
/// them — the first by path — and the others stay quiet. That way a workspace
/// check describes each loop once rather than once per file in it.
#[must_use]
pub fn import_loops(path: &Path) -> Vec<Vec<PathBuf>> {
    fn walk(
        current: &Path,
        start: &Path,
        chain: &mut Vec<PathBuf>,
        visited: &mut HashSet<PathBuf>,
        loops: &mut Vec<Vec<PathBuf>>,
    ) {
        for imported in direct_imports(current).unwrap_or_default() {
            if identity(&imported) == identity(start) {
                let mut found = chain.clone();
                found.push(imported);
                loops.push(found);
                continue;
            }
            // Only follow each file once: any loop that doesn't come back to
            // `start` belongs to the files it does run through.
            if !visited.insert(identity(&imported)) {
                continue;
            }
            chain.push(imported.clone());
            walk(&imported, start, chain, visited, loops);
            chain.pop();
        }
    }

    let mut loops = Vec::new();
    let mut visited = HashSet::new();
    visited.insert(identity(path));
    walk(
        path,
        path,
        &mut vec![path.to_path_buf()],
        &mut visited,
        &mut loops,
    );
    loops.retain(|chain| {
        chain
            .iter()
            .map(|file| identity(file))
            .min()
            .is_some_and(|first| first == identity(path))
    });
    loops
}

/// The existing files a single file imports through its `use {}` blocks.
fn direct_imports(path: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?;

    let result = parse(&source);
    let ast_root = Root::cast(result.syntax())
        .ok_or_else(|| anyhow::anyhow!("internal error: parse produced no Root node"))?;

    let base = path.parent().unwrap_or(Path::new("."));
    Ok(ast_root
        .use_decls()
        .filter_map(|use_decl| {
            let imported = normalize(&base.join(use_decl.path()?.as_str()));
            imported.exists().then_some(imported)
        })
        .collect())
}

/// Remove `.` and `..` components from a path, so that a chain of imports
/// written as `./ci/shared.ci` doesn't read as `a/./ci/./shared.ci` when it is
/// reported to the user.
#[must_use]
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir
                if matches!(out.components().next_back(), Some(Component::Normal(_))) =>
            {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// How a file is recognised as one already visited, so that two spellings of
/// the same path — or an import loop — don't send us round again.
fn identity(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
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
