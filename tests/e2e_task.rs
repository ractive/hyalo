mod common;

use common::{hyalo, write_md};
use std::fs;

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

/// Line numbers for the fixture file created by `setup_task_file`.
///
/// ```text
/// 1:  ---
/// 2:  title: Test
/// 3:  ---
/// 4:  # Tasks
/// 5:  - [ ] First task
/// 6:  - [x] Second task
/// 7:  - [/] Third task
/// 8:  (blank)
/// 9:  ```code
/// 10: - [ ] Not a real task
/// 11: ```
/// ```
const LINE_INCOMPLETE: usize = 5;
const LINE_COMPLETE: usize = 6;
const LINE_CUSTOM_STATUS: usize = 7;
const LINE_IN_CODE_BLOCK: usize = 10;
const LINE_HEADING: usize = 4;

fn setup_task_file(tmp: &tempfile::TempDir) {
    // Cannot use md!() raw string for backticks; write the content directly.
    let content = "---\ntitle: Test\n---\n# Tasks\n- [ ] First task\n- [x] Second task\n- [/] Third task\n\n```code\n- [ ] Not a real task\n```\n";
    write_md(tmp.path(), "tasks.md", content);
}

// ---------------------------------------------------------------------------
// Helper: run `task read` and return (status, stdout, stderr)
// ---------------------------------------------------------------------------

fn run_task_read(
    tmp: &tempfile::TempDir,
    file: &str,
    line: usize,
) -> (std::process::ExitStatus, String, String) {
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["task", "read", "--file", file, "--line", &line.to_string()]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status, stdout, stderr)
}

fn run_task_read_json(
    tmp: &tempfile::TempDir,
    file: &str,
    line: usize,
) -> (std::process::ExitStatus, serde_json::Value, String) {
    let (status, stdout, stderr) = run_task_read(tmp, file, line);
    let json: serde_json::Value = if status.success() {
        serde_json::from_str(&stdout).unwrap_or(serde_json::Value::Null)
    } else {
        serde_json::Value::Null
    };
    (status, json, stderr)
}

// ---------------------------------------------------------------------------
// Helper: run `task toggle`
// ---------------------------------------------------------------------------

fn run_task_toggle(
    tmp: &tempfile::TempDir,
    file: &str,
    line: usize,
) -> (std::process::ExitStatus, serde_json::Value, String) {
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args([
        "task",
        "toggle",
        "--file",
        file,
        "--line",
        &line.to_string(),
    ]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let json: serde_json::Value = if output.status.success() {
        serde_json::from_str(&stdout).unwrap_or(serde_json::Value::Null)
    } else {
        serde_json::Value::Null
    };
    (output.status, json, stderr)
}

// ---------------------------------------------------------------------------
// Helper: run `task set-status`
// ---------------------------------------------------------------------------

fn run_task_set_status(
    tmp: &tempfile::TempDir,
    file: &str,
    line: usize,
    status_char: &str,
) -> (std::process::ExitStatus, serde_json::Value, String) {
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args([
        "task",
        "set-status",
        "--file",
        file,
        "--line",
        &line.to_string(),
        "--status",
        status_char,
    ]);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let json: serde_json::Value = if output.status.success() {
        serde_json::from_str(&stdout).unwrap_or(serde_json::Value::Null)
    } else {
        serde_json::Value::Null
    };
    (output.status, json, stderr)
}

// ---------------------------------------------------------------------------
// task read — success cases
// ---------------------------------------------------------------------------

#[test]
fn task_read_incomplete_task_returns_correct_json() {
    let tmp = tempfile::tempdir().unwrap();
    setup_task_file(&tmp);

    let (status, json, stderr) = run_task_read_json(&tmp, "tasks.md", LINE_INCOMPLETE);
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["file"], "tasks.md");
    assert_eq!(json["line"], LINE_INCOMPLETE);
    assert_eq!(json["status"], " ");
    assert_eq!(json["text"], "First task");
    assert_eq!(json["done"], false);
}

#[test]
fn task_read_complete_task_done_true() {
    let tmp = tempfile::tempdir().unwrap();
    setup_task_file(&tmp);

    let (status, json, stderr) = run_task_read_json(&tmp, "tasks.md", LINE_COMPLETE);
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["status"], "x");
    assert_eq!(json["done"], true);
    assert_eq!(json["text"], "Second task");
}

