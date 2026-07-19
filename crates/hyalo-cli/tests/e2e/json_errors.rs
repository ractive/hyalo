//! Regression tests for iter-158 finding H-6 (argument-validation errors must
//! respect --format, not just eprintln! plain text), finding M (raw-output
//! control-character sanitization), and finding [19] (`links fix --index`
//! leaving the persisted link graph stale).
//!
//! H-6's root pattern was `CommandOutcome::UserError(format!("Error: {e}"))`
//! sites in dispatch.rs and bare `eprintln!`s in run.rs that never checked
//! the requested output format — meaning `--format json` (and the default
//! piped format) emitted plain text instead of `{"error": ...}` on stderr.

use super::common::{hyalo_no_hints, md, write_md};
use tempfile::TempDir;

fn setup_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Test
---
# Body
Some text.
"),
    );
    tmp
}

/// Assert `stderr` is a JSON object with a non-empty `.error` string field.
fn assert_json_error(stderr: &[u8]) -> serde_json::Value {
    let text = String::from_utf8_lossy(stderr);
    let json: serde_json::Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("stderr is not valid JSON: {e}\nstderr: {text}"));
    assert!(
        json.get("error")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|s| !s.is_empty()),
        "expected non-empty .error field, got: {json}"
    );
    json
}

// ---------------------------------------------------------------------------
// find: argument-validation errors under --format json
// ---------------------------------------------------------------------------

#[test]
fn find_bad_task_filter_is_json_under_format_json() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "json"])
        .args(["find", "--task", "???"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
    let json = assert_json_error(&output.stderr);
    assert!(json["error"].as_str().unwrap().contains("task filter"));
}

#[test]
fn find_bad_fields_is_json_under_format_json() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "json"])
        .args(["find", "--fields", "nosuchfield"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let json = assert_json_error(&output.stderr);
    assert!(json["error"].as_str().unwrap().contains("nosuchfield"));
}

#[test]
fn find_bad_sort_is_json_under_format_json() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "json"])
        .args(["find", "--sort", "nosuchsort"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let json = assert_json_error(&output.stderr);
    assert!(json["error"].as_str().unwrap().contains("nosuchsort"));
}

#[test]
fn find_bad_tag_is_json_under_format_json() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "json"])
        .args(["find", "--tag", "has space"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_json_error(&output.stderr);
}

#[test]
fn find_bad_section_regex_is_json_under_format_json() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "json"])
        .args(["find", "--section", "/[/"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_json_error(&output.stderr);
}

#[test]
fn find_bad_property_regex_is_json_under_format_json() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "json"])
        .args(["find", "--property", "title~=/[/"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_json_error(&output.stderr);
}

// ---------------------------------------------------------------------------
// set / remove: --where-property errors under --format json
// ---------------------------------------------------------------------------

#[test]
fn set_bad_where_property_is_json_under_format_json() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "json"])
        .args([
            "set",
            "--property",
            "x=1",
            "--where-property",
            "title~=/[/",
            "--glob",
            "**/*.md",
            "--dry-run",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_json_error(&output.stderr);
}

#[test]
fn remove_bad_where_property_is_json_under_format_json() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "json"])
        .args(["remove", "--property", "x", "--where-property", "p~=/[/"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_json_error(&output.stderr);
}

// ---------------------------------------------------------------------------
// mv: batch-mode filter-parsing errors under --format json
// ---------------------------------------------------------------------------

#[test]
fn mv_bad_where_filter_is_json_under_format_json() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "json"])
        .args([
            "mv",
            "--property",
            "p~=/[/",
            "--to",
            "renamed/",
            "--dry-run",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_json_error(&output.stderr);
}

#[test]
fn mv_no_source_selection_is_json_under_format_json() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "json"])
        .args(["mv", "--to", "renamed/"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let json = assert_json_error(&output.stderr);
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("no source selection")
    );
}

// ---------------------------------------------------------------------------
// views run: filter re-validation errors under --format json (same code
// pattern as `find`, exercised via a saved view instead of CLI flags —
// these were among the sites the review flagged as "links/mv where-filter
// parsing" line numbers, which had drifted onto this block).
// ---------------------------------------------------------------------------

