#![allow(clippy::missing_errors_doc)]
use anyhow::Result;
use hyalo_core::bm25::{Bm25InvertedIndex, PreTokenizedInput, TOKENIZER_VERSION, resolve_language};
use hyalo_core::discovery;
use hyalo_core::index::{
    IndexEntry, STALENESS_TOLERANCE_SECS, ScanOptions, ScannedIndex, SnapshotIndex, VaultIndex,
    find_stale_indexes, format_mtime,
};
use rayon::prelude::*;
use std::collections::{BTreeSet, HashSet};
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
/// from scratch and never loads the previous snapshot first (iter-313): it
/// is discarded by construction (`force || !replacing_existing` short-
/// circuits to `None`), not loaded and then thrown away. When the rerun
/// changed nothing at all — every entry reused, nothing removed, attachments
/// unchanged — the snapshot file itself is left untouched rather than
/// rewritten byte-for-byte identical content (`results.written: false`,
/// iter-313). That is decided by [`early_no_op`] before the postings are
/// decoded or the entries cloned (DEC-414): a no-op therefore does not
/// re-validate the postings beyond their MessagePack framing, and
/// `--force` is the cure for a damaged section it cannot see.
#[allow(clippy::fn_params_excessive_bools, clippy::too_many_arguments)]
pub fn create_index(
    dir: &Path,
    site_prefix: Option<&str>,
    output: Option<&Path>,
    format: Format,
    allow_outside_vault: bool,
    default_language: Option<&str>,
    frontmatter_link_props: Option<&[String]>,
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

    // Discover all markdown files and, in the same walk, how many the VCS
    // ignore sources hide (DEC-349). The count is stored in the snapshot
    // header so `summary --index` replays it instead of walking (DEC-342).
    let (all, dropped) = discovery::discover_files_counting_gitignored(dir, false)?;
    discovery::note_gitignore_dropped(dropped);
    let files: Vec<(PathBuf, String)> = all
        .into_iter()
        .map(|p| {
            let rel = discovery::relative_path(dir, &p);
            (p, rel)
        })
        .collect();

    // Serialize vault_dir as a canonical string (fall back to raw display)
    let vault_dir_str = crate::config::snapshot_vault_dir(dir);

    // iter-261 (BUG-5, BUG-6): record the vault's attachments alongside the
    // notes so an `--index` run resolves `![[img.png]]` and `[[Books.base]]`
    // exactly as a disk run does. A failed walk degrades to "no attachments",
    // which is the pre-iter-261 behaviour, not an error.
    //
    // DEC-414: walked before the previous snapshot is loaded, together with
    // the counters the walks leave behind, so the early no-op check and the
    // late one read the same facts. Only walks write the counters, never the
    // scan, so reading them here equals reading them after the build.
    let attachments = discovery::discover_attachments(dir).unwrap_or_default();
    let current_scan_exclude = discovery::scan_exclude_patterns();
    let walked = Walked {
        dir,
        files: &files,
        attachments: &attachments,
        vault_dir: &vault_dir_str,
        site_prefix,
        frontmatter_link_props,
        scan_excluded: discovery::scan_excluded_count() as u64,
        scan_exclude: current_scan_exclude,
        gitignore_dropped: discovery::gitignore_dropped_count() as u64,
    };

    // DEC-339: reuse the previous snapshot's unchanged entries and postings.
    let mut previous = if force || !replacing_existing {
        None
    } else {
        SnapshotIndex::load_for_reuse(&index_path)
            .filter(|prev| prev.validate(&vault_dir_str, site_prefix))
            // A-6: entries scanned under another `[links] frontmatter`
            // setting hold the wrong link set; re-scan them all.
            .filter(|prev| {
                hyalo_core::index::same_link_props(
                    hyalo_core::index::VaultIndex::frontmatter_link_props(prev),
                    frontmatter_link_props,
                )
            })
    };
    // DEC-414: decide a no-op before the postings are decoded, the entries
    // cloned or the graph built.
    if let Some(prev) = previous.as_ref()
        && let Some(no_op) = early_no_op(prev, &walked, default_language)
    {
        return Ok(no_op_outcome(&index_path, &no_op));
    }
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
        Some(entry.clone())
    };

    // Build the scanned index
    let build = ScannedIndex::build_reusing(
        &files,
        site_prefix,
        &ScanOptions {
            scan_body: true,
            bm25_tokenize: true,
            default_language,
            frontmatter_link_props,
        },
        true,
        &reuse,
    )?;
    let reused = reused.into_inner();

    // Collect, rather than stream, the per-file diagnostics (iter-265,
    // DEC-278): `results.warnings` already carries the count, and the
    // end-of-run summary line names where to see the details.
    record_warnings(&build.warnings);

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
                    frontmatter_link_props,
                    true,
                );
            }
            Some(bm25)
        }
        None => Bm25InvertedIndex::build_from_entries(build.index.entries()),
    };

    let file_count = build.index.entries().len();

    // A rerun that changed nothing never gets here: `early_no_op` (iter-313,
    // DEC-361, DEC-414) answered it before any of the work above. Everything
    // that reaches this point wrote something new, so the snapshot is saved.

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

    let result = CreateIndexResult {
        path: index_path.display().to_string(),
        files_indexed: file_count,
        warnings: build.warnings.len(),
        note: replacing_existing.then_some("replaced existing index"),
        reused,
        refreshed: file_count.saturating_sub(reused),
        skipped: files.len().saturating_sub(file_count),
        removed,
        rebuilt,
        written: true,
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
    /// Entries scanned and indexed by this run (new or changed files).
    refreshed: usize,
    /// Discovered files left out of the index (unparsable frontmatter).
    skipped: usize,
    /// Previous entries whose files no longer exist.
    removed: usize,
    /// Whether the index was built from scratch (no reusable snapshot,
    /// a format/tokenizer/setting mismatch, or `--force`).
    rebuilt: bool,
    /// Whether the snapshot file was actually (re)written. `false` only for a
    /// no-op rerun — every entry reused, nothing removed, attachments
    /// unchanged — which leaves the on-disk bytes untouched (iter-313).
    written: bool,
}

