/// E2E tests for `hyalo mv` batch mode (iter-135).
///
/// Tests T1–T14 from the iteration plan.
use super::common::{hyalo_no_hints, md, write_md};
use std::fs;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn t1_fixture() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "iterations/iteration-10-a.md",
        md!(r"
---
status: completed
type: iteration
---
Body A.
"),
    );
    write_md(
        tmp.path(),
        "iterations/iteration-11-b.md",
        md!(r"
---
status: completed
type: iteration
---
Body B.
"),
    );
    write_md(
        tmp.path(),
        "iterations/iteration-12-c.md",
        md!(r"
---
status: planned
type: iteration
---
Body C.
"),
    );
    write_md(
        tmp.path(),
        "notes/index.md",
        "See [[iterations/iteration-10-a]] and [[iterations/iteration-12-c]].\n",
    );
    tmp
}

// ---------------------------------------------------------------------------
// T1 — Glob-only batch, dry-run default
// ---------------------------------------------------------------------------

#[test]
fn t1_glob_only_batch_dry_run_default() {
    let tmp = t1_fixture();
    let dir = tmp.path().to_str().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", dir])
        .args([
            "mv",
            "--glob",
            "iterations/iteration-1*.md",
            "--to",
            "iterations/done/",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let results = &json["results"];
    assert_eq!(results["totals"]["moves"], 3, "should list 3 planned moves");
    assert_eq!(results["applied"], false, "dry-run is the default in batch");

    // No files should have changed on disk.
    assert!(
        tmp.path().join("iterations/iteration-10-a.md").exists(),
        "source files must not be moved in dry-run"
    );
    assert!(
        !tmp.path().join("iterations/done/").exists(),
        "destination dir must not be created in dry-run"
    );
}

// ---------------------------------------------------------------------------
// T2 — Glob ∩ property-filter intersection, --apply
// ---------------------------------------------------------------------------

#[test]
fn t2_glob_and_property_filter_apply() {
    let tmp = t1_fixture();
    let dir = tmp.path().to_str().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", dir])
        .args([
            "mv",
            "--glob",
            "iterations/iteration-1*.md",
            "--property",
            "status=completed",
            "--to",
            "iterations/done/",
            "--apply",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["totals"]["moves"], 2);
    assert_eq!(json["results"]["applied"], true);

    // Only completed files moved.
    assert!(
        tmp.path()
            .join("iterations/done/iteration-10-a.md")
            .exists()
    );
    assert!(
        tmp.path()
            .join("iterations/done/iteration-11-b.md")
            .exists()
    );
    // Planned-status file stays.
    assert!(tmp.path().join("iterations/iteration-12-c.md").exists());

    // Link in notes/index.md updated.
    let notes = fs::read_to_string(tmp.path().join("notes/index.md")).unwrap();
    assert!(
        notes.contains("[[iterations/done/iteration-10-a]]"),
        "moved file link must be rewritten: {notes}"
    );
    // Link to unmoved file unchanged.
    assert!(
        notes.contains("[[iterations/iteration-12-c]]"),
        "unmoved file link must remain: {notes}"
    );
}

// ---------------------------------------------------------------------------
// T3 — Property-filter batch (no glob)
// ---------------------------------------------------------------------------

#[test]
fn t3_property_filter_no_glob() {
    let tmp = t1_fixture();
    // Add a file that is status=completed but type=note (should NOT move).
    write_md(
        tmp.path(),
        "archive/old-note.md",
        md!(r"
---
status: completed
type: note
---
Old note.
"),
    );
    // Create destination dir.
    fs::create_dir_all(tmp.path().join("iterations/done")).unwrap();
    let dir = tmp.path().to_str().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", dir])
        .args([
            "mv",
            "--property",
            "status=completed",
            "--property",
            "type=iteration",
            "--to",
            "iterations/done/",
            "--apply",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Only iteration files moved.
    assert!(
        tmp.path()
            .join("iterations/done/iteration-10-a.md")
            .exists()
    );
    assert!(
        tmp.path()
            .join("iterations/done/iteration-11-b.md")
            .exists()
    );
    // Note file stays.
    assert!(tmp.path().join("archive/old-note.md").exists());
}

// ---------------------------------------------------------------------------
// T4 — Cross-batch link rewrite (A → B where both move)
// ---------------------------------------------------------------------------

#[test]
fn t4_cross_batch_link_rewrite() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "iterations/iteration-20-foo.md",
        md!(r"
---
status: completed
type: iteration
---
Related: [[iterations/iteration-21-bar]]
"),
    );
    write_md(
        tmp.path(),
        "iterations/iteration-21-bar.md",
        md!(r"
---
status: completed
type: iteration
---
Back: [[iterations/iteration-20-foo]]
"),
    );
    let dir = tmp.path().to_str().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", dir])
        .args([
            "mv",
            "--glob",
            "iterations/iteration-2*.md",
            "--to",
            "iterations/done/",
            "--apply",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Both files moved.
    assert!(
        tmp.path()
            .join("iterations/done/iteration-20-foo.md")
            .exists()
    );
    assert!(
        tmp.path()
            .join("iterations/done/iteration-21-bar.md")
            .exists()
    );

    let foo_content =
        fs::read_to_string(tmp.path().join("iterations/done/iteration-20-foo.md")).unwrap();
    assert!(
        foo_content.contains("[[iterations/done/iteration-21-bar]]"),
        "cross-batch link from foo to bar must be updated: {foo_content}"
    );
    assert!(
        !foo_content.contains("[[iterations/iteration-21-bar]]"),
        "old link must not remain in foo: {foo_content}"
    );

    let bar_content =
        fs::read_to_string(tmp.path().join("iterations/done/iteration-21-bar.md")).unwrap();
    assert!(
        bar_content.contains("[[iterations/done/iteration-20-foo]]"),
        "cross-batch link from bar to foo must be updated: {bar_content}"
    );
    assert!(
        !bar_content.contains("[[iterations/iteration-20-foo]]"),
        "old link must not remain in bar: {bar_content}"
    );
}

// ---------------------------------------------------------------------------
// T5 — Destination basename collision errors
// ---------------------------------------------------------------------------

#[test]
fn t5_basename_collision_errors() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a/dup.md",
        md!(r"
---
status: completed
---
"),
    );
    write_md(
        tmp.path(),
        "b/dup.md",
        md!(r"
---
status: completed
---
"),
    );
    fs::create_dir_all(tmp.path().join("archive")).unwrap();
    let dir = tmp.path().to_str().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", dir])
        .args([
            "mv",
            "--property",
            "status=completed",
            "--to",
            "archive/",
            "--apply",
        ])
        .output()
        .unwrap();

    // Must exit non-zero.
    assert!(
        !output.status.success(),
        "expected non-zero exit for collision"
    );

    // Neither source file should have moved.
    assert!(tmp.path().join("a/dup.md").exists(), "a/dup.md must remain");
    assert!(tmp.path().join("b/dup.md").exists(), "b/dup.md must remain");
    // archive/ must still be empty.
    let archive_entries: Vec<_> = fs::read_dir(tmp.path().join("archive")).unwrap().collect();
    assert!(archive_entries.is_empty(), "archive/ must be empty");
}

// ---------------------------------------------------------------------------
// T6 — --on-conflict=skip
// ---------------------------------------------------------------------------

#[test]
fn t6_on_conflict_skip() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a/dup.md",
        md!(r"
---
status: completed
---
A content.
"),
    );
    write_md(
        tmp.path(),
        "b/dup.md",
        md!(r"
---
status: completed
---
B content.
"),
    );
    fs::create_dir_all(tmp.path().join("archive")).unwrap();
    let dir = tmp.path().to_str().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", dir])
        .args([
            "mv",
            "--property",
            "status=completed",
            "--to",
            "archive/",
            "--on-conflict",
            "skip",
            "--apply",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let results = &json["results"];

    // Exactly one moved, one in skipped list.
    assert_eq!(results["totals"]["moves"], 1, "exactly 1 should move");
    let skipped = results["skipped"].as_array().unwrap();
    assert_eq!(skipped.len(), 1, "exactly 1 should be skipped");

    // The skipped entry must name the lexicographically-larger source (b/dup.md).
    let skipped_paths: Vec<&str> = skipped.iter().map(|v| v.as_str().unwrap()).collect();
    assert!(
        skipped_paths.iter().any(|s| s.contains("b/dup.md")),
        "b/dup.md must be in skipped list, got: {skipped_paths:?}"
    );

    // archive/dup.md exists (the lexicographically first source, a/dup.md).
    assert!(
        tmp.path().join("archive/dup.md").exists(),
        "archive/dup.md must exist"
    );

    // The surviving destination content must match the lex-first source (a/dup.md → "A content.").
    let dest_content = fs::read_to_string(tmp.path().join("archive/dup.md")).unwrap();
    assert!(
        dest_content.contains("A content."),
        "archive/dup.md should contain a/dup.md's content, got: {dest_content}"
    );
}

// ---------------------------------------------------------------------------
// T7 — Pre-existing file in destination errors
// ---------------------------------------------------------------------------

#[test]
fn t7_preexisting_file_errors() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "iterations/iteration-30-x.md",
        md!(r"
---
status: completed
---
Original.
"),
    );
    // Pre-create the destination file.
    write_md(
        tmp.path(),
        "iterations/done/iteration-30-x.md",
        "Already here.\n",
    );
    let dir = tmp.path().to_str().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", dir])
        .args([
            "mv",
            "--glob",
            "iterations/iteration-30-x.md",
            "--to",
            "iterations/done/",
            "--apply",
        ])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected non-zero exit for pre-existing destination"
    );

    // Source file must still be at original path.
    assert!(tmp.path().join("iterations/iteration-30-x.md").exists());
    // Pre-existing destination must be unmodified.
    let dest_content =
        fs::read_to_string(tmp.path().join("iterations/done/iteration-30-x.md")).unwrap();
    assert_eq!(dest_content, "Already here.\n");
}

// ---------------------------------------------------------------------------
// T8 — Empty selection errors out
// ---------------------------------------------------------------------------

#[test]
fn t8_empty_selection_errors() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "notes/index.md",
        md!(r"
---
status: draft
---
Index.
"),
    );
    let dir = tmp.path().to_str().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", dir])
        .args([
            "mv",
            "--property",
            "status=completed",
            "--to",
            "archive/",
            "--apply",
        ])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "empty selection must exit non-zero"
    );

    // archive/ must not be created.
    assert!(!tmp.path().join("archive").exists());
}

// ---------------------------------------------------------------------------
// T9 — --to must be a directory in batch mode
// ---------------------------------------------------------------------------

#[test]
fn t9_to_must_be_directory_in_batch() {
    let tmp = t1_fixture();
    let dir = tmp.path().to_str().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", dir])
        .args([
            "mv",
            "--property",
            "status=completed",
            "--to",
            "iterations/done.md",
            "--apply",
        ])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "batch --to with .md suffix must fail"
    );

    // No files moved.
    assert!(tmp.path().join("iterations/iteration-10-a.md").exists());
}

// ---------------------------------------------------------------------------
// T10 — Single-file mv behavior unchanged
// ---------------------------------------------------------------------------

#[test]
fn t10_single_file_mv_unchanged() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "old.md", "Content.\n");
    write_md(tmp.path(), "notes/index.md", "See [[old]] here.\n");
    let dir = tmp.path().to_str().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", dir])
        .args(["mv", "old.md", "--to", "new.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // JSON shape matches pre-135 single-file output.
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["from"], "old.md");
    assert_eq!(json["results"]["to"], "new.md");
    assert_eq!(json["results"]["dry_run"], false);

    // File moved.
    assert!(!tmp.path().join("old.md").exists());
    assert!(tmp.path().join("new.md").exists());

    // Link rewritten.
    let notes = fs::read_to_string(tmp.path().join("notes/index.md")).unwrap();
    assert!(notes.contains("[[new]]"), "link must be rewritten: {notes}");
}

// ---------------------------------------------------------------------------
// T11 — --glob negation excludes paths
// ---------------------------------------------------------------------------

#[test]
fn t11_glob_negation() {
    let tmp = t1_fixture();
    // Add a 4th file that should be excluded via negation.
    write_md(
        tmp.path(),
        "iterations/iteration-99-keep.md",
        md!(r"
---
status: completed
type: iteration
---
Keep me.
"),
    );
    let dir = tmp.path().to_str().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", dir])
        .args([
            "mv",
            "--glob",
            "iterations/iteration-*.md",
            "--glob",
            "!iterations/iteration-99-*.md",
            "--property",
            "status=completed",
            "--to",
            "iterations/done/",
            "--apply",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Negation-excluded file must stay.
    assert!(
        tmp.path().join("iterations/iteration-99-keep.md").exists(),
        "negated file must not move"
    );
    // Completed files that pass the filter moved.
    assert!(
        tmp.path()
            .join("iterations/done/iteration-10-a.md")
            .exists()
    );
    assert!(
        tmp.path()
            .join("iterations/done/iteration-11-b.md")
            .exists()
    );
}

// ---------------------------------------------------------------------------
// T12 — Frontmatter wikilink rewrite (related: fields)
// ---------------------------------------------------------------------------

#[test]
fn t12_frontmatter_wikilink_rewrite() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "iterations/iteration-40-host.md",
        md!(r#"
---
status: planned
type: iteration
related:
  - "[[iterations/iteration-41-dep]]"
---
Host body.
"#),
    );
    write_md(
        tmp.path(),
        "iterations/iteration-41-dep.md",
        md!(r"
---
status: completed
type: iteration
---
Dep body.
"),
    );
    let dir = tmp.path().to_str().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", dir])
        .args([
            "mv",
            "--property",
            "status=completed",
            "--to",
            "iterations/done/",
            "--apply",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Dep file moved.
    assert!(
        tmp.path()
            .join("iterations/done/iteration-41-dep.md")
            .exists()
    );
    // Host file stayed (status=planned).
    assert!(tmp.path().join("iterations/iteration-40-host.md").exists());

    // Host's frontmatter `related:` wikilink must be updated.
    let host_content =
        fs::read_to_string(tmp.path().join("iterations/iteration-40-host.md")).unwrap();
    assert!(
        host_content.contains("[[iterations/done/iteration-41-dep]]"),
        "frontmatter wikilink must be rewritten: {host_content}"
    );
    assert!(
        !host_content.contains("[[iterations/iteration-41-dep]]"),
        "old frontmatter wikilink must be gone: {host_content}"
    );
}

// ---------------------------------------------------------------------------
// T13 — Rollback on mid-batch rename failure
// ---------------------------------------------------------------------------

#[test]
fn t13_rollback_on_rename_failure() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "iterations/alpha.md",
        md!(r"
---
status: completed
---
Alpha.
"),
    );
    write_md(
        tmp.path(),
        "iterations/beta.md",
        md!(r"
---
status: completed
---
Beta.
"),
    );

    // Pre-create a DIRECTORY at the second destination path to cause rename failure.
    // We need to know which file sorts second lexicographically.
    // alpha.md < beta.md, so alpha moves first, then beta fails.
    // Create a directory at iterations/done/beta.md to block the rename.
    fs::create_dir_all(tmp.path().join("iterations/done/beta.md")).unwrap();
    let dir = tmp.path().to_str().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", dir])
        .args([
            "mv",
            "--glob",
            "iterations/*.md",
            "--property",
            "status=completed",
            "--to",
            "iterations/done/",
            "--apply",
        ])
        .output()
        .unwrap();

    // Must exit non-zero due to rename failure.
    assert!(!output.status.success(), "rollback test must exit non-zero");

    // Both files must be rolled back to original positions.
    assert!(
        tmp.path().join("iterations/alpha.md").exists(),
        "alpha.md must be rolled back"
    );
    assert!(
        tmp.path().join("iterations/beta.md").exists(),
        "beta.md must be rolled back"
    );
}

// ---------------------------------------------------------------------------
// T14 — Single-graph-build performance smoke test
// ---------------------------------------------------------------------------

#[test]
fn t14_single_graph_build_perf_smoke() {
    let tmp = TempDir::new().unwrap();

    // Create 50 completed iteration files.
    for i in 0..50 {
        write_md(
            tmp.path(),
            &format!("iterations/iteration-{i:03}.md"),
            &format!("---\nstatus: completed\ntype: iteration\n---\nIteration {i}.\n"),
        );
    }

    // Create 200 files that link to the 50 iteration files.
    for i in 0..200 {
        let target_idx = i % 50;
        let content =
            format!("---\nstatus: active\n---\nSee [[iterations/iteration-{target_idx:03}]].\n");
        write_md(tmp.path(), &format!("notes/note-{i:03}.md"), &content);
    }

    let dir = tmp.path().to_str().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", dir])
        .args([
            "mv",
            "--property",
            "status=completed",
            "--to",
            "iterations/done/",
            "--apply",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["totals"]["moves"], 50, "all 50 must move");
    assert_eq!(json["results"]["applied"], true);

    // Spot-check: first and last iteration files moved.
    assert!(tmp.path().join("iterations/done/iteration-000.md").exists());
    assert!(tmp.path().join("iterations/done/iteration-049.md").exists());

    // Spot-check: referencing notes updated.
    let note0 = fs::read_to_string(tmp.path().join("notes/note-000.md")).unwrap();
    assert!(
        note0.contains("[[iterations/done/iteration-000]]"),
        "note-000 link must be updated: {note0}"
    );
}

// ---------------------------------------------------------------------------
// T_NO_SELECTOR — mv with no FILE and no selectors is a UserError
// ---------------------------------------------------------------------------

#[test]
fn t_no_selector_rejected() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().to_str().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", dir])
        .args(["mv", "--to", "archive/"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected non-zero exit when no source is provided"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stderr}{stdout}");
    assert!(
        combined.contains("no source selection provided"),
        "error must mention 'no source selection provided', got: {combined}"
    );
}

// ---------------------------------------------------------------------------
// T_TO_SLASH — mv --to ./ is rejected (empty dest after normalization)
// ---------------------------------------------------------------------------

#[test]
fn t_to_slash_rejected() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\nstatus: done\n---\nBody.\n");
    let dir = tmp.path().to_str().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", dir])
        .args(["mv", "--property", "status=done", "--to", "./"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected non-zero exit for --to ./"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stderr}{stdout}");
    assert!(
        combined.contains("destination directory cannot be empty"),
        "error must mention 'destination directory cannot be empty', got: {combined}"
    );
}

// ---------------------------------------------------------------------------
// T_CONFLICT_GLOB_FILE — positional FILE + --glob is rejected by clap
// ---------------------------------------------------------------------------

#[test]
fn t_file_positional_and_glob_rejected() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "foo.md", "---\nstatus: x\n---\n");
    let dir = tmp.path().to_str().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", dir])
        .args(["mv", "foo.md", "--glob", "*.md", "--to", "archive/"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected non-zero exit when FILE and --glob both provided"
    );
}

// ---------------------------------------------------------------------------
// T_WIKILINK_ALIAS — batch mv preserves alias in wikilinks
// ---------------------------------------------------------------------------

#[test]
fn t_wikilink_alias_preserved_in_batch() {
    let tmp = TempDir::new().unwrap();

    // foo.md is referenced with an alias.
    write_md(
        tmp.path(),
        "foo.md",
        md!(r"
---
status: active
---
Content of foo.
"),
    );
    write_md(
        tmp.path(),
        "ref.md",
        "---\nstatus: other\n---\nSee [[foo|My Alias]] for details.\n",
    );
    let dir = tmp.path().to_str().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", dir])
        .args([
            "mv",
            "--property",
            "status=active",
            "--to",
            "archive/",
            "--apply",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // archive/foo.md must exist.
    assert!(
        tmp.path().join("archive/foo.md").exists(),
        "archive/foo.md must exist after batch mv"
    );

    // ref.md must have the link updated with alias preserved.
    let ref_content = fs::read_to_string(tmp.path().join("ref.md")).unwrap();
    assert!(
        ref_content.contains("[[archive/foo|My Alias]]"),
        "alias must be preserved in updated link, got: {ref_content}"
    );
}
