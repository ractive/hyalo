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
//! ## DEC-075 — GitHub slug forms are accepted too (iter-211, BUG-8)
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
//! ## DEC-099 — templated headings are never dead anchors (iter-215)
//!
//! A heading containing a Liquid/Jinja expression (`## {% data
//! variables.product.prodname_pro %}`) renders to something hyalo cannot
//! compute, so its real anchor (`#github-pro`) can never be derived from the
//! source text. When neither convention above matches and *either* side of the
//! comparison is templated — the fragment, or any heading in the target file —
//! the fragment is treated as matching rather than reported broken. See
//! [`is_templated_heading`]; this is the heading-side twin of iter-207's
//! `link_fix::is_templated_target` zone-skip for link destinations.
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

/// Whether heading (or fragment) text carries a *template expression* rather
/// than literal prose — iter-215, the heading-side twin of iter-207's
/// [`crate::link_fix::is_templated_target`].
///
/// GitHub Docs and every other Liquid/Jinja-templated corpus writes headings
/// like `## {% data variables.product.prodname_pro %}`, which the renderer
/// turns into `## GitHub Pro` and anchors as `#github-pro`. hyalo sees only
/// the pre-render source, so [`github_slug`] produces `-data-variables…` —
/// a slug no author ever writes and nothing can ever match. The link
/// `[x](f.md#github-pro)` is then reported as a dead anchor even though it is
/// perfectly correct in the rendered site.
///
/// Deliberately the *same* marker set as `is_templated_target` (`{%`, `{{`,
/// `${`), delegating to it so the two cannot drift: the question is identical
/// ("is this text something a template engine will rewrite?"), only the
/// subject differs.
#[must_use]
pub fn is_templated_heading(text: &str) -> bool {
    crate::link_fix::is_templated_target(text)
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

/// The one heading a broken fragment is a **prefix** of, if there is exactly
/// one (iter-261 / DEC-268).
///
/// The own knowledgebase writes `[[decision-log#DEC-068]]` — an anchor that
/// names a decision id, while the heading it means is
/// `## DEC-068: Snapshot index format`. Obsidian calls that broken, and it is:
/// the fix is to write the whole heading. This finds the heading to suggest.
///
/// Returns `None` when nothing starts with the fragment, and — deliberately —
/// also when **two or more** headings do: an ambiguous prefix is a guess, and a
/// guess is exactly what makes a silent prefix match hide typos. Comparison is
/// ASCII-case-insensitive on the trimmed heading text, matching
/// [`fragment_matches_headings`]. Block refs and templated headings are never
/// suggested.
///
/// The suggestion is *reported*, never applied on its own: `find --broken-links`
/// surfaces it as `suggested_fragment` so the author can confirm it.
#[must_use]
pub fn unique_heading_by_prefix<'a>(
    fragment: &str,
    sections: &'a [OutlineSection],
) -> Option<&'a str> {
    if is_block_ref(fragment) {
        return None;
    }
    let needle = normalize_for_match(fragment);
    if needle.is_empty() {
        return None;
    }
    let mut found: Option<&'a str> = None;
    for section in sections {
        let Some(heading) = section.heading.as_deref().map(str::trim) else {
            continue;
        };
        // iter-275 (BUG-7, DEC-309): `<`, not `<=`. A heading the folded
        // fragment matches *exactly* is the single most useful suggestion
        // there is — `#Predefined_fallback_options` next to
        // `## Predefined fallback options` — and the old `<=` excluded
        // precisely that case, so the fragment MDN writes most often was the
        // one shape that never got a suggestion.
        if heading.len() < needle.len() || is_templated_heading(heading) {
            continue;
        }
        let Some(prefix) = heading.get(..needle.len()) else {
            continue;
        };
        if !prefix_matches_fragment(prefix, &needle) {
            continue;
        }
        if found.is_some() {
            // Two headings share the prefix — ambiguous, so no suggestion.
            return None;
        }
        found = Some(heading);
    }
    found
}

/// Compare a heading's leading bytes against a fragment, folding ASCII case
/// *and* the three interchangeable word separators (iter-272 Part E).
///
/// DEC-268 forbids silently *resolving* a fragment by fuzzy matching; it says
/// nothing about what may be *suggested*. MDN slugs its headings with
/// underscores (`#Browser_compatibility`) while the heading itself is written
/// with spaces, so a strict comparison found no prefix for 1242 of the 1254
/// broken anchors on an MDN copy — every one of them a heading the reader
/// could see two lines away. `-`, `_` and a space are treated as one
/// character class here, and only here: the suggestion still has to be a
/// unique prefix of exactly one heading, and it is still printed rather than
/// applied.
fn prefix_matches_fragment(prefix: &str, needle: &str) -> bool {
    if prefix.len() != needle.len() {
        return false;
    }
    prefix
        .bytes()
        .zip(needle.bytes())
        .all(|(a, b)| separator_class(a) == separator_class(b))
}

