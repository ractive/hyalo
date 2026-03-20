pub mod links;
pub mod properties;
pub mod tags;

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

/// Convert a `FileResolveError` into a user-facing `CommandOutcome`.
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
