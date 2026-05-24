//! E2E tests for `--files-from` flag across `find`, `lint`, `set`, `remove`, `append`, and `mv`.

use super::common::{hyalo_no_hints, md, write_md};
use std::fs;
use std::io::Write as _;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Vault fixture
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
"),
    );

    write_md(
        tmp.path(),
        "sub/gamma.md",
        md!(r"
---
title: Gamma
tags:
  - other
---
# Gamma
"),
    );

    tmp
}

/// Write a temp file containing the given lines and return its path.
fn write_list_file(lines: &[&str]) -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    for line in lines {
        writeln!(f, "{line}").unwrap();
    }
    f
}

// ---------------------------------------------------------------------------
// find --files-from
// ---------------------------------------------------------------------------

#[test]
fn find_files_from_file_path() {
    let tmp = setup_vault();
    let list = write_list_file(&["alpha.md"]);

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["find", "--files-from", list.path().to_str().unwrap()]);
    cmd.args(["--format", "json"]);

    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let total = envelope["total"].as_u64().unwrap();
    assert_eq!(total, 1);
    let results = envelope["results"].as_array().unwrap();
    assert_eq!(results[0]["file"], "alpha.md");
}

#[test]
fn find_files_from_multiple_files() {
    let tmp = setup_vault();
    let list = write_list_file(&["alpha.md", "beta.md"]);

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["find", "--files-from", list.path().to_str().unwrap()]);
    cmd.args(["--format", "json"]);

    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(envelope["total"].as_u64().unwrap(), 2);
}

#[test]
fn find_files_from_empty_input_exits_zero() {
    let tmp = setup_vault();
    let list = write_list_file(&[]);

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["find", "--files-from", list.path().to_str().unwrap()]);
    cmd.args(["--format", "json"]);

    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(envelope["total"].as_u64().unwrap(), 0);
    // files_from counters must be present and all zero
    assert_eq!(envelope["files_missing"].as_u64().unwrap(), 0);
    assert_eq!(envelope["files_skipped_non_md"].as_u64().unwrap(), 0);
    assert_eq!(envelope["files_skipped_outside_vault"].as_u64().unwrap(), 0);
}

#[test]
fn find_files_from_mixed_counters() {
    let tmp = setup_vault();
    let list = write_list_file(&[
        "alpha.md",      // valid
        "missing.md",    // missing
        "config.toml",   // non-md
        "../outside.md", // outside vault
    ]);

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["find", "--files-from", list.path().to_str().unwrap()]);
    cmd.args(["--format", "json"]);

    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        envelope["total"].as_u64().unwrap(),
        1,
        "only alpha.md should match"
    );
    assert_eq!(envelope["files_missing"].as_u64().unwrap(), 1);
    assert_eq!(envelope["files_skipped_non_md"].as_u64().unwrap(), 1);
    assert_eq!(envelope["files_skipped_outside_vault"].as_u64().unwrap(), 1);
}

#[test]
fn find_files_from_mutual_exclusion_with_glob_fails() {
    let tmp = setup_vault();
    let list = write_list_file(&["alpha.md"]);

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args([
        "find",
        "--files-from",
        list.path().to_str().unwrap(),
        "--glob",
        "**/*.md",
    ]);

    let out = cmd.output().unwrap();
    assert!(
        !out.status.success(),
        "expected failure from mutual exclusion"
    );
}

#[test]
fn find_files_from_non_md_skipped() {
    let tmp = setup_vault();
    // write a non-md file in the vault so it exists on disk but should be skipped
    fs::write(tmp.path().join("config.toml"), "[tool]\n").unwrap();
    let list = write_list_file(&["config.toml", "alpha.md"]);

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["find", "--files-from", list.path().to_str().unwrap()]);
    cmd.args(["--format", "json"]);

    let out = cmd.output().unwrap();
    assert!(out.status.success());
    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(envelope["total"].as_u64().unwrap(), 1);
    assert_eq!(envelope["files_skipped_non_md"].as_u64().unwrap(), 1);
}

// ---------------------------------------------------------------------------
// lint --files-from
// ---------------------------------------------------------------------------

#[test]
fn lint_files_from_single_file_happy_path() {
    let tmp = setup_vault();
    let list = write_list_file(&["alpha.md"]);

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["lint", "--files-from", list.path().to_str().unwrap()]);
    cmd.args(["--format", "json"]);

    let out = cmd.output().unwrap();
    // Exit 0 because alpha.md has no schema violations (no schema configured)
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(envelope["files_missing"].as_u64().unwrap(), 0);
    assert_eq!(envelope["files_skipped_non_md"].as_u64().unwrap(), 0);
}