/// The previous snapshot's entry for `rel` when it still describes the file:
/// same size and mtime, and — for a tokenized entry — the current tokenizer
/// and the language this run would stem it with.
///
/// Borrowed (DEC-414): the early no-op asks this of every file and must not
/// clone the answer; the incremental build clones it only when it reuses it.
fn reusable_entry<'a>(
    previous: &'a SnapshotIndex,
    full: &Path,
    rel: &str,
    default_language: Option<&str>,
) -> Option<&'a IndexEntry> {
    let entry = previous.get(rel)?;
    let meta = std::fs::metadata(full).ok()?;
    let mtime = meta.modified().ok()?;
    if meta.len() != entry.size || format_mtime(mtime, full) != entry.modified {
        return None;
    }
    // Racily clean (git's rule): a file whose mtime is not safely older than
    // the previous snapshot could have been rewritten, same size, within the
    // same whole second after it was scanned. Never trust such an entry —
    // re-scan it, so create-index stays the cure for staleness.
    let (_, _, created_at, _) = previous.header_info();
    let mtime_secs = mtime
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .ok()?
        .as_secs();
    if mtime_secs.saturating_add(STALENESS_TOLERANCE_SECS) >= created_at {
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
    Some(entry)
}

/// What this run found on disk. Both no-op checks compare the previous
/// snapshot against exactly this, so they cannot disagree about a fact.
struct Walked<'a> {
    dir: &'a Path,
    /// Every discovered note, `(full path, vault-relative path)`.
    files: &'a [(PathBuf, String)],
    attachments: &'a [String],
    vault_dir: &'a str,
    site_prefix: Option<&'a str>,
    frontmatter_link_props: Option<&'a [String]>,
    scan_excluded: u64,
    scan_exclude: &'a [String],
    gitignore_dropped: u64,
}

