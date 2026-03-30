#![allow(clippy::missing_errors_doc)]
pub mod append;
pub mod backlinks;
pub mod create_index;
pub mod drop_index;
pub mod find;
pub mod init;
pub mod links;
pub(crate) mod mutation;
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
use hyalo_core::index::{ScanOptions, ScannedIndex, ScannedIndexBuild, SnapshotIndex, VaultIndex};
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
                    FileResolveError::NotFoundSuggestion { suggestion, .. } => {
                        format!("file not found: {path} (did you mean {suggestion}?)")
                    }
                    FileResolveError::MissingExtension { hint, .. } => {
                        format!("file not found: {path} (did you mean {hint}?)")
                    }
                    FileResolveError::IsDirectory { hint, .. } => {
                        format!("path is a directory, not a file: {path} (try {hint})")
                    }
                    FileResolveError::OutsideVault { .. } => {
                        format!("file resolves outside vault boundary: {path}")
                    }
                    FileResolveError::InvalidPath { reason, .. } => {
                        format!("invalid path ({reason}): {path}")
                    }
                };
                crate::warn::warn(&msg);
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

/// Outcome of building a scanned index — either success or a user-facing error.
pub enum ScannedIndexOutcome {
    Index(ScannedIndexBuild),
    Outcome(CommandOutcome),
}

/// Resolved index — either a borrowed snapshot or an owned scanned build.
pub(crate) enum ResolvedIndex<'a> {
    Snapshot(&'a SnapshotIndex),
    Scanned(ScannedIndexBuild),
}

impl ResolvedIndex<'_> {
    pub(crate) fn as_index(&self) -> &dyn VaultIndex {
        match self {
            ResolvedIndex::Snapshot(idx) => *idx,
            ResolvedIndex::Scanned(build) => &build.index,
        }
    }
}

/// Resolve the vault index: use the snapshot if available, otherwise scan from disk.
///
/// Returns `Ok(Ok(ResolvedIndex))` on success.
/// Returns `Ok(Err(CommandOutcome))` when file resolution produced a user-facing error.
/// Returns `Err(e)` for unexpected I/O or parse errors.
#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_index<'a>(
    snapshot: Option<&'a SnapshotIndex>,
    dir: &Path,
    files: &[String],
    globs: &[String],
    format: Format,
    site_prefix: Option<&str>,
    needs_full_vault: bool,
    options: ScanOptions,
) -> Result<Result<ResolvedIndex<'a>, CommandOutcome>> {
    if let Some(idx) = snapshot {
        return Ok(Ok(ResolvedIndex::Snapshot(idx)));
    }
    let outcome = build_scanned_index(
        dir,
        files,
        globs,
        format,
        site_prefix,
        needs_full_vault,
        &options,
    )?;
    match outcome {
        ScannedIndexOutcome::Index(build) => Ok(Ok(ResolvedIndex::Scanned(build))),
        ScannedIndexOutcome::Outcome(o) => Ok(Err(o)),
    }
}

