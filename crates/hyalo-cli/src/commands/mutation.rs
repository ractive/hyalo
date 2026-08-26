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
