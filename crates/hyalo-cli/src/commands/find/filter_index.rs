use anyhow::{Context, Result};
use globset::{GlobBuilder, GlobSetBuilder};
use std::collections::HashSet;

use hyalo_core::filter::Fields;
use hyalo_core::index::IndexEntry;

/// Filter index entries by optional `--file` and `--glob` scoping arguments.
///
/// - When both are empty, all entries are returned.
/// - `files_arg` entries are matched by exact `rel_path` equality.
/// - `globs` patterns use the same globset semantics as `discovery::match_globs`
///   (positive patterns require a match; negative `!pat` patterns exclude matches).
pub fn filter_index_entries<'a>(
    entries: &'a [IndexEntry],
    files_arg: &[String],
    globs: &[String],
) -> Result<Vec<&'a IndexEntry>> {
    if files_arg.is_empty() && globs.is_empty() {
        return Ok(entries.iter().collect());
    }

    if !files_arg.is_empty() && globs.is_empty() {
        // Exact path matching — build a HashSet for O(1) lookup instead of O(n×m)
        let file_set: HashSet<&str> = files_arg.iter().map(String::as_str).collect();
        let filtered: Vec<&IndexEntry> = entries
            .iter()
            .filter(|e| file_set.contains(e.rel_path.as_str()))
            .collect();
        return Ok(filtered);
    }

    // Glob filtering — build positive/negative glob sets (same logic as discovery::match_globs)
    let normalized: Vec<String> = globs
        .iter()
        .map(|p| {
            if let Some(rest) = p.strip_prefix("\\!") {
                format!("!{rest}")
            } else {
                p.clone()
            }
        })
        .collect();

    let mut positive: Vec<&str> = Vec::new();
    let mut negative: Vec<&str> = Vec::new();
    for p in &normalized {
        if let Some(neg) = p.strip_prefix('!') {
            anyhow::ensure!(
                !neg.is_empty(),
                "negation glob pattern must not be empty (got '!')"
            );
            negative.push(neg);
        } else {
            positive.push(p.as_str());
        }
    }

    let positive_set = if positive.is_empty() {
        None
    } else {
        let mut builder = GlobSetBuilder::new();
        for pat in &positive {
            builder.add(
                GlobBuilder::new(pat)
                    .literal_separator(true)
                    .build()
                    .context("invalid glob pattern")?,
            );
        }
        Some(
            builder
                .build()
                .context("failed to build positive globset")?,
        )
    };

    let negative_set = if negative.is_empty() {
        None
    } else {
        let mut builder = GlobSetBuilder::new();
        for pat in &negative {
            builder.add(
                GlobBuilder::new(pat)
                    .literal_separator(true)
                    .build()
                    .context("invalid glob negation pattern")?,
            );
        }
        Some(
            builder
                .build()
                .context("failed to build negative globset")?,
        )
    };

    let filtered: Vec<&IndexEntry> = entries
        .iter()
        .filter(|e| {
            // When files_arg is non-empty, an entry also passes if it is listed
            // explicitly — this handles the case where both files_arg and globs
            // are present (OR semantics between the two inclusion criteria).
            let passes_files = !files_arg.is_empty() && files_arg.iter().any(|f| f == &e.rel_path);
            let passes_positive = positive_set
                .as_ref()
                .is_none_or(|gs| gs.is_match(&e.rel_path));
            let passes_negative = negative_set
                .as_ref()
                .is_none_or(|gs| !gs.is_match(&e.rel_path));
            // An entry is included if it matches files_arg OR (matches positive
            // globs AND is not excluded by a negative glob).
            passes_files || (passes_positive && passes_negative)
        })
        .collect();

    Ok(filtered)
}

/// Returns `true` when the command needs body content (sections, tasks, links,
/// title, content search, or structural filters).
///
/// This is the core body-scan predicate shared between the `find` command
/// dispatch in `main.rs` and any other caller that needs to decide whether to
/// request a full body scan from [`crate::commands::build_scanned_index`].
///
/// Callers may add extra conditions on top (e.g. `sort_needs_links`,
/// `broken_links`, `has_title_filter`) that are specific to their dispatch
/// context.
pub fn needs_body(
    fields: &Fields,
    has_content_search: bool,
    has_task_filter: bool,
    has_section_filter: bool,
) -> bool {
    fields.sections
        || fields.tasks
        || fields.links
        || fields.title
        || has_content_search
        || has_task_filter
        || has_section_filter
}
