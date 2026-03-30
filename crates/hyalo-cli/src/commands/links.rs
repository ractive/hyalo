#![allow(clippy::missing_errors_doc)]
use std::path::Path;

use anyhow::Result;

use crate::output::{CommandOutcome, Format};
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
) -> Result<CommandOutcome> {
    let report = detect_broken_links_from_index(dir, index, site_prefix);

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

    if !dry_run {
        apply_fixes(dir, &fix_report.fixes, site_prefix)?;
    }

    let output = serde_json::json!({
        "broken": broken.len(),
        "fixable": fix_report.fixes.len(),
        "unfixable": fix_report.unfixable.len(),
        "ignored": ignored_count,
        "fixes": fix_report.fixes,
        "unfixable_links": fix_report.unfixable,
        "applied": !dry_run,
    });

    let _ = format;
    Ok(CommandOutcome::success(
        serde_json::to_string_pretty(&output).unwrap_or_default(),
    ))
}

// ---------------------------------------------------------------------------
