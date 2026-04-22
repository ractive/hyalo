#![allow(clippy::missing_errors_doc)]
use std::path::Path;

use anyhow::{Context, Result};

use crate::output::{CommandOutcome, Format};
use hyalo_core::case_index::CaseInsensitiveIndex;
use hyalo_core::discovery;
use hyalo_core::index::VaultIndex;
use hyalo_core::link_fix::{LinkMatcher, apply_fixes, detect_broken_links_from_index, plan_fixes};

// ---------------------------------------------------------------------------
// Command entry points
// ---------------------------------------------------------------------------

/// Run `hyalo links fix` using a pre-built index.
///
/// `dry_run = true`  → preview only (default)
/// `dry_run = false` → write fixes to disk (`--apply`)
///
/// Case-mismatch fixes (rule `link-case-mismatch`) are included alongside
/// ordinary broken-link fixes when `case_index` is provided.
///
/// Returns `(CommandOutcome, modified_files)` where `modified_files` contains
/// vault-relative paths of files that were rewritten on disk.  The caller is
/// responsible for patching the snapshot index with these paths.
#[allow(clippy::too_many_arguments)]
pub fn links_fix(
    index: &dyn VaultIndex,
    dir: &Path,
    site_prefix: Option<&str>,
    globs: &[String],
    dry_run: bool,
    threshold: f64,
    ignore_target: &[String],
    format: Format,
    case_index: Option<&CaseInsensitiveIndex>,
) -> Result<(CommandOutcome, Vec<String>)> {
    let report = detect_broken_links_from_index(dir, index, site_prefix, case_index);

    // Filter broken links by glob, if any were provided.
    let broken = if globs.is_empty() {
        report.broken
    } else {
        let all_files: Vec<std::path::PathBuf> = index
            .entries()
            .iter()
            .map(|e| dir.join(&e.rel_path))
            .collect();
        let matched = discovery::match_globs(dir, &all_files, globs)?;
        crate::warn::warn_glob_dir_overlap(dir, globs, matched.len());
        let matched_set: std::collections::HashSet<&str> =
            matched.iter().map(|(_, rel)| rel.as_str()).collect();
        report
            .broken
            .into_iter()
            .filter(|b| matched_set.contains(b.source.as_str()))
            .collect()
    };

    // Filter out ignored targets (--ignore-target substrings).
    let (broken, ignored_count) = if ignore_target.is_empty() {
        (broken, 0usize)
    } else {
        let before = broken.len();
        let filtered: Vec<_> = broken
            .into_iter()
            .filter(|b| {
                !ignore_target
                    .iter()
                    .any(|pat| b.target.contains(pat.as_str()))
            })
            .collect();
        let ignored = before - filtered.len();
        (filtered, ignored)
    };

    let matcher = LinkMatcher::from_index(index, threshold);
    let fix_report = plan_fixes(&broken, &matcher);

    // Collect all fixes: broken-link fixes + case-mismatch fixes.
    // Case-mismatch fixes come from the detection phase (not from plan_fixes).
    let case_mismatches = report.case_mismatches;
    let case_mismatch_count = case_mismatches.len();

    let mut modified_files = Vec::new();

    if !dry_run {
        // Merge broken-link fixes and case-mismatch fixes into a single batch
        // so `apply_fixes` reads and rewrites each source file once — two
        // separate passes over the same file would see the first pass's
        // rewrites and could misbehave on overlapping edits.
        let mut all_fixes = fix_report.fixes.clone();
        all_fixes.extend(case_mismatches.iter().cloned());
        if !all_fixes.is_empty() {
            apply_fixes(dir, &all_fixes, site_prefix)?;

            // Collect unique modified files for the caller to patch the index.
            let deduped: std::collections::HashSet<&str> =
                all_fixes.iter().map(|f| f.source.as_str()).collect();
            modified_files = deduped.into_iter().map(str::to_owned).collect();
        }
    }

    let output = serde_json::json!({
        "broken": broken.len(),
        "fixable": fix_report.fixes.len(),
        "unfixable": fix_report.unfixable.len(),
        "ignored": ignored_count,
        "fixes": fix_report.fixes,
        "unfixable_links": fix_report.unfixable,
        "applied": !dry_run,
        "case_mismatches": case_mismatch_count,
        "case_mismatch_fixes": case_mismatches,
    });

    let _ = format;
    Ok((
        CommandOutcome::success(
            serde_json::to_string_pretty(&output).context("failed to serialize")?,
        ),
        modified_files,
    ))
}

/// Run `hyalo links auto` using a pre-built index.
///
/// `apply = false` → preview only (default)
/// `apply = true`  → write `[[wikilinks]]` to disk
///
/// Returns `(CommandOutcome, modified_files)` where `modified_files` contains
/// vault-relative paths of files that were rewritten on disk.  The caller is
/// responsible for patching the snapshot index with these paths.
#[allow(clippy::too_many_arguments)]
pub fn links_auto(
    index: &dyn VaultIndex,
    dir: &Path,
    apply: bool,
    min_length: usize,
    exclude_titles: &[String],
    first_only: bool,
    exclude_target_globs: &[String],
    file_filter: Option<&str>,
    glob_filter: &[String],
    format: Format,
) -> Result<(CommandOutcome, Vec<String>)> {
    let opts = hyalo_core::auto_link::AutoLinkOptions {
        apply,
        min_length,
        exclude_titles,
        first_only,
        exclude_target_globs,
        file_filter,
        glob_filter,
    };
    let report = hyalo_core::auto_link::auto_link(index, dir, &opts)?;

    // Collect unique modified files for the caller to patch the index.
    let modified_files: Vec<String> = if report.applied {
        let deduped: std::collections::HashSet<&str> =
            report.matches.iter().map(|m| m.file.as_str()).collect();
        deduped.into_iter().map(str::to_owned).collect()
    } else {
        Vec::new()
    };

    let output = serde_json::json!({
        "scanned": report.scanned,
        "total": report.total,
        "matches": report.matches,
        "ambiguous_titles": report.ambiguous_titles,
        "applied": report.applied,
    });

    let _ = format;
    Ok((
        CommandOutcome::success(
            serde_json::to_string_pretty(&output).context("failed to serialize")?,
        ),
        modified_files,
    ))
}

// ---------------------------------------------------------------------------
