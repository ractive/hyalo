mod common;

use common::{hyalo, md, write_md};
use std::fs;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

/// Content with all task varieties, code block with task inside, and plain text.
/// 7 real tasks: [ ] open×2, [x] done, [-] cancelled, [?] question, [!] important, [X] done uppercase.
/// Code-block task must NOT be counted.
fn tasks_content() -> &'static str {
    md!(r#"
---
title: Test Tasks
status: in-progress
---

# My Tasks

- [ ] Open task one
- [x] Done task
- [-] Cancelled task
- [?] Question task
- [!] Important task

## Section Two

- [ ] Another open task
- [X] Also done (uppercase)

Regular text (not a task)
- Regular bullet (not a task)

```
- [ ] Task inside code block (should be ignored)
```
"#)
}

/// Write the standard fixture and return (TempDir, file path string).
fn setup_fixture() -> (TempDir, String) {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "tasks.md", tasks_content());
    let path = tmp.path().join("tasks.md");
    (tmp, path.to_str().unwrap().to_owned())
}

// ---------------------------------------------------------------------------
// `hyalo tasks` — happy paths
// ---------------------------------------------------------------------------

#[test]
fn tasks_file_lists_all() {
    let (tmp, _) = setup_fixture();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tasks", "--file", "tasks.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    // Shape: {file, tasks: [{line, status, text, done}], total}
    assert!(json["file"].is_string());
    assert!(json["tasks"].is_array());
    assert!(json["total"].is_number());

    let tasks = json["tasks"].as_array().unwrap();
    // 7 tasks: [ ] open, [x] done, [-] cancelled, [?] question, [!] important, [ ] another open, [X] also done
    assert_eq!(json["total"], 7);
    assert_eq!(tasks.len(), 7);

    // Verify each task entry has required fields
    for task in tasks {
        assert!(task["line"].is_number(), "missing line field: {task}");
        assert!(task["status"].is_string(), "missing status field: {task}");
        assert!(task["text"].is_string(), "missing text field: {task}");
        assert!(task["done"].is_boolean(), "missing done field: {task}");
    }
}

#[test]
fn tasks_file_done_filter() {
    let (tmp, _) = setup_fixture();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tasks", "--file", "tasks.md", "--done"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    let tasks = json["tasks"].as_array().unwrap();
    // [x] Done task and [X] Also done (uppercase)
    assert_eq!(json["total"], 2);
    assert_eq!(tasks.len(), 2);

    for task in tasks {
        assert_eq!(task["done"], true, "expected done=true for: {task}");
    }
}

#[test]
fn tasks_file_todo_filter() {
    let (tmp, _) = setup_fixture();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tasks", "--file", "tasks.md", "--todo"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    let tasks = json["tasks"].as_array().unwrap();
    // [ ] Open task one, [-] Cancelled, [?] Question, [!] Important, [ ] Another open
    assert_eq!(json["total"], 5);
    assert_eq!(tasks.len(), 5);

    for task in tasks {
        assert_eq!(task["done"], false, "expected done=false for: {task}");
    }
}

#[test]
fn tasks_file_status_filter() {
    let (tmp, _) = setup_fixture();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tasks", "--file", "tasks.md", "--status", "-"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    let tasks = json["tasks"].as_array().unwrap();
    assert_eq!(json["total"], 1);
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["status"], "-");
    assert_eq!(tasks[0]["text"], "Cancelled task");
}

#[test]
fn tasks_glob_returns_array() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "sub/a.md", "- [ ] Task A\n- [x] Done A\n");
    write_md(tmp.path(), "sub/b.md", "- [-] Custom B\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tasks", "--glob", "sub/*.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 2);

    // Each entry has file, tasks, total
    for entry in arr {
        assert!(entry["file"].is_string());
        assert!(entry["tasks"].is_array());
        assert!(entry["total"].is_number());
    }
}

#[test]
fn tasks_vault_wide() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "- [ ] Task A\n");
    write_md(tmp.path(), "b.md", "- [x] Task B\n");
    write_md(tmp.path(), "c.md", "No tasks.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .arg("tasks")
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let arr = json.as_array().unwrap();
    // 3 files: a.md, b.md, c.md
    assert_eq!(arr.len(), 3);
}

