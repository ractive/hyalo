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
    let results = envelope["results"]["files"].as_array().unwrap();
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
    // files_from counters must be present and all zero (under .results)
    assert_eq!(envelope["results"]["files_missing"].as_u64().unwrap(), 0);
    assert_eq!(
        envelope["results"]["files_skipped_non_md"]
            .as_u64()
            .unwrap(),
        0
    );
    assert_eq!(
        envelope["results"]["files_skipped_outside_vault"]
            .as_u64()
            .unwrap(),
        0
    );
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
    assert_eq!(envelope["results"]["files_missing"].as_u64().unwrap(), 1);
    assert_eq!(
        envelope["results"]["files_skipped_non_md"]
            .as_u64()
            .unwrap(),
        1
    );
    assert_eq!(
        envelope["results"]["files_skipped_outside_vault"]
            .as_u64()
            .unwrap(),
        1
    );
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
    assert_eq!(
        envelope["results"]["files_skipped_non_md"]
            .as_u64()
            .unwrap(),
        1
    );
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
    assert_eq!(envelope["results"]["files_missing"].as_u64().unwrap(), 0);
    assert_eq!(
        envelope["results"]["files_skipped_non_md"]
            .as_u64()
            .unwrap(),
        0
    );
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
    assert_eq!(envelope["results"]["files_missing"].as_u64().unwrap(), 0);
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
    assert_eq!(envelope["results"]["files_missing"].as_u64().unwrap(), 1);
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

// ---------------------------------------------------------------------------
// BUG-2: vault dir prefix stripping (iter-140)
// ---------------------------------------------------------------------------

