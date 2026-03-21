#![allow(clippy::missing_errors_doc)]
use anyhow::Result;
use std::path::Path;

use crate::commands::{
    FilesOrOutcome, collect_files, resolve_error_to_outcome, unwrap_single_file_result,
};
use crate::discovery;
use crate::output::{CommandOutcome, Format};
use crate::types::{FileTasks, TaskReadResult};

/// Filter for task listing.
#[derive(Debug, Clone, Copy)]
pub enum TaskFilter {
    All,
    Done,
    Todo,
    Status(char),
}

// ---------------------------------------------------------------------------
// `hyalo tasks` — list tasks across files
// ---------------------------------------------------------------------------

/// List tasks across one or more files, optionally filtered by completion state or status char.
pub fn tasks_list(
    dir: &Path,
    file: Option<&str>,
    glob: Option<&str>,
    filter: TaskFilter,
    format: Format,
) -> Result<CommandOutcome> {
    let files = collect_files(dir, file, glob, format)?;
    let files = match files {
        FilesOrOutcome::Files(f) => f,
        FilesOrOutcome::Outcome(o) => return Ok(o),
    };

    let mut results: Vec<serde_json::Value> = Vec::new();

    for (full_path, rel_path) in &files {
        let all_tasks = crate::tasks::extract_tasks(full_path)?;
        let filtered: Vec<_> = all_tasks
            .into_iter()
            .filter(|t| match filter {
                TaskFilter::All => true,
                TaskFilter::Done => t.done,
                TaskFilter::Todo => !t.done,
                TaskFilter::Status(ch) => t.status.starts_with(ch),
            })
            .collect();
        let total = filtered.len();
        let entry = FileTasks {
            file: rel_path.clone(),
            tasks: filtered,
            total,
        };
        results.push(serde_json::to_value(entry).expect("derived Serialize impl should not fail"));
    }

    let json_output = unwrap_single_file_result(file, results);

    Ok(CommandOutcome::Success(crate::output::format_success(
        format,
        &json_output,
    )))
}

// ---------------------------------------------------------------------------
// `hyalo task read` — read single task at a line
// ---------------------------------------------------------------------------

/// Read the task at a specific 1-based line number in a single file.
pub fn task_read(
    dir: &Path,
    file_arg: &str,
    line: usize,
    format: Format,
) -> Result<CommandOutcome> {
    let (full_path, rel_path) = match discovery::resolve_file(dir, file_arg) {
        Ok(r) => r,
        Err(e) => return Ok(resolve_error_to_outcome(e, format)),
    };

    match crate::tasks::read_task(&full_path, line)? {
        None => {
            let msg = format!("line {line} is not a task");
            let out = crate::output::format_error(
                format,
                &msg,
                Some(&rel_path),
                Some("use `hyalo tasks --file <path>` to list all tasks with their line numbers"),
                None,
            );
            Ok(CommandOutcome::UserError(out))
        }
        Some(info) => {
            let result = TaskReadResult {
                file: rel_path,
                line: info.line,
                status: info.status,
                text: info.text,
                done: info.done,
            };
            Ok(CommandOutcome::Success(crate::output::format_output(
                format, &result,
            )))
        }
    }
}

// ---------------------------------------------------------------------------
// `hyalo task toggle` — toggle task completion
// ---------------------------------------------------------------------------

/// Toggle task completion at a specific 1-based line number.
pub fn task_toggle(
    dir: &Path,
    file_arg: &str,
    line: usize,
    format: Format,
) -> Result<CommandOutcome> {
    let (full_path, rel_path) = match discovery::resolve_file(dir, file_arg) {
        Ok(r) => r,
        Err(e) => return Ok(resolve_error_to_outcome(e, format)),
    };

    match crate::tasks::toggle_task(&full_path, line) {
        Ok(info) => {
            let result = TaskReadResult {
                file: rel_path,
                line: info.line,
                status: info.status,
                text: info.text,
                done: info.done,
            };
            Ok(CommandOutcome::Success(crate::output::format_output(
                format, &result,
            )))
        }
        Err(e) => {
            let msg = e.to_string();
            Ok(CommandOutcome::UserError(crate::output::format_error(
                format,
                &msg,
                Some(&rel_path),
                None,
                None,
            )))
        }
    }
}

// ---------------------------------------------------------------------------
// `hyalo task set-status` — set custom status character
// ---------------------------------------------------------------------------

