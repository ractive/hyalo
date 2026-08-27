#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result, bail};
use std::path::Path;

use crate::cli::args::TaskAction;
use crate::commands::inputs::{ResolutionPolicy, ResolvedInputsOrOutcome, resolve_inputs};
use crate::commands::resolve_error_to_outcome;
use crate::output::{CommandOutcome, Format};
use hyalo_core::heading::{SectionFilter, build_section_scope, parse_atx_heading};
use hyalo_core::types::{TaskDryRunResult, TaskReadResult};
use hyalo_mdlint::profiles::section_scanner::SectionScanner;

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

        // Refuse when --section matches more than one distinct heading
        // instance (e.g. two "## Tasks" headings under different ADRs) —
        // mirrors the `links` ambiguous-target precedent (DEC-094): a
        // selector that silently spans multiple matches is unsafe for a
        // mutating command, which writes with no dry-run by default.
        let mut ss = SectionScanner::new();
        hyalo_core::scanner::scan_file_multi(full_path, &mut [&mut ss])?;
        let sections = ss.into_sections();
        let matched_headings =
            build_section_scope(&sections, std::slice::from_ref(&filter), usize::MAX);
        if matched_headings.len() > 1 {
            let lines = matched_headings
                .iter()
                .map(|r| r.start.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            bail!(
                "--section {section_str:?} matches {} distinct headings (lines {lines}); \
                 refusing to select tasks under all of them — use --line to target specific \
                 tasks, or a more specific --section (e.g. \"## Tasks\" to pin a level, or a \
                 /regex/ that only matches one heading)",
                matched_headings.len()
            );
        }

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
    let (full_path, rel_path) = match crate::commands::resolve_file_user(dir, file_arg) {
        Ok(r) => r,
        Err(e) => return Ok(resolve_error_to_outcome(e, format, dir)),
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
    journal: &mut crate::commands::journal::MutationJournal<'_>,
    dry_run: bool,
) -> Result<CommandOutcome> {
    let (full_path, rel_path) = match crate::commands::resolve_file_user(dir, file_arg) {
        Ok(r) => r,
        Err(e) => return Ok(resolve_error_to_outcome(e, format, dir)),
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

    match hyalo_core::tasks::toggle_tasks(dir, &full_path, &resolved) {
        Ok(infos) => {
            for info in &infos {
                journal.update_task(&full_path, &rel_path, info)?;
            }
            journal.flush()?;
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
    journal: &mut crate::commands::journal::MutationJournal<'_>,
    dry_run: bool,
) -> Result<CommandOutcome> {
    let (full_path, rel_path) = match crate::commands::resolve_file_user(dir, file_arg) {
        Ok(r) => r,
        Err(e) => return Ok(resolve_error_to_outcome(e, format, dir)),
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
                    let new_done = status == 'x' || status == 'X';
                    results.push(TaskDryRunResult {
                        file: rel_path.clone(),
                        line: info.line,
                        old_status: info.status,
                        status,
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

    match hyalo_core::tasks::set_tasks_status(dir, &full_path, &resolved, status) {
        Ok(infos) => {
            for info in &infos {
                journal.update_task(&full_path, &rel_path, info)?;
            }
            journal.flush()?;
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

#[cfg(test)]
#[allow(clippy::items_after_test_module)] // dispatch handler appended below (ARCH-1, iter-225)
mod tests {
    use super::*;
    use crate::commands::journal::MutationJournal;
    use std::fs;

    fn unwrap_success(outcome: CommandOutcome) -> String {
        match outcome {
            CommandOutcome::Success { output: s, .. } | CommandOutcome::RawOutput(s) => s,
            CommandOutcome::RawBytes(b) => String::from_utf8_lossy(&b).into_owned(),
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
                &mut MutationJournal::new(&mut None, None),
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
                &mut MutationJournal::new(&mut None, None),
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
            &mut MutationJournal::new(&mut None, None),
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
                &mut MutationJournal::new(&mut None, None),
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
                &mut MutationJournal::new(&mut None, None),
                false,
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
                &mut MutationJournal::new(&mut None, None),
                false,
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
            &mut MutationJournal::new(&mut None, None),
            false,
        )
        .unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn task_set_status_dry_run_does_not_write() {
        let tmp = tempfile::tempdir().unwrap();
        let original = "- [ ] My task\n";
        fs::write(tmp.path().join("note.md"), original).unwrap();
        let out = unwrap_success(
            task_set_status(
                tmp.path(),
                "note.md",
                &[1],
                None,
                false,
                '?',
                Format::Json,
                &mut MutationJournal::new(&mut None, None),
                true, // dry_run
            )
            .unwrap(),
        );
        assert!(out.contains("old_status"));
        assert!(out.contains("\"status\": \"?\"") || out.contains("\"status\":\"?\""));
        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert_eq!(content, original, "file was modified during --dry-run");
    }
}

// ---------------------------------------------------------------------------
// Dispatch handler (ARCH-1, iter-225)
// ---------------------------------------------------------------------------

/// The `hyalo task` dispatch arm, extracted verbatim from `dispatch.rs`.
/// `index_flags` on each sub-action was consumed earlier in `run.rs`
/// (snapshot loading) and never reaches here.
#[allow(clippy::items_after_statements)] // extracted handler keeps its mid-fn imports (ARCH-1, iter-225)
pub(crate) fn run(
    ctx: &mut crate::dispatch::CommandContext<'_>,
    action: TaskAction,
) -> Result<CommandOutcome> {
    let dir = ctx.dir;
    let effective_format = ctx.effective_format;
    let mut journal =
        crate::commands::journal::MutationJournal::new(&mut *ctx.snapshot_index, ctx.index_path);

    {
        match action {
            TaskAction::Read {
                selection,
                line,
                section,
                all,
                index_flags: _, // consumed in run.rs before dispatch
            } => {
                let configured_dir = ctx.configured_dir_str;
                match resolve_inputs(
                    &selection,
                    dir,
                    configured_dir,
                    journal.index(),
                    &ResolutionPolicy::Single { allow_glob: false },
                    effective_format,
                    false,
                )? {
                    ResolvedInputsOrOutcome::Outcome(o) => Ok(o),
                    ResolvedInputsOrOutcome::Resolved(r) => {
                        ctx.files_from_counters = r.counters;
                        let (_full, file) = r
                            .files
                            .into_iter()
                            .next()
                            .context("Single resolution returned no files")?;
                        crate::commands::tasks::task_read(
                            dir,
                            &file,
                            &line,
                            section.as_deref(),
                            all,
                            effective_format,
                        )
                    }
                }
            }
            TaskAction::Toggle {
                selection,
                line,
                section,
                all,
                dry_run,
                index_flags: _, // consumed in run.rs before dispatch
            } => {
                if selection.files_from.is_some() && !all && section.is_none() {
                    let out = crate::output::format_error(
                        effective_format,
                        "--files-from requires --all or --section",
                        None,
                        Some(
                            "try: --files-from <list> --all   or   --files-from <list> --section <heading>",
                        ),
                        Some(
                            "multi-file inputs need a selection that composes across files (--all or --section)",
                        ),
                    );
                    return Ok(CommandOutcome::UserError(out));
                }
                let configured_dir = ctx.configured_dir_str;
                match resolve_inputs(
                    &selection,
                    dir,
                    configured_dir,
                    journal.index(),
                    &ResolutionPolicy::SingleOrMany,
                    effective_format,
                    false,
                )? {
                    ResolvedInputsOrOutcome::Outcome(o) => Ok(o),
                    ResolvedInputsOrOutcome::Resolved(r) => {
                        ctx.files_from_counters.clone_from(&r.counters);
                        if r.files.len() == 1 {
                            // Single file: delegate directly — no wrapping.
                            let (_full_path, rel) = &r.files[0];
                            crate::commands::tasks::task_toggle(
                                dir,
                                rel,
                                &line,
                                section.as_deref(),
                                all,
                                effective_format,
                                &mut journal,
                                dry_run,
                            )
                        } else {
                            // Multi-file: collect each file's raw results into a
                            // flat array and let the pipeline wrap it in the
                            // standard `{"results": [...], "total": N}` envelope.
                            // `total` matches the flattened item count (consistent
                            // with other list-shaped outputs and `--count`).
                            let mut flat: Vec<serde_json::Value> = Vec::new();
                            for (_full_path, rel) in &r.files {
                                let outcome = crate::commands::tasks::task_toggle(
                                    dir,
                                    rel,
                                    &line,
                                    section.as_deref(),
                                    all,
                                    effective_format,
                                    &mut journal,
                                    dry_run,
                                )?;
                                match outcome {
                                    CommandOutcome::Success { output, .. } => {
                                        let val: serde_json::Value = serde_json::from_str(&output)
                                            .unwrap_or(serde_json::Value::Null);
                                        match val {
                                            serde_json::Value::Array(items) => {
                                                flat.extend(items);
                                            }
                                            other => flat.push(other),
                                        }
                                    }
                                    other => return Ok(other),
                                }
                            }
                            let total = flat.len() as u64;
                            let output = serde_json::to_string(&flat)
                                .context("failed to serialize multi-file task toggle output")?;
                            Ok(CommandOutcome::success_with_total(output, total))
                        }
                    }
                }
            }
            TaskAction::Set {
                selection,
                line,
                section,
                all,
                status,
                dry_run,
                index_flags: _, // consumed in run.rs before dispatch
            } => {
                if selection.files_from.is_some() && !all && section.is_none() {
                    let out = crate::output::format_error(
                        effective_format,
                        "--files-from requires --all or --section",
                        None,
                        Some(
                            "try: --files-from <list> --all --status <c>   or   --files-from <list> --section <heading> --status <c>",
                        ),
                        Some(
                            "multi-file inputs need a selection that composes across files (--all or --section)",
                        ),
                    );
                    return Ok(CommandOutcome::UserError(out));
                }
                if status.chars().count() != 1 {
                    let out = crate::output::format_error(
                        effective_format,
                        "--status must be a single character",
                        None,
                        Some("example: --status '?' or --status '-'"),
                        None,
                    );
                    return Ok(CommandOutcome::UserError(out));
                }
                // chars().count() == 1 guarantees next() returns Some.
                let ch = status
                    .chars()
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--status must be a single character"))?;

                let configured_dir = ctx.configured_dir_str;
                match resolve_inputs(
                    &selection,
                    dir,
                    configured_dir,
                    journal.index(),
                    &ResolutionPolicy::SingleOrMany,
                    effective_format,
                    false,
                )? {
                    ResolvedInputsOrOutcome::Outcome(o) => Ok(o),
                    ResolvedInputsOrOutcome::Resolved(r) => {
                        ctx.files_from_counters.clone_from(&r.counters);
                        if r.files.len() == 1 {
                            // Single file: delegate directly — no wrapping.
                            let (_full_path, rel) = &r.files[0];
                            crate::commands::tasks::task_set_status(
                                dir,
                                rel,
                                &line,
                                section.as_deref(),
                                all,
                                ch,
                                effective_format,
                                &mut journal,
                                dry_run,
                            )
                        } else {
                            // Multi-file: collect each file's raw results into a
                            // flat array and let the pipeline wrap it in the
                            // standard `{"results": [...], "total": N}` envelope.
                            // `total` matches the flattened item count.
                            let mut flat: Vec<serde_json::Value> = Vec::new();
                            for (_full_path, rel) in &r.files {
                                let outcome = crate::commands::tasks::task_set_status(
                                    dir,
                                    rel,
                                    &line,
                                    section.as_deref(),
                                    all,
                                    ch,
                                    effective_format,
                                    &mut journal,
                                    dry_run,
                                )?;
                                match outcome {
                                    CommandOutcome::Success { output, .. } => {
                                        let val: serde_json::Value = serde_json::from_str(&output)
                                            .unwrap_or(serde_json::Value::Null);
                                        match val {
                                            serde_json::Value::Array(items) => {
                                                flat.extend(items);
                                            }
                                            other => flat.push(other),
                                        }
                                    }
                                    other => return Ok(other),
                                }
                            }
                            let total = flat.len() as u64;
                            let output = serde_json::to_string(&flat)
                                .context("failed to serialize multi-file task set output")?;
                            Ok(CommandOutcome::success_with_total(output, total))
                        }
                    }
                }
            }
        }
    }
}
