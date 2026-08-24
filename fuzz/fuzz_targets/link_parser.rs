//! Fuzz target: `[[wikilink]]` / `[label](target)` extraction
//! (`crates/hyalo-core/src/links.rs`) — the bracket-matching, zone-overlap,
//! and label-extraction logic that runs on every scanned body line.
//!
//! Feeds arbitrary text straight to `extract_links_from_text` (bypassing the
//! scanner, which is covered separately) and then exercises the zone-overlap
//! helpers every caller uses alongside it, since a malformed/unbalanced zone
//! list is exactly the kind of input this logic has to stay correct under
//! (see iter-217's zone-scanner bracket-balance fixes).

#![no_main]

use hyalo_core::links::{Link, extract_links_from_text, inert_link_zones, overlaps_zone};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| {
    if text.len() > 64 * 1024 {
        return;
    }

    let mut out: Vec<Link> = Vec::new();
    extract_links_from_text(text, &mut out);

    let zones = inert_link_zones(text);
    for &(start, end) in &zones {
        let _ = overlaps_zone(&zones, start, end);
    }
});
