//! Unified file-input resolver for all commands that operate on one or more
//! files.
//!
//! Replaces three separate seams that previously existed:
//! - `resolve_files_from_for_command()` in `run.rs` — per-command `--files-from` handling
//! - `collect_files()` in `commands/mod.rs` — multi-file `--file`/`--glob` resolution
//! - `resolve_single_file()` in `cli/args.rs` — single-file positional/flag pick
//!
//! The single entry point is [`resolve_inputs`]. Callers declare their semantics
//! via [`ResolutionPolicy`] and receive a [`ResolvedInputs`] struct.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::cli::inputs::InputSelection;
use crate::commands::files_from::{
    FilesFromCounters, load as files_from_load, resolve as files_from_resolve,
    resolve_with_index as files_from_resolve_with_index,
};
use crate::commands::{FilesOrOutcome, collect_files, resolve_file_user_ci};
use crate::output::{CommandOutcome, Format};
use hyalo_core::index::SnapshotIndex;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Result of resolving file inputs.
pub(crate) struct ResolvedInputs {
    /// The resolved `(full_path, vault_relative_path)` pairs.
    pub files: Vec<(PathBuf, String)>,
    /// Counter summary when `--files-from` was active; `None` otherwise.
    /// Callers propagate this to `CommandContext::files_from_counters` so the
    /// output pipeline surfaces it in the envelope.
    pub counters: Option<FilesFromCounters>,
}

/// Semantics to apply when resolving inputs.
pub(crate) enum ResolutionPolicy {
    /// Zero or more files. `require_nonempty = true` means an empty result is
    /// returned as-is and the caller decides how to handle it (typically no-op).
    /// Used by: find, lint, set, remove, append, links fix/auto.
    #[allow(dead_code)]
    Multi { require_nonempty: bool },

    /// Exactly one file must be resolved. `allow_glob` = false means --glob
    /// returns a clear error. Used by: read, backlinks, task read (single-file policy).
    Single { allow_glob: bool },

