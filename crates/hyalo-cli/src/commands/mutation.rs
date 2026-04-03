//! Shared helpers for mutation commands (`set`, `remove`, `append`, `mv`).
//!
//! These are pure structural helpers that encapsulate the patterns mutation
//! commands share:
//!
//! 1. Patching a snapshot-index entry in memory after a file is mutated.
//! 2. Renaming an index entry after a file move (key change + link graph update).
//! 3. Flushing the index to disk when it has been dirtied.
//! 4. Collapsing a single-element `results` vec to a bare JSON object (vs. an array).

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
// Index-entry rename (for `mv`)
// ---------------------------------------------------------------------------

/// Rename an index entry after a file move, updating the link graph and
/// re-scanning affected files so that backlink/link queries remain accurate.
///
/// `rewritten_files` lists the vault-relative paths of files whose links were
/// rewritten by the move — their index entries are re-scanned to pick up
/// updated outbound links.
///
/// This is a no-op when `snapshot_index` is `None`.
pub fn rename_index_entry(
    snapshot_index: &mut Option<SnapshotIndex>,
    dir: &Path,
    old_rel: &str,
    new_rel: &str,
    rewritten_files: &[&str],
    index_dirty: &mut bool,
) -> Result<()> {
    let Some(idx) = snapshot_index.as_mut() else {
        return Ok(());
    };

    // 1. Move the entry: remove old key, re-scan the moved file, insert under new key.
    //    Uses rename_entry to rebuild the path index only once (not twice).
    //    If old_rel was not in the index, there's nothing to do.
    if !idx.rename_entry(dir, old_rel, new_rel)? {
        return Ok(());
    }

    // 2. Re-scan each file that had links rewritten (their outbound links changed).
    //    The moved file itself may appear in `rewritten_files` (plan_mv sets
    //    `rel_path = new_rel` for the outbound plan) — skip it since step 1
    //    already re-scanned it at the new path.
    //    Errors are best-effort: skip files that fail to refresh.
    for &rel in rewritten_files {
        if rel == new_rel {
            continue;
        }
        let _ = idx.refresh_entry(dir, rel);
    }

    // 3. Update the link graph: rename target keys and source paths.
    //    Link targets don't change during a move — only the source path and the
    //    link syntax do — so renaming keys + sources is sufficient.
    idx.graph_mut().rename_path(old_rel, new_rel);

    *index_dirty = true;
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
