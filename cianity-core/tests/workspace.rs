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
        "workflow ci {\n    use {\n        workflow ( location = ./shared.ci, name = shared, )\n    }\n\n    stage build {\n        job compile { cargo build }\n    }\n}\n",
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
        "workflow ci {\n    use {\n        workflow ( location = ./missing.ci, name = missing, )\n    }\n\n    stage build {\n        job compile { cargo build }\n    }\n}\n",
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
        "workflow ci {\n    use {\n        workflow ( location = ../shared/helpers.ci, name = helpers, )\n    }\n\n    stage build {\n        job compile { cargo build }\n    }\n}\n",
    );

    let refs = workspace::referenced_files(&root).unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(
        refs[0].canonicalize().unwrap(),
        tmp.path().join("shared/helpers.ci").canonicalize().unwrap()
    );
}
