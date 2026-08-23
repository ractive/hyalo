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
//! ## DEC-072 — GitHub slug forms are accepted too (iter-211, BUG-8)
//!
//! DEC-060 alone made hyalo unusable on any corpus written for a static-site
//! renderer. Every markdown renderer in wide use (GitHub, GitLab, MDN,
//! Docusaurus, mdBook) turns `### Sub Section` into the anchor
//! `#sub-section`, so authors write `[c](t.md#sub-section)` — and hyalo
//! reported all of them broken while *accepting* the raw-text spelling
//! `#Sub Section` that no renderer ever emits. On the GitHub Docs corpus 6 of
//! 7 checkable anchors were false positives.
//!
//! A fragment therefore matches when **either** convention holds:
//!
//! 1. DEC-060 raw-text equality (unchanged — Obsidian compatibility), or
//! 2. [`github_slug`] of the fragment equals the GitHub slug of the heading,
//!    where repeated heading slugs get the renderer's `-1`, `-2`, … dedupe
//!    suffixes in document order.
//!
//! Slugifying *both* sides makes the check idempotent: an already-slugged
//! fragment (`#sub-section`) slugs to itself, and a raw-text fragment
//! (`#Sub Section`) slugs to the same value the heading does. The union is
//! deliberately permissive — this check exists to catch dead anchors, and a
//! false positive costs a user far more than a missed exotic spelling.
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

/// Convert heading text into the anchor slug a GitHub-style markdown renderer
/// would generate for it (iter-211, BUG-8).
///
/// The algorithm every mainstream renderer converges on:
///
/// 1. Trim, then lowercase (Unicode-aware, so `Ü` → `ü`).
/// 2. Drop every character that is not alphanumeric, `-`, `_`, or whitespace —
///    this strips `.,:;!?()[]{}"'*` and backticks, plus emoji.
/// 3. Replace each remaining whitespace character with `-`.
///
/// Consecutive spaces deliberately produce consecutive hyphens (`a  b` →
/// `a--b`), matching GitHub rather than collapsing them.
///
/// Duplicate slugs within one document are *not* handled here — that is a
/// document-level concern, applied by [`heading_slugs`].
#[must_use]
pub fn github_slug(heading: &str) -> String {
    let trimmed = heading.trim();
    let mut out = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if ch.is_alphanumeric() || ch == '-' || ch == '_' {
            out.extend(ch.to_lowercase());
        } else if ch.is_whitespace() {
            out.push('-');
        }
        // Everything else (punctuation, emoji, markdown markers) is dropped.
    }
    out
}

/// The GitHub anchor slug for each heading in `sections`, in document order,
/// with the renderer's duplicate-suffix rule applied.
///
/// A slug seen for the *n*-th time (n > 1) gets `-{n-1}` appended, so three
/// `## Notes` headings yield `notes`, `notes-1`, `notes-2`. Sections with
/// `heading: None` contribute nothing.
fn heading_slugs(sections: &[OutlineSection]) -> Vec<String> {
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut slugs = Vec::with_capacity(sections.len());
    for section in sections {
        let Some(heading) = section.heading.as_deref() else {
            continue;
        };
        let base = github_slug(heading);
        if base.is_empty() {
            continue;
        }
        let count = seen.entry(base.clone()).or_insert(0);
        let slug = if *count == 0 {
            base.clone()
        } else {
            format!("{base}-{count}")
        };
        *count += 1;
        slugs.push(slug);
    }
    slugs
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
    // DEC-060: raw heading text, case-insensitive.
    if sections.iter().any(|s| {
        s.heading
            .as_deref()
            .is_some_and(|h| h.trim().eq_ignore_ascii_case(&needle))
    }) {
        return true;
    }
    // DEC-072: GitHub-rendered slug, with duplicate suffixes.
    let needle_slug = github_slug(&needle);
    if needle_slug.is_empty() {
        return false;
    }
    heading_slugs(sections).iter().any(|s| *s == needle_slug)
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
        // DEC-060's raw-text comparison is ASCII-case-insensitive only, but the
        // DEC-072 slug path lowercases Unicode the way a renderer does, so the
        // lowercased spelling now matches (iter-211, BUG-8).
        assert!(fragment_matches_headings("überschrift", &secs));
    }

    // --- DEC-072: GitHub slug forms (iter-211, BUG-8) ---

    #[test]
    fn github_slug_lowercases_and_hyphenates() {
        assert_eq!(github_slug("Sub Section"), "sub-section");
        assert_eq!(github_slug("  Trimmed  "), "trimmed");
        assert_eq!(github_slug("Already-Hyphenated"), "already-hyphenated");
        assert_eq!(github_slug("snake_case Name"), "snake_case-name");
    }

    #[test]
    fn github_slug_strips_punctuation_and_keeps_double_hyphens() {
        assert_eq!(github_slug("What's New?"), "whats-new");
        assert_eq!(github_slug("`code` blocks (v2)"), "code-blocks-v2");
        // GitHub does NOT collapse runs of whitespace.
        assert_eq!(github_slug("a  b"), "a--b");
    }

    #[test]
    fn github_slug_lowercases_unicode() {
        assert_eq!(github_slug("Überschrift"), "überschrift");
    }

    #[test]
    fn slug_fragment_matches_raw_heading() {
        // The BUG-8 headline case: `#sub-section` against `### Sub Section`.
        let secs = [sec(Some("Sub Section"))];
        assert!(fragment_matches_headings("sub-section", &secs));
    }

    #[test]
    fn slug_fragment_still_rejects_a_dead_anchor() {
        let secs = [sec(Some("Sub Section"))];
        assert!(!fragment_matches_headings("nope", &secs));
        assert!(!fragment_matches_headings("sub-sections", &secs));
    }

    #[test]
    fn slug_fragment_matches_heading_with_punctuation() {
        let secs = [sec(Some("What's New?"))];
        assert!(fragment_matches_headings("whats-new", &secs));
    }

    #[test]
    fn duplicate_headings_get_renderer_dedupe_suffixes() {
        let secs = [sec(Some("Notes")), sec(Some("Notes")), sec(Some("Notes"))];
        assert!(fragment_matches_headings("notes", &secs));
        assert!(fragment_matches_headings("notes-1", &secs));
        assert!(fragment_matches_headings("notes-2", &secs));
        assert!(!fragment_matches_headings("notes-3", &secs));
    }

    #[test]
    fn raw_text_form_still_accepted_alongside_slug() {
        // Obsidian spelling keeps working (DEC-060 is not replaced).
        let secs = [sec(Some("Sub Section"))];
        assert!(fragment_matches_headings("Sub Section", &secs));
        assert!(fragment_matches_headings("sub%20section", &secs));
    }

    #[test]
    fn punctuation_only_fragment_is_broken_not_matched() {
        // Slugifying `!!!` yields an empty slug; it must not match every
        // heading by accident.
        let secs = [sec(Some("Tasks"))];
        assert!(!fragment_matches_headings("!!!", &secs));
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
