mod common;

use common::{hyalo_no_hints, md, write_md};
use std::fs;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

fn setup() -> TempDir {
    let tmp = TempDir::new().unwrap();

    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Test Note
status: draft
tags:
  - rust
---
# Body

Some content here.

## Tasks

- [ ] First task
- [x] Second task
"),
    );

    write_md(
        tmp.path(),
        "other.md",
        md!(r"
---
title: Other
---
See [[note]] for details.
"),
    );

    tmp
}

// ---------------------------------------------------------------------------
// read: positional file
// ---------------------------------------------------------------------------

#[test]
fn read_positional_file() {
    let tmp = setup();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "note.md", "--format", "text"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Some content here."));
}

#[test]
fn read_positional_matches_flag() {
    let tmp = setup();

    let positional = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "note.md"])
        .output()
        .unwrap();

    let flag = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(positional.status.success());
    assert!(flag.status.success());
    assert_eq!(positional.stdout, flag.stdout);
}

// ---------------------------------------------------------------------------
// read: conflict — both positional and --file
// ---------------------------------------------------------------------------

#[test]
fn read_positional_and_flag_conflicts() {
    let tmp = setup();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "note.md", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot be used with"),
        "expected conflict error, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// read: no file at all → error
// ---------------------------------------------------------------------------

#[test]
fn read_no_file_at_all_errors() {
    let tmp = setup();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FILE") || stderr.contains("file"),
        "expected missing file error, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// find: positional file + --file flag conflicts
// ---------------------------------------------------------------------------

#[test]
fn find_positional_file_and_flag_conflicts() {
    let tmp = setup();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "pattern", "note.md", "--file", "other.md"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot be used with"),
        "expected conflict error, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// backlinks: positional file
// ---------------------------------------------------------------------------

#[test]
fn backlinks_positional_file() {
    let tmp = setup();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["backlinks", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // other.md links to note via [[note]]
    assert_eq!(envelope["total"], 1);
}

// ---------------------------------------------------------------------------
// mv: positional file
// ---------------------------------------------------------------------------

#[test]
fn mv_positional_file() {
    let tmp = setup();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "note.md", "--to", "archive/note.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["results"]["from"], "note.md");
    assert_eq!(envelope["results"]["to"], "archive/note.md");
    assert!(tmp.path().join("archive/note.md").exists());
    assert!(!tmp.path().join("note.md").exists());
}

// ---------------------------------------------------------------------------
// set: positional file(s)
// ---------------------------------------------------------------------------

#[test]
fn set_positional_file() {
    let tmp = setup();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["set", "note.md", "--property", "priority=5"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains("priority: 5"));
}

// ---------------------------------------------------------------------------
// remove: positional file(s)
// ---------------------------------------------------------------------------

#[test]
fn remove_positional_file() {
    let tmp = setup();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["remove", "note.md", "--property", "status"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(!content.contains("status:"));
}

// ---------------------------------------------------------------------------
// append: positional file(s)
// ---------------------------------------------------------------------------

#[test]
fn append_positional_file() {
    let tmp = setup();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["append", "note.md", "--property", "tags=cli"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains("cli"));
    assert!(content.contains("rust"));
}

// ---------------------------------------------------------------------------
// find: positional pattern + positional file
// ---------------------------------------------------------------------------

#[test]
fn find_positional_pattern_and_file() {
    let tmp = setup();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "content", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["total"], 1);
    assert_eq!(envelope["results"][0]["file"], "note.md");
}

#[test]
fn find_positional_file_only_no_pattern() {
    // When only one positional is given to find, it's treated as PATTERN, not file.
    // To pass a file without a pattern, --file is required.
    let tmp = setup();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["total"], 1);
}

// ---------------------------------------------------------------------------
// task read: positional file
// ---------------------------------------------------------------------------

#[test]
fn task_read_positional_file() {
    let tmp = setup();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["task", "read", "note.md", "--all"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let results = envelope["results"].as_array().unwrap();
    assert_eq!(results.len(), 2);
}

// ---------------------------------------------------------------------------
// task toggle: positional file
// ---------------------------------------------------------------------------

#[test]
fn task_toggle_positional_file() {
    let tmp = setup();
    // Toggle line 13 (- [ ] First task)
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["task", "toggle", "note.md", "--line", "13"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains("- [x] First task"));
}

// ---------------------------------------------------------------------------
// task set-status: positional file
// ---------------------------------------------------------------------------

#[test]
fn task_set_status_positional_file() {
    let tmp = setup();
    // Set line 13 (- [ ] First task) to status '?'
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "task",
            "set-status",
            "note.md",
            "--line",
            "13",
            "--status",
            "?",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains("- [?] First task"));
}
