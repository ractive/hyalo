#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result, bail};
use std::path::Path;

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
fn format_one_or_many<T: serde::Serialize>(results: &[T], _format: Format) -> serde_json::Value {
    if let [single] = results {
        crate::output::output_value(single)
    } else {
        crate::output::output_value(&results)
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

    read_resolved((full_path, rel_path), lines, section, all, format)
}

pub(crate) fn read_prepared(
    _dir: &Path,
    target: crate::prepared::SingleTargetRequest,
    lines: &[usize],
    section: Option<&str>,
    all: bool,
    format: Format,
) -> Result<CommandOutcome> {
    read_resolved(target.into_file(), lines, section, all, format)
}

fn read_resolved(
    (full_path, rel_path): (std::path::PathBuf, String),
    lines: &[usize],
    section: Option<&str>,
    all: bool,
    format: Format,
) -> Result<CommandOutcome> {
    let resolved = match resolve_task_lines(&full_path, lines, section, all) {
        Ok(v) => v,
        Err(e) => {
            let msg = e.to_string();
            let out = crate::output::user_diagnostic(
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
                let out = crate::output::user_diagnostic(
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

/// Toggle one or more tasks using captured preparation and effect accounting.
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
    let pair = match crate::commands::resolve_file_user(dir, file_arg) {
        Ok(pair) => pair,
        Err(error) => return Ok(resolve_error_to_outcome(error, format, dir)),
    };
    mutate_files(
        dir,
        &[pair],
        lines,
        section,
        all,
        None,
        format,
        journal,
        dry_run,
    )
}

/// Set a status using the same preparation as toggle and dry-run.
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
    let pair = match crate::commands::resolve_file_user(dir, file_arg) {
        Ok(pair) => pair,
        Err(error) => return Ok(resolve_error_to_outcome(error, format, dir)),
    };
    mutate_files(
        dir,
        &[pair],
        lines,
        section,
        all,
        Some(status),
        format,
        journal,
        dry_run,
    )
}

#[allow(clippy::too_many_arguments)]
fn mutate_files(
    dir: &Path,
    files: &[(std::path::PathBuf, String)],
    lines: &[usize],
    section: Option<&str>,
    all: bool,
    status: Option<char>,
    format: Format,
    journal: &mut crate::commands::journal::MutationJournal<'_>,
    dry_run: bool,
) -> Result<CommandOutcome> {
    let mut preparation = super::apply::PreparedChangeSet::new(dir, files.len())?;
    let mut results = Vec::new();
    for (_, rel) in files {
        let captured = preparation.capture(rel)?;
        let bytes = captured.bytes()?;
        let content = std::str::from_utf8(&bytes).context("task source is not UTF-8")?;
        let prepared = (|| -> Result<_> {
            let tasks = hyalo_core::tasks::find_task_lines_in(&bytes)?;
            let resolved = if !lines.is_empty() {
                lines.to_vec()
            } else if let Some(section) = section {
                let filter = SectionFilter::parse(section)
                    .map_err(|e| anyhow::anyhow!("invalid --section: {e}"))?;
                let mut scanner = SectionScanner::new();
                hyalo_core::scanner::scan_slice_multi(&bytes, &mut [&mut scanner])?;
                let sections = scanner.into_sections();
                if build_section_scope(&sections, std::slice::from_ref(&filter), usize::MAX).len()
                    > 1
                {
                    bail!(
                        "--section {section:?} matches multiple distinct headings; use --line or a more specific section"
                    );
                }
                let matched: Vec<_> = tasks
                    .iter()
                    .filter(|task| {
                        parse_atx_heading(&task.section)
                            .is_some_and(|(level, text)| filter.matches(level, text))
                    })
                    .map(|task| task.line)
                    .collect();
                if matched.is_empty() {
                    bail!("no tasks found in section {section:?}");
                }
                matched
            } else if all {
                if tasks.is_empty() {
                    bail!("no tasks found in file");
                }
                tasks.iter().map(|task| task.line).collect()
            } else {
                bail!("specify at least one of --line, --section, or --all");
            };
            let (rendered, infos) = hyalo_core::tasks::render_tasks(content, &resolved, status)?;
            for info in infos {
                let value = if dry_run {
                    let old_status = tasks
                        .iter()
                        .find(|task| task.line == info.line)
                        .context("prepared task missing")?
                        .status;
                    crate::output::output_value(&TaskDryRunResult {
                        file: rel.clone(),
                        line: info.line,
                        old_status,
                        status: info.status,
                        text: info.text,
                        done: info.done,
                    })
                } else {
                    crate::output::output_value(&TaskReadResult {
                        file: rel.clone(),
                        line: info.line,
                        status: info.status,
                        text: info.text,
                        done: info.done,
                    })
                };
                preparation.charge_output(&value)?;
                results.push(value);
            }
            Ok(rendered)
        })();
        let rendered = match prepared {
            Ok(rendered) => rendered,
            Err(error) => {
                return Ok(CommandOutcome::UserError(crate::output::user_diagnostic(
                    format,
                    &error.to_string(),
                    Some(rel),
                    None,
                    None,
                )));
            }
        };
        preparation.push(captured, (rendered != bytes).then_some(rendered.as_slice()))?;
    }
    let total = results.len() as u64;
    let outcome = if files.len() == 1 {
        CommandOutcome::success(super::mutation::unwrap_single_result(results))
    } else {
        CommandOutcome::success_with_total(serde_json::Value::Array(results), total)
    };
    if let CommandOutcome::Success { output, .. } = &outcome {
        preparation.check_output(output)?;
    }
    Ok(outcome.with_apply_report(if dry_run {
        preparation.preview()
    } else {
        preparation.apply(journal, "updating tasks")
    }))
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)] // dispatch handler appended below (ARCH-1, iter-225)
mod tests {
    use super::*;
    use crate::commands::journal::MutationJournal;
    use std::fs;

    fn unwrap_success(outcome: CommandOutcome) -> String {
        match outcome {
            CommandOutcome::Success { output: s, .. } => s.to_string(),
            CommandOutcome::RawOutput(s) => s,
            CommandOutcome::RawBytes(b) => String::from_utf8_lossy(&b).into_owned(),
            CommandOutcome::UserError(s) => panic!("expected success, got user error: {s}"),
        }
    }

    #[test]
    fn task_specific_second_file_conflict_retains_first_effect_and_invalidates_index() {
        use hyalo_core::index::{ScanOptions, ScannedIndex, SnapshotIndex};
        let dir = tempfile::tempdir().unwrap();
        let mut pairs = Vec::new();
        for name in ["a.md", "b.md"] {
            let path = dir.path().join(name);
            fs::write(&path, "- [ ] task\n").unwrap();
            pairs.push((path, name.to_owned()));
        }
        let built = ScannedIndex::build(
            &pairs,
            None,
            &ScanOptions {
                scan_body: true,
                bm25_tokenize: false,
                default_language: None,
                frontmatter_link_props: None,
            },
        )
        .unwrap();
        let index_path = dir.path().join(".hyalo-index");
        SnapshotIndex::save(
            &built.index,
            &index_path,
            dir.path().to_str().unwrap(),
            None,
            None,
        )
        .unwrap();
        let mut index = SnapshotIndex::load(&index_path).unwrap();
        let mut changes = crate::commands::apply::PreparedChangeSet::new(dir.path(), 2).unwrap();
        for (_, name) in &pairs {
            let captured = changes.capture(name).unwrap();
            let bytes = captured.bytes().unwrap();
            let content = std::str::from_utf8(&bytes).unwrap();
            let (rendered, _) = hyalo_core::tasks::render_tasks(content, &[1], None).unwrap();
            changes.push(captured, Some(&rendered)).unwrap();
        }
        let mut journal = MutationJournal::new(&mut index, Some(&index_path));
        let report = changes.apply_with(
            &mut journal,
            |position, _| {
                if position == 1 {
                    fs::write(dir.path().join("b.md"), "editor change\n")?;
                }
                Ok(())
            },
            |_| {},
        );
        assert_eq!(
            report.paths[0].state,
            crate::commands::apply::EffectState::Committed
        );
        assert_eq!(
            report.paths[1].state,
            crate::commands::apply::EffectState::FailedBeforeCommit
        );
        assert_eq!(
            report.index,
            crate::commands::apply::IndexDisposition::Invalidated
        );
        assert_eq!(fs::read_to_string(&pairs[0].0).unwrap(), "- [x] task\n");
        assert_eq!(fs::read_to_string(&pairs[1].0).unwrap(), "editor change\n");
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

#[allow(clippy::too_many_arguments)] // prepared target ownership plus existing task selectors
pub(crate) fn mutate_prepared(
    dir: &Path,
    targets: crate::prepared::BatchTargetRequest,
    lines: &[usize],
    section: Option<&str>,
    all: bool,
    status: Option<char>,
    format: Format,
    journal: &mut crate::commands::journal::MutationJournal<'_>,
    dry_run: bool,
) -> Result<CommandOutcome> {
    let _provenance = targets.provenance();
    mutate_files(
        dir,
        &targets.into_files(),
        lines,
        section,
        all,
        status,
        format,
        journal,
        dry_run,
    )
}
