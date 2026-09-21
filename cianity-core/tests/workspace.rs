use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;

use cianity_core::workspace;

// ── helpers ───────────────────────────────────────────────────────────────────

fn touch(dir: &TempDir, rel: &str) -> PathBuf {
    let path = dir.path().join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, "").unwrap();
    path
}

fn write(dir: &TempDir, rel: &str, content: &str) -> PathBuf {
    let path = dir.path().join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, content).unwrap();
    path
}

// ── discover_from ─────────────────────────────────────────────────────────────

#[test]
fn discover_finds_workflow_ci_in_start_dir() {
    let tmp = TempDir::new().unwrap();
    let expected = touch(&tmp, "workflow.ci");

    let found = workspace::discover_from(tmp.path()).unwrap();
    assert_eq!(found, expected);
}

#[test]
fn discover_finds_hidden_workflow_ci_in_start_dir() {
    let tmp = TempDir::new().unwrap();
    let expected = touch(&tmp, ".workflow.ci");

    let found = workspace::discover_from(tmp.path()).unwrap();
    assert_eq!(found, expected);
}

#[test]
fn discover_prefers_workflow_ci_over_hidden() {
    let tmp = TempDir::new().unwrap();
    let expected = touch(&tmp, "workflow.ci");
    touch(&tmp, ".workflow.ci");

    let found = workspace::discover_from(tmp.path()).unwrap();
    assert_eq!(found, expected);
}

#[test]
fn discover_finds_workflow_ci_in_parent() {
    let tmp = TempDir::new().unwrap();
    let expected = touch(&tmp, "workflow.ci");
    fs::create_dir_all(tmp.path().join("child")).unwrap();
    let start = tmp.path().join("child");

    let found = workspace::discover_from(&start).unwrap();
    assert_eq!(found, expected);
}

#[test]
fn discover_finds_workflow_ci_in_grandparent() {
    let tmp = TempDir::new().unwrap();
    let expected = touch(&tmp, "workflow.ci");
    fs::create_dir_all(tmp.path().join("a/b")).unwrap();
    let start = tmp.path().join("a/b");

    let found = workspace::discover_from(&start).unwrap();
    assert_eq!(found, expected);
}

#[test]
fn discover_stops_at_nearest_ancestor() {
    let tmp = TempDir::new().unwrap();
    touch(&tmp, "workflow.ci");
    let expected = touch(&tmp, "child/workflow.ci");
    fs::create_dir_all(tmp.path().join("child/grandchild")).unwrap();
    let start = tmp.path().join("child/grandchild");

    let found = workspace::discover_from(&start).unwrap();
    assert_eq!(found, expected);
}

#[test]
fn discover_finds_hidden_in_parent_when_no_primary() {
    let tmp = TempDir::new().unwrap();
    let expected = touch(&tmp, ".workflow.ci");
    fs::create_dir_all(tmp.path().join("sub")).unwrap();
    let start = tmp.path().join("sub");

    let found = workspace::discover_from(&start).unwrap();
    assert_eq!(found, expected);
}

#[test]
fn discover_errors_when_no_workflow_file_found() {
    let tmp = TempDir::new().unwrap();

    let err =
        workspace::discover_from(tmp.path()).expect_err("should fail when no workflow.ci exists");
    assert!(
        err.to_string().contains("no workflow.ci found"),
        "unexpected error: {err}"
    );
}

// ── resolve_root ──────────────────────────────────────────────────────────────

#[test]
fn resolve_root_explicit_file_returned_as_is() {
    let tmp = TempDir::new().unwrap();
    let file = touch(&tmp, "my.ci");

    let resolved = workspace::resolve_root(Some(&file), None).unwrap();
    assert_eq!(resolved, file);
}

#[test]
fn resolve_root_workspace_dir_finds_workflow_ci() {
    let tmp = TempDir::new().unwrap();
    let expected = touch(&tmp, "workflow.ci");

    let resolved = workspace::resolve_root(None, Some(tmp.path())).unwrap();
    assert_eq!(resolved, expected);
}

#[test]
fn resolve_root_both_args_errors() {
    let tmp = TempDir::new().unwrap();
    let file = touch(&tmp, "my.ci");

    let err = workspace::resolve_root(Some(&file), Some(tmp.path()))
        .expect_err("providing both file and workspace should fail");
    assert!(
        err.to_string().contains("cannot specify both"),
        "unexpected error: {err}"
    );
}

#[test]
fn resolve_root_workspace_dir_missing_workflow_errors() {
    let tmp = TempDir::new().unwrap();

    let err = workspace::resolve_root(None, Some(tmp.path()))
        .expect_err("empty workspace dir should fail");
    assert!(
        err.to_string().contains("no workflow.ci found"),
        "unexpected error: {err}"
    );
}

