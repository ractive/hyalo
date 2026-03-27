#![allow(clippy::missing_errors_doc)]
pub mod append;
pub mod backlinks;
pub mod create_index;
pub mod drop_index;
pub mod find;
pub mod init;
pub mod mv;
pub mod properties;
pub mod read;
pub mod remove;
pub mod section_scanner;
pub mod set;
pub mod summary;
pub mod tags;
pub mod tasks;

use crate::output::{CommandOutcome, Format};
use anyhow::Result;
use hyalo_core::discovery::{self, FileResolveError};
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
/// Returns a user-error outcome for invalid inputs (e.g. file not found).
/// A glob that matches no files returns an empty file list with exit 0, not an error.
pub fn collect_files(
    dir: &Path,
    files: &[String],
    globs: &[String],
    format: Format,
) -> Result<FilesOrOutcome> {
    match (files.is_empty(), globs.is_empty()) {
        (false, true) => {
            // Resolve each file, best-effort: collect successes and errors
            let mut resolved = Vec::new();
            let mut errors = Vec::new();
            for f in files {
                match discovery::resolve_file(dir, f) {
                    Ok(r) => resolved.push(r),
                    Err(e) => errors.push((f.clone(), e)),
                }
            }
            if resolved.is_empty() {
                // All files failed — return error for the first one (no warning needed)
                let (_, first_err) = errors.into_iter().next().expect("at least one error");
                return Ok(FilesOrOutcome::Outcome(resolve_error_to_outcome(
                    first_err, format,
                )));
            }
            // Some succeeded — warn about the ones that didn't
            for (path, err) in &errors {
                let msg = match err {
                    FileResolveError::NotFound { .. } => format!("file not found: {path}"),
                    FileResolveError::MissingExtension { hint, .. } => {
                        format!("file not found: {path} (did you mean {hint}?)")
                    }
                    FileResolveError::OutsideVault { .. } => {
                        format!("file resolves outside vault: {path}")
                    }
                    FileResolveError::InvalidPath { reason, .. } => {
                        format!("invalid path ({reason}): {path}")
                    }
                };
                eprintln!("warning: {msg}");
            }
            Ok(FilesOrOutcome::Files(resolved))
        }
        (true, false) => {
            let all = discovery::discover_files(dir)?;
            let matched = discovery::match_globs(dir, &all, globs)?;
            Ok(FilesOrOutcome::Files(matched))
        }
        (true, true) => {
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
        (false, false) => {
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
    files: &[String],
    globs: &[String],
    command_name: &str,
    format: Format,
) -> Option<CommandOutcome> {
    if files.is_empty() && globs.is_empty() {
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

/// If exactly one file was specified and there is exactly one result, unwrap to a bare
/// JSON object. Otherwise return the full array.
#[must_use]
pub fn unwrap_single_file_result(
    files: &[String],
    mut results: Vec<serde_json::Value>,
) -> serde_json::Value {
    if files.len() == 1 && results.len() == 1 {
        results.pop().unwrap_or_default()
    } else {
        serde_json::json!(results)
    }
}

// ---------------------------------------------------------------------------
// Shared time formatting
// ---------------------------------------------------------------------------

/// Format Unix timestamp (seconds since epoch) as ISO 8601 UTC.
/// Delegates to the canonical implementation in `hyalo_core::index`.
pub(crate) use hyalo_core::index::format_iso8601;

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
        FileResolveError::OutsideVault { path } => {
            CommandOutcome::UserError(crate::output::format_error(
                format,
                "file resolves outside vault boundary",
                Some(&path),
                None,
                None,
            ))
        }
        FileResolveError::InvalidPath { path, reason } => CommandOutcome::UserError(
            crate::output::format_error(format, "invalid path", Some(&path), Some(reason), None),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso8601_epoch() {
        assert_eq!(format_iso8601(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn iso8601_known_date() {
        assert_eq!(format_iso8601(1_705_314_600), "2024-01-15T10:30:00Z");
    }
}