/// `b` folded to lowercase, with every word separator mapped to one byte.
fn separator_class(b: u8) -> u8 {
    match b {
        b'-' | b'_' | b' ' => b'-',
        other => other.to_ascii_lowercase(),
    }
}

/// A slug with `-`, `_` and every whitespace character folded to one
/// separator, so the three interchangeable word separators compare equal
/// (iter-275, DEC-309).
///
/// [`github_slug`] already lowercases and maps whitespace to `-`, but it keeps
/// `_` verbatim — GitHub's own rule. MDN, on the other hand, slugs
/// `## Browser compatibility` as `#Browser_compatibility`, so the two
/// conventions disagree on exactly one byte and hyalo reported 10 929 dead
/// anchors on an MDN checkout that every browser resolves. DEC-268 forbids
/// *guessing* a heading; folding a separator is not a guess — no two headings
/// in the wild differ only in which separator their words are joined with, and
/// a renderer that emits one form is read by an author writing the other.
fn fold_separators(slug: &str) -> String {
    slug.chars()
        .map(|c| {
            if c == '-' || c == '_' || c.is_whitespace() {
                '-'
            } else {
                c
            }
        })
        .collect()
}

/// Whether one heading-path segment names `heading`, under the same two
/// conventions a whole fragment uses: raw text (DEC-060) or the renderer's
/// slug with separators folded (DEC-075 + DEC-309).
///
/// Deliberately per-heading, with no duplicate-suffix bookkeeping: a heading
/// path names a position in the outline, so the `-1`/`-2` disambiguation
/// [`heading_slugs`] applies to a flat slug list has nothing to disambiguate.
fn segment_matches_heading(segment: &str, heading: &str) -> bool {
    let needle = normalize_for_match(segment);
    let heading = heading.trim();
    if heading.eq_ignore_ascii_case(&needle) {
        return true;
    }
    let needle_slug = github_slug(&needle);
    !needle_slug.is_empty()
        && fold_separators(&github_slug(heading)) == fold_separators(&needle_slug)
}

