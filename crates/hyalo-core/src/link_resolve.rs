//! Rewrite policy adapter over the shared source-aware catalog resolver.

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
    aliases: bool,
}

impl<'a> LinkResolver<'a> {
    /// Create a resolver backed by the given case index.
    pub fn new(case_index: &'a CaseInsensitiveIndex, site_prefix: Option<&'a str>) -> Self {
        Self {
            case_index,
            site_prefix,
            aliases: case_index.aliases_enabled(),
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
        let _ = old_stem;
        crate::catalog::resolve(
            self.case_index,
            source_rel,
            span.kind,
            &span.link.target,
            crate::catalog::ResolutionOptions {
                aliases: self.aliases,
                site_prefix: self.site_prefix,
            },
        )
        .path()
            == Some(old_rel)
    }

    /// Whether `span` is a **directory reference** to `old_rel` (iter-203).
    ///
    /// A link written as `/foo`, `foo`, `foo/` or `[[foo]]` names the
    /// directory whose index file is `foo/index.md`. When `old_rel` is such an
    /// index file and the span spells its directory, this returns
    /// `Some(trailing_slash)` — the flag records whether the author wrote the
    /// slash, so [`crate::link_write::LinkWriter`] can reproduce the exact
    /// spelling after the move.
    ///
    /// Returns `None` for every other link, including a directory spelling
    /// that a real file outranks (`foo` when `foo.md` exists) — mirroring the
    /// precedence in `discovery::resolve_target`.
    pub(crate) fn dir_index_match(
        &self,
        span: &LinkSpan,
        source_rel: &str,
        old_rel: &str,
    ) -> Option<bool> {
        use crate::link_graph::strip_site_prefix;

        if crate::catalog::resolve(
            self.case_index,
            source_rel,
            span.kind,
            &span.link.target,
            crate::catalog::ResolutionOptions {
                aliases: self.aliases,
                site_prefix: self.site_prefix,
            },
        )
        .path()
            != Some(old_rel)
        {
            return None;
        }
        let old_dir = crate::discovery::directory_for_index_file(old_rel)?;

        let raw = span.link.target.replace('\\', "/");
        let trailing_slash = raw.ends_with('/');
        let trimmed = raw.trim_end_matches('/');
        if trimmed.is_empty() {
            return None;
        }

        // Normalize exactly like the read-side resolvers do, per link kind.
        let candidates: Vec<String> = match span.kind {
            LinkKind::Wikilink => match trimmed.strip_prefix("./") {
                Some(rest) => vec![normalize_target(Path::new(source_rel), rest)],
                None => vec![trimmed.to_owned()],
            },
            LinkKind::Markdown => {
                if trimmed.starts_with('/') {
                    vec![strip_site_prefix(trimmed, self.site_prefix)]
                } else if trimmed.contains('/') {
                    vec![normalize_target(Path::new(source_rel), trimmed)]
                } else {
                    // Bare basename: source-relative first, then vault-root —
                    // the same two candidates `normalize_link_target` tries.
                    vec![
                        normalize_target(Path::new(source_rel), trimmed),
                        trimmed.to_owned(),
                    ]
                }
            }
        };

        let matched = candidates.iter().any(|candidate| {
            let candidate = candidate.trim_end_matches('/');
            candidate == old_dir
                || (self.case_index.case_insensitive_paths_enabled()
                    && candidate.eq_ignore_ascii_case(old_dir))
        });
        if !matched {
            return None;
        }

        // Precedence guard: a spelling that names a real file is not a
        // directory reference — unless the trailing slash made it explicit.
        if !trailing_slash {
            for candidate in &candidates {
                if self.case_index.contains_path(candidate)
                    || self.case_index.lookup_unique(candidate).is_some()
                {
                    return None;
                }
                let with_md = format!("{candidate}.md");
                if self.case_index.contains_path(&with_md)
                    || self.case_index.lookup_unique(&with_md).is_some()
                {
                    return None;
                }
            }
        }

        Some(trailing_slash)
    }

    /// Resolve a bare wikilink stem to a [`Resolution`].
    ///
    /// Used by the ambiguity-detection path in `mv` to distinguish `Hit` (safe
    /// to rewrite) from `Ambiguous` (warn + skip without `--allow-ambiguous`).
    pub fn resolve_stem(&self, stem: &str) -> Resolution {
        // A move deliberately has a stronger refusal policy than a read: a
        // root-path winner does not make a multiply declared bare name safe
        // to rewrite, and a stable alias should keep its authored spelling.
        let candidates = self.case_index.lookup_stem_all(stem.trim());
        match candidates {
            [] => Resolution::Broken,
            [only] => Resolution::Hit {
                vault_path: only.to_owned(),
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
