//! Small shared helpers used by more than one hint generator.
//!
//! Split out of the single 5,059-line `hints.rs` in iteration 247 (deep-review
//! hotspot). This is a file split only: the items keep the visibility they had
//! inside the one module, so `hints::...` paths and behaviour are unchanged.

// ---------------------------------------------------------------------------
// Status priority helpers
// ---------------------------------------------------------------------------

/// Priority rank for a status value: lower = more interesting.
pub(super) fn status_priority(value: &str) -> u8 {
    if value.eq_ignore_ascii_case("in-progress")
        || value.eq_ignore_ascii_case("in progress")
        || value.eq_ignore_ascii_case("active")
    {
        0
    } else if value.eq_ignore_ascii_case("planned") || value.eq_ignore_ascii_case("todo") {
        1
    } else if value.eq_ignore_ascii_case("draft") || value.eq_ignore_ascii_case("idea") {
        2
    } else if value.eq_ignore_ascii_case("completed")
        || value.eq_ignore_ascii_case("done")
        || value.eq_ignore_ascii_case("archived")
    {
        4
    } else {
        3
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Extract the first modified file path from mutation output (single object or array).
pub(super) fn first_modified_file(data: &serde_json::Value) -> Option<&str> {
    fn extract(obj: &serde_json::Value) -> Option<&str> {
        obj.get("modified")
            .and_then(|m| m.as_array())
            .and_then(|a| a.first())
            .and_then(|f| f.as_str())
    }
    if let Some(arr) = data.as_array() {
        arr.iter().find_map(extract)
    } else {
        extract(data)
    }
}