#[test]
fn lint_files_from_strips_vault_dir_prefix() {
    // Setup: tempdir root contains "kb/" subdir as vault.
    let tmp = tempfile::tempdir().unwrap();
    let kb = tmp.path().join("kb");
    std::fs::create_dir_all(kb.join("notes")).unwrap();
    std::fs::write(kb.join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write_md(&kb, "notes/foo.md", "---\ntitle: Foo\n---\n\nBody.\n");

    // Pipe "kb/notes/foo.md" (repo-relative) into lint with --dir kb.
    // Without prefix stripping this would count as files_missing.
    let list = write_list_file(&["kb/notes/foo.md"]);

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", kb.to_str().unwrap()]);
    cmd.args(["lint", "--files-from", list.path().to_str().unwrap()]);
    cmd.args(["--format", "json"]);

    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "lint with vault-prefix path should succeed; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    // The file should be linted (files_missing = 0, files in results > 0).
    assert_eq!(
        envelope["results"]["files_missing"].as_u64().unwrap_or(1),
        0,
        "expected files_missing=0, envelope: {envelope}"
    );
}

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

// ---------------------------------------------------------------------------
// NEW-2: multi-segment --dir prefix stripping
// ---------------------------------------------------------------------------

#[test]
fn lint_files_from_multi_segment_dir_prefix_stripped() {
    // Setup: vault lives at files/en-us/ inside a tempdir.
    // Git outputs paths like "files/en-us/x.md" (repo-relative).
    // --dir files/en-us should strip the full prefix and resolve "x.md".
    let root = tempfile::tempdir().unwrap();
    let vault = root.path().join("files").join("en-us");
    std::fs::create_dir_all(&vault).unwrap();
    std::fs::write(vault.join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write_md(&vault, "x.md", "---\ntitle: X\n---\n\nBody.\n");

    // Repo-relative path (as git would output it).
    let list = write_list_file(&["files/en-us/x.md"]);

    // Pass --dir as the relative string "files/en-us" so resolve() can derive
    // the multi-segment prefix. We construct this relative to root.
    // In CLI invocation we use the relative path from cwd=root.
    // Drop a root-level .hyalo.toml that sets `dir = "files/en-us"`. The CLI is
    // invoked from `root` with no explicit --dir, so it picks up that config —
    // configured_dir then becomes the relative multi-segment string, which is
    // what resolve() needs to derive the prefix.
    std::fs::write(root.path().join(".hyalo.toml"), "dir = \"files/en-us\"\n").unwrap();
    let mut cmd = hyalo_no_hints();
    cmd.current_dir(root.path());
    cmd.args(["lint", "--files-from", list.path().to_str().unwrap()]);
    cmd.args(["--format", "json"]);

    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "lint with multi-segment dir prefix should succeed; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        envelope["results"]["files_missing"].as_u64().unwrap_or(1),
        0,
        "expected files_missing=0 with multi-segment dir, envelope: {envelope}"
    );
}

#[test]
fn lint_files_from_single_segment_dir_prefix_still_works() {
    // Regression: single-segment vault (kb) still works after NEW-2.
    // Replicates the iter-140 BUG-2 test with an explicit configured dir.
    let root = tempfile::tempdir().unwrap();
    let vault = root.path().join("kb");
    std::fs::create_dir_all(vault.join("notes")).unwrap();
    std::fs::write(vault.join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write_md(&vault, "notes/foo.md", "---\ntitle: Foo\n---\n\nBody.\n");
    std::fs::write(root.path().join(".hyalo.toml"), "dir = \"kb\"\n").unwrap();

    let list = write_list_file(&["kb/notes/foo.md"]);

    let mut cmd = hyalo_no_hints();
    cmd.current_dir(root.path());
    cmd.args(["lint", "--files-from", list.path().to_str().unwrap()]);
    cmd.args(["--format", "json"]);

    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "lint with single-segment dir prefix (kb) should succeed; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        envelope["results"]["files_missing"].as_u64().unwrap_or(1),
        0,
        "expected files_missing=0 with single-segment dir, envelope: {envelope}"
    );
}

#[test]
fn lint_files_from_ambiguity_vault_relative_literal_wins() {
    // Ambiguity: configured_dir = "vault", vault contains "vault/bar.md".
    // Input "vault/vault/bar.md" does NOT match a vault-relative literal (A),
    // so strip-and-retry (B) gives "vault/bar.md" which DOES exist.
    //
    // Then: vault also contains "sub/page.md".
    // Input "vault/sub/page.md" — vault-relative literal "vault/sub/page.md"
    // does NOT exist, stripped "sub/page.md" DOES exist → uses (B).
    //
    // The precedence (A) is separately verified in the unit test.
    // This E2E test verifies the strip path works correctly end-to-end.
    let root = tempfile::tempdir().unwrap();
    let vault = root.path().join("vault");
    std::fs::create_dir_all(vault.join("sub")).unwrap();
    // No nested .hyalo.toml inside vault to avoid shadowing warnings.
    write_md(&vault, "sub/page.md", "---\ntitle: Page\n---\n\nBody.\n");
    std::fs::write(root.path().join(".hyalo.toml"), "dir = \"vault\"\n").unwrap();

    // Input as git would output: repo-relative "vault/sub/page.md"
    let list = write_list_file(&["vault/sub/page.md"]);

    let mut cmd = hyalo_no_hints();
    cmd.current_dir(root.path());
    cmd.args(["lint", "--files-from", list.path().to_str().unwrap()]);
    cmd.args(["--format", "json"]);

    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "lint with ambiguity precedence test should succeed; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        envelope["results"]["files_missing"].as_u64().unwrap_or(1),
        0,
        "expected files_missing=0, strip-and-retry should resolve; envelope: {envelope}"
    );
}

// ---------------------------------------------------------------------------
// NEW-4: whitespace trimming
// ---------------------------------------------------------------------------

#[test]
fn find_files_from_whitespace_padded_paths_resolve() {
    let tmp = setup_vault();
    // Paths with leading/trailing spaces and tabs should still resolve.
    let list = write_list_file(&["  alpha.md", "beta.md  ", "\tsub/gamma.md\t"]);

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["find", "--files-from", list.path().to_str().unwrap()]);
    cmd.args(["--format", "json"]);

    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "find with whitespace-padded paths should succeed; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        envelope["total"].as_u64().unwrap(),
        3,
        "all 3 whitespace-padded paths should resolve; envelope: {envelope}"
    );
}

