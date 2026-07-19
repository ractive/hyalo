//! Exact-heading anchor matching for link fragments (L-21, iter-190).
//!
//! A link like `[[Foo#Tasks]]` or `[t](foo.md#Tasks)` carries a `#fragment`
//! naming a heading in the target file. This module validates that fragment
//! against the target file's indexed headings so `find --broken-links` can
//! report a *broken anchor* — a link whose target file exists but whose
//! `#heading` does not.
//!
//! This is deliberately **NOT** [`crate::heading::SectionFilter`] (the
//! `--section` matcher): that one is a substring, case-insensitive-by-default,
//! optionally-regex *selector* for slicing a document. Anchor validation needs
//! an *exact* heading-existence check with a fixed, documented convention, so
//! it lives in its own type.
//!
//! ## DEC-060 — anchor-match convention
//!
//! A fragment matches a heading iff the **trimmed** heading text equals the
//! **percent-decoded, trimmed** fragment under a **case-insensitive** ASCII
//! comparison. This mirrors Obsidian, which resolves `[[Foo#tasks]]` against a
//! `## Tasks` heading regardless of case. Markdown fragments may be
//! percent-encoded (`foo.md#my%20heading`); the encoded form is preserved in
//! the written link (the rewrite span never covers the fragment) and decoded
//! only for matching here.
//!
//! `^block-id` fragments (fragment starting with `^`) are Obsidian block
//! references. hyalo does not index block ids, so these are **skipped** — never
//! reported broken.

use crate::types::OutlineSection;

/// Return `true` when a fragment is an Obsidian block reference (`^block-id`)
/// and must therefore be skipped from anchor validation.
#[must_use]
pub fn is_block_ref(fragment: &str) -> bool {
    fragment.starts_with('^')
}

/// Normalize a fragment or heading for comparison: percent-decode (if it
/// contains escapes), then trim surrounding ASCII/Unicode whitespace.
///
/// Percent-decoding only kicks in when the input actually contains a valid
/// escape sequence; a literal `#100%done` heading is compared verbatim.
fn normalize_for_match(s: &str) -> String {
    let decoded = crate::discovery::percent_decode_path(s);
    match decoded {
        Some(d) => d.trim().to_string(),
        None => s.trim().to_string(),
    }
}

/// Validate a link fragment against a target file's outline sections.
///
/// Returns `true` when the fragment matches one of the headings under the
/// DEC-060 convention, or when the fragment is a `^block-id` (always treated as
/// valid — see module docs). Returns `false` only when the fragment names a
/// heading that does not exist in `sections`.
///
/// `sections` is the target file's [`OutlineSection`] list as already stored in
/// the index (`IndexEntry.sections`) — no file read is required on the index
/// path. Sections with `heading: None` (pre-heading outline entries) never
/// match a non-empty fragment.
#[must_use]
pub fn fragment_matches_headings(fragment: &str, sections: &[OutlineSection]) -> bool {
    // Block references are not validated — we do not index block ids.
    if is_block_ref(fragment) {
        return true;
    }
    let needle = normalize_for_match(fragment);
    if needle.is_empty() {
        // An empty (or whitespace-only) fragment is not a real anchor; treat as
        // matching so it is never reported broken.
        return true;
    }
    sections.iter().any(|s| {
        s.heading
            .as_deref()
            .is_some_and(|h| h.trim().eq_ignore_ascii_case(&needle))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sec(heading: Option<&str>) -> OutlineSection {
        OutlineSection {
            level: 2,
            heading: heading.map(str::to_string),
            line: 1,
            links: Vec::new(),
            tasks: None,
            code_blocks: Vec::new(),
        }
    }

    #[test]
    fn exact_match() {
        let secs = [sec(Some("Tasks"))];
        assert!(fragment_matches_headings("Tasks", &secs));
    }

    #[test]
    fn case_insensitive_match() {
        // Obsidian resolves [[Foo#tasks]] against `## Tasks`.
        let secs = [sec(Some("Tasks"))];
        assert!(fragment_matches_headings("tasks", &secs));
        assert!(fragment_matches_headings("TASKS", &secs));
    }

    #[test]
    fn trim_heading_and_fragment() {
        let secs = [sec(Some("  Tasks  "))];
        assert!(fragment_matches_headings("Tasks", &secs));
        let secs2 = [sec(Some("Tasks"))];
        assert!(fragment_matches_headings("  Tasks  ", &secs2));
    }

    #[test]
    fn multiple_headings_one_matches() {
        let secs = [sec(Some("Intro")), sec(Some("Tasks")), sec(Some("Done"))];
        assert!(fragment_matches_headings("Tasks", &secs));
    }

    #[test]
    fn no_match_reports_false() {
        let secs = [sec(Some("Tasks"))];
        assert!(!fragment_matches_headings("Nope", &secs));
    }

    #[test]
    fn heading_none_never_matches() {
        // Pre-heading outline entries carry heading: None and must not match a
        // non-empty fragment.
        let secs = [sec(None)];
        assert!(!fragment_matches_headings("anything", &secs));
    }

    #[test]
    fn unicode_heading() {
        let secs = [sec(Some("Überschrift"))];
        assert!(fragment_matches_headings("Überschrift", &secs));
        // ASCII-case-insensitive: a non-ASCII case fold is NOT expected to match.
        assert!(!fragment_matches_headings("überschrift", &secs));
    }

    #[test]
    fn percent_encoded_fragment() {
        // `foo.md#my%20heading` → decode to "my heading".
        let secs = [sec(Some("my heading"))];
        assert!(fragment_matches_headings("my%20heading", &secs));
    }

    #[test]
    fn percent_encoded_case_insensitive() {
        let secs = [sec(Some("My Heading"))];
        assert!(fragment_matches_headings("my%20heading", &secs));
    }

    #[test]
    fn block_ref_always_valid() {
        // `^block-id` refs are skipped: never reported broken even with no
        // matching heading.
        let secs = [sec(Some("Tasks"))];
        assert!(fragment_matches_headings("^my-block", &secs));
        // Even against an empty section list.
        assert!(fragment_matches_headings("^my-block", &[]));
    }

    #[test]
    fn empty_fragment_is_valid() {
        assert!(fragment_matches_headings("", &[]));
        assert!(fragment_matches_headings("   ", &[]));
    }

    #[test]
    fn literal_percent_not_decoded() {
        // A heading with a literal stray `%` (no valid escape) is compared
        // verbatim.
        let secs = [sec(Some("100%done"))];
        assert!(fragment_matches_headings("100%done", &secs));
    }
}
