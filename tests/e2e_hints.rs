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
    assert!(stdout.contains("properties summary"));
    assert!(stdout.contains("tags summary"));
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
        stdout.contains("tasks --todo"),
        "should suggest tasks --todo when there are open tasks"
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

    // in-progress should be suggested before completed
    assert!(
        stdout.contains("--value in-progress"),
        "should suggest in-progress status"
    );
}

// ---------------------------------------------------------------------------
// properties summary --hints
// ---------------------------------------------------------------------------

#[test]
fn properties_summary_hints_text() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "summary", "--hints", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should suggest property find for top properties
    assert!(stdout.contains("property find --name"));
}

// ---------------------------------------------------------------------------
// tags summary --hints
// ---------------------------------------------------------------------------

#[test]
fn tags_summary_hints_text() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "summary", "--hints", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should suggest tag find for top tags (rust has 2 files)
    assert!(
        stdout.contains("tag find --name rust"),
        "should suggest tag find for top tag: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// property find --hints
// ---------------------------------------------------------------------------

#[test]
fn property_find_hints_suggests_outline() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property",
            "find",
            "--name",
            "status",
            "--value",
            "in-progress",
            "--hints",
            "--format",
            "text",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(
        stdout.contains("outline --file"),
        "should suggest outline for found files: {stdout}"
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
            "summary",
            "--glob",
            "notes/*.md",
            "--hints",
            "--format",
            "text",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Aggregate hints (property find) should include --glob
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
// outline --hints
// ---------------------------------------------------------------------------

#[test]
fn outline_hints_suggest_property_and_tag_find() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "outline",
            "--file",
            "notes/alpha.md",
            "--hints",
            "--format",
            "text",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(
        stdout.contains("property find"),
        "should suggest property find: {stdout}"
    );
    assert!(
        stdout.contains("tag find"),
        "should suggest tag find: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// tasks --hints
// ---------------------------------------------------------------------------

#[test]
fn tasks_hints_suggest_task_read() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tasks", "--todo", "--hints", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(
        stdout.contains("task read --file"),
        "should suggest task read: {stdout}"
    );
}
