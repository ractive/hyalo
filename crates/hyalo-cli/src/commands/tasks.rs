#![allow(clippy::missing_errors_doc)]
use anyhow::{Result, bail};
use std::path::Path;

use crate::commands::resolve_error_to_outcome;
use crate::output::{CommandOutcome, Format};
use hyalo_core::discovery;
use hyalo_core::heading::{SectionFilter, parse_atx_heading};
use hyalo_core::index::{SnapshotIndex, format_modified};
use hyalo_core::types::{TaskDryRunResult, TaskInfo, TaskReadResult};

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Selector resolution
// ---------------------------------------------------------------------------

/// Resolve task selectors to a sorted, deduplicated list of 1-based line numbers.
fn resolve_task_lines(
    full_path: &Path,
    lines: &[usize],
    section: Option<&str>,
    all: bool,
) -> Result<Vec<usize>> {
    if !lines.is_empty() {
        let mut sorted = lines.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        return Ok(sorted);
    }

    if let Some(section_str) = section {
        let filter = SectionFilter::parse(section_str)
            .map_err(|e| anyhow::anyhow!("invalid --section: {e}"))?;
        let tasks = hyalo_core::tasks::find_task_lines(full_path)?;
        let matched: Vec<usize> = tasks
            .iter()
            .filter(|t| {
                // t.section is formatted as "## heading text" — parse it back
                if t.section.is_empty() {
                    return false;
                }
                if let Some((level, text)) = parse_atx_heading(&t.section) {
                    filter.matches(level, text)
                } else {
                    false
                }
            })
            .map(|t| t.line)
            .collect();
        if matched.is_empty() {
            bail!("no tasks found in section {section_str:?}");
        }
        return Ok(matched);
    }

    if all {
        let tasks = hyalo_core::tasks::find_task_lines(full_path)?;
        if tasks.is_empty() {
            bail!("no tasks found in file");
        }
        return Ok(tasks.iter().map(|t| t.line).collect());
    }

    bail!("specify at least one of --line, --section, or --all")
}

/// Format a slice of results: single object when exactly 1 element, Vec when
/// multiple. The output pipeline later wraps this in the
/// `{"results": ..., "hints": [...]}` envelope. Generic over the result type
/// so both `TaskReadResult` and `TaskDryRunResult` share the same branching.
fn format_one_or_many<T: serde::Serialize>(results: &[T], format: Format) -> String {
    if let [single] = results {
        crate::output::format_output(format, single)
    } else {
        crate::output::format_output(format, &results)
    }
}

// ---------------------------------------------------------------------------
// `hyalo task read` — read task(s) at given line(s)
// ---------------------------------------------------------------------------

/// Read one or more tasks by line selector.
pub fn task_read(
    dir: &Path,
    file_arg: &str,
    lines: &[usize],
    section: Option<&str>,
    all: bool,
    format: Format,
) -> Result<CommandOutcome> {
    let (full_path, rel_path) = match discovery::resolve_file(dir, file_arg) {
        Ok(r) => r,
        Err(e) => return Ok(resolve_error_to_outcome(e, format)),
    };

    let resolved = match resolve_task_lines(&full_path, lines, section, all) {
        Ok(v) => v,
        Err(e) => {
            let msg = e.to_string();
            let out = crate::output::format_error(
                format,
                &msg,
                Some(&rel_path),
                Some(
                    "use `hyalo find --task any --file <path>` to list all tasks with their line numbers",
                ),
                None,
            );
            return Ok(CommandOutcome::UserError(out));
        }
    };

    let mut results = Vec::with_capacity(resolved.len());
    for line in resolved {
        match hyalo_core::tasks::read_task(&full_path, line)? {
            None => {
                let msg = format!("line {line} is not a task");
                let out = crate::output::format_error(
                    format,
                    &msg,
                    Some(&rel_path),
                    Some(
                        "use `hyalo find --task any --file <path>` to list all tasks with their line numbers",
                    ),
                    None,
                );
                return Ok(CommandOutcome::UserError(out));
            }
            Some(info) => {
                results.push(TaskReadResult {
                    file: rel_path.clone(),
                    line: info.line,
                    status: info.status,
                    text: info.text,
                    done: info.done,
                });
            }
        }
    }

    Ok(CommandOutcome::success(format_one_or_many(
        &results, format,
    )))
}

// ---------------------------------------------------------------------------
// `hyalo task toggle` — toggle task completion
// ---------------------------------------------------------------------------