    /// One file expected, but `--files-from` and `--glob` may expand to many.
    /// Used by: task toggle, task set.
    SingleOrMany,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Resolve file inputs from an [`InputSelection`] according to `policy`.
///
/// Handles all three sources (`--file`, `--glob`, `--files-from`) and the
/// positional `FILE` argument in a single place, eliminating the duplicate
/// logic that previously lived in `resolve_files_from_for_command`,
/// `collect_files`, and `resolve_single_file`.
///
/// `configured_dir` is the `.hyalo.toml` `dir` value as a string (e.g. `"kb"`
/// or `"."`) — used for prefix stripping in `--files-from` resolution.
///
/// `case_insensitive` only affects the [`ResolutionPolicy::Single`] branch's
/// literal `--file`/positional lookup: when `true` and the exact-casing lookup
/// misses, it falls back to a case-insensitive directory scan (mirrors
/// `[links] case_insensitive`). Pass `false` for commands that don't opt into
/// case-insensitive CLI-argument resolution.
pub(crate) fn resolve_inputs(
    selection: &InputSelection,
    dir: &Path,
    configured_dir: &str,
    snapshot_index: Option<&SnapshotIndex>,
    policy: &ResolutionPolicy,
    format: Format,
    case_insensitive: bool,
) -> Result<ResolvedInputsOrOutcome> {
    // --files-from takes priority: resolve it and apply the policy's cardinality
    // contract so callers don't receive a 0- or many-element result for a Single
    // policy.
    if let Some(source) = &selection.files_from {
        let (files_pairs, counters) =
            resolve_files_from(source, dir, configured_dir, snapshot_index)?;
        if let ResolutionPolicy::Single { .. } = policy
            && files_pairs.len() > 1
        {
            let out = crate::output::format_error(
                format,
                "--files-from resolved to multiple files but this command accepts only one",
                None,
                Some("provide a list with exactly one entry, or use a multi-file command"),
                None,
            );
            return Ok(ResolvedInputsOrOutcome::Outcome(CommandOutcome::UserError(
                out,
            )));
        }
        return Ok(ResolvedInputsOrOutcome::Resolved(ResolvedInputs {
            files: files_pairs,
            counters: Some(counters),
        }));
    }

    // Build file/glob lists, merging positional into --file.
    let (files_vec, globs_vec) = merge_selection(selection);

    match policy {
        ResolutionPolicy::Single { allow_glob } => {
            if !allow_glob && !globs_vec.is_empty() {
                let out = crate::output::format_error(
                    format,
                    "--glob is not supported for this command",
                    None,
                    Some("pass a single FILE argument or use --file"),
                    None,
                );
                return Ok(ResolvedInputsOrOutcome::Outcome(CommandOutcome::UserError(
                    out,
                )));
            }

            // Require exactly one file.
            let single_file = match single_file_from_selection(selection) {
                Ok(f) => f,
                Err(e) => {
                    return Ok(ResolvedInputsOrOutcome::Outcome(CommandOutcome::UserError(
                        format!("{e}"),
                    )));
                }
            };

            // Resolve the single file.
            match resolve_file_user_ci(dir, &single_file, case_insensitive) {
                Ok(pair) => Ok(ResolvedInputsOrOutcome::Resolved(ResolvedInputs {
                    files: vec![pair],
                    counters: None,
                })),
                Err(e) => Ok(ResolvedInputsOrOutcome::Outcome(
                    crate::commands::resolve_error_to_outcome(e, format),
                )),
            }
        }

        ResolutionPolicy::Multi { .. } | ResolutionPolicy::SingleOrMany => {
            match collect_files(dir, &files_vec, &globs_vec, format)? {
                FilesOrOutcome::Files(pairs) => {
                    Ok(ResolvedInputsOrOutcome::Resolved(ResolvedInputs {
                        files: pairs,
                        counters: None,
                    }))
                }
                FilesOrOutcome::Outcome(outcome) => Ok(ResolvedInputsOrOutcome::Outcome(outcome)),
            }
        }
    }
}

/// Either a successful resolution or a user-facing error outcome.
pub(crate) enum ResolvedInputsOrOutcome {
    Resolved(ResolvedInputs),
    Outcome(CommandOutcome),
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Merge positional file argument into the file list.
/// Returns `(files, globs)` ready for `collect_files`.
fn merge_selection(sel: &InputSelection) -> (Vec<String>, Vec<String>) {
    let mut files = sel.file.clone();
    if let Some(ref pos) = sel.file_positional {
        // Clap enforces conflicts between positional and --file/--glob at parse
        // time, so at most one of these has content.
        files.push(pos.clone());
    }
    (files, sel.glob.clone())
}

/// Extract the single file argument from an `InputSelection` (positional or --file).
/// Returns an error if neither or both are provided (the latter blocked by clap).
fn single_file_from_selection(sel: &InputSelection) -> anyhow::Result<String> {
    match (&sel.file_positional, sel.file.first()) {
        (Some(f), None) | (None, Some(f)) => Ok(f.clone()),
        (None, None) => {
            anyhow::bail!("required argument missing: provide <FILE> or --file <FILE>")
        }
        // Clap prevents this at parse time.
        (Some(_), Some(_)) => anyhow::bail!("cannot specify both <FILE> and --file"),
    }
}

/// Load and resolve a `--files-from` source.
///
/// Returns `(vec_of_(full, rel), counters)`. Emits a warning to stderr when
/// all entries were missing.
fn resolve_files_from(
    source: &str,
    dir: &Path,
    configured_dir: &str,
    snapshot_index: Option<&SnapshotIndex>,
) -> Result<(Vec<(PathBuf, String)>, FilesFromCounters)> {
    let entries = files_from_load(source)?;
    let total_inputs = entries.len();

    let resolved = if let Some(idx) = snapshot_index {
        files_from_resolve_with_index(dir, &entries, configured_dir, idx)?
    } else {
        files_from_resolve(dir, &entries, configured_dir)?
    };

    let files = resolved.files;
    let counters = resolved.counters;

    // Emit hint when all inputs were missing.
    let vault_dir_display = {
        let normalized = configured_dir.replace('\\', "/");
        let trimmed = normalized.trim_end_matches('/');
        if trimmed.is_empty() {
            ".".to_owned()
        } else {
            trimmed.to_owned()
        }
    };
    if let Some(hint) = counters.all_missing_hint(files.len(), total_inputs, &vault_dir_display) {
        crate::warn::note(hint);
    }

    Ok((files, counters))
}

/// Resolve vault-relative paths from a `--files-from` source.
///
/// Identical to [`resolve_files_from`] but returns only the vault-relative
/// strings — used by the legacy `resolve_files_from_for_command` path that
/// feeds into `collect_files`.
pub(crate) fn resolve_files_from_to_rel_paths(
    source: &str,
    dir: &Path,
    configured_dir: &str,
    snapshot_index: Option<&SnapshotIndex>,
) -> Result<(Vec<String>, FilesFromCounters)> {
    let (pairs, counters) = resolve_files_from(source, dir, configured_dir, snapshot_index)?;
    let rel_paths = pairs.into_iter().map(|(_full, rel)| rel).collect();
    Ok((rel_paths, counters))
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn make_dir_with_files(names: &[&str]) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        for name in names {
            let path = tmp.path().join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(path, "").unwrap();
        }
        tmp
    }

