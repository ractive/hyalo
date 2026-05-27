use super::common::{hyalo_no_hints, write_md};
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
    let mut cmd = hyalo_no_hints();
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
        let envelope: serde_json::Value = serde_json::from_str(&stdout)
            .unwrap_or_else(|e| panic!("invalid JSON: {e}\nstdout: {stdout}\nstderr: {stderr}"));
        envelope["results"].clone()
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
    let mut cmd = hyalo_no_hints();
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
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let json: serde_json::Value = if output.status.success() {
        let envelope: serde_json::Value =
            serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
                let stdout = String::from_utf8_lossy(&output.stdout);
                panic!("invalid JSON: {e}\nstdout: {stdout}\nstderr: {stderr}")
            });
        envelope["results"].clone()
    } else {
        serde_json::Value::Null
    };
    (output.status, json, stderr)
}

// ---------------------------------------------------------------------------
// Helper: run `task set`
// ---------------------------------------------------------------------------

fn run_task_set_status(
    tmp: &tempfile::TempDir,
    file: &str,
    line: usize,
    status_char: &str,
) -> (std::process::ExitStatus, serde_json::Value, String) {
    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args([
        "task",
        "set",
        "--file",
        file,
        "--line",
        &line.to_string(),
        "--status",
        status_char,
    ]);
    let output = cmd.output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let json: serde_json::Value = if output.status.success() {
        let envelope: serde_json::Value =
            serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
                let stdout = String::from_utf8_lossy(&output.stdout);
                panic!("invalid JSON: {e}\nstdout: {stdout}\nstderr: {stderr}")
            });
        envelope["results"].clone()
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

    let mut cmd = hyalo_no_hints();
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
// task set — success cases
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
// task set — error cases
// ---------------------------------------------------------------------------

