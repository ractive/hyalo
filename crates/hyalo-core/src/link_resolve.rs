//! Unified link resolver for all `hyalo` link-mutating commands.
//!
//! [`LinkResolver`] consolidates the three independent resolver implementations
//! that existed before iter-150:
//! - `StemIndex` in `link_fix.rs`
//! - `CaseInsensitiveIndex` in `link_graph.rs`
//! - Ad-hoc canonicalization in `link_rewrite.rs:plan_inbound_rewrites`
//!
//! A single ordered precedence for match strategies is applied:
//! 1. Exact path match (stem or `.md` form).
//! 2. Case-insensitive path match (through [`CaseInsensitiveIndex`]).
//! 3. `.md`-suffix tolerance (wikilinks with explicit `.md`).
//! 4. Bare-basename stem lookup — returns `Ambiguous` when >1 match.

use std::path::Path;

use crate::case_index::CaseInsensitiveIndex;
use crate::link_graph::normalize_target;
use crate::links::{LinkKind, LinkSpan, Resolution, WrittenForm};

// ---------------------------------------------------------------------------
// WrittenForm detection
// ---------------------------------------------------------------------------

/// Detect the syntactic form used in a wikilink target string as written by
/// the user (before any normalization).
///
/// This is called on the **raw** target text extracted from the source file,
/// NOT on the normalized form stored in `link.target`.
pub fn detect_wikilink_form(raw_target: &str) -> WrittenForm {
    // `./` prefix: explicit current-directory form (takes highest priority).
    if raw_target.starts_with("./") {
        return WrittenForm::DotRelative;
    }
    // Path separator → path-relative form (takes priority over .md suffix).
    // `[[sub/note.md]]` is written as PathRelative, not MdSuffixed, because
    // the user's intent is a directory-qualified reference.
    if raw_target.contains('/') || raw_target.contains('\\') {
        return WrittenForm::PathRelative;
    }
    // Explicit `.md` suffix (case-insensitive) on a bare stem.
    if raw_target.len() > 3 {
        let split_at = raw_target.len() - 3;
        let last3 = &raw_target.as_bytes()[split_at..];
        if last3.eq_ignore_ascii_case(b".md") {
            return WrittenForm::MdSuffixed;
        }
    }
    // Bare: no separator, no `.md`.
    WrittenForm::Bare
}

// ---------------------------------------------------------------------------
// LinkResolver
// ---------------------------------------------------------------------------

/// Unified resolver: given a [`LinkSpan`] and its source file, decide whether
/// the link targets a known vault file and which vault path it resolves to.
///
/// Used by `mv`, `links fix`, and `links auto` to replace the three
/// independent per-callsite resolver implementations.
pub struct LinkResolver<'a> {
    case_index: &'a CaseInsensitiveIndex,
    site_prefix: Option<&'a str>,
}

impl<'a> LinkResolver<'a> {
    /// Create a resolver backed by the given case index.
    pub fn new(case_index: &'a CaseInsensitiveIndex, site_prefix: Option<&'a str>) -> Self {
        Self {
            case_index,
            site_prefix,
        }
    }

    /// Resolve a link span against the vault.
    ///
    /// `source_rel` is the vault-relative path of the file that contains the
    /// link (forward-slash form, e.g. `"notes/a.md"`).
    ///
    /// The `old_rel` / `old_stem` pair is what we are matching *against* — the
    /// pre-move vault path of the file being renamed.  Both variants are
    /// checked so that callers don't have to compute the stem themselves.
    pub(crate) fn matches_target(
        &self,
        span: &LinkSpan,
        source_rel: &str,
        old_rel: &str,
        old_stem: &str,
    ) -> bool {
        match span.kind {
            LinkKind::Wikilink => self.wikilink_matches(span, source_rel, old_rel, old_stem),
            LinkKind::Markdown => self.markdown_matches(span, source_rel, old_rel, old_stem),
        }
    }

    fn wikilink_matches(
        &self,
        span: &LinkSpan,
        source_rel: &str,
        old_rel: &str,
        old_stem: &str,
    ) -> bool {
        let t = &span.link.target;
        // Normalize `./`-prefixed wikilinks against source dir.
        let resolved: std::borrow::Cow<str> = if let Some(wo) = t.strip_prefix("./") {
            std::borrow::Cow::Owned(normalize_target(Path::new(source_rel), wo))
        } else {
            std::borrow::Cow::Borrowed(t.as_str())
        };
        let t = resolved.as_ref();

        let is_bare = !(t.contains('/') || t.contains('\\'));
        if is_bare {
            // Bare wikilinks: only rewrite when unambiguously resolved to old_rel.
            let t_norm = t.to_ascii_lowercase();
            let stem = if Path::new(&t_norm)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
            {
                &t_norm[..t_norm.len() - 3]
            } else {
                &t_norm
            };
            let canonical = self.case_index.lookup_stem(stem);
            canonical == Some(old_rel) || canonical == Some(old_stem)
        } else if t == old_stem || t == old_rel {
            true
        } else {
            // Case-insensitive path canonicalization.
            let t_norm = t.replace('\\', "/").to_ascii_lowercase();
            let canonical = self.case_index.lookup_unique(&t_norm).or_else(|| {
                let with_md = format!("{t_norm}.md");
                self.case_index.lookup_unique(&with_md)
            });
            canonical == Some(old_rel) || canonical == Some(old_stem)
        }
    }