#[test]
fn task_read_custom_status_char() {
    let tmp = tempfile::tempdir().unwrap();
    setup_task_file(&tmp);

    let (status, json, stderr) = run_task_read_json(&tmp, "tasks.md", LINE_CUSTOM_STATUS);
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["status"], "/");
    assert_eq!(json["done"], false);
    assert_eq!(json["text"], "Third task");
}

// ---------------------------------------------------------------------------
// task read — error cases
// ---------------------------------------------------------------------------

#[test]
fn task_read_non_task_line_exits_1() {
    let tmp = tempfile::tempdir().unwrap();
    setup_task_file(&tmp);

    let (status, _stdout, _stderr) = run_task_read(&tmp, "tasks.md", LINE_HEADING);
    assert!(!status.success());
    assert_eq!(status.code(), Some(1));
}

#[test]
fn task_read_nonexistent_file_exits_1() {
    let tmp = tempfile::tempdir().unwrap();

    let (status, _stdout, _stderr) = run_task_read(&tmp, "does_not_exist.md", 1);
    assert!(!status.success());
    assert_eq!(status.code(), Some(1));
}

#[test]
fn task_read_inside_code_block_exits_1() {
    let tmp = tempfile::tempdir().unwrap();
    setup_task_file(&tmp);

    // LINE_IN_CODE_BLOCK is inside a fenced code block — must not be treated as a task
    let (status, _stdout, _stderr) = run_task_read(&tmp, "tasks.md", LINE_IN_CODE_BLOCK);
    assert!(!status.success());
    assert_eq!(status.code(), Some(1));
}

// ---------------------------------------------------------------------------
// task toggle — success cases
// ---------------------------------------------------------------------------

#[test]
fn task_toggle_incomplete_becomes_complete() {
    let tmp = tempfile::tempdir().unwrap();
    setup_task_file(&tmp);

    let (status, json, stderr) = run_task_toggle(&tmp, "tasks.md", LINE_INCOMPLETE);
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["status"], "x");
    assert_eq!(json["done"], true);
}

#[test]
fn task_toggle_incomplete_modifies_file_on_disk() {
    let tmp = tempfile::tempdir().unwrap();
    setup_task_file(&tmp);

    let (status, _json, stderr) = run_task_toggle(&tmp, "tasks.md", LINE_INCOMPLETE);
    assert!(status.success(), "stderr: {stderr}");

    let content = fs::read_to_string(tmp.path().join("tasks.md")).unwrap();
    assert!(
        content.contains("- [x] First task"),
        "expected '- [x] First task' in file, got:\n{content}"
    );
}

#[test]
fn task_toggle_complete_becomes_incomplete() {
    let tmp = tempfile::tempdir().unwrap();
    setup_task_file(&tmp);

    let (status, json, stderr) = run_task_toggle(&tmp, "tasks.md", LINE_COMPLETE);
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["status"], " ");
    assert_eq!(json["done"], false);
}

