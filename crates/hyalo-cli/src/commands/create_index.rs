#![allow(clippy::missing_errors_doc)]
use anyhow::Result;
use hyalo_core::bm25::{Bm25InvertedIndex, PreTokenizedInput, TOKENIZER_VERSION, resolve_language};
use hyalo_core::discovery;
use hyalo_core::index::{
    IndexEntry, ScanOptions, ScannedIndex, SnapshotIndex, VaultIndex, find_stale_indexes,
    format_mtime,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::output::{CommandOutcome, Format, output_value};

/// Build a snapshot index from disk and write it to `output` (default:
/// `<dir>/.hyalo-index`).
///
/// Prints warnings for any skipped files, then reports the path and file count
/// on success.
///
/// Incremental by default (DEC-339): when `output` already holds a
/// current-format snapshot of this vault, built by the current tokenizer with
/// the same `[search] code_blocks`, unchanged files (same size and mtime) keep
/// their entries, changed and new files are re-scanned, removed files are
/// dropped, and the BM25 postings are patched in place. `force` rebuilds
/// from scratch.
#[allow(clippy::fn_params_excessive_bools)]
pub fn create_index(
    dir: &Path,
    site_prefix: Option<&str>,
    output: Option<&Path>,
    format: Format,
    allow_outside_vault: bool,
    default_language: Option<&str>,
    force: bool,
) -> Result<CommandOutcome> {
    // Determine output path
    let index_path = match output {
        Some(p) => p.to_path_buf(),
        None => dir.join(".hyalo-index"),
    };

    // Vault boundary check: run early (before the expensive scan) when the
    // caller specified a custom output path.
    if output.is_some() && !allow_outside_vault {
        let canonical_dir = discovery::canonicalize_vault_dir(dir)?;
        // A bare relative filename (e.g. `--index-file idx.bin`) yields
        // `parent() == Some("")`, which is not a canonicalizable path. Treat an
        // empty parent as the current directory so the boundary check compares
        // against `.` rather than failing on an empty path.
        let parent = match index_path.parent() {
            Some(p) if p.as_os_str().is_empty() => Path::new("."),
            Some(p) => p,
            None => {
                anyhow::bail!("output path has no parent directory");
            }
        };
        // iter-274 (BUG-25): a --output whose directory does not exist is the
        // caller's mistake — envelope and exit 1, not exit 2.
        let canonical_parent = dunce::canonicalize(parent).map_err(|e| {
            hyalo_core::user_error_with(
                format!("output directory does not exist: {}", parent.display()),
                Some(
                    "create the directory first, or write the snapshot somewhere that exists"
                        .to_owned(),
                ),
                Some(e.to_string()),
            )
        })?;
        if !canonical_parent.starts_with(&canonical_dir) {
            let out = crate::output::user_diagnostic(
                format,
                &hyalo_core::outside_vault_message("output path", Some(&canonical_parent)),
                Some(&index_path.display().to_string()),
                // iter-274 (UX-25): name the two real ways forward and nothing
                // else. `create-index` WRITES the snapshot, so the reader's
                // options are an in-vault output path or the override — never
                // a read-side `--index` flag this command does not have.
                Some(&format!(
                    "write the snapshot inside the vault (-o {}/<name>) or pass \
                     --allow-outside-vault to write here anyway",
                    dir.display()
                )),
                None,
            );
            return Ok(CommandOutcome::UserError(out));
        }
    }

    // Check if we're replacing an existing index.
    let replacing_existing = index_path.exists();

    // Discover all markdown files
    let all = discovery::discover_files(dir)?;
    let files: Vec<(PathBuf, String)> = all
        .into_iter()
        .map(|p| {
            let rel = discovery::relative_path(dir, &p);
            (p, rel)
        })
        .collect();

    // Serialize vault_dir as a canonical string (fall back to raw display)
    let vault_dir_str = std::fs::canonicalize(dir)
        .unwrap_or_else(|_| dir.to_path_buf())
        .to_string_lossy()
        .into_owned();

    // DEC-339: reuse the previous snapshot's unchanged entries and postings.
    let mut previous = if force || !replacing_existing {
        None
    } else {
        SnapshotIndex::load_for_reuse(&index_path)
            .filter(|prev| prev.validate(&vault_dir_str, site_prefix))
    };
    let mut old_bm25 = previous.as_mut().and_then(SnapshotIndex::take_bm25_index);
    if old_bm25.is_none() {
        previous = None;
    }
    let removed = previous.as_ref().map_or(0, |prev| {
        let discovered: HashSet<&str> = files.iter().map(|(_, rel)| rel.as_str()).collect();
        prev.entries()
            .iter()
            .filter(|e| !discovered.contains(e.rel_path.as_str()))
            .count()
    });
    let reused = AtomicUsize::new(0);
    let reuse = |full: &Path, rel: &str| -> Option<IndexEntry> {
        let entry = reusable_entry(previous.as_ref()?, full, rel, default_language)?;
        reused.fetch_add(1, Ordering::Relaxed);
        Some(entry)
    };

    // Build the scanned index
    let build = ScannedIndex::build_reusing(
        &files,
        site_prefix,
        &ScanOptions {
            scan_body: true,
            bm25_tokenize: true,
            default_language,
            frontmatter_link_props: None,
        },
        true,
        &reuse,
    )?;
    let reused = reused.into_inner();

    // Collect, rather than stream, the per-file diagnostics (iter-265,
    // DEC-278): `results.warnings` already carries the count, and the
    // end-of-run summary line names where to see the details.
    for w in &build.warnings {
        let kind = if w.message == hyalo_core::index::INVALID_UTF8_INDEX_MESSAGE {
            hyalo_core::warn::SkipKind::Other
        } else {
            hyalo_core::warn::SkipKind::Frontmatter
        };
        hyalo_core::warn::record_skip(w.rel_path.as_str(), w.message.as_str(), kind);
    }

    // Build the BM25 inverted index from tokenized entries (if any have
    // tokens), or patch the previous postings with the re-scanned documents.
    let rebuilt = old_bm25.is_none();
    let bm25_index = match old_bm25.take() {
        Some(mut bm25) => {
            if !patch_postings(&mut bm25, build.index.entries()) {
                // The previous postings disagree with the entries they were
                // saved beside: never guess — rebuild from scratch.
                return create_index(
                    dir,
                    site_prefix,
                    output,
                    format,
                    allow_outside_vault,
                    default_language,
                    true,
                );
            }
            Some(bm25)
        }
        None => Bm25InvertedIndex::build_from_entries(build.index.entries()),
    };

    // iter-261 (BUG-5, BUG-6): record the vault's attachments alongside the
    // notes so an `--index` run resolves `![[img.png]]` and `[[Books.base]]`
    // exactly as a disk run does. A failed walk degrades to "no attachments",
    // which is the pre-iter-261 behaviour, not an error.
    let attachments = discovery::discover_attachments(dir).unwrap_or_default();

    // Save the snapshot (with the persisted BM25 index when available).
    let publication = SnapshotIndex::save_with_attachments_observed(
        &build.index,
        &index_path,
        &vault_dir_str,
        site_prefix,
        bm25_index.as_ref(),
        &attachments,
    )?;

    if let Some(error) = publication.finalization_error {
        let report = crate::commands::apply::ApplyReport {
            paths: vec![crate::commands::apply::PathEffect {
                file: index_path.display().to_string(),
                state: crate::commands::apply::EffectState::CommittedWithFinalizationError,
                error: Some(error),
                category: Some(crate::commands::apply::EffectFailure::Finalization),
            }],
            index: crate::commands::apply::IndexDisposition::NotUsed,
            index_error: None,
        };
        return Ok(CommandOutcome::success(serde_json::Value::Null).with_apply_report(report));
    }

    // Check for stale indexes in the same directory.
    // Only run this check when we wrote to the default location; if the caller
    // redirected output elsewhere, they are managing paths themselves and a
    // warning about an unrelated default-location index would be misleading.
    // Compare against the resolved default path (handles `-o <default>` correctly).
    let wrote_to_default = index_path == dir.join(".hyalo-index");
    if wrote_to_default {
        // Same sweep, other orphan: a fallback case-sensitivity probe that was
        // killed between creating and deleting its file leaves a dot-prefixed
        // `.hyalo-case-probe-*` behind, invisible to `hyalo find`. `create-index`
        // is the one command that already writes to the vault, so it is the
        // natural place to clean up.
        hyalo_core::sweep_stale_case_probes(dir);
    }
    if wrote_to_default && let Ok(stale) = find_stale_indexes(dir) {
        for (stale_path, stale_vault, stale_ts) in stale {
            // Don't warn about the file we just wrote
            if stale_path == index_path {
                continue;
            }
            crate::warn::warn(format!(
                "stale index at {} (vault: {}, created: {})",
                stale_path.display(),
                stale_vault,
                stale_ts,
            ));
        }
    }

    let file_count = build.index.entries().len();
    let result = CreateIndexResult {
        path: index_path.display().to_string(),
        files_indexed: file_count,
        warnings: build.warnings.len(),
        note: replacing_existing.then_some("replaced existing index"),
        reused,
        refreshed: files.len() - reused,
        removed,
        rebuilt,
    };

    let report = crate::commands::apply::ApplyReport {
        paths: vec![crate::commands::apply::PathEffect {
            file: index_path.display().to_string(),
            state: crate::commands::apply::EffectState::Committed,
            error: None,
            category: None,
        }],
        index: crate::commands::apply::IndexDisposition::NotUsed,
        index_error: None,
    };
    Ok(CommandOutcome::success(output_value(&result)).with_apply_report(report))
}

/// Serialized CreateIndexResult command contract.
#[derive(serde::Serialize)]
struct CreateIndexResult<'a> {
    /// Path of the created index.
    path: String,
    /// Number of indexed Markdown files.
    files_indexed: usize,
    /// Number of scan warnings.
    warnings: usize,
    /// Replacement notice, omitted for a new index.
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<&'a str>,
    /// Entries kept from the previous snapshot unchanged (DEC-339).
    reused: usize,
    /// Files scanned and tokenized by this run (new or changed).
    refreshed: usize,
    /// Previous entries whose files no longer exist.
    removed: usize,
    /// Whether the index was built from scratch (no reusable snapshot,
    /// a format/tokenizer/setting mismatch, or `--force`).
    rebuilt: bool,
}

