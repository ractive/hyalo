//! Hand-written text renderings for the OKF and MADR generators.
//!
//! Split out of the single 3,744-line `output.rs` in iteration 247
//! (deep-review hotspot). A file split only: every item keeps the visibility it
//! had in the one module, so `output::...` paths and behaviour are unchanged.

use super::*;

/// Format a generator command result (`okf index`, `okf log`, `madr toc`) as
/// readable text. Returns `None` for an unrecognized command so the caller
/// falls back to the generic renderer.
pub(super) fn format_generator_output_text(
    command: &str,
    map: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    match command {
        "okf index" => Some(format_okf_index_text(map)),
        "okf log" => Some(format_okf_log_text(map)),
        "madr toc" => Some(format_madr_toc_text(map)),
        _ => None,
    }
}

/// Render an `okf index` result: a header line plus one line per changed file
/// (`  <action> <path>[ (preserving N existing lines)]`).
pub(super) fn format_okf_index_text(map: &serde_json::Map<String, serde_json::Value>) -> String {
    let apply = map.get("apply").and_then(serde_json::Value::as_bool) == Some(true);
    let changed = map
        .get("changed")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let scanned = map
        .get("scanned")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let skipped = map
        .get("skipped_malformed")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);

    let skipped_markers = map
        .get("skipped_markers")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let write_failures = map
        .get("write_failures")
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len) as u64;

    // "written" reads correctly for both singular and plural ("1 file written",
    // "3 files written"), unlike "1 file wrote".
    let verb = if apply { "written" } else { "would change" };
    let mut out = String::new();
    let _ = write!(
        out,
        "okf index: {changed} {} {verb} ({scanned} scanned)",
        if changed == 1 { "file" } else { "files" }
    );
    if skipped > 0 {
        let _ = write!(out, ", {skipped} skipped (malformed frontmatter)");
    }
    if skipped_markers > 0 {
        let _ = write!(out, ", {skipped_markers} skipped (malformed markers)");
    }
    if write_failures > 0 {
        let _ = write!(out, ", {write_failures} write failure(s)");
    }
    append_generator_file_lines(&mut out, map.get("files"));
    out
}

/// Append one indented `  <action> <path>` line per entry in a generator's
/// `files` array (used by `okf index`).
pub(super) fn append_generator_file_lines(out: &mut String, files: Option<&serde_json::Value>) {
    let Some(arr) = files.and_then(serde_json::Value::as_array) else {
        return;
    };
    for f in arr {
        let Some(obj) = f.as_object() else { continue };
        let file = obj
            .get("file")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("?");
        let action = obj
            .get("action")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("change");
        let _ = write!(out, "\n  {action} {file}");
        if let Some(n) = obj
            .get("preserved_lines")
            .and_then(serde_json::Value::as_u64)
        {
            let _ = write!(
                out,
                " (preserving {n} existing {})",
                if n == 1 { "line" } else { "lines" }
            );
        }
        if let Some(reason) = obj.get("reason").and_then(serde_json::Value::as_str) {
            let _ = write!(out, " — {reason}");
        }
    }
}

/// Render an `okf log` result: a single line describing the written entry.
pub(super) fn format_okf_log_text(map: &serde_json::Map<String, serde_json::Value>) -> String {
    let apply = map.get("apply").and_then(serde_json::Value::as_bool) == Some(true);
    let file = map
        .get("file")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("log.md");
    let date = map
        .get("date")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let created = map.get("created").and_then(serde_json::Value::as_bool) == Some(true);
    let verb = if apply { "logged" } else { "would log" };
    let mut out = format!("okf log: {verb} entry under {date} in {file}");
    if created {
        out.push_str(" (created)");
    }
    if let Some(entry) = map.get("entry").and_then(serde_json::Value::as_str) {
        let _ = write!(out, "\n  {entry}");
    }
    out
}

/// Render a `madr toc` result: a single line describing the TOC action.
pub(super) fn format_madr_toc_text(map: &serde_json::Map<String, serde_json::Value>) -> String {
    let apply = map.get("apply").and_then(serde_json::Value::as_bool) == Some(true);
    let file = map
        .get("file")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("README.md");
    let action = map
        .get("action")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unchanged");
    let adrs = map
        .get("adrs")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);

    let mut out = if apply {
        format!("madr toc: {action} {file} ({adrs} ADRs)")
    } else if action == "unchanged" {
        format!("madr toc: {file} up to date ({adrs} ADRs)")
    } else {
        format!("madr toc: would {action} {file} ({adrs} ADRs)")
    };
    if let Some(n) = map
        .get("preserved_lines")
        .and_then(serde_json::Value::as_u64)
    {
        let _ = write!(
            out,
            " (preserving {n} existing {})",
            if n == 1 { "line" } else { "lines" }
        );
    }
    out
}