#[test]
fn tasks_text_format() {
    let (tmp, _) = setup_fixture();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tasks", "--file", "tasks.md", "--format", "text"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Text format should contain task text
    assert!(
        stdout.contains("Open task one"),
        "expected 'Open task one' in: {stdout}"
    );
    assert!(
        stdout.contains("Done task"),
        "expected 'Done task' in: {stdout}"
    );
}

#[test]
fn tasks_jq_filter() {
    let (tmp, _) = setup_fixture();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", ".total"])
        .args(["tasks", "--file", "tasks.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "7");
}

#[test]
fn tasks_file_no_tasks() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "empty.md",
        "---\ntitle: Empty\n---\n# No tasks here\n\nJust text.\n",
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tasks", "--file", "empty.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 0);
    assert!(json["tasks"].as_array().unwrap().is_empty());
}

#[test]
fn tasks_skips_code_blocks() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
- [ ] Real task before code

```
- [ ] Task inside code block (ignored)
```

- [x] Real task after code
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tasks", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let tasks = json["tasks"].as_array().unwrap();
    // Only 2 real tasks; code-block task is excluded
    assert_eq!(json["total"], 2);
    assert_eq!(tasks.len(), 2);

    let texts: Vec<&str> = tasks.iter().map(|t| t["text"].as_str().unwrap()).collect();
    assert!(texts.contains(&"Real task before code"));
    assert!(texts.contains(&"Real task after code"));
    assert!(!texts.contains(&"Task inside code block (ignored)"));
}

#[test]
fn tasks_file_no_frontmatter() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "raw.md",
        "- [ ] Task without frontmatter\n- [x] Done without frontmatter\n",
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tasks", "--file", "raw.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 2);
    assert_eq!(json["tasks"].as_array().unwrap().len(), 2);
}