// ── referenced_files ──────────────────────────────────────────────────────────

#[test]
fn referenced_files_returns_existing_imports() {
    let tmp = TempDir::new().unwrap();
    let shared = touch(&tmp, "shared.ci");
    let root = write(
        &tmp,
        "workflow.ci",
        "workflow ci {\n    use shared ( path = ./shared.ci )\n\n    stage build {\n        job compile { cargo build }\n    }\n}\n",
    );

    let refs = workspace::referenced_files(&root).unwrap();
    assert_eq!(refs, vec![shared]);
}

#[test]
fn referenced_files_skips_missing_imports() {
    let tmp = TempDir::new().unwrap();
    let root = write(
        &tmp,
        "workflow.ci",
        "workflow ci {\n    use missing ( path = ./missing.ci )\n\n    stage build {\n        job compile { cargo build }\n    }\n}\n",
    );

    let refs = workspace::referenced_files(&root).unwrap();
    assert!(refs.is_empty(), "expected no refs, got: {refs:?}");
}

#[test]
fn referenced_files_empty_for_no_use_block() {
    let tmp = TempDir::new().unwrap();
    let root = write(
        &tmp,
        "workflow.ci",
        "workflow ci {\n    stage build {\n        job compile { cargo build }\n    }\n}\n",
    );

    let refs = workspace::referenced_files(&root).unwrap();
    assert!(refs.is_empty());
}

#[test]
fn referenced_files_resolves_paths_relative_to_root_parent() {
    let tmp = TempDir::new().unwrap();
    touch(&tmp, "shared/helpers.ci");
    let root = write(
        &tmp,
        "ci/workflow.ci",
        "workflow ci {\n    use helpers ( path = ../shared/helpers.ci )\n\n    stage build {\n        job compile { cargo build }\n    }\n}\n",
    );

    let refs = workspace::referenced_files(&root).unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(
        refs[0].canonicalize().unwrap(),
        tmp.path().join("shared/helpers.ci").canonicalize().unwrap()
    );
}

#[test]
fn referenced_files_follows_imports_transitively() {
    let tmp = TempDir::new().unwrap();
    let deep = touch(&tmp, "ci/deep.ci");
    let mid = write(
        &tmp,
        "ci/mid.ci",
        "workflow mid {\n    use deep ( path = ./deep.ci )\n}\n",
    );
    let root = write(
        &tmp,
        "workflow.ci",
        "workflow ci {\n    use mid ( path = ./ci/mid.ci )\n}\n",
    );

    let refs = workspace::referenced_files(&root).unwrap();
    assert_eq!(refs.len(), 2, "refs: {refs:?}");
    for expected in [&mid, &deep] {
        assert!(
            refs.iter()
                .any(|r| r.canonicalize().unwrap() == expected.canonicalize().unwrap()),
            "{expected:?} missing from {refs:?}"
        );
    }
}

#[test]
fn referenced_files_breaks_import_loops() {
    let tmp = TempDir::new().unwrap();
    // root → a → b → a, and b → root
    write(
        &tmp,
        "a.ci",
        "workflow a {\n    use b ( path = ./b.ci )\n}\n",
    );
    write(
        &tmp,
        "b.ci",
        "workflow b {\n    use a ( path = ./a.ci )\n    use root ( path = ./workflow.ci )\n}\n",
    );
    let root = write(
        &tmp,
        "workflow.ci",
        "workflow ci {\n    use a ( path = ./a.ci )\n}\n",
    );

    let refs = workspace::referenced_files(&root).unwrap();
    assert_eq!(refs.len(), 2, "each file once, and not the root: {refs:?}");
}

#[test]
fn referenced_files_lists_each_file_once() {
    let tmp = TempDir::new().unwrap();
    // diamond: root → a, root → b, a → shared, b → shared
    touch(&tmp, "shared.ci");
    write(
        &tmp,
        "a.ci",
        "workflow a {\n    use s ( path = ./shared.ci )\n}\n",
    );
    write(
        &tmp,
        "b.ci",
        "workflow b {\n    use s ( path = ./shared.ci )\n}\n",
    );
    let root = write(
        &tmp,
        "workflow.ci",
        "workflow ci {\n    use a ( path = ./a.ci )\n    use b ( path = ./b.ci )\n}\n",
    );

    let refs = workspace::referenced_files(&root).unwrap();
    assert_eq!(refs.len(), 3, "a, b and shared once each: {refs:?}");
}

