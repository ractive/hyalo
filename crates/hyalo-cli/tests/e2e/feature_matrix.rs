//! E2E tests for the feature-fanout matrix runtime layer (Gate 2, runtime side).
//!
//! For each command in the `files_from_counters` envelope list, this test
//! runs `hyalo <cmd> --files-from -` with a fixture vault and asserts the
//! JSON envelope has the expected counter keys under `.results`.

use super::common::{md, write_md};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Fixture vault builder
// ---------------------------------------------------------------------------

fn setup_vault() -> TempDir {
    let tmp = tempfile::tempdir().unwrap();

    write_md(
        tmp.path(),
        "alpha.md",
        md!(r"
---
title: Alpha
status: planned
tags:
  - rust
---
# Alpha

Some content.
"),
    );

    write_md(
        tmp.path(),
        "beta.md",
        md!(r"
---
title: Beta
status: completed
tags:
  - rust
---
# Beta

Some content.
"),
    );

    tmp
}

/// Run `hyalo --dir <dir> <args...> --format json` with stdin input and return parsed JSON.
fn run_with_stdin(dir: &std::path::Path, cmd_args: &[&str], stdin_data: &str) -> serde_json::Value {
    let out = run_mutation_with_stdin(dir, cmd_args, stdin_data);
    assert!(
        out.status.success(),
        "hyalo exited non-zero\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "failed to parse JSON: {e}\nstdout: {}",
            String::from_utf8_lossy(&out.stdout)
        )
    })
}

/// Run a command with `stdin_data` piped in and return the raw output.
///
/// Goes through `assert_cmd::Command` (not raw `std::process::Command`)
/// because assert_cmd honors the `CARGO_TARGET_<TRIPLE>_RUNNER` configured by
/// cross/qemu in the aarch64 release matrix — a raw spawn of the target-arch
/// binary bypasses the runner and fails with `Exec format error` there.
fn run_mutation_with_stdin(
    dir: &std::path::Path,
    cmd_args: &[&str],
    stdin_data: &str,
) -> std::process::Output {
    assert_cmd::Command::cargo_bin("hyalo")
        .expect("hyalo binary")
        .args(["--no-hints", "--dir", dir.to_str().unwrap()])
        .args(cmd_args)
        .args(["--format", "json"])
        .write_stdin(stdin_data)
        .output()
        .expect("failed to run hyalo")
}

// ---------------------------------------------------------------------------
// find --files-from -
// ---------------------------------------------------------------------------

#[test]
fn find_files_from_stdin_has_counter_keys() {
    let tmp = setup_vault();
    let envelope = run_with_stdin(tmp.path(), &["find", "--files-from", "-"], "alpha.md\n");

    assert!(
        envelope["results"]["files_missing"].is_number(),
        "missing files_missing counter in results: {envelope}"
    );
    assert!(
        envelope["results"]["files_skipped_non_md"].is_number(),
        "missing files_skipped_non_md counter: {envelope}"
    );
    assert!(
        envelope["results"]["files_skipped_outside_vault"].is_number(),
        "missing files_skipped_outside_vault counter: {envelope}"
    );
    assert_eq!(
        envelope["results"]["files_missing"].as_u64().unwrap(),
        0,
        "expected no missing files: {envelope}"
    );
}

// ---------------------------------------------------------------------------
// lint --files-from -
// ---------------------------------------------------------------------------

#[test]
fn lint_files_from_stdin_has_counter_keys() {
    let tmp = setup_vault();
    let envelope = run_with_stdin(tmp.path(), &["lint", "--files-from", "-"], "alpha.md\n");

    assert!(
        envelope["results"]["files_missing"].is_number(),
        "missing files_missing counter in lint results: {envelope}"
    );
    assert!(
        envelope["results"]["files_skipped_non_md"].is_number(),
        "missing files_skipped_non_md counter: {envelope}"
    );
    assert!(
        envelope["results"]["files_skipped_outside_vault"].is_number(),
        "missing files_skipped_outside_vault counter: {envelope}"
    );
}

// ---------------------------------------------------------------------------
// set --files-from -
// ---------------------------------------------------------------------------

#[test]
fn set_files_from_stdin_succeeds() {
    // Use a fresh tempdir copy so the mutation doesn't affect other tests.
    let tmp = setup_vault();
    let out = run_mutation_with_stdin(
        tmp.path(),
        &["set", "--property", "reviewed=true", "--files-from", "-"],
        "alpha.md\n",
    );
    assert!(
        out.status.success(),
        "set --files-from - failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    // Verify the mutation was applied.
    let content = std::fs::read_to_string(tmp.path().join("alpha.md")).unwrap();
    assert!(
        content.contains("reviewed: true"),
        "mutation not applied:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// remove --files-from -
// ---------------------------------------------------------------------------

#[test]
fn remove_files_from_stdin_succeeds() {
    let tmp = setup_vault();
    let out = run_mutation_with_stdin(
        tmp.path(),
        &["remove", "--property", "status", "--files-from", "-"],
        "alpha.md\n",
    );
    assert!(
        out.status.success(),
        "remove --files-from - failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let content = std::fs::read_to_string(tmp.path().join("alpha.md")).unwrap();
    assert!(
        !content.contains("status:"),
        "status property should be removed:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// append --files-from -
// ---------------------------------------------------------------------------

#[test]
fn append_files_from_stdin_succeeds() {
    let tmp = setup_vault();
    let out = run_mutation_with_stdin(
        tmp.path(),
        &[
            "append",
            "--property",
            "aliases=note-alpha",
            "--files-from",
            "-",
        ],
        "alpha.md\n",
    );
    assert!(
        out.status.success(),
        "append --files-from - failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let content = std::fs::read_to_string(tmp.path().join("alpha.md")).unwrap();
    assert!(
        content.contains("note-alpha"),
        "aliases property should be appended:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// mv --files-from - (read-only dry-run to avoid ordering issues)
// ---------------------------------------------------------------------------

#[test]
fn mv_files_from_stdin_dry_run_succeeds() {
    let tmp = setup_vault();
    // Create archive dir so mv target exists.
    std::fs::create_dir(tmp.path().join("archive")).unwrap();

    let out = run_mutation_with_stdin(
        tmp.path(),
        &["mv", "--files-from", "-", "--to", "archive/"],
        "alpha.md\n",
    );
    // mv without --apply is dry-run by default — should succeed.
    assert!(
        out.status.success(),
        "mv --files-from - (dry-run) failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

// ---------------------------------------------------------------------------
// Stdin counter test: missing file is counted
// ---------------------------------------------------------------------------

#[test]
fn find_files_from_stdin_missing_file_counted() {
    let tmp = setup_vault();
    let envelope = run_with_stdin(
        tmp.path(),
        &["find", "--files-from", "-"],
        "nonexistent.md\n",
    );
    assert_eq!(
        envelope["results"]["files_missing"].as_u64().unwrap(),
        1,
        "expected files_missing=1: {envelope}"
    );
}