/// Toggle one or more tasks by line selector.
#[allow(clippy::too_many_arguments)]
pub fn task_toggle(
    dir: &Path,
    file_arg: &str,
    lines: &[usize],
    section: Option<&str>,
    all: bool,
    format: Format,
    snapshot_index: &mut Option<SnapshotIndex>,
    index_path: Option<&Path>,
    dry_run: bool,
) -> Result<CommandOutcome> {
    let (full_path, rel_path) = match discovery::resolve_file(dir, file_arg) {
        Ok(r) => r,
        Err(e) => return Ok(resolve_error_to_outcome(e, format)),
    };

    let resolved = match resolve_task_lines(&full_path, lines, section, all) {
        Ok(v) => v,
        Err(e) => {
            let msg = e.to_string();
            return Ok(CommandOutcome::UserError(crate::output::format_error(
                format,
                &msg,
                Some(&rel_path),
                None,
                None,
            )));
        }
    };

    if dry_run {
        // In dry-run mode: compute the toggled state without writing to disk.
        //
        // Single-pass scan: collect every task in the file once, then look up
        // each resolved target line. Avoids O(n * file_length) from calling
        // `read_task` per line when --all or a large --line list is used.
        //
        // We emit `TaskDryRunResult` (carrying both `old_status` and `status`)
        // so the text formatter can render `"file":line [old] -> [new] text`
        // and make the direction of change explicit. The dispatch layer always
        // forces JSON here; text rendering happens later in the output
        // pipeline via a shape-specific jq filter.
        let tasks_by_line: std::collections::HashMap<usize, hyalo_core::types::FindTaskInfo> =
            hyalo_core::tasks::find_task_lines(&full_path)?
                .into_iter()
                .map(|t| (t.line, t))
                .collect();
        let mut results: Vec<TaskDryRunResult> = Vec::with_capacity(resolved.len());
        for &line_num in &resolved {
            match tasks_by_line.get(&line_num) {
                None => {
                    let msg = format!("line {line_num} is not a task");
                    return Ok(CommandOutcome::UserError(crate::output::format_error(
                        format,
                        &msg,
                        Some(&rel_path),
                        None,
                        None,
                    )));
                }
                Some(info) => {
                    // Simulate what toggle would do: flip done state.
                    let new_done = !info.done;
                    let new_status = if new_done { 'x' } else { ' ' };
                    results.push(TaskDryRunResult {
                        file: rel_path.clone(),
                        line: info.line,
                        old_status: info.status,
                        status: new_status,
                        text: info.text.clone(),
                        done: new_done,
                    });
                }
            }
        }
        return Ok(CommandOutcome::success(format_one_or_many(
            &results, format,
        )));
    }

    match hyalo_core::tasks::toggle_tasks(&full_path, &resolved) {
        Ok(infos) => {
            for info in &infos {
                patch_index(&full_path, &rel_path, info, snapshot_index, index_path)?;
            }
            let results: Vec<TaskReadResult> = infos
                .into_iter()
                .map(|info| TaskReadResult {
                    file: rel_path.clone(),
                    line: info.line,
                    status: info.status,
                    text: info.text,
                    done: info.done,
                })
                .collect();
            Ok(CommandOutcome::success(format_one_or_many(
                &results, format,
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
// `hyalo task set` — set custom status character
// ---------------------------------------------------------------------------

/// Set status on one or more tasks by line selector.
#[allow(clippy::too_many_arguments)]
pub fn task_set_status(
    dir: &Path,
    file_arg: &str,
    lines: &[usize],
    section: Option<&str>,
    all: bool,
    status: char,
    format: Format,
    snapshot_index: &mut Option<SnapshotIndex>,
    index_path: Option<&Path>,
) -> Result<CommandOutcome> {
    let (full_path, rel_path) = match discovery::resolve_file(dir, file_arg) {
        Ok(r) => r,
        Err(e) => return Ok(resolve_error_to_outcome(e, format)),
    };

    let resolved = match resolve_task_lines(&full_path, lines, section, all) {
        Ok(v) => v,
        Err(e) => {
            let msg = e.to_string();
            return Ok(CommandOutcome::UserError(crate::output::format_error(
                format,
                &msg,
                Some(&rel_path),
                None,
                None,
            )));
        }
    };

    match hyalo_core::tasks::set_tasks_status(&full_path, &resolved, status) {
        Ok(infos) => {
            for info in &infos {
                patch_index(&full_path, &rel_path, info, snapshot_index, index_path)?;
            }
            let results: Vec<TaskReadResult> = infos
                .into_iter()
                .map(|info| TaskReadResult {
                    file: rel_path.clone(),
                    line: info.line,
                    status: info.status,
                    text: info.text,
                    done: info.done,
                })
                .collect();
            Ok(CommandOutcome::success(format_one_or_many(
                &results, format,
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
// Index patching helper
// ---------------------------------------------------------------------------

fn patch_index(
    full_path: &Path,
    rel_path: &str,
    info: &TaskInfo,
    snapshot_index: &mut Option<SnapshotIndex>,
    index_path: Option<&Path>,
) -> Result<()> {
    if let (Some(idx), Some(idx_path)) = (snapshot_index.as_mut(), index_path) {
        if let Some(entry) = idx.get_mut(rel_path) {
            if let Some(task) = entry.tasks.iter_mut().find(|t| t.line == info.line) {
                task.status = info.status;
                task.done = info.done;
            }
            // Rebuild section task counts from the updated task list.
            // Each section owns the range [section.line, next_section.line).
            let section_starts: Vec<usize> = entry.sections.iter().map(|s| s.line).collect();
            for (si, section) in entry.sections.iter_mut().enumerate() {
                let start = section_starts[si];
                let end = section_starts.get(si + 1).copied().unwrap_or(usize::MAX);
                let total = entry
                    .tasks
                    .iter()
                    .filter(|t| t.line >= start && t.line < end)
                    .count();
                if total > 0 {
                    let done = entry
                        .tasks
                        .iter()
                        .filter(|t| t.line >= start && t.line < end && t.done)
                        .count();
                    section.tasks = Some(hyalo_core::types::TaskCount { total, done });
                } else {
                    section.tasks = None;
                }
            }
            entry.modified = format_modified(full_path)?;
        }
        idx.save_to(idx_path)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn unwrap_success(outcome: CommandOutcome) -> String {
        match outcome {
            CommandOutcome::Success { output: s, .. } | CommandOutcome::RawOutput(s) => s,
            CommandOutcome::UserError(s) => panic!("expected success, got user error: {s}"),
        }
    }

    // --- task_read ---

    #[test]
    fn task_read_finds_task() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "- [ ] My task\n").unwrap();
        let out = unwrap_success(
            task_read(tmp.path(), "note.md", &[1], None, false, Format::Json).unwrap(),
        );
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
        let outcome = task_read(tmp.path(), "note.md", &[1], None, false, Format::Json).unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn task_read_file_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let outcome = task_read(tmp.path(), "nope.md", &[1], None, false, Format::Json).unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    // --- task_toggle ---

    #[test]
    fn task_toggle_open_to_done() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "- [ ] My task\n").unwrap();
        let out = unwrap_success(
            task_toggle(
                tmp.path(),
                "note.md",
                &[1],
                None,
                false,
                Format::Json,
                &mut None,
                None,
                false,
            )
            .unwrap(),
        );
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
        let out = unwrap_success(
            task_toggle(
                tmp.path(),
                "note.md",
                &[1],
                None,
                false,
                Format::Json,
                &mut None,
                None,
                false,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["status"], " ");
        assert_eq!(parsed["done"], false);
    }

    #[test]
    fn task_toggle_non_task_returns_user_error() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "Not a task\n").unwrap();
        let outcome = task_toggle(
            tmp.path(),
            "note.md",
            &[1],
            None,
            false,
            Format::Json,
            &mut None,
            None,
            false,
        )
        .unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn task_toggle_dry_run_does_not_modify_file() {
        let tmp = tempfile::tempdir().unwrap();
        let original = "- [ ] My task\n";
        fs::write(tmp.path().join("note.md"), original).unwrap();

        let out = unwrap_success(
            task_toggle(
                tmp.path(),
                "note.md",
                &[1],
                None,
                false,
                Format::Json,
                &mut None,
                None,
                true, // dry_run
            )
            .unwrap(),
        );

        // Output should reflect the toggled state (done=true)
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["status"], "x");
        assert_eq!(parsed["done"], true);

        // But the file on disk must be unchanged
        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert_eq!(content, original, "file was modified during --dry-run");
    }

    // --- task_set_status ---

    #[test]
    fn task_set_status_custom_char() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "- [ ] My task\n").unwrap();
        let out = unwrap_success(
            task_set_status(
                tmp.path(),
                "note.md",
                &[1],
                None,
                false,
                '?',
                Format::Json,
                &mut None,
                None,
            )
            .unwrap(),
        );
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
        let out = unwrap_success(
            task_set_status(
                tmp.path(),
                "note.md",
                &[1],
                None,
                false,
                'x',
                Format::Json,
                &mut None,
                None,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["status"], "x");
        assert_eq!(parsed["done"], true);
    }

    #[test]
    fn task_set_status_non_task_returns_user_error() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "# Heading\n").unwrap();
        let outcome = task_set_status(
            tmp.path(),
            "note.md",
            &[1],
            None,
            false,
            'x',
            Format::Json,
            &mut None,
            None,
        )
        .unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }
}
