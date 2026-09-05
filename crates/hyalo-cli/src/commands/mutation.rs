//! Shared helpers for mutation commands (`set`, `remove`, `append`, `mv`).
//!
//! Since iter-226 (ARCH-3) index maintenance for mutating commands goes
//! through [`crate::commands::journal::MutationJournal`] — the per-command
//! entry/update/rename/rescan/flush helpers that used to live here were
//! folded into it. What remains in this module is output shaping shared by
//! the mutation commands.

use serde_json::Value;

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

/// One file a bulk mutation scanned but did not write, with why (UX-1,
/// iter-276).
///
/// `skipped` / `skipped_count` have always meant one specific thing — the
/// property or tag was already at its target value — while a file the run
/// *refused* (its frontmatter would not parse) did not appear in the envelope
/// at all. A caller reading `modified: []` could not tell "nothing needed
/// changing" from "nothing could be changed". `skipped_detail` lists both,
/// each with its reason, and is a superset of `skipped`.
#[derive(Debug, serde::Serialize)]
pub struct SkippedFile {
    /// Vault-relative path.
    pub file: String,
    /// Why the file was not written: `unchanged` (already at the requested
    /// value — the `skipped` set) or `unparsable` (the YAML frontmatter would
    /// not parse, so the file was refused; see `hyalo lint --rule HYALO005`).
    pub reason: &'static str,
}

/// Reason string for a file whose value already matched.
pub const SKIP_UNCHANGED: &str = "unchanged";
/// Reason string for a file whose frontmatter would not parse.
pub const SKIP_UNPARSABLE: &str = "unparsable";

/// Build the `skipped_detail` array from the two sets every bulk mutation
/// already tracks, in stable (unchanged-then-unparsable) order.
#[must_use]
pub fn skipped_detail(unchanged: &[String], unparsable: &[String]) -> Vec<SkippedFile> {
    unchanged
        .iter()
        .map(|f| SkippedFile {
            file: f.clone(),
            reason: SKIP_UNCHANGED,
        })
        .chain(unparsable.iter().map(|f| SkippedFile {
            file: f.clone(),
            reason: SKIP_UNPARSABLE,
        }))
        .collect()
}