#[test]
fn task_set_status_non_task_line_exits_1() {
    let tmp = tempfile::tempdir().unwrap();
    setup_task_file(&tmp);

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args([
        "task",
        "set",
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

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args([
        "task",
        "set",
        "--file",
        "tasks.md",
        "--line",
        &LINE_INCOMPLETE.to_string(),
        "--status",
        "ab",
    ]);
    let output = cmd.output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr.contains("single character"),
        "expected 'single character' in stderr, got: {stderr}"
    );
}

#[test]
fn task_set_status_empty_string_exits_1() {
    let tmp = tempfile::tempdir().unwrap();
    setup_task_file(&tmp);

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args([
        "task",
        "set",
        "--file",
        "tasks.md",
        "--line",
        &LINE_INCOMPLETE.to_string(),
        "--status",
        "",
    ]);
    let output = cmd.output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr.contains("single character"),
        "expected 'single character' in stderr, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Bulk operations fixture
// ---------------------------------------------------------------------------

/// Multi-section fixture for bulk operation tests.
///
/// ```text
///  1: ---
///  2: title: Bulk Test
///  3: ---
///  4: # Tasks
///  5: - [ ] Task A
///  6: - [ ] Task B
///  7:
///  8: ## Acceptance criteria
///  9: - [ ] AC one
/// 10: - [ ] AC two
/// 11: - [x] AC three
/// 12:
/// 13: ## Other section
/// 14: - [ ] Other task
/// ```
fn setup_bulk_file(tmp: &tempfile::TempDir) {
    let content = "---\ntitle: Bulk Test\n---\n# Tasks\n- [ ] Task A\n- [ ] Task B\n\n## Acceptance criteria\n- [ ] AC one\n- [ ] AC two\n- [x] AC three\n\n## Other section\n- [ ] Other task\n";
    write_md(tmp.path(), "bulk.md", content);
}

// ---------------------------------------------------------------------------
// Helper: run arbitrary task subcommand with extra args
// ---------------------------------------------------------------------------

fn run_task_cmd(
    tmp: &tempfile::TempDir,
    args: &[&str],
) -> (std::process::ExitStatus, String, String) {
    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(args);
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status, stdout, stderr)
}

fn run_task_cmd_json(
    tmp: &tempfile::TempDir,
    args: &[&str],
) -> (std::process::ExitStatus, serde_json::Value, String) {
    let (status, stdout, stderr) = run_task_cmd(tmp, args);
    let json: serde_json::Value = if status.success() {
        serde_json::from_str(&stdout)
            .unwrap_or_else(|e| panic!("invalid JSON: {e}\nstdout: {stdout}\nstderr: {stderr}"))
    } else {
        serde_json::Value::Null
    };
    (status, json, stderr)
}

// ---------------------------------------------------------------------------
// Bulk: repeatable --line
// ---------------------------------------------------------------------------

#[test]
fn task_toggle_multiple_lines() {
    let tmp = tempfile::tempdir().unwrap();
    setup_bulk_file(&tmp);

    let (status, json, stderr) = run_task_cmd_json(
        &tmp,
        &[
            "task", "toggle", "--file", "bulk.md", "--line", "5", "--line", "6",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");

    let results = json["results"].as_array().expect("expected results array");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["status"], "x");
    assert_eq!(results[0]["text"], "Task A");
    assert_eq!(results[1]["status"], "x");
    assert_eq!(results[1]["text"], "Task B");

    let content = fs::read_to_string(tmp.path().join("bulk.md")).unwrap();
    assert!(content.contains("- [x] Task A"));
    assert!(content.contains("- [x] Task B"));
}

#[test]
fn task_read_multiple_lines() {
    let tmp = tempfile::tempdir().unwrap();
    setup_bulk_file(&tmp);

    let (status, json, stderr) = run_task_cmd_json(
        &tmp,
        &[
            "task", "read", "--file", "bulk.md", "--line", "9", "--line", "10",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");

    let results = json["results"].as_array().expect("expected results array");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["text"], "AC one");
    assert_eq!(results[1]["text"], "AC two");
}

#[test]
fn task_toggle_comma_separated_lines() {
    let tmp = tempfile::tempdir().unwrap();
    setup_bulk_file(&tmp);

    let (status, json, stderr) = run_task_cmd_json(
        &tmp,
        &["task", "toggle", "--file", "bulk.md", "--line", "5,6"],
    );
    assert!(status.success(), "stderr: {stderr}");

    let results = json["results"].as_array().expect("expected results array");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["status"], "x");
    assert_eq!(results[0]["text"], "Task A");
    assert_eq!(results[1]["status"], "x");
    assert_eq!(results[1]["text"], "Task B");

    let content = fs::read_to_string(tmp.path().join("bulk.md")).unwrap();
    assert!(content.contains("- [x] Task A"));
    assert!(content.contains("- [x] Task B"));
}

#[test]
fn task_set_status_multiple_lines() {
    let tmp = tempfile::tempdir().unwrap();
    setup_bulk_file(&tmp);

    let (status, json, stderr) = run_task_cmd_json(
        &tmp,
        &[
            "task", "set", "--file", "bulk.md", "--line", "5", "--line", "6", "--status", "?",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");

    let results = json["results"].as_array().expect("expected results array");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["status"], "?");
    assert_eq!(results[1]["status"], "?");

    let content = fs::read_to_string(tmp.path().join("bulk.md")).unwrap();
    assert!(content.contains("- [?] Task A"));
    assert!(content.contains("- [?] Task B"));
}

// ---------------------------------------------------------------------------
// Bulk: --section
// ---------------------------------------------------------------------------

#[test]
fn task_toggle_section() {
    let tmp = tempfile::tempdir().unwrap();
    setup_bulk_file(&tmp);

    let (status, json, stderr) = run_task_cmd_json(
        &tmp,
        &[
            "task",
            "toggle",
            "--file",
            "bulk.md",
            "--section",
            "Acceptance criteria",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");

    let results = json["results"].as_array().expect("expected results array");
    assert_eq!(results.len(), 3, "section has 3 tasks");
    // [ ] -> [x], [ ] -> [x], [x] -> [ ]
    assert_eq!(results[0]["status"], "x");
    assert_eq!(results[1]["status"], "x");
    assert_eq!(results[2]["status"], " ");

    let content = fs::read_to_string(tmp.path().join("bulk.md")).unwrap();
    assert!(content.contains("- [x] AC one"));
    assert!(content.contains("- [x] AC two"));
    assert!(content.contains("- [ ] AC three"));
    // Other section untouched
    assert!(content.contains("- [ ] Other task"));
}

#[test]
fn task_read_section() {
    let tmp = tempfile::tempdir().unwrap();
    setup_bulk_file(&tmp);

    let (status, json, stderr) = run_task_cmd_json(
        &tmp,
        &[
            "task",
            "read",
            "--file",
            "bulk.md",
            "--section",
            "Acceptance criteria",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");

    let results = json["results"].as_array().expect("expected results array");
    assert_eq!(results.len(), 3);
    assert_eq!(results[0]["text"], "AC one");
    assert_eq!(results[1]["text"], "AC two");
    assert_eq!(results[2]["text"], "AC three");
}

#[test]
fn task_set_status_section() {
    let tmp = tempfile::tempdir().unwrap();
    setup_bulk_file(&tmp);

    let (status, _json, stderr) = run_task_cmd_json(
        &tmp,
        &[
            "task",
            "set",
            "--file",
            "bulk.md",
            "--section",
            "Other section",
            "--status",
            "-",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");

    let content = fs::read_to_string(tmp.path().join("bulk.md")).unwrap();
    assert!(content.contains("- [-] Other task"));
}

#[test]
fn task_section_substring_match() {
    let tmp = tempfile::tempdir().unwrap();
    setup_bulk_file(&tmp);

    // "Acceptance" is a substring of "Acceptance criteria"
    let (status, json, stderr) = run_task_cmd_json(
        &tmp,
        &[
            "task",
            "read",
            "--file",
            "bulk.md",
            "--section",
            "Acceptance",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");

    let results = json["results"].as_array().expect("expected results array");
    assert_eq!(results.len(), 3);
}

#[test]
fn task_section_no_match_exits_1() {
    let tmp = tempfile::tempdir().unwrap();
    setup_bulk_file(&tmp);

    let (status, _stdout, _stderr) = run_task_cmd(
        &tmp,
        &[
            "task",
            "read",
            "--file",
            "bulk.md",
            "--section",
            "Nonexistent",
        ],
    );
    assert!(!status.success());
}

// ---------------------------------------------------------------------------
// Bulk: --all
// ---------------------------------------------------------------------------

#[test]
fn task_toggle_all() {
    let tmp = tempfile::tempdir().unwrap();
    setup_bulk_file(&tmp);

    let (status, json, stderr) =
        run_task_cmd_json(&tmp, &["task", "toggle", "--file", "bulk.md", "--all"]);
    assert!(status.success(), "stderr: {stderr}");

    let results = json["results"].as_array().expect("expected results array");
    // 6 tasks total in the file: Task A, Task B, AC one, AC two, AC three, Other task
    assert_eq!(results.len(), 6, "expected 6 tasks in file");

    let content = fs::read_to_string(tmp.path().join("bulk.md")).unwrap();
    // All [ ] become [x], the one [x] becomes [ ]
    assert!(content.contains("- [x] Task A"));
    assert!(content.contains("- [x] Task B"));
    assert!(content.contains("- [x] AC one"));
    assert!(content.contains("- [x] AC two"));
    assert!(content.contains("- [ ] AC three"));
    assert!(content.contains("- [x] Other task"));
}

#[test]
fn task_read_all() {
    let tmp = tempfile::tempdir().unwrap();
    setup_bulk_file(&tmp);

    let (status, json, stderr) =
        run_task_cmd_json(&tmp, &["task", "read", "--file", "bulk.md", "--all"]);
    assert!(status.success(), "stderr: {stderr}");

    let results = json["results"].as_array().expect("expected results array");
    assert_eq!(results.len(), 6);
}

#[test]
fn task_set_status_all() {
    let tmp = tempfile::tempdir().unwrap();
    setup_bulk_file(&tmp);

    let (status, _json, stderr) = run_task_cmd_json(
        &tmp,
        &["task", "set", "--file", "bulk.md", "--all", "--status", "x"],
    );
    assert!(status.success(), "stderr: {stderr}");

    let content = fs::read_to_string(tmp.path().join("bulk.md")).unwrap();
    assert!(content.contains("- [x] Task A"));
    assert!(content.contains("- [x] Task B"));
    assert!(content.contains("- [x] AC one"));
    assert!(content.contains("- [x] AC two"));
    assert!(content.contains("- [x] AC three"));
    assert!(content.contains("- [x] Other task"));
}

// ---------------------------------------------------------------------------
// Bulk: error cases
// ---------------------------------------------------------------------------

#[test]
fn task_no_selector_exits_2() {
    // No --line, --section, or --all -> clap should reject or dispatch should error
    let tmp = tempfile::tempdir().unwrap();
    setup_bulk_file(&tmp);

    let (status, _stdout, _stderr) = run_task_cmd(&tmp, &["task", "toggle", "--file", "bulk.md"]);
    assert!(!status.success());
}

#[test]
fn task_conflicting_selectors_exits_2() {
    let tmp = tempfile::tempdir().unwrap();
    setup_bulk_file(&tmp);

    let (status, _stdout, _stderr) = run_task_cmd(
        &tmp,
        &[
            "task", "toggle", "--file", "bulk.md", "--line", "5", "--all",
        ],
    );
    assert!(!status.success());
}

#[test]
fn task_all_on_empty_file_exits_1() {
    let tmp = tempfile::tempdir().unwrap();
    write_md(
        tmp.path(),
        "empty.md",
        "---\ntitle: Empty\n---\nNo tasks here.\n",
    );

    let (status, _stdout, _stderr) =
        run_task_cmd(&tmp, &["task", "toggle", "--file", "empty.md", "--all"]);
    assert!(!status.success());
}

// ---------------------------------------------------------------------------
// Backward compatibility: single --line returns single object (no results wrapper)
// ---------------------------------------------------------------------------

#[test]
fn task_single_line_returns_flat_object() {
    let tmp = tempfile::tempdir().unwrap();
    setup_bulk_file(&tmp);

    let (status, stdout, stderr) =
        run_task_cmd(&tmp, &["task", "read", "--file", "bulk.md", "--line", "5"]);
    assert!(status.success(), "stderr: {stderr}");

    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    // Single-line result is still wrapped in the output envelope with a "results" key,
    // but the value is a single object (not an array).
    assert!(
        json.get("results").is_some(),
        "single-line result should be in envelope with results"
    );
    assert!(
        !json["results"].is_array(),
        "single-line result should be a flat object, not an array"
    );
    assert_eq!(json["results"]["text"], "Task A");
}

// ---------------------------------------------------------------------------
// task read — line 0 boundary case
// ---------------------------------------------------------------------------

#[test]
fn task_read_line_zero_exits_1() {
    let tmp = tempfile::tempdir().unwrap();
    setup_task_file(&tmp);

    let (status, _stdout, stderr) = run_task_read(&tmp, "tasks.md", 0);
    assert!(!status.success(), "expected failure for line 0");
    assert_eq!(status.code(), Some(1));
    assert!(
        stderr.contains("not a task"),
        "expected 'not a task' error, got: {stderr}"
    );
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

// ---------------------------------------------------------------------------
// BUG-4: Deeply indented checkboxes (0, 4, 8, 16 spaces) are detected
// ---------------------------------------------------------------------------

#[test]
fn task_toggle_all_deeply_indented_checkboxes() {
    let tmp = tempfile::tempdir().unwrap();
    // Create a file with checkboxes at 0, 4, 8, and 16 spaces indentation.
    // Use concat! to avoid line-continuation whitespace trimming in string literals.
    let content = concat!(
        "---\ntitle: Nested Tasks\n---\n# Tasks\n",
        "- [ ] Task at 0 spaces\n",
        "    - [ ] Task at 4 spaces\n",
        "        - [ ] Task at 8 spaces\n",
        "                - [ ] Task at 16 spaces\n",
    );
    write_md(tmp.path(), "nested.md", content);

    let (status, json, stderr) =
        run_task_cmd_json(&tmp, &["task", "toggle", "--file", "nested.md", "--all"]);
    assert!(status.success(), "stderr: {stderr}");

    let results = json["results"].as_array().expect("expected results array");
    assert_eq!(
        results.len(),
        4,
        "all 4 indentation levels should be detected: {results:?}"
    );

    let content_after = fs::read_to_string(tmp.path().join("nested.md")).unwrap();
    assert!(
        content_after.contains("- [x] Task at 0 spaces"),
        "0-space task should be toggled"
    );
    assert!(
        content_after.contains("    - [x] Task at 4 spaces"),
        "4-space task should be toggled"
    );
    assert!(
        content_after.contains("        - [x] Task at 8 spaces"),
        "8-space task should be toggled"
    );
    assert!(
        content_after.contains("                - [x] Task at 16 spaces"),
        "16-space task should be toggled"
    );
}

#[test]
fn task_read_all_deeply_indented_checkboxes() {
    let tmp = tempfile::tempdir().unwrap();
    let content = concat!(
        "- [ ] Top level\n",
        "    - [x] Four spaces\n",
        "        - [ ] Eight spaces\n",
        "                - [x] Sixteen spaces\n",
    );
    write_md(tmp.path(), "nested.md", content);

    let (status, json, stderr) =
        run_task_cmd_json(&tmp, &["task", "read", "--file", "nested.md", "--all"]);
    assert!(status.success(), "stderr: {stderr}");

    let results = json["results"].as_array().expect("expected results array");
    assert_eq!(
        results.len(),
        4,
        "all 4 indentation levels should be read: {results:?}"
    );
    // Verify done state is correctly detected at all indentation levels
    assert_eq!(results[0]["done"], false, "top level task is incomplete");
    assert_eq!(results[1]["done"], true, "4-space task is complete");
    assert_eq!(results[2]["done"], false, "8-space task is incomplete");
    assert_eq!(results[3]["done"], true, "16-space task is complete");
}

// ---------------------------------------------------------------------------
// UX-3 / NEW-1: `task toggle --dry-run` surfaces the direction of change
// via `old_status` in JSON and `[old] -> [new]` in text output.
// ---------------------------------------------------------------------------

#[test]
fn task_toggle_dry_run_json_includes_old_status() {
    let tmp = tempfile::tempdir().unwrap();
    let content = "- [ ] Open task\n- [x] Done task\n";
    write_md(tmp.path(), "tasks.md", content);

    let (status, json, stderr) = run_task_cmd_json(
        &tmp,
        &["task", "toggle", "--file", "tasks.md", "--all", "--dry-run"],
    );
    assert!(status.success(), "stderr: {stderr}");

    let results = json["results"].as_array().expect("expected results array");
    assert_eq!(results.len(), 2);

    // Line 1: [ ] -> [x]
    assert_eq!(results[0]["line"], 1);
    assert_eq!(results[0]["old_status"], " ");
    assert_eq!(results[0]["status"], "x");
    assert_eq!(results[0]["done"], true);

    // Line 2: [x] -> [ ]
    assert_eq!(results[1]["line"], 2);
    assert_eq!(results[1]["old_status"], "x");
    assert_eq!(results[1]["status"], " ");
    assert_eq!(results[1]["done"], false);

    // File on disk must be unchanged
    let after = fs::read_to_string(tmp.path().join("tasks.md")).unwrap();
    assert_eq!(after, content, "file was modified during --dry-run");
}

#[test]
fn task_toggle_dry_run_text_uses_arrow_format() {
    let tmp = tempfile::tempdir().unwrap();
    let content = "- [ ] Open task\n- [x] Done task\n";
    write_md(tmp.path(), "tasks.md", content);

    let (status, stdout, stderr) = run_task_cmd(
        &tmp,
        &[
            "task",
            "toggle",
            "--file",
            "tasks.md",
            "--all",
            "--dry-run",
            "--format",
            "text",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");

    // Expect lines shaped like: "tasks.md":1 [ ] -> [x] Open task
    assert!(
        stdout.contains(r#""tasks.md":1 [ ] -> [x] Open task"#),
        "missing arrow format for incomplete->complete:\n{stdout}"
    );
    assert!(
        stdout.contains(r#""tasks.md":2 [x] -> [ ] Done task"#),
        "missing arrow format for complete->incomplete:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// task set --dry-run
// ---------------------------------------------------------------------------

#[test]
fn task_set_status_dry_run_does_not_modify_file() {
    let tmp = tempfile::tempdir().unwrap();
    setup_task_file(&tmp);

    let original = fs::read_to_string(tmp.path().join("tasks.md")).unwrap();

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args([
        "task",
        "set",
        "--file",
        "tasks.md",
        "--line",
        &LINE_INCOMPLETE.to_string(),
        "--status",
        "?",
        "--dry-run",
    ]);
    let output = cmd.output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let result = &json["results"];
    assert_eq!(result["old_status"], " ");
    assert_eq!(result["status"], "?");

    let after = fs::read_to_string(tmp.path().join("tasks.md")).unwrap();
    assert_eq!(original, after, "file was modified during --dry-run");
}

// ---------------------------------------------------------------------------
// Unified input resolver: --files-from and --glob on task toggle/set
// ---------------------------------------------------------------------------

#[test]
fn task_toggle_files_from_list_file() {
    let tmp = tempfile::tempdir().unwrap();
    setup_bulk_file(&tmp);
    // Create a second file to toggle.
    write_md(
        tmp.path(),
        "other.md",
        "---\ntitle: Other\n---\n- [ ] Other task\n",
    );

    // Write a files-from list referencing both files.
    let list_path = tmp.path().join("list.txt");
    fs::write(&list_path, "bulk.md\nother.md\n").unwrap();

    let (status, json, stderr) = run_task_cmd_json(
        &tmp,
        &[
            "task",
            "toggle",
            "--files-from",
            list_path.to_str().unwrap(),
            "--all",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");

    // Multi-file with --files-from: results carries a flat `files` array of all
    // toggled tasks plus the `files_*` counters from the envelope.
    let results = json["results"]
        .as_object()
        .expect("expected results object with files-from counters");
    let files = results["files"]
        .as_array()
        .expect("expected results.files array");
    assert!(files.len() >= 2, "expected tasks from both files");
    assert_eq!(results["files_missing"], 0);

    // bulk.md tasks should be toggled.
    let content = fs::read_to_string(tmp.path().join("bulk.md")).unwrap();
    assert!(content.contains("- [x] Task A"));
    assert!(content.contains("- [x] Task B"));

    // other.md task should be toggled.
    let other_content = fs::read_to_string(tmp.path().join("other.md")).unwrap();
    assert!(other_content.contains("- [x] Other task"));
}

#[test]
fn task_toggle_files_from_stdin() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let tmp = tempfile::tempdir().unwrap();
    setup_bulk_file(&tmp);

    // Pass file list via stdin using "-" as the source.
    // Use std::process::Command directly since assert_cmd::Command doesn't
    // expose stdin piping.
    let hyalo_bin = assert_cmd::cargo::cargo_bin("hyalo");
    let mut child = Command::new(&hyalo_bin)
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "--no-hints",
            "task",
            "toggle",
            "--files-from",
            "-",
            "--section",
            "Tasks",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    child.stdin.take().unwrap().write_all(b"bulk.md\n").unwrap();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    assert!(output.status.success(), "stderr: {stderr}");

    let json: serde_json::Value = serde_json::from_str(&stdout).expect("expected JSON output");
    let results = json["results"]
        .as_object()
        .expect("expected results object with files-from counters");
    let files = results["files"]
        .as_array()
        .expect("expected results.files array");
    // Section "Tasks" has Task A and Task B.
    assert_eq!(files.len(), 2, "expected 2 tasks in Tasks section");

    let content = fs::read_to_string(tmp.path().join("bulk.md")).unwrap();
    assert!(content.contains("- [x] Task A"));
    assert!(content.contains("- [x] Task B"));
    // Other section untouched.
    assert!(content.contains("- [ ] AC one"));
}

#[test]
fn task_set_files_from_list_file() {
    let tmp = tempfile::tempdir().unwrap();
    setup_bulk_file(&tmp);
    write_md(
        tmp.path(),
        "other.md",
        "---\ntitle: Other\n---\n- [ ] Other task\n",
    );

    let list_path = tmp.path().join("list.txt");
    fs::write(&list_path, "bulk.md\nother.md\n").unwrap();

    let (status, json, stderr) = run_task_cmd_json(
        &tmp,
        &[
            "task",
            "set",
            "--files-from",
            list_path.to_str().unwrap(),
            "--all",
            "--status",
            "x",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");

    let results = json["results"]
        .as_object()
        .expect("expected results object with files-from counters");
    let files = results["files"]
        .as_array()
        .expect("expected results.files array");
    assert!(files.len() >= 2, "expected tasks from both files");

    // All tasks should be marked done.
    for r in files {
        assert_eq!(r["status"], "x", "expected all tasks set to x");
    }
}

#[test]
fn task_toggle_files_from_with_line_is_rejected_at_clap() {
    // --line is per-file and cannot compose with --files-from; clap rejects.
    let tmp = tempfile::tempdir().unwrap();
    setup_bulk_file(&tmp);
    let list_path = tmp.path().join("list.txt");
    fs::write(&list_path, "bulk.md\n").unwrap();

    let (status, _stdout, stderr) = run_task_cmd(
        &tmp,
        &[
            "task",
            "toggle",
            "--files-from",
            list_path.to_str().unwrap(),
            "--line",
            "5",
        ],
    );
    assert!(
        !status.success(),
        "expected clap rejection, stderr: {stderr}"
    );
    let combined = stderr.to_lowercase();
    assert!(
        combined.contains("--line") || combined.contains("line"),
        "expected --line in error, got: {stderr}"
    );
    assert!(
        combined.contains("--files-from") || combined.contains("files-from"),
        "expected --files-from in error, got: {stderr}"
    );
}

#[test]
fn task_set_files_from_with_line_is_rejected_at_clap() {
    let tmp = tempfile::tempdir().unwrap();
    setup_bulk_file(&tmp);
    let list_path = tmp.path().join("list.txt");
    fs::write(&list_path, "bulk.md\n").unwrap();

    let (status, _stdout, stderr) = run_task_cmd(
        &tmp,
        &[
            "task",
            "set",
            "--files-from",
            list_path.to_str().unwrap(),
            "--line",
            "5",
            "--status",
            "?",
        ],
    );
    assert!(
        !status.success(),
        "expected clap rejection, stderr: {stderr}"
    );
    let combined = stderr.to_lowercase();
    assert!(
        combined.contains("--line") || combined.contains("line"),
        "expected --line in error, got: {stderr}"
    );
    assert!(
        combined.contains("--files-from") || combined.contains("files-from"),
        "expected --files-from in error, got: {stderr}"
    );
}

#[test]
fn task_set_files_from_without_all_or_section_errors() {
    // --files-from requires --all or --section for `task set` too (post-parse validation).
    let tmp = tempfile::tempdir().unwrap();
    setup_bulk_file(&tmp);
    let list_path = tmp.path().join("list.txt");
    fs::write(&list_path, "bulk.md\n").unwrap();

    let (status, stdout, stderr) = run_task_cmd(
        &tmp,
        &[
            "task",
            "set",
            "--files-from",
            list_path.to_str().unwrap(),
            "--status",
            "?",
        ],
    );
    assert!(!status.success(), "expected error, stderr: {stderr}");
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("--all") && combined.contains("--section"),
        "expected hint mentioning --all/--section, got: {combined}"
    );
}

#[test]
fn task_toggle_files_from_without_all_or_section_errors() {
    // --files-from requires --all or --section (post-parse validation).
    let tmp = tempfile::tempdir().unwrap();
    setup_bulk_file(&tmp);
    let list_path = tmp.path().join("list.txt");
    fs::write(&list_path, "bulk.md\n").unwrap();

    let (status, stdout, stderr) = run_task_cmd(
        &tmp,
        &[
            "task",
            "toggle",
            "--files-from",
            list_path.to_str().unwrap(),
        ],
    );
    assert!(!status.success(), "expected error, stderr: {stderr}");
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("--all") && combined.contains("--section"),
        "expected hint mentioning --all/--section, got: {combined}"
    );
}

#[test]
fn task_toggle_files_from_counters_for_missing_and_outside_vault() {
    let tmp = tempfile::tempdir().unwrap();
    setup_bulk_file(&tmp);

    // list mixes: one real file, one missing file, one path outside the vault.
    let list_path = tmp.path().join("list.txt");
    let outside = std::env::temp_dir()
        .join("definitely-outside-vault.md")
        .to_string_lossy()
        .into_owned();
    fs::write(
        &list_path,
        format!("bulk.md\ndoes-not-exist.md\n{outside}\n"),
    )
    .unwrap();

    let (status, json, stderr) = run_task_cmd_json(
        &tmp,
        &[
            "task",
            "toggle",
            "--files-from",
            list_path.to_str().unwrap(),
            "--section",
            "Tasks",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");
    let results = json["results"]
        .as_object()
        .expect("expected results object with files-from counters");
    assert!(
        results["files_missing"].as_u64().unwrap_or(0) >= 1,
        "expected files_missing >= 1, got: {results:?}"
    );
    assert!(
        results["files_skipped_outside_vault"].as_u64().unwrap_or(0) >= 1,
        "expected files_skipped_outside_vault >= 1, got: {results:?}"
    );
}

#[test]
fn task_toggle_files_from_empty_input_exits_zero() {
    let tmp = tempfile::tempdir().unwrap();
    setup_bulk_file(&tmp);
    let list_path = tmp.path().join("empty.txt");
    fs::write(&list_path, "").unwrap();

    let (status, _json, stderr) = run_task_cmd_json(
        &tmp,
        &[
            "task",
            "toggle",
            "--files-from",
            list_path.to_str().unwrap(),
            "--all",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");
}

#[test]
fn task_toggle_glob_single_file() {
    // Passing --glob that matches exactly one file works (SingleOrMany policy).
    let tmp = tempfile::tempdir().unwrap();
    setup_bulk_file(&tmp);

    let (status, json, stderr) = run_task_cmd_json(
        &tmp,
        &["task", "toggle", "--glob", "bulk.md", "--section", "Tasks"],
    );
    assert!(status.success(), "stderr: {stderr}");

    // Single file matched: results is an array of toggled tasks (Task A + Task B).
    let results = json["results"].as_array().expect("expected results array");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["status"], "x");
    assert_eq!(results[1]["status"], "x");
}
