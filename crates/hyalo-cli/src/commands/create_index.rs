#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result};
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
        let parent = index_path
            .parent()
            .context("output path has no parent directory")?;
        let canonical_parent = dunce::canonicalize(parent).with_context(|| {
            format!(
                "failed to canonicalize parent of output path: {}",
                parent.display()
            )
        })?;
        if !canonical_parent.starts_with(&canonical_dir) {
            let out = crate::output::format_error(
                format,
                "output path is outside the vault boundary",
                Some(&index_path.display().to_string()),
                Some("use --allow-outside-vault to override"),
                None,
            );
            return Ok(CommandOutcome::UserError(out));
        }
    }

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
        },
    )?;

    // Warn about skipped files
    for w in &build.warnings {
        crate::warn::warn(format!("skipped {}: {}", w.rel_path, w.message));
    }

    // Serialize vault_dir as a canonical string (fall back to raw display)
    let vault_dir_str = std::fs::canonicalize(dir)
        .unwrap_or_else(|_| dir.to_path_buf())
        .to_string_lossy()
        .into_owned();

    // Save the snapshot
    SnapshotIndex::save(&build.index, &index_path, &vault_dir_str, site_prefix)?;

    // Check for stale indexes in the same directory
    if let Ok(stale) = find_stale_indexes(dir) {
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
    let result = serde_json::json!({
        "path": index_path.display().to_string(),
        "files_indexed": file_count,
        "warnings": build.warnings.len(),
    });

    Ok(CommandOutcome::success(format_success(format, &result)))
}