#[test]
fn views_run_bad_tag_override_is_json_under_format_json() {
    let tmp = setup_vault();
    let set_output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["views", "set", "myview", "--property", "title=Test"])
        .output()
        .unwrap();
    assert!(
        set_output.status.success(),
        "views set failed: {set_output:?}"
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "json"])
        .args(["views", "run", "myview", "--tag", "has space"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_json_error(&output.stderr);
}

// ---------------------------------------------------------------------------
// run.rs-level errors: --dir validation, unknown view, --count/--jq conflicts
// ---------------------------------------------------------------------------

#[test]
fn dir_missing_is_json_under_format_json() {
    let tmp = TempDir::new().unwrap();
    let nonexistent = tmp.path().join("does_not_exist");
    let output = hyalo_no_hints()
        .args(["--dir", nonexistent.to_str().unwrap()])
        .args(["--format", "json"])
        .args(["find"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
    let json = assert_json_error(&output.stderr);
    assert!(json["error"].as_str().unwrap().contains("does not exist"));
}

#[test]
fn dir_missing_is_text_under_format_text() {
    let tmp = TempDir::new().unwrap();
    let nonexistent = tmp.path().join("does_not_exist");
    let output = hyalo_no_hints()
        .args(["--dir", nonexistent.to_str().unwrap()])
        .args(["--format", "text"])
        .args(["find"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("Error: --dir path"),
        "expected human-readable text, got: {stderr}"
    );
    assert!(serde_json::from_str::<serde_json::Value>(&stderr).is_err());
}

#[test]
fn dir_is_a_file_is_json_under_format_json() {
    let tmp = setup_vault();
    let file_path = tmp.path().join("note.md");
    let output = hyalo_no_hints()
        .args(["--dir", file_path.to_str().unwrap()])
        .args(["--format", "json"])
        .args(["find"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let json = assert_json_error(&output.stderr);
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("is a file, not a directory")
    );
}

#[test]
fn unknown_view_is_json_under_format_json() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "json"])
        .args(["find", "--view", "nosuchview"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let json = assert_json_error(&output.stderr);
    assert!(json["error"].as_str().unwrap().contains("unknown view"));
}

#[test]
fn unknown_view_is_text_under_format_text() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "text"])
        .args(["find", "--view", "nosuchview"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown view"), "stderr: {stderr}");
    assert!(serde_json::from_str::<serde_json::Value>(&stderr).is_err());
}

#[test]
fn count_with_jq_conflict_is_json_by_default() {
    let tmp = setup_vault();
    // No explicit --format: --jq forces JSON internally, so the conflict
    // error must also be JSON (this is the exact H-6 repro).
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--count", "--jq", ".total"])
        .args(["find"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1)); // iter-181 task 2: user error
    let json = assert_json_error(&output.stderr);
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("--count cannot be combined with --jq")
    );
}

#[test]
fn jq_with_format_text_conflict_is_json_under_format_json_precedence() {
    let tmp = setup_vault();
    // --format json + --jq + (irrelevant since format already json) — the
    // interesting case is --format text + --jq, which must still render the
    // *conflict error itself* as text since `format` at that point is Text.
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "text"])
        .args(["--jq", ".total"])
        .args(["tags", "summary"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1)); // iter-181 task 2: user error
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--jq cannot be combined with --format text"));
    assert!(serde_json::from_str::<serde_json::Value>(&stderr).is_err());
}

#[test]
fn count_unsupported_command_is_json_under_format_json_success_arm() {
    // Exercises OutputPipeline::finalize's Success arm (total == None) --
    // e.g. `set` never returns a `total`, so --count is rejected there.
    let tmp = setup_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "json"])
        .args(["--count", "set", "--property", "x=1", "--file", "note.md"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1)); // iter-181 task 2: user error
    let json = assert_json_error(&output.stderr);
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("only supported for list commands")
    );
}

#[test]
fn count_unsupported_command_is_json_under_format_json_raw_output_arm() {
    // Exercises OutputPipeline::finalize's RawOutput arm -- `read` bypasses
    // the JSON pipeline entirely in text mode.
    let tmp = setup_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "json"])
        .args(["--count", "read", "note.md"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1)); // iter-181 task 2: user error
    assert_json_error(&output.stderr);
}

#[test]
fn count_unsupported_command_is_json_under_format_json_pre_dispatch() {
    // Exercises the run.rs early rejection (before Init/Deinit/Completion/
    // Config dispatch) -- these commands never reach the output pipeline.
    let tmp = setup_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "json"])
        .args(["--count", "config"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1)); // iter-181 task 2: user error
    let json = assert_json_error(&output.stderr);
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("only supported for list commands")
    );
}

#[test]
fn count_unsupported_command_is_text_under_format_text() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "text"])
        .args(["--count", "read", "note.md"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("only supported for list commands"));
    assert!(serde_json::from_str::<serde_json::Value>(&stderr).is_err());
}