    fn sel_positional(path: &str) -> InputSelection {
        InputSelection {
            file_positional: Some(path.to_owned()),
            ..Default::default()
        }
    }

    fn sel_file(paths: &[&str]) -> InputSelection {
        InputSelection {
            file: paths.iter().map(ToString::to_string).collect(),
            ..Default::default()
        }
    }

    fn sel_glob(patterns: &[&str]) -> InputSelection {
        InputSelection {
            glob: patterns.iter().map(ToString::to_string).collect(),
            ..Default::default()
        }
    }

    fn sel_files_from(source: &str) -> InputSelection {
        InputSelection {
            files_from: Some(source.to_owned()),
            ..Default::default()
        }
    }

    fn sel_empty() -> InputSelection {
        InputSelection::default()
    }

    // -----------------------------------------------------------------------
    // Single policy
    // -----------------------------------------------------------------------

    #[test]
    fn single_positional_resolves() {
        let tmp = make_dir_with_files(&["a.md"]);
        let sel = sel_positional("a.md");
        let result = resolve_inputs(
            &sel,
            tmp.path(),
            ".",
            None,
            &ResolutionPolicy::Single { allow_glob: false },
            Format::Json,
            false,
        )
        .unwrap();
        let ResolvedInputsOrOutcome::Resolved(r) = result else {
            panic!("expected resolved")
        };
        assert_eq!(r.files.len(), 1);
        assert_eq!(r.files[0].1, "a.md");
        assert!(r.counters.is_none());
    }

    #[test]
    fn single_flag_resolves() {
        let tmp = make_dir_with_files(&["b.md"]);
        let sel = sel_file(&["b.md"]);
        let result = resolve_inputs(
            &sel,
            tmp.path(),
            ".",
            None,
            &ResolutionPolicy::Single { allow_glob: false },
            Format::Json,
            false,
        )
        .unwrap();
        let ResolvedInputsOrOutcome::Resolved(r) = result else {
            panic!("expected resolved")
        };
        assert_eq!(r.files[0].1, "b.md");
    }

