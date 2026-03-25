#![allow(clippy::missing_errors_doc)]
pub mod append;
pub mod find;
pub mod init;
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
                let hint = if pattern.starts_with('!') {
                    Some("negation glob excluded all files")
                } else {
                    None
                };
                let out = crate::output::format_error(
                    format,
                    "no files match pattern",
                    Some(pattern),
                    hint,
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

// ---------------------------------------------------------------------------
// Shared time formatting
// ---------------------------------------------------------------------------

/// Format Unix timestamp (seconds since epoch) as ISO 8601 UTC.
/// Output: `YYYY-MM-DDTHH:MM:SSZ`
///
/// Manual implementation to avoid adding a time/chrono dependency.
/// Uses Howard Hinnant's civil_from_days algorithm.
pub(crate) fn format_iso8601(secs: u64) -> String {
    const SECS_PER_MIN: u64 = 60;
    const SECS_PER_HOUR: u64 = 3600;
    const SECS_PER_DAY: u64 = 86400;

    let days = secs / SECS_PER_DAY;
    let rem = secs % SECS_PER_DAY;
    let hh = rem / SECS_PER_HOUR;
    let mm = (rem % SECS_PER_HOUR) / SECS_PER_MIN;
    let ss = rem % SECS_PER_MIN;

    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
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
        FileResolveError::OutsideVault { path } => {
            CommandOutcome::UserError(crate::output::format_error(
                format,
                "file resolves outside vault boundary",
                Some(&path),
                None,
                None,
            ))
        }
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
