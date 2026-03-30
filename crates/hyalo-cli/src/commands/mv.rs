#![allow(clippy::missing_errors_doc)]
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::output::{CommandOutcome, Format};
use hyalo_core::discovery;
use hyalo_core::index::{IndexEntry, SnapshotIndex, format_modified};
use hyalo_core::link_rewrite::{self, Replacement, RewritePlan};

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct MvResult {
    from: String,
    to: String,
    dry_run: bool,
    updated_files: Vec<UpdatedFile>,
    total_files_updated: usize,
    total_links_updated: usize,
}

#[derive(Serialize)]
struct UpdatedFile {
    file: String,
    replacements: Vec<Replacement>,
}

// ---------------------------------------------------------------------------
// Command entry point
// ---------------------------------------------------------------------------

/// Run `hyalo mv --file <old> --to <new> [--dry-run]`.
#[allow(clippy::too_many_arguments)]
pub fn mv(
    dir: &Path,
    file_arg: &str,
    to_arg: &str,
    dry_run: bool,
    format: Format,
    site_prefix: Option<&str>,
    snapshot_index: &mut Option<SnapshotIndex>,
    index_path: Option<&Path>,
) -> Result<CommandOutcome> {
    // 1. Validate source exists
    let (_src_full, old_rel) = match discovery::resolve_file(dir, file_arg) {
        Ok(r) => r,
        Err(e) => return Ok(crate::commands::resolve_error_to_outcome(e, format)),
    };

    // 2. Validate target path
    let new_rel = match validate_target(dir, to_arg, &old_rel, format) {
        Ok(rel) => rel,
        Err(outcome) => return Ok(outcome),
    };

    // 3. Plan all rewrites
    let plans = link_rewrite::plan_mv(dir, &old_rel, &new_rel, site_prefix)?;

    // 4. Build result
    let updated_files: Vec<UpdatedFile> = plans
        .iter()
        .map(|p| UpdatedFile {
            file: p.rel_path.clone(),
            replacements: p.replacements.clone(),
        })
        .collect();
    let total_links: usize = updated_files.iter().map(|f| f.replacements.len()).sum();

    let result = MvResult {
        from: old_rel.clone(),
        to: new_rel.clone(),
        dry_run,
        updated_files,
        total_files_updated: plans.len(),
        total_links_updated: total_links,
    };

    // 5. If not dry-run, execute the move and rewrites
    if !dry_run {
        execute_mv(dir, &old_rel, &new_rel, &plans)?;

        // Patch index: update rel_path for the moved file.
        // Note: the link graph and per-entry outbound links are NOT updated here —
        // backlink queries against the index may be stale after mv. This is a known
        // limitation; property/tag/task queries remain accurate.
        if let (Some(idx), Some(idx_path)) = (snapshot_index.as_mut(), index_path) {
            let new_full = dir.join(&new_rel);
            let old_entry_opt: Option<IndexEntry> = idx.get_mut(&old_rel).cloned();
            if let Some(old_entry) = old_entry_opt {
                idx.remove_entry(&old_rel);
                let mut new_entry = old_entry;
                new_entry.rel_path.clone_from(&new_rel);
                new_entry.modified = format_modified(&new_full)?;
                idx.insert_entry(new_entry);
            }
            idx.save_to(idx_path)?;
        }
    }

    // 6. Format output (always JSON internally; pipeline handles user-facing format)
    let _ = format;
    Ok(CommandOutcome::success(
        serde_json::to_string_pretty(&result).unwrap_or_default(),
    ))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Validate the `--to` argument and return a normalized vault-relative path.
/// Returns `Err(CommandOutcome)` for user-facing errors, `Ok(String)` on success.
fn validate_target(
    dir: &Path,
    to_arg: &str,
    src_rel: &str,
    format: Format,
) -> std::result::Result<String, CommandOutcome> {
    // Normalize forward slashes and strip leading "./" for consistent comparison
    let normalized = to_arg.replace('\\', "/");
    let normalized = normalized
        .strip_prefix("./")
        .unwrap_or(&normalized)
        .to_owned();

    // Must end with .md — intentionally case-sensitive because discover_files
    // only picks up lowercase .md extensions.
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    if !normalized.ends_with(".md") {
        let out = crate::output::format_error(
            format,
            "target path must end with .md",
            Some(&normalized),
            Some(&format!("did you mean {normalized}.md?")),
            None,
        );
        return Err(CommandOutcome::UserError(out));
    }

    // Reject path traversal (component-based, consistent with discovery::resolve_file)
    let has_traversal = std::path::Path::new(&normalized).components().any(|c| {
        matches!(
            c,
            std::path::Component::ParentDir | std::path::Component::RootDir
        )
    }) || std::path::Path::new(&normalized).is_absolute();
    if has_traversal {
        let out = crate::output::format_error(
            format,
            "target path must be relative and within the vault",
            Some(&normalized),
            None,
            None,
        );
        return Err(CommandOutcome::UserError(out));
    }

    // Source and destination must differ
    if normalized == src_rel {
        let out = crate::output::format_error(
            format,
            "source and destination are the same path",
            Some(&normalized),
            Some("choose a different destination path"),
            None,
        );
        return Err(CommandOutcome::UserError(out));
    }

    // Target must not already exist
    let target_path = dir.join(&normalized);
    if target_path.exists() {
        let out = crate::output::format_error(
            format,
            "target file already exists",
            Some(&normalized),
            None,
            None,
        );
        return Err(CommandOutcome::UserError(out));
    }

    Ok(normalized)
}

/// Execute the file move and apply all rewrite plans.
fn execute_mv(dir: &Path, old_rel: &str, new_rel: &str, plans: &[RewritePlan]) -> Result<()> {
    let src = dir.join(old_rel);
    let dst = dir.join(new_rel);

    // Create destination directory if needed
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    // Move the file
    fs::rename(&src, &dst)
        .with_context(|| format!("failed to move {} to {}", src.display(), dst.display()))?;

    // Apply link rewrites
    link_rewrite::execute_plans(dir, plans)?;

    Ok(())
}