/// The figures a no-op rerun reports.
struct NoOp {
    file_count: usize,
    warnings: usize,
    skipped: usize,
    reused: usize,
    removed: usize,
    rebuilt: bool,
}

/// The result of a rerun that left the snapshot untouched.
fn no_op_outcome(index_path: &Path, no_op: &NoOp) -> CommandOutcome {
    let report = crate::commands::apply::ApplyReport {
        paths: vec![crate::commands::apply::PathEffect {
            file: index_path.display().to_string(),
            state: crate::commands::apply::EffectState::Unchanged,
            error: None,
            category: None,
        }],
        index: crate::commands::apply::IndexDisposition::NotUsed,
        index_error: None,
    };
    let result = CreateIndexResult {
        path: index_path.display().to_string(),
        files_indexed: no_op.file_count,
        warnings: no_op.warnings,
        note: Some("nothing changed since the last create-index — snapshot left untouched"),
        reused: no_op.reused,
        refreshed: 0,
        skipped: no_op.skipped,
        removed: no_op.removed,
        rebuilt: no_op.rebuilt,
        written: false,
    };
    CommandOutcome::success(output_value(&result)).with_apply_report(report)
}

/// Whether `prev`'s header still describes the vault this run walked, given
/// the unparsable notes `current_skipped` and the entry count `file_count`
/// the run would write (DEC-361, DEC-368, iter-327 W2).
///
/// Review round (PR #379): `reused == file_count` alone is not enough — a
/// file that silently dropped out of the entries (broken frontmatter: still
/// "discovered", so not `removed`, but no longer scannable) keeps a stale
/// entry under a skipped write; `prev.entries().len() == file_count` catches
/// it. The run's own frontmatter-skip list and the `[scan] exclude`/gitignore
/// counters must match the previous header's too: each is replayed verbatim
/// by `summary --index` and friends instead of being recomputed, so a header a
/// skipped write leaves behind must still describe the vault exactly. And the
/// cheap directory-mtime probe (DEC-280) must be clean: if a directory moved
/// since this snapshot's `created_at` — even one that nets back to the same
/// entry set, e.g. `mkdir x; rmdir x` — a no-op would leave `created_at`
/// behind the vault forever, tripping the stale-index hint on every read.
fn header_still_describes(
    prev: &SnapshotIndex,
    walked: &Walked<'_>,
    current_skipped: &BTreeSet<&str>,
    file_count: usize,
) -> bool {
    let prev_skipped: BTreeSet<&str> = prev.skipped().iter().map(String::as_str).collect();
    prev.entries().len() == file_count
        && prev.attachments() == walked.attachments
        && prev_skipped == *current_skipped
        && prev.scan_excluded() == walked.scan_excluded
        && prev.scan_exclude() == walked.scan_exclude
        && prev.gitignore_dropped() == walked.gitignore_dropped
        // W2 (iter-327): the file set is what the `[scan] include` globs and
        // the ignore files say, so a header written under others must be
        // rewritten even when every entry matches.
        && prev
            .build_info()
            .refusal(&hyalo_core::index::RunBuild {
                vault_dir: walked.vault_dir,
                site_prefix: walked.site_prefix,
                skip_code_blocks: hyalo_core::bm25::search_settings().skip_code_blocks,
                frontmatter_link_props: walked.frontmatter_link_props,
                scan_include: &discovery::scan_include_patterns(),
            })
            .is_none()
        // DEC-368: the shared rule, so a slow publish tail is not a moved
        // tree on every later run.
        && !hyalo_core::index::tree_moved(prev, walked.dir)
}