// ---------------------------------------------------------------------------
// read: control-character sanitization (finding M)
// ---------------------------------------------------------------------------

#[test]
fn read_text_mode_strips_raw_control_bytes() {
    let tmp = TempDir::new().unwrap();
    // Body contains a raw ANSI color escape (\x1b[31m ... \x1b[0m) and a bell
    // (\x07). Neither byte should reach the terminal in text mode.
    write_md(
        tmp.path(),
        "evil.md",
        "---\ntitle: Evil\n---\n# Body\nline with \x1b[31mred\x1b[0m and bell \x07 end\n",
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "text"])
        .args(["read", "evil.md"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        !output.stdout.contains(&0x1b),
        "raw ESC byte leaked into text-mode stdout"
    );
    assert!(
        !output.stdout.contains(&0x07),
        "raw BEL byte leaked into text-mode stdout"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("red"),
        "sanitization should not eat surrounding text"
    );
}

#[test]
fn read_json_mode_escapes_control_bytes() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "evil.md",
        "---\ntitle: Evil\n---\n# Body\nline with \x1b[31mred\x1b[0m and bell \x07 end\n",
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "json"])
        .args(["read", "evil.md"])
        .output()
        .unwrap();
    assert!(output.status.success());
    // No raw control bytes on the wire...
    assert!(!output.stdout.contains(&0x1b));
    assert!(!output.stdout.contains(&0x07));
    // ...but the content is preserved, JSON-escaped.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let content = json["results"]["content"].as_str().unwrap();
    assert!(
        content.contains('\u{1b}'),
        "escaped ESC should round-trip through JSON"
    );
    assert!(
        content.contains('\u{7}'),
        "escaped BEL should round-trip through JSON"
    );
}

// ---------------------------------------------------------------------------
// links fix --index: persisted LinkGraph must be patched, not just the entry
// (finding [19])
// ---------------------------------------------------------------------------

#[test]
fn links_fix_apply_index_keeps_backlinks_graph_in_sync() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        md!(r"
---
title: A
---
# A
See [[B]] for more.
"),
    );
    write_md(
        tmp.path(),
        "b.md",
        md!(r"
---
title: B
---
# B
Nothing here.
"),
    );

    let dir = tmp.path().to_str().unwrap();

    let create = hyalo_no_hints()
        .args(["--dir", dir])
        .args(["--format", "json"])
        .args(["create-index"])
        .output()
        .unwrap();
    assert!(create.status.success(), "create-index failed: {create:?}");

    // Case-mismatch fix: [[B]] -> [[b]] (b.md is the on-disk file).
    let fix = hyalo_no_hints()
        .args(["--dir", dir])
        .args(["--format", "json"])
        .args(["links", "fix", "--apply", "--index"])
        .output()
        .unwrap();
    assert!(fix.status.success(), "links fix failed: {fix:?}");
    let fix_json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&fix.stdout)).unwrap();
    assert_eq!(
        fix_json["results"]["case_mismatches"], 1,
        "expected exactly one case-mismatch fix"
    );

    // The on-disk file was rewritten...
    let a_contents = std::fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(
        a_contents.contains("[[b]]"),
        "expected rewritten target on disk: {a_contents}"
    );

    // ...and a FRESH process querying backlinks --index must see the same
    // thing the live (non-indexed) scan sees — before the fix, refresh_entry
    // alone left the persisted LinkGraph pointing at the stale "B" target,
    // so this backlinks --index query returned zero results.
    let indexed = hyalo_no_hints()
        .args(["--dir", dir])
        .args(["--format", "json"])
        .args(["backlinks", "b.md", "--index"])
        .output()
        .unwrap();
    assert!(
        indexed.status.success(),
        "backlinks --index failed: {indexed:?}"
    );
    let indexed_json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&indexed.stdout)).unwrap();

    let live = hyalo_no_hints()
        .args(["--dir", dir])
        .args(["--format", "json"])
        .args(["backlinks", "b.md"])
        .output()
        .unwrap();
    assert!(live.status.success(), "backlinks (live) failed: {live:?}");
    let live_json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&live.stdout)).unwrap();

    assert_eq!(
        indexed_json["total"], 1,
        "stale graph: expected 1 backlink via --index"
    );
    assert_eq!(
        indexed_json["results"]["backlinks"], live_json["results"]["backlinks"],
        "indexed backlinks must match the live scan after links fix --apply --index"
    );
}
