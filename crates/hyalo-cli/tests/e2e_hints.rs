mod common;

use common::{hyalo, md, write_md};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

fn setup_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();

    write_md(
        tmp.path(),
        "notes/alpha.md",
        md!(r#"
---
title: Alpha
status: in-progress
tags:
  - rust
  - cli
---
# Alpha

- [ ] Open task
- [x] Done task
"#),
    );

    write_md(
        tmp.path(),
        "notes/beta.md",
        md!(r#"
---
title: Beta
status: completed
tags:
  - rust
---
# Beta

- [x] Completed
"#),
    );

    write_md(
        tmp.path(),
        "docs/readme.md",
        md!(r#"
---
title: Readme
status: planned
tags:
  - docs
---
# Readme

No tasks here.
"#),
    );

    tmp
}

// ---------------------------------------------------------------------------
// summary --hints
// ---------------------------------------------------------------------------

#[test]
fn summary_hints_json_has_data_and_hints() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["summary", "--hints", "--format", "json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    // Envelope must have "data" and "hints"
    assert!(parsed.get("data").is_some(), "missing 'data' key");
    assert!(parsed.get("hints").is_some(), "missing 'hints' key");

    // Data must be the vault summary
    assert!(parsed["data"]["files"]["total"].as_u64().unwrap() > 0);

    // Hints must be an array of strings
    let hints = parsed["hints"].as_array().unwrap();
    assert!(!hints.is_empty());
    for hint in hints {
        assert!(hint.as_str().unwrap().starts_with("hyalo"));
    }
}

#[test]
fn summary_hints_text_has_arrow_lines() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["summary", "--hints", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(
        stdout.contains("Files:"),
        "should have normal summary output"
    );
    assert!(
        stdout.contains("  -> hyalo"),
        "should have hint lines with arrow prefix"
    );
    assert!(
        stdout.contains("properties"),
        "should suggest properties command: {stdout}"
    );
    assert!(
        stdout.contains("tags"),
        "should suggest tags command: {stdout}"
    );
}

#[test]
fn summary_hints_suggests_tasks_todo_when_open_tasks() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["summary", "--hints", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(
        stdout.contains("find --task todo"),
        "should suggest find --task todo when there are open tasks: {stdout}"
    );
}

#[test]
fn summary_hints_prefers_interesting_status() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["summary", "--hints", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // in-progress should be suggested
    assert!(
        stdout.contains("in-progress"),
        "should suggest in-progress status: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// properties --hints
// ---------------------------------------------------------------------------

#[test]
fn properties_hints_text() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "--hints", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should suggest find --property for top properties
    assert!(
        stdout.contains("find --property"),
        "should suggest find --property: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// tags --hints
// ---------------------------------------------------------------------------

#[test]
fn tags_hints_text() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "--hints", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should suggest find --tag for top tags (rust has 2 files)
    assert!(
        stdout.contains("find --tag rust"),
        "should suggest find --tag for top tag: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Without --hints: output unchanged (regression)
// ---------------------------------------------------------------------------

#[test]
fn summary_without_hints_no_arrows() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["summary", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(!stdout.contains("  -> "), "should not have hint arrows");
}

#[test]
fn summary_without_hints_json_no_envelope() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["summary", "--format", "json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    // Without --hints, should NOT have envelope
    assert!(
        parsed.get("data").is_none(),
        "should not have 'data' envelope without --hints"
    );
    // Should have direct summary fields
    assert!(parsed.get("files").is_some());
}

// ---------------------------------------------------------------------------
// --hints + --jq suppresses hints
// ---------------------------------------------------------------------------

#[test]
fn hints_suppressed_with_jq() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["summary", "--hints", "--jq", ".tasks.total"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(
        !stdout.contains("hints"),
        "hints should be suppressed with --jq"
    );
    assert!(!stdout.contains("->"), "no arrow lines with --jq");
    // Should just be a number
    let val: u64 = stdout.trim().parse().unwrap();
    assert!(val > 0);
}

// ---------------------------------------------------------------------------
// --dir propagation
// ---------------------------------------------------------------------------

#[test]
fn hints_propagate_dir_flag() {
    let tmp = setup_vault();
    let dir_str = tmp.path().to_str().unwrap();
    let output = hyalo()
        .args(["--dir", dir_str])
        .args(["summary", "--hints", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // All hints should include --dir
    for line in stdout.lines() {
        if line.starts_with("  -> ") {
            assert!(
                line.contains("--dir"),
                "hint should include --dir flag: {line}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// --glob propagation
// ---------------------------------------------------------------------------

#[test]
fn hints_propagate_glob_for_aggregate_commands() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "properties",
            "--glob",
            "notes/*.md",
            "--hints",
            "--format",
            "text",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Aggregate hints (find --property) should include --glob
    let hint_lines: Vec<&str> = stdout.lines().filter(|l| l.starts_with("  -> ")).collect();
    assert!(!hint_lines.is_empty(), "should have hints");
    for line in &hint_lines {
        assert!(
            line.contains("--glob"),
            "aggregate hint should propagate --glob: {line}"
        );
    }
}

// ---------------------------------------------------------------------------
// find --hints
// ---------------------------------------------------------------------------

#[test]
fn find_hints_with_task_filter() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--task", "todo", "--hints", "--format", "text"])
        .output()
        .unwrap();
    // Should succeed — hints may or may not have drill-down suggestions for find
    assert!(output.status.success());
}

// ---------------------------------------------------------------------------
// Mutation commands with --hints
// Mutation commands (set, remove, append) accept --hints but do not generate
// hint suggestions — they produce the same JSON output as without --hints.
// These tests verify the flag is accepted and output remains valid/correct.
// ---------------------------------------------------------------------------

#[test]
fn set_hints_accepted_produces_valid_json() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--hints"])
        .args([
            "set",
            "--property",
            "status=updated",
            "--file",
            "notes/alpha.md",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("expected valid JSON, got: {stdout}\nerr: {e}"));
    // Output should be the mutation result, not wrapped in a hints envelope
    assert!(
        parsed.get("modified").is_some(),
        "should have modified field: {parsed}"
    );
    // Mutation commands do not generate hints, so no envelope
    assert!(
        parsed.get("hints").is_none(),
        "mutation commands should not wrap output in hints envelope: {parsed}"
    );
}

#[test]
fn remove_hints_accepted_produces_valid_json() {
    let tmp = setup_vault();
    // notes/alpha.md has status: in-progress — remove it
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--hints"])
        .args(["remove", "--property", "status", "--file", "notes/alpha.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("expected valid JSON, got: {stdout}\nerr: {e}"));
    assert!(
        parsed.get("modified").is_some(),
        "should have modified field: {parsed}"
    );
    assert!(
        parsed.get("hints").is_none(),
        "mutation commands should not wrap output in hints envelope: {parsed}"
    );
}

#[test]
fn append_hints_accepted_produces_valid_json() {
    let tmp = setup_vault();
    // Append a new alias to notes/alpha.md
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--hints"])
        .args([
            "append",
            "--property",
            "aliases=alpha-note",
            "--file",
            "notes/alpha.md",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("expected valid JSON, got: {stdout}\nerr: {e}"));
    assert!(
        parsed.get("modified").is_some(),
        "should have modified field: {parsed}"
    );
    assert!(
        parsed.get("hints").is_none(),
        "mutation commands should not wrap output in hints envelope: {parsed}"
    );
}
