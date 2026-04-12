use hyalo_core::filter::{self, SortField};
use hyalo_core::index::IndexEntry;
use hyalo_core::link_graph::LinkGraph;
use hyalo_core::types::FileObject;

use super::build::extract_title;

/// Apply the requested sort order to the results.
pub(super) fn apply_sort(
    results: &mut [FileObject],
    sort: Option<&SortField>,
    link_graph: Option<&LinkGraph>,
) {
    match sort.unwrap_or(&SortField::File) {
        SortField::File => results.sort_by(|a, b| a.file.cmp(&b.file)),
        SortField::Modified => results.sort_by(|a, b| a.modified.cmp(&b.modified)),
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
                b_count.cmp(&a_count)
            });
        }
        SortField::LinksCount => {
            results.sort_by(|a, b| {
                let a_count = a.links.as_ref().map_or(0, Vec::len);
                let b_count = b.links.as_ref().map_or(0, Vec::len);
                b_count.cmp(&a_count)
            });
        }
        SortField::Title => {
            results.sort_by(|a, b| {
                let a_val = a.title.as_ref();
                let b_val = b.title.as_ref();
                filter::compare_property_values(a_val, b_val).then_with(|| a.file.cmp(&b.file))
            });
        }
        SortField::Property(key) => {
            results.sort_by(|a, b| {
                let a_val = a.properties.as_ref().and_then(|p| p.get(key));
                let b_val = b.properties.as_ref().and_then(|p| p.get(key));
                filter::compare_property_values(a_val, b_val).then_with(|| a.file.cmp(&b.file))
            });
        }
        SortField::Score => {
            results.sort_by(|a, b| {
                let a_score = a.score.unwrap_or(0.0);
                let b_score = b.score.unwrap_or(0.0);
                b_score
                    .partial_cmp(&a_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.file.cmp(&b.file))
            });
        }
    }
}

/// Pre-sort index entries by the requested sort key so that the early-exit
/// optimisation can collect the first N matches in final order.
///
/// This mirrors `apply_sort` but operates on `&IndexEntry` references
/// instead of `FileObject` values, avoiding construction of the full object.
pub(super) fn presort_index_entries(
    entries: &mut [&IndexEntry],
    sort: Option<&SortField>,
    link_graph: &LinkGraph,
) {
    match sort.unwrap_or(&SortField::File) {
        SortField::File => entries.sort_by(|a, b| a.rel_path.cmp(&b.rel_path)),
        SortField::Modified => entries.sort_by(|a, b| a.modified.cmp(&b.modified)),
        SortField::BacklinksCount => {
            // Descending by backlink count — matches apply_sort.
            entries.sort_by(|a, b| {
                let a_count = link_graph.backlinks(&a.rel_path).len();
                let b_count = link_graph.backlinks(&b.rel_path).len();
                b_count.cmp(&a_count)
            });
        }
        SortField::LinksCount => {
            entries.sort_by(|a, b| {
                let a_count = a.links.len();
                let b_count = b.links.len();
                b_count.cmp(&a_count)
            });
        }
        SortField::Title => {
            entries.sort_by(|a, b| {
                let a_val = extract_title(&a.properties, Some(&a.sections));
                let b_val = extract_title(&b.properties, Some(&b.sections));
                filter::compare_property_values(Some(&a_val), Some(&b_val))
                    .then_with(|| a.rel_path.cmp(&b.rel_path))
            });
        }
        SortField::Property(key) => {
            entries.sort_by(|a, b| {
                let a_val = a.properties.get(key.as_str());
                let b_val = b.properties.get(key.as_str());
                filter::compare_property_values(a_val, b_val)
                    .then_with(|| a.rel_path.cmp(&b.rel_path))
            });
        }
        // Score sorting is applied after BM25 scoring, not during pre-sort.
        SortField::Score => {}
    }
}