    #[test]
    fn single_no_file_returns_user_error() {
        let tmp = make_dir_with_files(&[]);
        let sel = sel_empty();
        let result = resolve_inputs(
            &sel,
            tmp.path(),
            ".",
            None,
            &ResolutionPolicy::Single { allow_glob: false },
            Format::Json,
            false,
        )
        .unwrap();
        assert!(matches!(
            result,
            ResolvedInputsOrOutcome::Outcome(CommandOutcome::UserError(_))
        ));
    }

    #[test]
    fn single_glob_not_allowed_returns_user_error() {
        let tmp = make_dir_with_files(&["a.md"]);
        let sel = sel_glob(&["*.md"]);
        let result = resolve_inputs(
            &sel,
            tmp.path(),
            ".",
            None,
            &ResolutionPolicy::Single { allow_glob: false },
            Format::Json,
            false,
        )
        .unwrap();
        assert!(matches!(
            result,
            ResolvedInputsOrOutcome::Outcome(CommandOutcome::UserError(_))
        ));
    }

    #[test]
    fn single_missing_file_returns_user_error() {
        let tmp = make_dir_with_files(&[]);
        let sel = sel_positional("nonexistent.md");
        let result = resolve_inputs(
            &sel,
            tmp.path(),
            ".",
            None,
            &ResolutionPolicy::Single { allow_glob: false },
            Format::Json,
            false,
        )
        .unwrap();
        assert!(matches!(
            result,
            ResolvedInputsOrOutcome::Outcome(CommandOutcome::UserError(_))
        ));
    }

    // -----------------------------------------------------------------------
    // Multi policy
    // -----------------------------------------------------------------------

    #[test]
    fn multi_empty_returns_all_files() {
        let tmp = make_dir_with_files(&["a.md", "b.md"]);
        let sel = sel_empty();
        let result = resolve_inputs(
            &sel,
            tmp.path(),
            ".",
            None,
            &ResolutionPolicy::Multi {
                require_nonempty: false,
            },
            Format::Json,
            false,
        )
        .unwrap();
        let ResolvedInputsOrOutcome::Resolved(r) = result else {
            panic!("expected resolved")
        };
        assert_eq!(r.files.len(), 2);
    }

    #[test]
    fn multi_file_only_resolves_named_file() {
        let tmp = make_dir_with_files(&["a.md", "b.md"]);
        let sel = sel_file(&["a.md"]);
        let result = resolve_inputs(
            &sel,
            tmp.path(),
            ".",
            None,
            &ResolutionPolicy::Multi {
                require_nonempty: false,
            },
            Format::Json,
            false,
        )
        .unwrap();
        let ResolvedInputsOrOutcome::Resolved(r) = result else {
            panic!("expected resolved")
        };
        assert_eq!(r.files.len(), 1);
        assert_eq!(r.files[0].1, "a.md");
    }

    #[test]
    fn multi_glob_only() {
        let tmp = make_dir_with_files(&["a.md", "b.md", "sub/c.md"]);
        let sel = sel_glob(&["sub/*.md"]);
        let result = resolve_inputs(
            &sel,
            tmp.path(),
            ".",
            None,
            &ResolutionPolicy::Multi {
                require_nonempty: false,
            },
            Format::Json,
            false,
        )
        .unwrap();
        let ResolvedInputsOrOutcome::Resolved(r) = result else {
            panic!("expected resolved")
        };
        assert_eq!(r.files.len(), 1);
        assert_eq!(r.files[0].1, "sub/c.md");
    }

    #[test]
    fn multi_missing_file_returns_outcome() {
        let tmp = make_dir_with_files(&[]);
        let sel = sel_file(&["missing.md"]);
        let result = resolve_inputs(
            &sel,
            tmp.path(),
            ".",
            None,
            &ResolutionPolicy::Multi {
                require_nonempty: false,
            },
            Format::Json,
            false,
        )
        .unwrap();
        assert!(matches!(
            result,
            ResolvedInputsOrOutcome::Outcome(CommandOutcome::UserError(_))
        ));
    }

