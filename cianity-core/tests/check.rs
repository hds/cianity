//! Integration tests for `cianity_core::check::run`.
//!
//! Fixture files under `tests/fixtures/valid/` must pass `check` with no
//! errors.  Files under `tests/fixtures/invalid/` must fail `check` with at
//! least one error-level diagnostic.  Adding a `.ci` file to either directory
//! automatically includes it in the appropriate test.

use std::path::{Path, PathBuf};

use cianity_core::check;

// ── helpers ───────────────────────────────────────────────────────────────────

fn fixtures(sub: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(sub)
}

fn collect_ci_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("fixtures directory should exist")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "ci"))
        .collect();
    files.sort();
    files
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[test]
fn valid_fixtures_all_pass() {
    let files = collect_ci_files(&fixtures("valid"));
    assert!(
        !files.is_empty(),
        "no valid fixture files found in tests/fixtures/valid/"
    );

    let failures: Vec<String> = files
        .iter()
        .filter_map(|path| {
            check::run(path)
                .err()
                .map(|e| format!("{}: {e}", path.display()))
        })
        .collect();

    assert!(
        failures.is_empty(),
        "valid fixture files should pass `check`:\n{}",
        failures.join("\n")
    );
}

#[test]
fn invalid_fixtures_all_fail() {
    let files = collect_ci_files(&fixtures("invalid"));
    assert!(
        !files.is_empty(),
        "no invalid fixture files found in tests/fixtures/invalid/"
    );

    let unexpected_passes: Vec<String> = files
        .iter()
        .filter(|path| check::run(path).is_ok())
        .map(|path| path.display().to_string())
        .collect();

    assert!(
        unexpected_passes.is_empty(),
        "invalid fixture files should fail `check`:\n{}",
        unexpected_passes.join("\n")
    );
}