/// Whether `segments` names a chain of headings, each nested inside the one
/// before it (DEC-311, iter-275).
///
/// Obsidian's `[[note#Heading One#Sub Two]]` means "the `Sub Two` heading that
/// sits under `Heading One`", so the check walks the outline rather than
/// comparing strings: each segment must match a heading at a strictly deeper
/// level than its parent, and *within* the parent's subtree — the run of
/// sections up to the next heading at the parent's level or shallower. A
/// segment that matches several headings is retried against each, so an
/// ambiguous first segment cannot mask a valid path under a later one.
fn heading_path_matches(segments: &[&str], sections: &[OutlineSection]) -> bool {
    let Some((first, rest)) = segments.split_first() else {
        return true;
    };
    for (i, section) in sections.iter().enumerate() {
        let Some(heading) = section.heading.as_deref() else {
            continue;
        };
        if !segment_matches_heading(first, heading) {
            continue;
        }
        if rest.is_empty() {
            return true;
        }
        let level = section.level;
        let subtree_end = sections
            .iter()
            .enumerate()
            .skip(i + 1)
            .find(|(_, s)| s.heading.is_some() && s.level <= level)
            .map_or(sections.len(), |(j, _)| j);
        if heading_path_matches(rest, &sections[i + 1..subtree_end]) {
            return true;
        }
    }
    false
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
    // DEC-311 (iter-275, BUG-36): `[[a#Heading One#Sub Two]]` is Obsidian's
    // *heading path* — "Sub Two, nested under Heading One" — not a heading
    // literally named `Heading One#Sub Two`. Tried first, and only when the
    // fragment actually carries an inner `#`; a heading that genuinely
    // contains one still matches through the literal checks below.
    if needle.contains('#') {
        let segments: Vec<&str> = needle.split('#').map(str::trim).collect();
        if segments.iter().all(|s| !s.is_empty()) && heading_path_matches(&segments, sections) {
            return true;
        }
    }

    // DEC-060: raw heading text, case-insensitive.
    if sections.iter().any(|s| {
        s.heading
            .as_deref()
            .is_some_and(|h| h.trim().eq_ignore_ascii_case(&needle))
    }) {
        return true;
    }
    // DEC-075: GitHub-rendered slug, with duplicate suffixes.
    let needle_slug = github_slug(&needle);
    if !needle_slug.is_empty() {
        let slugs = heading_slugs(sections);
        if slugs.contains(&needle_slug) {
            return true;
        }
        // DEC-309 (iter-275, BUG-7): `-`, `_` and a space are one word
        // separator. `#Browser_compatibility` names `## Browser compatibility`
        // in every renderer MDN ships; only GitHub's slug rule says otherwise.
        let needle_folded = fold_separators(&needle_slug);
        if slugs.iter().any(|s| fold_separators(s) == needle_folded) {
            return true;
        }
    }
    // DEC-099 (iter-215): nothing matched literally — but if either side is
    // *templated*, hyalo is comparing pre-render source against a fragment
    // written for the rendered output and cannot know whether they agree.
    //
    // Either side, because the mismatch can come from either direction: a
    // templated heading (`## {% data variables.product.prodname_pro %}`,
    // rendered `## GitHub Pro`, linked as `#github-pro`) or a templated
    // fragment (`[x](f.md#{{ anchor }})`).
    //
    // The heading test is file-wide rather than per-heading on purpose: a
    // templated heading's rendered slug is unknowable, so *any* templated
    // heading in the target file could be the one this fragment names. Being
    // conservative here costs a missed dead anchor in files that use
    // templating; the alternative cost is every anchor into such a file
    // reported broken, which is what made hyalo unusable on the GitHub Docs
    // corpus (6 of 7 checkable anchors were false positives before DEC-075,
    // and the templated remainder after it). Consistent with the module's
    // stated bias: a false positive costs a user far more than a miss.
    if is_templated_heading(&needle) {
        return true;
    }
    sections
        .iter()
        .any(|s| s.heading.as_deref().is_some_and(is_templated_heading))
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

    // --- iter-272 Part E: separator-insensitive suggestion matching ---

    #[test]
    fn suggested_fragment_folds_underscores_hyphens_and_spaces() {
        let sections = vec![sec(Some("Browser compatibility and support"))];
        // MDN slugs with underscores; the heading is written with spaces.
        assert_eq!(
            unique_heading_by_prefix("Browser_compatibility", &sections),
            Some("Browser compatibility and support")
        );
        assert_eq!(
            unique_heading_by_prefix("browser-compatibility", &sections),
            Some("Browser compatibility and support")
        );
        // A genuinely different fragment still gets no suggestion.
        assert_eq!(unique_heading_by_prefix("Server_compat", &sections), None);
    }

    #[test]
    fn separator_folding_does_not_make_a_suggestion_ambiguous_match() {
        let sections = vec![
            sec(Some("Browser compatibility")),
            sec(Some("Browser_compatibility notes")),
        ];
        assert_eq!(unique_heading_by_prefix("Browser_com", &sections), None);
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
        // DEC-075 slug path lowercases Unicode the way a renderer does, so the
        // lowercased spelling now matches (iter-211, BUG-8).
        assert!(fragment_matches_headings("überschrift", &secs));
    }

    // --- DEC-075: GitHub slug forms (iter-211, BUG-8) ---

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

    // --- DEC-099: templated headings (iter-215) ---

    #[test]
    fn is_templated_heading_recognizes_the_marker_forms() {
        assert!(is_templated_heading(
            "{% data variables.product.prodname_pro %}"
        ));
        assert!(is_templated_heading("Upgrade to {{ product.name }}"));
        assert!(is_templated_heading("${BASE} overview"));
        assert!(!is_templated_heading("Sub Section"));
        // A lone brace is not a template marker (mirrors `is_templated_target`).
        assert!(!is_templated_heading("Set {name} here"));
    }

    #[test]
    fn rendered_anchor_into_a_liquid_heading_is_not_broken() {
        // The headline case: `## {% data variables.product.prodname_pro %}`
        // renders as `## GitHub Pro` → `#github-pro`, which hyalo cannot derive.
        let secs = [sec(Some("{% data variables.product.prodname_pro %}"))];
        assert!(fragment_matches_headings("github-pro", &secs));
    }

    #[test]
    fn templated_fragment_is_not_broken() {
        let secs = [sec(Some("Real"))];
        assert!(fragment_matches_headings("{{anchor}}", &secs));
        assert!(fragment_matches_headings("{% raw %}x{% endraw %}", &secs));
    }

    #[test]
    fn templated_heading_makes_the_whole_file_permissive() {
        // Deliberate: any templated heading could be the one a fragment names,
        // so no fragment in that file can be proven dead.
        let secs = [sec(Some("Real")), sec(Some("{{ product }}"))];
        assert!(fragment_matches_headings("anything-at-all", &secs));
    }

    #[test]
    fn untemplated_file_still_reports_dead_anchors() {
        // The escape hatch must not leak into ordinary files.
        let secs = [sec(Some("Real")), sec(Some("Sub Section"))];
        assert!(!fragment_matches_headings("nowhere", &secs));
        assert!(!fragment_matches_headings("!!!", &secs));
    }

    #[test]
    fn percent_encoded_template_markers_are_recognized() {
        // `#%7B%7Banchor%7D%7D` decodes to `{{anchor}}` — the check runs on the
        // normalized (decoded) fragment, not the raw text.
        let secs = [sec(Some("Real"))];
        assert!(fragment_matches_headings("%7B%7Banchor%7D%7D", &secs));
    }

    #[test]
    fn literal_percent_not_decoded() {
        // A heading with a literal stray `%` (no valid escape) is compared
        // verbatim.
        let secs = [sec(Some("100%done"))];
        assert!(fragment_matches_headings("100%done", &secs));
    }

    // --- iter-261 / DEC-268: unique heading prefix ---

    fn secs(headings: &[&str]) -> Vec<OutlineSection> {
        headings
            .iter()
            .enumerate()
            .map(|(i, h)| OutlineSection {
                level: 2,
                heading: Some((*h).to_owned()),
                line: i + 1,
                links: Vec::new(),
                tasks: None,
                code_blocks: Vec::new(),
            })
            .collect()
    }

    #[test]
    fn unique_prefix_suggests_the_full_heading() {
        let sections = secs(&["DEC-068: Snapshot index format", "DEC-070: Something else"]);
        assert_eq!(
            unique_heading_by_prefix("DEC-068", &sections),
            Some("DEC-068: Snapshot index format")
        );
        // Case-insensitive, like every other anchor comparison.
        assert_eq!(
            unique_heading_by_prefix("dec-070", &sections),
            Some("DEC-070: Something else")
        );
    }

    #[test]
    fn ambiguous_prefix_suggests_nothing() {
        let sections = secs(&["DEC-06: first", "DEC-06: second"]);
        assert_eq!(unique_heading_by_prefix("DEC-06", &sections), None);
    }

    #[test]
    fn prefix_of_nothing_suggests_nothing_but_a_whole_heading_does() {
        let sections = secs(&["DEC-068: Snapshot index format"]);
        // No heading starts with it.
        assert_eq!(unique_heading_by_prefix("DEC-999", &sections), None);
        // iter-275 (BUG-7, DEC-309): a fragment covering the *whole* heading is
        // the most useful suggestion there is — it is what MDN's
        // `#Browser_compatibility` looks like once the separators are folded.
        // A byte-identical fragment never reaches this helper (the anchor
        // resolves), so answering it costs nothing.
        assert_eq!(
            unique_heading_by_prefix("DEC-068: Snapshot index format", &sections),
            Some("DEC-068: Snapshot index format")
        );
    }

    #[test]
    fn a_whole_heading_written_with_underscores_is_suggested() {
        let sections = secs(&["Predefined fallback options"]);
        assert_eq!(
            unique_heading_by_prefix("Predefined_fallback_options", &sections),
            Some("Predefined fallback options")
        );
    }

    // --- iter-275 (DEC-309): separator folding on *resolution* ---

    #[test]
    fn underscore_fragment_resolves_to_a_space_separated_heading() {
        let secs = [sec(Some("Browser compatibility"))];
        assert!(fragment_matches_headings("Browser_compatibility", &secs));
        assert!(fragment_matches_headings("browser-compatibility", &secs));
        assert!(!fragment_matches_headings("Browser_compatibilty", &secs));
    }

    // --- iter-275 (DEC-311): nested heading paths ---

    #[test]
    fn a_nested_heading_path_resolves_and_respects_nesting() {
        let sections = vec![
            OutlineSection {
                level: 2,
                heading: Some("Heading One".into()),
                line: 1,
                links: Vec::new(),
                tasks: None,
                code_blocks: Vec::new(),
            },
            OutlineSection {
                level: 3,
                heading: Some("Sub Two".into()),
                line: 2,
                links: Vec::new(),
                tasks: None,
                code_blocks: Vec::new(),
            },
            OutlineSection {
                level: 2,
                heading: Some("Other".into()),
                line: 3,
                links: Vec::new(),
                tasks: None,
                code_blocks: Vec::new(),
            },
            OutlineSection {
                level: 3,
                heading: Some("Elsewhere".into()),
                line: 4,
                links: Vec::new(),
                tasks: None,
                code_blocks: Vec::new(),
            },
        ];
        assert!(fragment_matches_headings("Heading One#Sub Two", &sections));
        // `Elsewhere` exists, but not under `Heading One`.
        assert!(!fragment_matches_headings(
            "Heading One#Elsewhere",
            &sections
        ));
        // Separator folding applies per segment.
        assert!(fragment_matches_headings("heading_one#sub_two", &sections));
        // A single segment still resolves on its own.
        assert!(fragment_matches_headings("Sub Two", &sections));
    }

    #[test]
    fn block_refs_and_templated_headings_are_never_suggested() {
        let sections = secs(&["{{ title }} rest"]);
        assert_eq!(unique_heading_by_prefix("{{ title }}", &sections), None);
        assert_eq!(
            unique_heading_by_prefix("^abc123", &secs(&["^abc123 x"])),
            None
        );
    }
}
