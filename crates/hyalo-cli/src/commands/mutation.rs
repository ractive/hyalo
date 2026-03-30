//! Shared helpers for mutation commands (`set`, `remove`, `append`).
//!
//! These are pure structural helpers that encapsulate the two patterns all three
//! commands duplicate identically:
//!
//! 1. Patching a snapshot-index entry in memory after a file is mutated.
//! 2. Flushing the index to disk when it has been dirtied.
//! 3. Collapsing a single-element `results` vec to a bare JSON object (vs. an array).

use anyhow::Result;
use indexmap::IndexMap;
use serde_json::Value;
use std::path::Path;

use hyalo_core::filter::extract_tags;
use hyalo_core::index::{SnapshotIndex, format_modified};

// ---------------------------------------------------------------------------
// Index-entry patch
// ---------------------------------------------------------------------------

/// Patch the snapshot-index entry for `rel_path` after its frontmatter was mutated.
///
/// Extracts the new tag set from the already-updated `props`, writes
/// `properties`, `tags`, and `modified` back into the in-memory entry, then
/// marks `index_dirty` so the caller knows a save is needed.
///
/// This is a no-op when `snapshot_index` is `None` or when the entry for
/// `rel_path` is not present in the index (e.g. a newly created file that
/// hasn't been indexed yet).
pub fn update_index_entry(
    snapshot_index: &mut Option<SnapshotIndex>,
    rel_path: &str,
    props: IndexMap<String, Value>,
    full_path: &Path,
    index_dirty: &mut bool,
) -> Result<()> {
    if let Some(idx) = snapshot_index.as_mut()
        && let Some(entry) = idx.get_mut(rel_path)
    {
        let new_tags = extract_tags(&props);
        entry.properties = props;
        entry.tags = new_tags;
        entry.modified = format_modified(full_path)?;
        *index_dirty = true;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Index flush
// ---------------------------------------------------------------------------

/// Flush the snapshot index to `index_path` when `index_dirty` is `true`.
///
/// This is a no-op when `index_dirty` is `false`, or when either `snapshot_index`
/// or `index_path` is `None`.
pub fn save_index_if_dirty(
    snapshot_index: &mut Option<SnapshotIndex>,
    index_path: Option<&Path>,
    index_dirty: bool,
) -> Result<()> {
    if index_dirty && let (Some(idx), Some(idx_path)) = (snapshot_index.as_mut(), index_path) {
        idx.save_to(idx_path)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Output shaping
// ---------------------------------------------------------------------------

/// Collapse a `results` vec to a bare JSON object when it contains exactly one
/// entry, or return it as a JSON array otherwise.
///
/// All three mutation commands (`set`, `remove`, `append`) use this pattern:
/// a single mutation produces a plain object; multiple mutations produce an array.
#[must_use]
pub fn unwrap_single_result(mut results: Vec<Value>) -> Value {
    if results.len() == 1 {
        results.pop().unwrap_or_default()
    } else {
        serde_json::json!(results)
    }
}
