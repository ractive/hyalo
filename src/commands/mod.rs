#![allow(clippy::missing_errors_doc)]
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

/// Build the standard JSON output for list-mutation commands
/// (`property add-to-list`, `property remove-from-list`, `tag add`, `tag remove`).
///
/// - `key_name`: top-level key (e.g. `"property"` or `"tag"`)
/// - `key_value`: the name that was mutated
/// - `values_label`: label for the list-of-values key (e.g. `"values"`)
/// - `values`: the values that were requested (may be empty — tags only have one)
/// - `modified`, `skipped`: file lists from the operation
#[must_use]
pub fn build_list_mutation_json(
    key_name: &str,
    key_value: &str,
    values_label: Option<&str>,
    values: Option<&[String]>,
    modified: &[String],
    skipped: &[String],
) -> serde_json::Value {
    use serde_json::json;
    let total = modified.len() + skipped.len();
    let mut obj = serde_json::Map::new();
    obj.insert(key_name.to_owned(), json!(key_value));
    if let (Some(label), Some(vals)) = (values_label, values) {
        obj.insert(label.to_owned(), json!(vals));
    }
    obj.insert("modified".to_owned(), json!(modified));
    obj.insert("skipped".to_owned(), json!(skipped));
    obj.insert("total".to_owned(), json!(total));
    serde_json::Value::Object(obj)
}

/// Build the standard JSON output for find commands
/// (`tag find`, `property find`).
///
/// - `key_name`: top-level key (e.g. `"tag"` or `"property"`)
/// - `key_value`: the name that was searched
/// - `value_filter`: the optional value filter (may be `None`)
/// - `files`: the matched file paths
#[must_use]
pub fn build_find_json(
    key_name: &str,
    key_value: &str,
    value_filter: Option<&str>,
    files: &[String],
) -> serde_json::Value {
    use serde_json::json;
    let total = files.len();
    let mut obj = serde_json::Map::new();
    obj.insert(key_name.to_owned(), json!(key_value));
    if let Some(v) = value_filter {
        obj.insert("value".to_owned(), json!(v));
    }
    obj.insert("files".to_owned(), json!(files));
    obj.insert("total".to_owned(), json!(total));
    serde_json::Value::Object(obj)
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