    // -----------------------------------------------------------------------
    // --files-from policy
    // -----------------------------------------------------------------------

    #[test]
    fn files_from_file_path_resolves() {
        let tmp = make_dir_with_files(&["a.md", "b.md"]);
        let mut list_file = tempfile::NamedTempFile::new().unwrap();
        writeln!(list_file, "a.md").unwrap();
        writeln!(list_file, "b.md").unwrap();

        let sel = sel_files_from(list_file.path().to_str().unwrap());
        let result = resolve_inputs(
            &sel,
            tmp.path(),
            ".",
            None,
            &ResolutionPolicy::Multi {
                require_nonempty: false,
            },
            Format::Json,
            false,
        )
        .unwrap();
        let ResolvedInputsOrOutcome::Resolved(r) = result else {
            panic!("expected resolved")
        };
        assert_eq!(r.files.len(), 2);
        assert!(r.counters.is_some());
        let c = r.counters.unwrap();
        assert_eq!(c.files_missing, 0);
    }

    #[test]
    fn files_from_missing_entry_counted() {
        let tmp = make_dir_with_files(&["a.md"]);
        let mut list_file = tempfile::NamedTempFile::new().unwrap();
        writeln!(list_file, "a.md").unwrap();
        writeln!(list_file, "nonexistent.md").unwrap();

        let sel = sel_files_from(list_file.path().to_str().unwrap());
        let result = resolve_inputs(
            &sel,
            tmp.path(),
            ".",
            None,
            &ResolutionPolicy::Multi {
                require_nonempty: false,
            },
            Format::Json,
            false,
        )
        .unwrap();
        let ResolvedInputsOrOutcome::Resolved(r) = result else {
            panic!("expected resolved")
        };
        assert_eq!(r.files.len(), 1);
        let c = r.counters.unwrap();
        assert_eq!(c.files_missing, 1);
    }

    #[test]
    fn files_from_non_md_skipped() {
        let tmp = make_dir_with_files(&["a.md"]);
        let mut list_file = tempfile::NamedTempFile::new().unwrap();
        writeln!(list_file, "a.md").unwrap();
        writeln!(list_file, "readme.txt").unwrap();

        let sel = sel_files_from(list_file.path().to_str().unwrap());
        let result = resolve_inputs(
            &sel,
            tmp.path(),
            ".",
            None,
            &ResolutionPolicy::Multi {
                require_nonempty: false,
            },
            Format::Json,
            false,
        )
        .unwrap();
        let ResolvedInputsOrOutcome::Resolved(r) = result else {
            panic!("expected resolved")
        };
        assert_eq!(r.files.len(), 1);
        let c = r.counters.unwrap();
        assert_eq!(c.files_skipped_non_md, 1);
    }

    // -----------------------------------------------------------------------
    // SingleOrMany
    // -----------------------------------------------------------------------

    #[test]
    fn single_or_many_with_glob_expands() {
        let tmp = make_dir_with_files(&["a.md", "b.md"]);
        let sel = sel_glob(&["*.md"]);
        let result = resolve_inputs(
            &sel,
            tmp.path(),
            ".",
            None,
            &ResolutionPolicy::SingleOrMany,
            Format::Json,
            false,
        )
        .unwrap();
        let ResolvedInputsOrOutcome::Resolved(r) = result else {
            panic!("expected resolved")
        };
        assert_eq!(r.files.len(), 2);
    }

    #[test]
    fn single_or_many_with_single_file() {
        let tmp = make_dir_with_files(&["a.md"]);
        let sel = sel_positional("a.md");
        let result = resolve_inputs(
            &sel,
            tmp.path(),
            ".",
            None,
            &ResolutionPolicy::SingleOrMany,
            Format::Json,
            false,
        )
        .unwrap();
        let ResolvedInputsOrOutcome::Resolved(r) = result else {
            panic!("expected resolved")
        };
        assert_eq!(r.files.len(), 1);
    }
}
