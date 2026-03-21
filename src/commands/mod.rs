#![allow(clippy::missing_errors_doc)]
pub mod links;
pub mod outline;
pub mod properties;
pub mod summary;
pub mod tags;
pub mod tasks;

use crate::discovery::{self, FileResolveError};
use crate::output::{CommandOutcome, Format};
use anyhow::Result;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Shared file resolution helpers
// ---------------------------------------------------------------------------

/// Outcome of resolving the set of files to operate on.
/// Either a list of `(full_path, rel_path)` pairs or a pre-formed `CommandOutcome`
/// (user error) when the resolution failed.
pub enum FilesOrOutcome {
    Files(Vec<(PathBuf, String)>),
    Outcome(CommandOutcome),
}

/// Resolve the set of files to operate on based on `--file` / `--glob` / all files.
/// Returns a user-error outcome for invalid inputs (file not found, no glob matches).
pub fn collect_files(
    dir: &Path,
    file: Option<&str>,
    glob: Option<&str>,
    format: Format,
) -> Result<FilesOrOutcome> {
    match (file, glob) {
        (Some(f), None) => {
            let resolved = match discovery::resolve_file(dir, f) {
                Ok(r) => r,
                Err(e) => return Ok(FilesOrOutcome::Outcome(resolve_error_to_outcome(e, format))),
            };
            Ok(FilesOrOutcome::Files(vec![resolved]))
        }
        (None, Some(pattern)) => {
            let all = discovery::discover_files(dir)?;
            let matched = discovery::match_glob(dir, &all, pattern)?;
            if matched.is_empty() {
                let out = crate::output::format_error(
                    format,
                    "no files match pattern",
                    Some(pattern),
                    None,
                    None,
                );
                return Ok(FilesOrOutcome::Outcome(CommandOutcome::UserError(out)));
            }
            Ok(FilesOrOutcome::Files(matched))
        }
        (None, None) => {
            // Operate on all .md files
            let all = discovery::discover_files(dir)?;
            let with_rel: Vec<(PathBuf, String)> = all
                .into_iter()
                .map(|p| {
                    let rel = discovery::relative_path(dir, &p);
                    (p, rel)
                })
                .collect();
            Ok(FilesOrOutcome::Files(with_rel))
        }
        (Some(_), Some(_)) => {
            // Clap enforces mutual exclusivity; this branch is unreachable in practice
            let out = crate::output::format_error(
                format,
                "--file and --glob are mutually exclusive",
                None,
                None,
                None,
            );
            Ok(FilesOrOutcome::Outcome(CommandOutcome::UserError(out)))
        }
    }
}

/// Guard that mutation commands require `--file` or `--glob`.
///
/// Returns `Some(CommandOutcome::UserError(...))` when neither flag is provided, or `None`
/// when the caller may proceed.  The `command_name` is used in the error message.
#[must_use]
pub fn require_file_or_glob(
    file: Option<&str>,
    glob: Option<&str>,
    command_name: &str,
    format: Format,
) -> Option<CommandOutcome> {
    if file.is_none() && glob.is_none() {
        let out = crate::output::format_error(
            format,
            &format!("{command_name} requires --file or --glob"),
            None,
            Some(
                "use --file <path> to target a single file or --glob <pattern> to target multiple files",
            ),
            None,
        );
        Some(CommandOutcome::UserError(out))
    } else {
        None
    }
}

/// If `--file` was used (single-file mode), unwrap a one-element results Vec into a bare
/// JSON object. Otherwise return the full array.
#[must_use]
pub fn unwrap_single_file_result(
    file: Option<&str>,
    mut results: Vec<serde_json::Value>,
) -> serde_json::Value {
    if file.is_some() && results.len() == 1 {
        results.pop().unwrap_or_default()
    } else {
        serde_json::json!(results)
    }
}

/// Convert a `FileResolveError` into a user-facing `CommandOutcome`.
#[must_use]
pub fn resolve_error_to_outcome(err: FileResolveError, format: Format) -> CommandOutcome {
    match err {
        FileResolveError::MissingExtension { path, hint } => {
            CommandOutcome::UserError(crate::output::format_error(
                format,
                "file not found",
                Some(&path),
                Some(&format!("did you mean {hint}?")),
                None,
            ))
        }
        FileResolveError::NotFound { path } => CommandOutcome::UserError(
            crate::output::format_error(format, "file not found", Some(&path), None, None),
        ),
    }
}