/// Set a custom single-character status on a task at a specific 1-based line number.
pub fn task_set_status(
    dir: &Path,
    file_arg: &str,
    line: usize,
    status: char,
    format: Format,
) -> Result<CommandOutcome> {
    let (full_path, rel_path) = match discovery::resolve_file(dir, file_arg) {
        Ok(r) => r,
        Err(e) => return Ok(resolve_error_to_outcome(e, format)),
    };

    match crate::tasks::set_task_status(&full_path, line, status) {
        Ok(info) => {
            let result = TaskReadResult {
                file: rel_path,
                line: info.line,
                status: info.status,
                text: info.text,
                done: info.done,
            };
            Ok(CommandOutcome::Success(crate::output::format_output(
                format, &result,
            )))
        }
        Err(e) => {
            let msg = e.to_string();
            Ok(CommandOutcome::UserError(crate::output::format_error(
                format,
                &msg,
                Some(&rel_path),
                None,
                None,
            )))
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    macro_rules! md {
        ($s:expr) => {
            $s.strip_prefix('\n').unwrap_or($s)
        };
    }

    fn setup_vault() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("a.md"),
            md!(r"
---
title: A
---
- [ ] Open task
- [x] Done task
- [-] Cancelled task
"),
        )
        .unwrap();
        fs::write(
            tmp.path().join("b.md"),
            md!(r"
- [X] Another done task
- [ ] Another open task
"),
        )
        .unwrap();
        fs::write(tmp.path().join("c.md"), "No tasks here.\n").unwrap();
        tmp
    }

    fn unwrap_success(outcome: CommandOutcome) -> String {
        match outcome {
            CommandOutcome::Success(s) => s,
            CommandOutcome::UserError(s) => panic!("expected success, got user error: {s}"),
        }
    }

    // --- tasks_list ---

    #[test]
    fn tasks_list_all_tasks() {
        let tmp = setup_vault();
        let out = unwrap_success(
            tasks_list(
                tmp.path(),
                Some("a.md"),
                None,
                TaskFilter::All,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["total"], 3);
        let tasks = parsed["tasks"].as_array().unwrap();
        assert_eq!(tasks.len(), 3);
    }

    #[test]
    fn tasks_list_done_filter() {
        let tmp = setup_vault();
        let out = unwrap_success(
            tasks_list(
                tmp.path(),
                Some("a.md"),
                None,
                TaskFilter::Done,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["total"], 1);
        let tasks = parsed["tasks"].as_array().unwrap();
        assert_eq!(tasks[0]["status"], "x");
        assert_eq!(tasks[0]["done"], true);
    }

    #[test]
    fn tasks_list_todo_filter() {
        let tmp = setup_vault();
        let out = unwrap_success(
            tasks_list(
                tmp.path(),
                Some("a.md"),
                None,
                TaskFilter::Todo,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        // a.md has [ ] and [-] as not-done tasks (only [x] is done)
        assert_eq!(parsed["total"], 2);
        let tasks = parsed["tasks"].as_array().unwrap();
        assert_eq!(tasks.len(), 2);
        // First not-done task is the open one
        assert_eq!(tasks[0]["status"], " ");
    }

    #[test]
    fn tasks_list_status_filter() {
        let tmp = setup_vault();
        let out = unwrap_success(
            tasks_list(
                tmp.path(),
                Some("a.md"),
                None,
                TaskFilter::Status('-'),
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["total"], 1);
        let tasks = parsed["tasks"].as_array().unwrap();
        assert_eq!(tasks[0]["status"], "-");
    }

    #[test]
    fn tasks_list_multi_file_returns_array() {
        let tmp = setup_vault();
        let out = unwrap_success(
            tasks_list(tmp.path(), None, None, TaskFilter::All, Format::Json).unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 3); // a.md, b.md, c.md
    }

    #[test]
    fn tasks_list_file_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let outcome = tasks_list(
            tmp.path(),
            Some("nope.md"),
            None,
            TaskFilter::All,
            Format::Json,
        )
        .unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    // --- task_read ---

    #[test]
    fn task_read_finds_task() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "- [ ] My task\n").unwrap();
        let out = unwrap_success(task_read(tmp.path(), "note.md", 1, Format::Json).unwrap());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["line"], 1);
        assert_eq!(parsed["status"], " ");
        assert_eq!(parsed["text"], "My task");
        assert_eq!(parsed["done"], false);
        assert!(parsed["file"].as_str().unwrap().ends_with("note.md"));
    }

    #[test]
    fn task_read_non_task_line_returns_user_error() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "Just a regular line\n").unwrap();
        let outcome = task_read(tmp.path(), "note.md", 1, Format::Json).unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn task_read_file_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let outcome = task_read(tmp.path(), "nope.md", 1, Format::Json).unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    // --- task_toggle ---

    #[test]
    fn task_toggle_open_to_done() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "- [ ] My task\n").unwrap();
        let out = unwrap_success(task_toggle(tmp.path(), "note.md", 1, Format::Json).unwrap());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["status"], "x");
        assert_eq!(parsed["done"], true);

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains("- [x] My task"));
    }

    #[test]
    fn task_toggle_done_to_open() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "- [x] Done task\n").unwrap();
        let out = unwrap_success(task_toggle(tmp.path(), "note.md", 1, Format::Json).unwrap());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["status"], " ");
        assert_eq!(parsed["done"], false);
    }

    #[test]
    fn task_toggle_non_task_returns_user_error() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "Not a task\n").unwrap();
        let outcome = task_toggle(tmp.path(), "note.md", 1, Format::Json).unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    // --- task_set_status ---

    #[test]
    fn task_set_status_custom_char() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "- [ ] My task\n").unwrap();
        let out =
            unwrap_success(task_set_status(tmp.path(), "note.md", 1, '?', Format::Json).unwrap());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["status"], "?");
        assert_eq!(parsed["done"], false);

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains("- [?] My task"));
    }

    #[test]
    fn task_set_status_to_done() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "- [ ] My task\n").unwrap();
        let out =
            unwrap_success(task_set_status(tmp.path(), "note.md", 1, 'x', Format::Json).unwrap());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["status"], "x");
        assert_eq!(parsed["done"], true);
    }

    #[test]
    fn task_set_status_non_task_returns_user_error() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "# Heading\n").unwrap();
        let outcome = task_set_status(tmp.path(), "note.md", 1, 'x', Format::Json).unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }
}