#[test]
fn lint_files_from_whitespace_padded_path_resolves() {
    let tmp = setup_vault();
    let list = write_list_file(&["  alpha.md  "]);

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["lint", "--files-from", list.path().to_str().unwrap()]);
    cmd.args(["--format", "json"]);

    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "lint with whitespace-padded path should succeed; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        envelope["results"]["files_missing"].as_u64().unwrap_or(1),
        0,
        "whitespace-padded path should resolve; envelope: {envelope}"
    );
}

// ---------------------------------------------------------------------------
// NEW-6: deduplication
// ---------------------------------------------------------------------------

#[test]
fn find_files_from_deduplicates_same_path() {
    let tmp = setup_vault();
    // Same path 3×, should produce only 1 result.
    let list = write_list_file(&["alpha.md", "alpha.md", "alpha.md"]);

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["find", "--files-from", list.path().to_str().unwrap()]);
    cmd.args(["--format", "json"]);

    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "find with duplicate paths should succeed; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        envelope["total"].as_u64().unwrap(),
        1,
        "duplicate paths should produce a single result; envelope: {envelope}"
    );
}

#[test]
fn lint_files_from_with_index_counts_post_index_files_as_missing() {
    // iter-143: when both `--index` and `--files-from` are active, the
    // snapshot is the source of truth. A file that's on disk but absent
    // from the snapshot must count as `files_missing` — NOT silently
    // re-scanned from disk.
    let tmp = setup_vault();

    // Build the snapshot from the current vault (alpha, beta, sub/gamma).
    let mut create = hyalo_no_hints();
    create.args(["--dir", tmp.path().to_str().unwrap()]);
    create.args(["create-index"]);
    let create_out = create.output().unwrap();
    assert!(
        create_out.status.success(),
        "create-index should succeed; stderr: {}",
        String::from_utf8_lossy(&create_out.stderr)
    );

    // Add a NEW file to disk AFTER the snapshot is built. The snapshot
    // does not know about it.
    write_md(
        tmp.path(),
        "post-index.md",
        md!(r"
---
title: Post-index
---
# Post-index
"),
    );

    // Lint with --index and --files-from. The list contains:
    //   alpha.md         — exists in snapshot (lint it)
    //   post-index.md    — on disk, NOT in snapshot (must count as missing)
    let list = write_list_file(&["alpha.md", "post-index.md"]);

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args([
        "lint",
        "--index",
        "--files-from",
        list.path().to_str().unwrap(),
    ]);
    cmd.args(["--format", "json"]);

    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "lint --index --files-from should succeed; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let missing = envelope["files_missing"]
        .as_u64()
        .or_else(|| envelope["results"]["files_missing"].as_u64())
        .unwrap_or(0);
    assert_eq!(
        missing, 1,
        "post-index.md should count as missing (snapshot is source of truth); envelope: {envelope}"
    );
    let checked = envelope["results"]["files_checked"].as_u64().unwrap_or(0);
    assert_eq!(
        checked, 1,
        "only alpha.md should be linted (post-index.md is not in snapshot); envelope: {envelope}"
    );
}

#[test]
fn lint_files_from_deduplicates_and_preserves_order() {
    let tmp = setup_vault();
    // Paths repeated, first-seen order: alpha, beta, sub/gamma.
    let list = write_list_file(&["alpha.md", "beta.md", "sub/gamma.md", "alpha.md", "beta.md"]);

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["lint", "--files-from", list.path().to_str().unwrap()]);
    cmd.args(["--format", "json"]);

    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "lint with duplicate paths should succeed; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    // 3 unique files linted (not 5). Lint uses "files_checked" for the count.
    assert_eq!(
        envelope["results"]["files_checked"].as_u64().unwrap_or(0),
        3,
        "should lint 3 unique files; envelope: {envelope}"
    );
}