#[test]
fn tasks_glob_no_match() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tasks", "--glob", "nonexistent/*.md"])
        .output()
        .unwrap();

    // A glob that matches no files is treated as a user error (exit 1).
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no files match"),
        "expected 'no files match' in stderr: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// `hyalo tasks` — unhappy paths
// ---------------------------------------------------------------------------

#[test]
fn tasks_done_todo_conflict() {
    let (tmp, _) = setup_fixture();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tasks", "--file", "tasks.md", "--done", "--todo"])
        .output()
        .unwrap();

    // clap conflict → exit code 2
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn tasks_done_status_conflict() {
    let (tmp, _) = setup_fixture();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tasks", "--file", "tasks.md", "--done", "--status", "x"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn tasks_status_multichar_error() {
    let (tmp, _) = setup_fixture();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tasks", "--file", "tasks.md", "--status", "xx"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("single character") || stderr.contains("single char"),
        "expected single-character error in stderr: {stderr}"
    );
}

#[test]
fn tasks_file_not_found() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tasks", "--file", "missing.md"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

// ---------------------------------------------------------------------------
// `hyalo task read` — happy paths
// ---------------------------------------------------------------------------

#[test]
fn task_read_finds_task() {
    let tmp = TempDir::new().unwrap();
    // Line 1: "- [ ] My task"
    write_md(tmp.path(), "note.md", "- [ ] My task\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["task", "read", "--file", "note.md", "--line", "1"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // Shape: {file, line, status, text, done}
    assert!(json["file"].as_str().unwrap().ends_with("note.md"));
    assert_eq!(json["line"], 1);
    assert_eq!(json["status"], " ");
    assert_eq!(json["text"], "My task");
    assert_eq!(json["done"], false);
}

#[test]
fn task_read_text_format() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "- [x] Finished work\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "task", "read", "--file", "note.md", "--line", "1", "--format", "text",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Finished work"),
        "expected text in: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// `hyalo task read` — unhappy paths
// ---------------------------------------------------------------------------

#[test]
fn task_read_file_not_found() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["task", "read", "--file", "missing.md", "--line", "1"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn task_read_line_out_of_range() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "short.md", "- [ ] Only line\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["task", "read", "--file", "short.md", "--line", "999"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn task_read_not_a_task() {
    let tmp = TempDir::new().unwrap();
    // Line 1 is plain text, not a task
    write_md(tmp.path(), "note.md", "Regular text here\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["task", "read", "--file", "note.md", "--line", "1"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not a task"),
        "expected 'not a task' in stderr: {stderr}"
    );
}

#[test]
fn task_read_missing_file_arg() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["task", "read", "--line", "1"])
        .output()
        .unwrap();

    // clap missing required arg → exit code 2
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
}

// ---------------------------------------------------------------------------
// `hyalo task toggle` — happy paths
// ---------------------------------------------------------------------------

#[test]
fn task_toggle_open_to_done() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "- [ ] Open task\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["task", "toggle", "--file", "note.md", "--line", "1"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "x");
    assert_eq!(json["done"], true);
}

#[test]
fn task_toggle_done_to_open() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "- [x] Done task\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["task", "toggle", "--file", "note.md", "--line", "1"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], " ");
    assert_eq!(json["done"], false);
}

#[test]
fn task_toggle_uppercase_x_to_open() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "- [X] Done uppercase\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["task", "toggle", "--file", "note.md", "--line", "1"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], " ");
    assert_eq!(json["done"], false);
}

#[test]
fn task_toggle_custom_to_done() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "- [-] Cancelled task\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["task", "toggle", "--file", "note.md", "--line", "1"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "x");
    assert_eq!(json["done"], true);
}

#[test]
fn task_toggle_persists() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "- [ ] Persist test\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["task", "toggle", "--file", "note.md", "--line", "1"])
        .output()
        .unwrap();

    assert!(output.status.success());

    // Verify the change is on disk
    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(
        content.contains("- [x] Persist test"),
        "expected toggled line on disk, got: {content}"
    );
}

// ---------------------------------------------------------------------------
// `hyalo task toggle` — unhappy paths
// ---------------------------------------------------------------------------

#[test]
fn task_toggle_not_a_task() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "Regular text, not a task\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["task", "toggle", "--file", "note.md", "--line", "1"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn task_toggle_file_not_found() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["task", "toggle", "--file", "missing.md", "--line", "1"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn task_toggle_line_out_of_range() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "- [ ] Only task\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["task", "toggle", "--file", "note.md", "--line", "999"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

// ---------------------------------------------------------------------------
// `hyalo task set-status` — happy paths
// ---------------------------------------------------------------------------

#[test]
fn task_set_status_custom_char() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "- [ ] Open task\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "task",
            "set-status",
            "--file",
            "note.md",
            "--line",
            "1",
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

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "?");
    assert_eq!(json["done"], false);
}

#[test]
fn task_set_status_to_done() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "- [ ] Open task\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "task",
            "set-status",
            "--file",
            "note.md",
            "--line",
            "1",
            "--status",
            "x",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "x");
    assert_eq!(json["done"], true);
}

#[test]
fn task_set_status_persists() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "- [ ] Persist test\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "task",
            "set-status",
            "--file",
            "note.md",
            "--line",
            "1",
            "--status",
            "!",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());

    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(
        content.contains("- [!] Persist test"),
        "expected set-status result on disk, got: {content}"
    );
}

// ---------------------------------------------------------------------------
// `hyalo task set-status` — unhappy paths
// ---------------------------------------------------------------------------

#[test]
fn task_set_status_not_a_task() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "# Heading not a task\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "task",
            "set-status",
            "--file",
            "note.md",
            "--line",
            "1",
            "--status",
            "x",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn task_set_status_multichar() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "- [ ] Some task\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "task",
            "set-status",
            "--file",
            "note.md",
            "--line",
            "1",
            "--status",
            "xx",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("single character") || stderr.contains("single char"),
        "expected single-character error in stderr: {stderr}"
    );
}

#[test]
fn task_set_status_file_not_found() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "task",
            "set-status",
            "--file",
            "missing.md",
            "--line",
            "1",
            "--status",
            "?",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}