    fn markdown_matches(
        &self,
        span: &LinkSpan,
        source_rel: &str,
        old_rel: &str,
        old_stem: &str,
    ) -> bool {
        use crate::link_graph::strip_site_prefix;

        let norm = if span.link.target.starts_with('/') {
            strip_site_prefix(&span.link.target, self.site_prefix)
        } else {
            normalize_target(Path::new(source_rel), &span.link.target)
        };
        if norm == old_rel || norm == old_stem {
            return true;
        }
        let norm_lower = norm.to_ascii_lowercase();
        let canonical = self.case_index.lookup_unique(&norm_lower).or_else(|| {
            let with_md = format!("{norm_lower}.md");
            self.case_index.lookup_unique(&with_md)
        });
        canonical == Some(old_rel) || canonical == Some(old_stem)
    }

    /// Resolve a bare wikilink stem to a [`Resolution`].
    ///
    /// Used by the ambiguity-detection path in `mv` to distinguish `Hit` (safe
    /// to rewrite) from `Ambiguous` (warn + skip without `--allow-ambiguous`).
    pub fn resolve_stem(&self, stem: &str) -> Resolution {
        let stem_lower = stem.to_ascii_lowercase();
        let candidates = self.case_index.lookup_stem_all(&stem_lower);
        match candidates.len() {
            0 => Resolution::Broken,
            1 => Resolution::Hit {
                vault_path: candidates[0].clone(),
            },
            _ => Resolution::Ambiguous(candidates.to_vec()),
        }
    }
}

// ---------------------------------------------------------------------------
// WrittenForm helpers (used by LinkWriter)
// ---------------------------------------------------------------------------

/// Given a span and the raw line text, extract the written form of the target
/// as the user typed it (before normalization).
#[allow(dead_code)]
pub(crate) fn written_form_for_span(span: &LinkSpan, line: &str) -> WrittenForm {
    match span.kind {
        LinkKind::Wikilink => {
            // Raw text between the `[[` delimiters for the target portion.
            let raw_target = &line[span.target_start..span.target_end];
            detect_wikilink_form(raw_target)
        }
        LinkKind::Markdown => {
            let raw = &line[span.target_start..span.target_end];
            if raw.starts_with('/') {
                WrittenForm::VaultAbsolute
            } else {
                WrittenForm::PathRelative
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_form_bare() {
        assert_eq!(detect_wikilink_form("note"), WrittenForm::Bare);
        assert_eq!(detect_wikilink_form("Note"), WrittenForm::Bare);
    }

    #[test]
    fn detect_form_path_relative() {
        assert_eq!(detect_wikilink_form("sub/note"), WrittenForm::PathRelative);
        assert_eq!(detect_wikilink_form("a/b/c"), WrittenForm::PathRelative);
    }

    #[test]
    fn detect_form_dot_relative() {
        assert_eq!(detect_wikilink_form("./note"), WrittenForm::DotRelative);
    }

    #[test]
    fn detect_form_md_suffixed() {
        assert_eq!(detect_wikilink_form("note.md"), WrittenForm::MdSuffixed);
        assert_eq!(detect_wikilink_form("note.MD"), WrittenForm::MdSuffixed);
    }

    #[test]
    fn detect_form_path_with_md_suffix() {
        // path/note.md — has a slash so PathRelative wins over MdSuffixed
        // (the `.md` suffix is handled by PathRelative + suffix check at emit time).
        assert_eq!(
            detect_wikilink_form("path/note.md"),
            WrittenForm::PathRelative
        );
    }

    #[test]
    fn resolve_stem_unique() {
        let mut idx = CaseInsensitiveIndex::new();
        idx.insert("sub/note.md");
        let resolver = LinkResolver::new(&idx, None);
        let res = resolver.resolve_stem("note");
        assert!(matches!(res, Resolution::Hit { .. }));
    }

    #[test]
    fn resolve_stem_ambiguous() {
        let mut idx = CaseInsensitiveIndex::new();
        idx.insert("a/note.md");
        idx.insert("b/note.md");
        let resolver = LinkResolver::new(&idx, None);
        let res = resolver.resolve_stem("note");
        assert!(matches!(res, Resolution::Ambiguous(_)));
    }

    #[test]
    fn resolve_stem_broken() {
        let idx = CaseInsensitiveIndex::new();
        let resolver = LinkResolver::new(&idx, None);
        let res = resolver.resolve_stem("note");
        assert_eq!(res, Resolution::Broken);
    }
}