/// DEC-414: decide that nothing changed before doing any of the work that only
/// a change needs — decoding the postings, cloning every entry into a new
/// index, resolving the link graph. `None` means "not provably a no-op": the
/// caller takes the incremental path unchanged and this function has recorded
/// nothing.
///
/// A no-op needs exactly what the late check needs: every discovered note has
/// an entry that [`reusable_entry`] accepts (size, mtime, the racily-clean
/// rule, tokenizer and language), the previous entries are those notes and no
/// others, the notes the previous build could not parse are still unparsable,
/// and [`header_still_describes`].
fn early_no_op(
    prev: &SnapshotIndex,
    walked: &Walked<'_>,
    default_language: Option<&str>,
) -> Option<NoOp> {
    let skipped_before: BTreeSet<&str> = prev.skipped().iter().map(String::as_str).collect();
    let file_count = prev.entries().len();
    // An added or removed note changes this count: no stat needed to know.
    if walked.files.len() != file_count + skipped_before.len() {
        return None;
    }
    let reusable = |(full, rel): &(PathBuf, String)| {
        reusable_entry(prev, full, rel, default_language).is_some()
    };
    if !walked
        .files
        .par_iter()
        .all(|f| reusable(f) || skipped_before.contains(f.1.as_str()))
    {
        return None;
    }
    // The only notes without a reusable entry are the ones the previous build
    // could not parse; each must still fail to, and no other may.
    let unparsable: Vec<(PathBuf, String)> = if skipped_before.is_empty() {
        Vec::new()
    } else {
        walked
            .files
            .iter()
            .filter(|f| skipped_before.contains(f.1.as_str()) && !reusable(f))
            .cloned()
            .collect()
    };
    if unparsable.len() != skipped_before.len() {
        return None;
    }
    let rescan = if unparsable.is_empty() {
        None
    } else {
        let build = ScannedIndex::build(
            &unparsable,
            walked.site_prefix,
            &ScanOptions {
                scan_body: true,
                bm25_tokenize: true,
                default_language,
                frontmatter_link_props: walked.frontmatter_link_props,
            },
        )
        .ok()?;
        if !build.index.entries().is_empty() {
            return None;
        }
        Some(build)
    };
    let rescan_warnings = rescan.as_ref().map_or(&[][..], |b| b.warnings.as_slice());
    let current_skipped: BTreeSet<&str> = rescan_warnings
        .iter()
        .filter(|w| w.message != hyalo_core::index::INVALID_UTF8_INDEX_MESSAGE)
        .map(|w| w.rel_path.as_str())
        .collect();
    if !header_still_describes(prev, walked, &current_skipped, file_count) {
        return None;
    }
    // The one integrity check a no-op keeps (DEC-414): the postings are not
    // decoded, but a section that is not well-formed MessagePack to the end of
    // the file (a torn copy) still sends the run down the rebuilding path.
    if !prev.bm25_section_is_well_formed() {
        return None;
    }
    // The late path reports these through `record_skip` before it decides.
    record_warnings(rescan_warnings);
    let mut warnings = rescan_warnings.len();
    for entry in prev.entries() {
        if hyalo_core::index::reused_entry_is_invalid_utf8(entry, true) {
            hyalo_core::warn::record_skip(
                entry.rel_path.as_str(),
                hyalo_core::index::INVALID_UTF8_INDEX_MESSAGE,
                hyalo_core::warn::SkipKind::InvalidUtf8,
            );
            warnings += 1;
        }
    }
    Some(NoOp {
        file_count,
        warnings,
        skipped: walked.files.len().saturating_sub(file_count),
        reused: file_count,
        removed: 0,
        rebuilt: false,
    })
}

/// Record per-file build diagnostics for the end-of-run summary (iter-265,
/// DEC-278).
fn record_warnings(warnings: &[hyalo_core::index::IndexWarning]) {
    for w in warnings {
        let kind = if w.message == hyalo_core::index::INVALID_UTF8_INDEX_MESSAGE {
            hyalo_core::warn::SkipKind::InvalidUtf8
        } else {
            hyalo_core::warn::SkipKind::Frontmatter
        };
        hyalo_core::warn::record_skip(w.rel_path.as_str(), w.message.as_str(), kind);
    }
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