#[test]
fn referenced_files_skips_missing_imports_deeper_in_the_tree() {
    let tmp = TempDir::new().unwrap();
    let mid = write(
        &tmp,
        "mid.ci",
        "workflow mid {\n    use gone ( path = ./gone.ci )\n}\n",
    );
    let root = write(
        &tmp,
        "workflow.ci",
        "workflow ci {\n    use mid ( path = ./mid.ci )\n}\n",
    );

    let refs = workspace::referenced_files(&root).unwrap();
    assert_eq!(refs.len(), 1, "refs: {refs:?}");
    assert_eq!(refs[0].canonicalize().unwrap(), mid.canonicalize().unwrap());
}

// ── import_loops ──────────────────────────────────────────────────────────────

/// Loops seen by `file`, rendered as `a.ci -> b.ci -> a.ci` against `base`.
fn loops_from(file: &std::path::Path, base: &std::path::Path) -> Vec<String> {
    workspace::import_loops(file)
        .into_iter()
        .map(|chain| {
            chain
                .iter()
                .map(|p| p.strip_prefix(base).unwrap_or(p).display().to_string())
                .collect::<Vec<_>>()
                .join(" -> ")
        })
        .collect()
}

/// Every loop among `files`, which should be reported by exactly one of them.
fn all_loops(files: &[PathBuf], base: &std::path::Path) -> Vec<String> {
    files.iter().flat_map(|f| loops_from(f, base)).collect()
}

#[test]
fn import_loops_finds_self_import() {
    let tmp = TempDir::new().unwrap();
    let root = write(
        &tmp,
        "workflow.ci",
        "workflow ci {\n    use me ( path = ./workflow.ci )\n}\n",
    );
    assert_eq!(
        loops_from(&root, tmp.path()),
        vec!["workflow.ci -> workflow.ci"]
    );
}

#[test]
fn import_loops_reports_a_mutual_loop_once() {
    let tmp = TempDir::new().unwrap();
    let other = write(
        &tmp,
        "other.ci",
        "workflow other {\n    use root ( path = ./workflow.ci )\n}\n",
    );
    let root = write(
        &tmp,
        "workflow.ci",
        "workflow ci {\n    use other ( path = ./other.ci )\n}\n",
    );
    assert_eq!(
        all_loops(&[root, other], tmp.path()),
        vec!["other.ci -> workflow.ci -> other.ci"],
        "one report, from the first file by path"
    );
}

#[test]
fn import_loops_reports_a_longer_loop_once() {
    let tmp = TempDir::new().unwrap();
    let a = write(
        &tmp,
        "a.ci",
        "workflow a {\n    use b ( path = ./b.ci )\n}\n",
    );
    let b = write(
        &tmp,
        "b.ci",
        "workflow b {\n    use root ( path = ./workflow.ci )\n}\n",
    );
    let root = write(
        &tmp,
        "workflow.ci",
        "workflow ci {\n    use a ( path = ./a.ci )\n}\n",
    );
    assert_eq!(
        all_loops(&[root, a, b], tmp.path()),
        vec!["a.ci -> b.ci -> workflow.ci -> a.ci"]
    );
}

#[test]
fn import_loops_empty_when_imports_only_share_files() {
    let tmp = TempDir::new().unwrap();
    // diamond, no loop: root -> a -> shared, root -> b -> shared
    let shared = touch(&tmp, "shared.ci");
    let a = write(
        &tmp,
        "a.ci",
        "workflow a {\n    use s ( path = ./shared.ci )\n}\n",
    );
    let b = write(
        &tmp,
        "b.ci",
        "workflow b {\n    use s ( path = ./shared.ci )\n}\n",
    );
    let root = write(
        &tmp,
        "workflow.ci",
        "workflow ci {\n    use a ( path = ./a.ci )\n    use b ( path = ./b.ci )\n}\n",
    );
    assert!(
        all_loops(&[root, a, b, shared], tmp.path()).is_empty(),
        "a shared import is not a loop"
    );
}

#[test]
fn import_loops_reports_a_loop_the_root_is_not_part_of() {
    let tmp = TempDir::new().unwrap();
    // root -> a, and a <-> b
    let a = write(
        &tmp,
        "a.ci",
        "workflow a {\n    use b ( path = ./b.ci )\n}\n",
    );
    let b = write(
        &tmp,
        "b.ci",
        "workflow b {\n    use a ( path = ./a.ci )\n}\n",
    );
    let root = write(
        &tmp,
        "workflow.ci",
        "workflow ci {\n    use a ( path = ./a.ci )\n}\n",
    );
    assert!(
        loops_from(&root, tmp.path()).is_empty(),
        "the root is not in the loop"
    );
    assert_eq!(
        all_loops(&[root, a, b], tmp.path()),
        vec!["a.ci -> b.ci -> a.ci"]
    );
}