/// Build a [`ScannedIndex`] from disk, handling file discovery, warnings, and user errors.
///
/// When `needs_full_vault` is `true`, all `.md` files in `dir` are scanned regardless of
/// `files_arg` and `globs`.  Otherwise the normal `collect_files` resolution is used and a
/// user-error outcome is propagated if resolution fails.
pub fn build_scanned_index(
    dir: &Path,
    files_arg: &[String],
    globs: &[String],
    format: Format,
    site_prefix: Option<&str>,
    needs_full_vault: bool,
    options: &ScanOptions,
) -> Result<ScannedIndexOutcome> {
    let files: Vec<(PathBuf, String)> = if needs_full_vault {
        // Validate --file arguments even when doing a full-vault scan.
        // Without this, missing files silently produce zero results instead
        // of the expected UserError.
        if !files_arg.is_empty() {
            let mut resolved = Vec::new();
            let mut first_err = None;
            for f in files_arg {
                match discovery::resolve_file(dir, f) {
                    Ok(r) => resolved.push(r),
                    Err(e) if first_err.is_none() => first_err = Some(e),
                    Err(_) => {}
                }
            }
            if resolved.is_empty()
                && let Some(e) = first_err
            {
                return Ok(ScannedIndexOutcome::Outcome(resolve_error_to_outcome(
                    e, format,
                )));
            }
        }
        discovery::discover_files(dir)?
            .into_iter()
            .map(|p| {
                let rel = discovery::relative_path(dir, &p);
                (p, rel)
            })
            .collect()
    } else {
        match collect_files(dir, files_arg, globs, format)? {
            FilesOrOutcome::Outcome(o) => return Ok(ScannedIndexOutcome::Outcome(o)),
            FilesOrOutcome::Files(f) => f,
        }
    };

    let build = ScannedIndex::build(&files, site_prefix, options)?;

    for w in &build.warnings {
        crate::warn::warn(format!("skipping {}: {}", w.rel_path, w.message));
    }

    Ok(ScannedIndexOutcome::Index(build))
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

/// Characters that form the start of comparison operators in filter syntax (`>=`, `<=`,
/// `!=`, `~=`).  When a `--property` key ends with one of these in a mutation command
/// (`set`, `remove`, `append`), it almost certainly means the user intended
/// `--where-property` instead.
const FILTER_OP_SUFFIXES: &[char] = &['<', '>', '!', '~'];

/// Reject a `--property` key that looks like a filter expression (ends with a comparison
/// operator prefix).  Returns `Some(CommandOutcome::UserError(...))` when rejected, or
/// `None` when the key is fine.
#[must_use]
pub fn reject_filter_in_mutation_property(key: &str, format: Format) -> Option<CommandOutcome> {
    let trimmed = key.trim_end();
    let ch = trimmed.chars().last()?;
    if !FILTER_OP_SUFFIXES.contains(&ch) {
        return None;
    }
    let out = crate::output::format_error(
        format,
        &format!(
            "invalid property name '{trimmed}': ends with '{ch}' which looks like a filter \
             operator (e.g. >=, <=, !=, ~=)"
        ),
        None,
        Some(
            "--property in mutation commands is for mutation, not filtering — \
             use --where-property to filter which files are mutated",
        ),
        None,
    );
    Some(CommandOutcome::UserError(out))
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
        FileResolveError::NotFoundSuggestion { path, suggestion } => {
            CommandOutcome::UserError(crate::output::format_error(
                format,
                "file not found",
                Some(&path),
                Some(&format!("did you mean {suggestion}?")),
                None,
            ))
        }
        FileResolveError::IsDirectory { path, hint } => {
            CommandOutcome::UserError(crate::output::format_error(
                format,
                "path is a directory, not a file",
                Some(&path),
                Some(&hint),
                None,
            ))
        }
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
    use hyalo_core::index::format_iso8601;

    // --- reject_filter_in_mutation_property ---

    #[test]
    fn reject_filter_gt() {
        assert!(reject_filter_in_mutation_property("priority>", Format::Json).is_some());
    }

    #[test]
    fn reject_filter_lt() {
        assert!(reject_filter_in_mutation_property("priority<", Format::Json).is_some());
    }

    #[test]
    fn reject_filter_bang() {
        assert!(reject_filter_in_mutation_property("status!", Format::Json).is_some());
    }

    #[test]
    fn reject_filter_tilde() {
        assert!(reject_filter_in_mutation_property("name~", Format::Json).is_some());
    }

    #[test]
    fn accept_plain_key() {
        assert!(reject_filter_in_mutation_property("status", Format::Json).is_none());
    }

    #[test]
    fn accept_hyphenated_key() {
        assert!(reject_filter_in_mutation_property("my-key", Format::Json).is_none());
    }

    #[test]
    fn accept_underscored_key() {
        assert!(reject_filter_in_mutation_property("key_name", Format::Json).is_none());
    }

    #[test]
    fn accept_empty_key() {
        // Empty keys are handled elsewhere; the guard should not panic
        assert!(reject_filter_in_mutation_property("", Format::Json).is_none());
    }

    // --- iso8601 ---

    #[test]
    fn iso8601_epoch() {
        assert_eq!(format_iso8601(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn iso8601_known_date() {
        assert_eq!(format_iso8601(1_705_314_600), "2024-01-15T10:30:00Z");
    }
}