#[test]
fn lint_files_from_empty_exits_zero() {
    let tmp = setup_vault();
    let list = write_list_file(&[]);

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["lint", "--files-from", list.path().to_str().unwrap()]);
    cmd.args(["--format", "json"]);

    let out = cmd.output().unwrap();
    assert!(out.status.success());
    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(envelope["files_missing"].as_u64().unwrap(), 0);
}

#[test]
fn lint_files_from_mutual_exclusion_with_type_fails() {
    let tmp = setup_vault();
    let list = write_list_file(&["alpha.md"]);

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args([
        "lint",
        "--files-from",
        list.path().to_str().unwrap(),
        "--type",
        "note",
    ]);

    let out = cmd.output().unwrap();
    assert!(
        !out.status.success(),
        "expected failure from mutual exclusion"
    );
}

#[test]
fn lint_files_from_missing_counted() {
    let tmp = setup_vault();
    let list = write_list_file(&["alpha.md", "nonexistent.md"]);

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["lint", "--files-from", list.path().to_str().unwrap()]);
    cmd.args(["--format", "json"]);

    let out = cmd.output().unwrap();
    assert!(out.status.success());
    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(envelope["files_missing"].as_u64().unwrap(), 1);
}

// ---------------------------------------------------------------------------
// set --files-from
// ---------------------------------------------------------------------------

#[test]
fn set_files_from_happy_path() {
    let tmp = setup_vault();
    let list = write_list_file(&["alpha.md"]);

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args([
        "set",
        "--property",
        "reviewed=true",
        "--files-from",
        list.path().to_str().unwrap(),
    ]);
    cmd.args(["--format", "json"]);

    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let content = fs::read_to_string(tmp.path().join("alpha.md")).unwrap();
    assert!(content.contains("reviewed: true"), "content:\n{content}");
}

#[test]
fn set_files_from_mutual_exclusion_with_file_fails() {
    let tmp = setup_vault();
    let list = write_list_file(&["alpha.md"]);

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args([
        "set",
        "--property",
        "x=1",
        "--files-from",
        list.path().to_str().unwrap(),
        "--file",
        "beta.md",
    ]);

    let out = cmd.output().unwrap();
    assert!(
        !out.status.success(),
        "expected failure from mutual exclusion"
    );
}

// ---------------------------------------------------------------------------
// remove --files-from
// ---------------------------------------------------------------------------

#[test]
fn remove_files_from_happy_path() {
    let tmp = setup_vault();
    let list = write_list_file(&["alpha.md"]);

    // alpha.md has status: planned — remove it
    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args([
        "remove",
        "--property",
        "status",
        "--files-from",
        list.path().to_str().unwrap(),
    ]);
    cmd.args(["--format", "json"]);

    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let content = fs::read_to_string(tmp.path().join("alpha.md")).unwrap();
    assert!(
        !content.contains("status:"),
        "status should be removed: {content}"
    );
}

// ---------------------------------------------------------------------------
// append --files-from
// ---------------------------------------------------------------------------

#[test]
fn append_files_from_happy_path() {
    let tmp = setup_vault();
    let list = write_list_file(&["alpha.md"]);

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args([
        "append",
        "--property",
        "aliases=note-a",
        "--files-from",
        list.path().to_str().unwrap(),
    ]);
    cmd.args(["--format", "json"]);

    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let content = fs::read_to_string(tmp.path().join("alpha.md")).unwrap();
    assert!(
        content.contains("note-a"),
        "aliases should contain note-a: {content}"
    );
}

// ---------------------------------------------------------------------------
// mv --files-from
// ---------------------------------------------------------------------------

#[test]
fn mv_files_from_batch_moves_files() {
    let tmp = setup_vault();
    let dest = tmp.path().join("archive");
    fs::create_dir(&dest).unwrap();

    let list = write_list_file(&["alpha.md"]);

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args([
        "mv",
        "--files-from",
        list.path().to_str().unwrap(),
        "--to",
        "archive/",
        "--apply",
    ]);
    cmd.args(["--format", "json"]);

    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // alpha.md should have moved
    assert!(!tmp.path().join("alpha.md").exists());
    assert!(tmp.path().join("archive/alpha.md").exists());
}

// ---------------------------------------------------------------------------
// strip leading ./ from input
// ---------------------------------------------------------------------------

#[test]
fn find_files_from_strips_leading_dot_slash() {
    let tmp = setup_vault();
    let list = write_list_file(&["./alpha.md"]);

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["find", "--files-from", list.path().to_str().unwrap()]);
    cmd.args(["--format", "json"]);

    let out = cmd.output().unwrap();
    assert!(out.status.success());
    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(envelope["total"].as_u64().unwrap(), 1);
}
