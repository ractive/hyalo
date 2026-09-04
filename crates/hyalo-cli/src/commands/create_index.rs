#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result};
use hyalo_core::bm25::Bm25InvertedIndex;
use hyalo_core::discovery;
use hyalo_core::index::{ScanOptions, ScannedIndex, SnapshotIndex, VaultIndex, find_stale_indexes};
use std::path::{Path, PathBuf};

use crate::output::{CommandOutcome, Format, format_success};

/// Build a snapshot index from disk and write it to `output` (default:
/// `<dir>/.hyalo-index`).
///
/// Prints warnings for any skipped files, then reports the path and file count
/// on success.
pub fn create_index(
    dir: &Path,
    site_prefix: Option<&str>,
    output: Option<&Path>,
    format: Format,
    allow_outside_vault: bool,
    default_language: Option<&str>,
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
        let canonical_parent = dunce::canonicalize(parent).with_context(|| {
            format!(
                "failed to canonicalize parent of output path: {}",
                parent.display()
            )
        })?;
        if !canonical_parent.starts_with(&canonical_dir) {
            let out = crate::output::format_error(
                format,
                &hyalo_core::outside_vault_message("output path", Some(&canonical_parent)),
                Some(&index_path.display().to_string()),
                Some("use --allow-outside-vault to override"),
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

    // Build the scanned index
    let build = ScannedIndex::build(
        &files,
        site_prefix,
        &ScanOptions {
            scan_body: true,
            bm25_tokenize: true,
            default_language,
            frontmatter_link_props: None,
        },
    )?;

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

    // Serialize vault_dir as a canonical string (fall back to raw display)
    let vault_dir_str = std::fs::canonicalize(dir)
        .unwrap_or_else(|_| dir.to_path_buf())
        .to_string_lossy()
        .into_owned();

    // Build the BM25 inverted index from tokenized entries (if any have tokens).
    let bm25_index = Bm25InvertedIndex::build_from_entries(build.index.entries());

    // iter-261 (BUG-5, BUG-6): record the vault's attachments alongside the
    // notes so an `--index` run resolves `![[img.png]]` and `[[Books.base]]`
    // exactly as a disk run does. A failed walk degrades to "no attachments",
    // which is the pre-iter-261 behaviour, not an error.
    let attachments = discovery::discover_attachments(dir).unwrap_or_default();

    // Save the snapshot (with the persisted BM25 index when available).
    SnapshotIndex::save_with_attachments(
        &build.index,
        &index_path,
        &vault_dir_str,
        site_prefix,
        bm25_index.as_ref(),
        &attachments,
    )?;

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
    let mut result = serde_json::json!({
        "path": index_path.display().to_string(),
        "files_indexed": file_count,
        "warnings": build.warnings.len(),
    });
    if replacing_existing {
        result["note"] = serde_json::json!("replaced existing index");
    }

    Ok(CommandOutcome::success(format_success(format, &result)))
}
