use std::cmp::Ordering;

use hyalo_core::filter::{self, SortField};
use hyalo_core::index::IndexEntry;
use hyalo_core::link_graph::LinkGraph;
use hyalo_core::types::FileObject;

use super::build::extract_title;

/// Apply `--reverse` to a primary-key comparison.
///
/// Only the primary key flips: the `file` tiebreak stays ascending so a
/// reversed run is still deterministic and reads in path order within a tie
/// (iter-264, DEC-273).
fn dir(ordering: Ordering, reverse: bool) -> Ordering {
    if reverse {
        ordering.reverse()
    } else {
        ordering
    }
}

/// Compare two optional property values with missing/null pinned last in both
/// directions (iter-264, DEC-274).
///
/// `--reverse` answers "which end of the *values* do I want first"; a file that
/// has no value at all has no place at either end, so it always trails.
fn compare_nulls_last(
    a: Option<&serde_json::Value>,
    b: Option<&serde_json::Value>,
    reverse: bool,
) -> Ordering {
    let a_missing = a.is_none_or(serde_json::Value::is_null);
    let b_missing = b.is_none_or(serde_json::Value::is_null);
    match (a_missing, b_missing) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => dir(filter::compare_property_values(a, b), reverse),
    }
}

/// Apply the requested sort order to the results.
///
/// Every key orders **ascending** and `--reverse` inverts it (iter-264,
/// DEC-273). `score` is the single exception: it ranks best-match-first, so its
/// unreversed order is descending relevance and `--reverse score` puts the
/// weakest match first.
pub(super) fn apply_sort(
    results: &mut [FileObject],
    sort: Option<&SortField>,
    link_graph: Option<&LinkGraph>,
    reverse: bool,
) {
    match sort.unwrap_or(&SortField::File) {
        SortField::File => results.sort_by(|a, b| dir(a.file.cmp(&b.file), reverse)),
        SortField::Modified => results.sort_by(|a, b| {
            dir(a.modified.cmp(&b.modified), reverse).then_with(|| a.file.cmp(&b.file))
        }),
        SortField::BacklinksCount => {
            results.sort_by(|a, b| {
                let a_count = a.backlinks.as_ref().map_or_else(
                    || link_graph.map_or(0, |g| g.backlinks(&a.file).len()),
                    Vec::len,
                );
                let b_count = b.backlinks.as_ref().map_or_else(
                    || link_graph.map_or(0, |g| g.backlinks(&b.file).len()),
                    Vec::len,
                );
                dir(a_count.cmp(&b_count), reverse).then_with(|| a.file.cmp(&b.file))
            });
        }
        SortField::LinksCount => {
            results.sort_by(|a, b| {
                let a_count = a.links.as_ref().map_or(0, Vec::len);
                let b_count = b.links.as_ref().map_or(0, Vec::len);
                dir(a_count.cmp(&b_count), reverse).then_with(|| a.file.cmp(&b.file))
            });
        }
        SortField::Title => {
            results.sort_by(|a, b| {
                compare_nulls_last(a.title.as_ref(), b.title.as_ref(), reverse)
                    .then_with(|| a.file.cmp(&b.file))
            });
        }
        SortField::Property(key) => {
            results.sort_by(|a, b| {
                let a_val = a.properties.as_ref().and_then(|p| p.get(key));
                let b_val = b.properties.as_ref().and_then(|p| p.get(key));
                compare_nulls_last(a_val, b_val, reverse).then_with(|| a.file.cmp(&b.file))
            });
        }
        SortField::Score => {
            results.sort_by(|a, b| {
                let a_score = a.score.unwrap_or(0.0);
                let b_score = b.score.unwrap_or(0.0);
                // Best match first: descending relevance is `score`'s
                // unreversed order (DEC-273).
                let cmp = b_score.partial_cmp(&a_score).unwrap_or(Ordering::Equal);
                dir(cmp, reverse).then_with(|| a.file.cmp(&b.file))
            });
        }
    }
}

/// Pre-sort index entries by the requested sort key so that the early-exit
/// optimisation can collect the first N matches in final order.
///
/// This mirrors `apply_sort` but operates on `&IndexEntry` references
/// instead of `FileObject` values, avoiding construction of the full object.
/// Only reached when `--reverse` is off (see `presorted` in the caller), so it
/// needs no direction parameter.
pub(super) fn presort_index_entries(
    entries: &mut [&IndexEntry],
    sort: Option<&SortField>,
    link_graph: &LinkGraph,
) {
    match sort.unwrap_or(&SortField::File) {
        SortField::File => entries.sort_by(|a, b| a.rel_path.cmp(&b.rel_path)),
        SortField::Modified => entries.sort_by(|a, b| {
            a.modified
                .cmp(&b.modified)
                .then_with(|| a.rel_path.cmp(&b.rel_path))
        }),
        SortField::BacklinksCount => {
            // Ascending by backlink count — matches apply_sort (DEC-273).
            entries.sort_by(|a, b| {
                let a_count = link_graph.backlinks(&a.rel_path).len();
                let b_count = link_graph.backlinks(&b.rel_path).len();
                a_count
                    .cmp(&b_count)
                    .then_with(|| a.rel_path.cmp(&b.rel_path))
            });
        }
        SortField::LinksCount => {
            entries.sort_by(|a, b| {
                let a_count = a.links.len();
                let b_count = b.links.len();
                a_count
                    .cmp(&b_count)
                    .then_with(|| a.rel_path.cmp(&b.rel_path))
            });
        }
        SortField::Title => {
            entries.sort_by(|a, b| {
                let a_val = extract_title(&a.properties, Some(&a.sections));
                let b_val = extract_title(&b.properties, Some(&b.sections));
                compare_nulls_last(Some(&a_val), Some(&b_val), false)
                    .then_with(|| a.rel_path.cmp(&b.rel_path))
            });
        }
        SortField::Property(key) => {
            entries.sort_by(|a, b| {
                let a_val = a.properties.get(key.as_str());
                let b_val = b.properties.get(key.as_str());
                compare_nulls_last(a_val, b_val, false).then_with(|| a.rel_path.cmp(&b.rel_path))
            });
        }
        // Score sorting is applied after BM25 scoring, not during pre-sort.
        SortField::Score => {}
    }
}