/// The previous snapshot's entry for `rel` when it still describes the file:
/// same size and mtime, and — for a tokenized entry — the current tokenizer
/// and the language this run would stem it with.
fn reusable_entry(
    previous: &SnapshotIndex,
    full: &Path,
    rel: &str,
    default_language: Option<&str>,
) -> Option<IndexEntry> {
    let entry = previous.get(rel)?;
    let meta = std::fs::metadata(full).ok()?;
    if meta.len() != entry.size || format_mtime(meta.modified().ok()?, full) != entry.modified {
        return None;
    }
    if let Some(version) = entry.bm25_tokenizer_version {
        let fm_lang = entry.properties.get("language").and_then(|v| v.as_str());
        let lang = resolve_language(fm_lang, None, default_language);
        if version != TOKENIZER_VERSION
            || entry.bm25_language.as_deref() != Some(lang.canonical_name())
        {
            return None;
        }
    }
    Some(entry.clone())
}

/// Patch `bm25` so it holds exactly the tokenized documents of `entries`:
/// re-scanned entries (carrying fresh tokens) replace their old postings,
/// documents of removed or now-untokenizable files are dropped. Returns
/// `false` when a reused entry's document is missing from the postings.
fn patch_postings(bm25: &mut Bm25InvertedIndex, entries: &[IndexEntry]) -> bool {
    let live: std::collections::HashMap<&str, &IndexEntry> =
        entries.iter().map(|e| (e.rel_path.as_str(), e)).collect();
    let indexed: HashSet<&str> = bm25.document_paths().collect();
    let consistent = entries.iter().all(|e| {
        e.bm25_tokens.is_some()
            || e.bm25_tokenizer_version.is_none()
            || indexed.contains(e.rel_path.as_str())
    });
    if !consistent {
        return false;
    }
    let remove: HashSet<String> = indexed
        .iter()
        .filter(|path| {
            live.get(*path)
                .is_none_or(|e| e.bm25_tokens.is_some() || e.bm25_tokenizer_version.is_none())
        })
        .map(|path| (*path).to_owned())
        .collect();
    drop(indexed);
    let add: Vec<PreTokenizedInput> = entries
        .iter()
        .filter_map(|e| {
            e.bm25_tokens.as_ref().map(|tokens| PreTokenizedInput {
                rel_path: e.rel_path.clone(),
                tokens: tokens.clone(),
            })
        })
        .collect();
    let remove_refs: HashSet<&str> = remove.iter().map(String::as_str).collect();
    bm25.apply_updates(&remove_refs, add);
    true
}