#[test]
fn task_toggle_complete_modifies_file_on_disk() {
    let tmp = tempfile::tempdir().unwrap();
    setup_task_file(&tmp);

    let (status, _json, stderr) = run_task_toggle(&tmp, "tasks.md", LINE_COMPLETE);
    assert!(status.success(), "stderr: {stderr}");

    let content = fs::read_to_string(tmp.path().join("tasks.md")).unwrap();
    assert!(
        content.contains("- [ ] Second task"),
        "expected '- [ ] Second task' in file after toggle, got:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// task toggle — error cases
// ---------------------------------------------------------------------------

#[test]
fn task_toggle_non_task_line_exits_1() {
    let tmp = tempfile::tempdir().unwrap();
    setup_task_file(&tmp);

    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args([
        "task",
        "toggle",
        "--file",
        "tasks.md",
        "--line",
        &LINE_HEADING.to_string(),
    ]);
    let output = cmd.output().unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

// ---------------------------------------------------------------------------
// task set-status — success cases
// ---------------------------------------------------------------------------

#[test]
fn task_set_status_slash_on_incomplete_task() {
    let tmp = tempfile::tempdir().unwrap();
    setup_task_file(&tmp);

    let (status, json, stderr) = run_task_set_status(&tmp, "tasks.md", LINE_INCOMPLETE, "/");
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["status"], "/");
    assert_eq!(json["done"], false);
}

#[test]
fn task_set_status_question_mark_on_complete_task() {
    let tmp = tempfile::tempdir().unwrap();
    setup_task_file(&tmp);

    let (status, json, stderr) = run_task_set_status(&tmp, "tasks.md", LINE_COMPLETE, "?");
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["status"], "?");
    assert_eq!(json["done"], false);
}

#[test]
fn task_set_status_modifies_file_on_disk() {
    let tmp = tempfile::tempdir().unwrap();
    setup_task_file(&tmp);

    let (status, _json, stderr) = run_task_set_status(&tmp, "tasks.md", LINE_INCOMPLETE, "?");
    assert!(status.success(), "stderr: {stderr}");

    let content = fs::read_to_string(tmp.path().join("tasks.md")).unwrap();
    assert!(
        content.contains("- [?] First task"),
        "expected '- [?] First task' in file, got:\n{content}"
    );
}

#[test]
fn task_set_status_x_sets_done_true() {
    let tmp = tempfile::tempdir().unwrap();
    setup_task_file(&tmp);

    let (status, json, stderr) = run_task_set_status(&tmp, "tasks.md", LINE_INCOMPLETE, "x");
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["status"], "x");
    assert_eq!(json["done"], true);
}

// ---------------------------------------------------------------------------
// task set-status — error cases
// ---------------------------------------------------------------------------

#[test]
fn task_set_status_non_task_line_exits_1() {
    let tmp = tempfile::tempdir().unwrap();
    setup_task_file(&tmp);

    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args([
        "task",
        "set-status",
        "--file",
        "tasks.md",
        "--line",
        &LINE_HEADING.to_string(),
        "--status",
        "x",
    ]);
    let output = cmd.output().unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn task_set_status_multi_char_status_exits_1() {
    let tmp = tempfile::tempdir().unwrap();
    setup_task_file(&tmp);

    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args([
        "task",
        "set-status",
        "--file",
        "tasks.md",
        "--line",
        &LINE_INCOMPLETE.to_string(),
        "--status",
        "xx",
    ]);
    let output = cmd.output().unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

// ---------------------------------------------------------------------------
// JSON output shape
// ---------------------------------------------------------------------------

#[test]
fn task_read_json_has_all_required_fields() {
    let tmp = tempfile::tempdir().unwrap();
    write_md(tmp.path(), "note.md", "- [ ] My task\n");

    let (status, json, stderr) = run_task_read_json(&tmp, "note.md", 1);
    assert!(status.success(), "stderr: {stderr}");

    assert!(json["file"].is_string(), "missing file field");
    assert!(json["line"].is_number(), "missing line field");
    assert!(json["status"].is_string(), "missing status field");
    assert!(json["text"].is_string(), "missing text field");
    assert!(json["done"].is_boolean(), "missing done field");
}

#[test]
fn task_toggle_json_has_all_required_fields() {
    let tmp = tempfile::tempdir().unwrap();
    write_md(tmp.path(), "note.md", "- [ ] My task\n");

    let (status, json, stderr) = run_task_toggle(&tmp, "note.md", 1);
    assert!(status.success(), "stderr: {stderr}");

    assert!(json["file"].is_string(), "missing file field");
    assert!(json["line"].is_number(), "missing line field");
    assert!(json["status"].is_string(), "missing status field");
    assert!(json["text"].is_string(), "missing text field");
    assert!(json["done"].is_boolean(), "missing done field");
}

#[test]
fn task_set_status_json_has_all_required_fields() {
    let tmp = tempfile::tempdir().unwrap();
    write_md(tmp.path(), "note.md", "- [ ] My task\n");

    let (status, json, stderr) = run_task_set_status(&tmp, "note.md", 1, "/");
    assert!(status.success(), "stderr: {stderr}");

    assert!(json["file"].is_string(), "missing file field");
    assert!(json["line"].is_number(), "missing line field");
    assert!(json["status"].is_string(), "missing status field");
    assert!(json["text"].is_string(), "missing text field");
    assert!(json["done"].is_boolean(), "missing done field");
}
