//! Snapshot tests for `cianity_core::build`.
//!
//! Each `.ci` file in `tests/fixtures/build/` must have a corresponding
//! `.gitlab-ci.yml` snapshot.  The test renders the workflow with the GitLab CI
//! target and asserts byte-for-byte equality with the snapshot file.
//!
//! To regenerate a snapshot after an intentional change to the renderer, run:
//!
//! ```text
//! cianity build <fixture>.ci -t gitlab -o <fixture>.gitlab-ci.yml
//! ```
//!
//! ## Coverage map
//!
//! | Fixture | Features exercised |
//! |---|---|
//! | `simple_stage` | basic inline job; plain job name |
//! | `multiline_job` | multi-line `- \|` block scalar; bare step reference; image |
//! | `template_and_inherit` | `steps` keyword expansion; step body override; unquoted `'` in script |
//! | `workflow_import` | `use {}` block handled gracefully; stage-level attr ignored |
//! | `job_dependencies` | `needs:` section; image; unquoted `--`/`-D` dashes in script |
//! | `name_conflict` | `stage.job` qualified names when the same job name appears in multiple stages; qualified names in `needs:` |

use std::path::{Path, PathBuf};

use cianity_core::build;

// ── helpers ───────────────────────────────────────────────────────────────────

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("build")
}

fn collect_ci_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("build fixtures directory should exist")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "ci"))
        .collect();
    files.sort();
    files
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[test]
fn gitlab_ci_snapshots() {
    let dir = fixtures_dir();
    let ci_files = collect_ci_files(&dir);
    assert!(
        !ci_files.is_empty(),
        "no .ci fixture files found in tests/fixtures/build/"
    );

    let mut failures: Vec<String> = Vec::new();

    for ci_path in &ci_files {
        let stem = ci_path
            .file_stem()
            .expect("ci fixture path always has a stem")
            .to_string_lossy();
        let snapshot_path = ci_path.with_file_name(format!("{stem}.gitlab-ci.yml"));

        assert!(
            snapshot_path.exists(),
            "missing snapshot: {} — generate with: cianity build {} -t gitlab -o {}",
            snapshot_path.display(),
            ci_path.display(),
            snapshot_path.display(),
        );

        let source = std::fs::read_to_string(ci_path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", ci_path.display()));
        let expected = std::fs::read_to_string(&snapshot_path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", snapshot_path.display()));

        let actual = build::render_to_string(&source, build::Target::Gitlab)
            .unwrap_or_else(|e| panic!("{}: build failed: {e}", ci_path.display()));

        if actual != expected {
            failures.push(format!(
                "snapshot mismatch for {stem}.ci\n\
                 --- expected ({stem}.gitlab-ci.yml) ---\n\
                 {expected}\
                 --- actual ---\n\
                 {actual}\
                 ---"
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} snapshot(s) did not match:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

#[test]
fn build_fails_on_parse_errors() {
    let err = build::render_to_string("stage {", build::Target::Gitlab)
        .expect_err("a file with parse errors should not produce output");
    assert!(
        err.to_string().contains("parse errors"),
        "unexpected error message: {err}"
    );
}
