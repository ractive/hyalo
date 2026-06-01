//! Unified link writer for all `hyalo` link-mutating commands (iter-150).
//!
//! [`LinkWriter::rewrite`] splices a `LinkSpan` in the source line, replacing
//! the target text with a new vault-relative path while preserving:
//! - The user's written form (`WrittenForm`) under `PreserveForm::Preserve`.
//! - Fragment (`#section`) and alias (`|label`) from the original span.
//! - Outer delimiters (`[[…]]` or `[…](…)`).
//!
//! All link-text mutators (`mv`, `links fix --apply`, `links auto --apply`,
//! frontmatter wikilink rewriter) route through this module instead of
//! building the replacement string inline.

use crate::link_graph::relative_path_between;
use crate::link_resolve::detect_wikilink_form;
use crate::links::{LinkKind, LinkSpan, PreserveForm, WrittenForm};

/// A computed text replacement for a single span within a line.
///
/// This is a thin companion to [`crate::link_rewrite::Replacement`]; the
/// caller can promote it to a `Replacement` by adding the `line` number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpanReplacement {
    /// Byte offset of the first byte of the full link syntax in the original line.
    pub byte_offset: usize,
    /// The original full link syntax (e.g. `[[old/path|alias]]`).
    pub old_text: String,
    /// The replacement full link syntax.
    pub new_text: String,
}

/// Unified link text writer.
pub struct LinkWriter;

impl LinkWriter {
    /// Compute the replacement text for a single link span within `line`.
    ///
    /// `new_vault_rel` is the new vault-relative target path (with `.md`
    /// suffix, forward slashes).  `source_rel` is the vault-relative path of
    /// the file that contains the span (needed for relative markdown target
    /// computation).
    ///
    /// Returns `None` when the computed replacement equals the original text
    /// (i.e. no rewrite needed).
    pub(crate) fn rewrite(
        span: &LinkSpan,
        line: &str,
        new_vault_rel: &str,
        source_rel: &str,
        policy: PreserveForm,
        site_prefix: Option<&str>,
    ) -> Option<SpanReplacement> {
        let new_target =
            Self::compute_new_target(span, line, new_vault_rel, source_rel, policy, site_prefix);

        let old_text = line[span.full_start..span.full_end].to_string();
        let new_text = format!(
            "{}{}{}",
            &line[span.full_start..span.target_start],
            new_target,
            &line[span.target_end..span.full_end]
        );

        if old_text == new_text {
            None
        } else {
            Some(SpanReplacement {
                byte_offset: span.full_start,
                old_text,
                new_text,
            })
        }
    }

    fn compute_new_target(
        span: &LinkSpan,
        line: &str,
        new_vault_rel: &str,
        source_rel: &str,
        policy: PreserveForm,
        site_prefix: Option<&str>,
    ) -> String {
        let new_stem = new_vault_rel.strip_suffix(".md").unwrap_or(new_vault_rel);

        match span.kind {
            LinkKind::Wikilink => Self::compute_wikilink_target(
                span,
                line,
                new_vault_rel,
                new_stem,
                policy,
                source_rel,
            ),
            LinkKind::Markdown => Self::compute_markdown_target(
                span,
                new_vault_rel,
                new_stem,
                source_rel,
                site_prefix,
            ),
        }
    }

    fn compute_wikilink_target(
        span: &LinkSpan,
        line: &str,
        new_vault_rel: &str,
        new_stem: &str,
        policy: PreserveForm,
        source_rel: &str,
    ) -> String {
        match policy {
            PreserveForm::Bare => {
                // Auto-link mode: always emit bare stem (Obsidian short-form).
                let basename = new_stem.rsplit('/').next().unwrap_or(new_stem);
                basename.to_string()
            }
            PreserveForm::Preserve => {
                // Detect the user's original written form from the raw target text.
                let raw_target = &line[span.target_start..span.target_end];
                let form = detect_wikilink_form(raw_target);
                Self::emit_wikilink_with_form(form, new_vault_rel, new_stem, source_rel)
            }
        }
    }

    /// Emit a wikilink target in the given form.
    fn emit_wikilink_with_form(
        form: WrittenForm,
        _new_vault_rel: &str,
        new_stem: &str,
        source_rel: &str,
    ) -> String {
        let new_basename_stem = new_stem.rsplit('/').next().unwrap_or(new_stem);
        match form {
            WrittenForm::Bare => new_basename_stem.to_string(),
            WrittenForm::PathRelative => new_stem.to_string(),
            WrittenForm::DotRelative => {
                // DotRelative (`[[./foo]]`) means the user wrote the target with
                // an explicit `./` prefix. Preserve the `./` form when the new
                // target is reachable from the linker's directory via `./…`.
                //
                // Same-directory case: source_dir == new_dir → `./{basename}`.
                // Cross-directory case (NEW-2): if new_stem starts with
                // `source_dir/`, emit `./tail` where tail is the portion after
                // the source dir prefix. This handles moves like
                // `mv bulk/f.md bulk/g.md` from `linker.md` with `[[./bulk/f]]`
                // — source_dir="" and new_stem="bulk/g", so tail="bulk/g" and
                // the result is `./bulk/g`, preserving the `./` prefix.
                // When neither condition holds, fall back to PathRelative (full
                // vault-relative stem), which is always unambiguous.
                let source_dir = source_rel.rsplit_once('/').map_or("", |(d, _)| d);
                let new_dir = new_stem.rsplit_once('/').map_or("", |(d, _)| d);
                if source_dir == new_dir {
                    // Same directory: keep `./basename`.
                    format!("./{new_basename_stem}")
                } else if source_dir.is_empty() {
                    // Linker is at vault root; any target is reachable as `./path`.
                    format!("./{new_stem}")
                } else if let Some(tail) = new_stem.strip_prefix(&format!("{source_dir}/")) {
                    // New target is under the linker's directory: `./tail`.
                    format!("./{tail}")
                } else {
                    // Different directory hierarchy — upgrade to PathRelative.
                    new_stem.to_string()
                }
            }
            WrittenForm::MdSuffixed => {
                // MdSuffixed: preserve the `.md` suffix. The user's stylistic
                // choice is to always write the `.md` extension. Re-apply it
                // to the full vault-relative stem so cross-dir moves preserve
                // both the path and the suffix (NEW-2).
                // Note: detect_wikilink_form returns PathRelative for paths
                // containing `/` even with `.md` suffix, so MdSuffixed here
                // means the original was a bare `[[stem.md]]` (no slash).
                // We preserve just the basename with suffix.
                format!("{new_basename_stem}.md")
            }
            WrittenForm::VaultAbsolute => {
                // Vault-absolute is a markdown-only form; for wikilinks fall back
                // to path-relative.
                new_stem.to_string()
            }
        }
    }

    fn compute_markdown_target(
        span: &LinkSpan,
        new_vault_rel: &str,
        new_stem: &str,
        source_rel: &str,
        site_prefix: Option<&str>,
    ) -> String {
        if span.link.target.starts_with('/') {
            // Preserve absolute-path style (vault-absolute).
            // Mirror presence/absence of `.md` suffix from the original.
            let had_md = span.link.target.to_ascii_lowercase().ends_with(".md");
            let target = if had_md { new_vault_rel } else { new_stem };
            match site_prefix {
                Some(prefix) => format!("/{prefix}/{target}"),
                None => format!("/{target}"),
            }
        } else {
            relative_path_between(source_rel, new_vault_rel)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::links::{LinkKind, extract_link_spans};

    fn span_from(text: &str, idx: usize) -> LinkSpan {
        let spans = extract_link_spans(text);
        spans.into_iter().nth(idx).expect("no span at index")
    }

    // --- WrittenForm round-trip tests ---

    #[test]
    fn rewrite_bare_wikilink_stays_bare() {
        let line = "See [[note]] here";
        let span = span_from(line, 0);
        assert_eq!(span.kind, LinkKind::Wikilink);
        let r = LinkWriter::rewrite(
            &span,
            line,
            "sub/renamed.md",
            "a.md",
            PreserveForm::Preserve,
            None,
        )
        .unwrap();
        // bare form → basename of new target
        assert_eq!(r.new_text, "[[renamed]]");
    }

    #[test]
    fn rewrite_path_wikilink_stays_path() {
        let line = "See [[sub/note]] here";
        let span = span_from(line, 0);
        let r = LinkWriter::rewrite(
            &span,
            line,
            "sub/renamed.md",
            "a.md",
            PreserveForm::Preserve,
            None,
        )
        .unwrap();
        // path form → stem of new target (with directory)
        assert_eq!(r.new_text, "[[sub/renamed]]");
    }

    #[test]
    fn rewrite_md_suffixed_wikilink_stays_md_suffixed() {
        let line = "See [[note.md]] here";
        let span = span_from(line, 0);
        let r = LinkWriter::rewrite(
            &span,
            line,
            "sub/renamed.md",
            "a.md",
            PreserveForm::Preserve,
            None,
        )
        .unwrap();
        assert_eq!(r.new_text, "[[renamed.md]]");
    }

    #[test]
    fn rewrite_dot_relative_wikilink_root_linker_preserves_dot_prefix() {
        // [[./b]] — dot-relative form, source at root. After moving target into
        // `notes/`, the linker is at root (source_dir="") so the new target
        // notes/renamed is reachable as ./notes/renamed. DotRelative is preserved
        // (iter-151 NEW-2).
        let line = "See [[./b]] here";
        let span = span_from(line, 0);
        let r = LinkWriter::rewrite(
            &span,
            line,
            "notes/renamed.md",
            "a.md",
            PreserveForm::Preserve,
            None,
        )
        .unwrap();
        assert_eq!(r.new_text, "[[./notes/renamed]]");
    }

    #[test]
    fn rewrite_dot_relative_wikilink_upgrades_to_path_when_linker_nested() {
        // [[./b]] — source at sub/a.md (nested linker). New target moves to
        // other/b.md. source_dir="sub", new_stem="other/b", "other/b" does NOT
        // start with "sub/" → upgrade to PathRelative.
        let line = "See [[./b]] here";
        let span = span_from(line, 0);
        let r = LinkWriter::rewrite(
            &span,
            line,
            "other/b.md",
            "sub/a.md",
            PreserveForm::Preserve,
            None,
        )
        .unwrap();
        assert_eq!(r.new_text, "[[other/b]]");
    }

    #[test]
    fn rewrite_dot_relative_wikilink_preserved_when_dir_unchanged() {
        // [[./b]] — source at notes/a.md, target moves from notes/b.md to
        // notes/renamed.md (same dir). DotRelative form is preserved.
        let line = "See [[./b]] here";
        let span = span_from(line, 0);
        let r = LinkWriter::rewrite(
            &span,
            line,
            "notes/renamed.md",
            "notes/a.md",
            PreserveForm::Preserve,
            None,
        )
        .unwrap();
        assert_eq!(r.new_text, "[[./renamed]]");
    }

    #[test]
    fn rewrite_wikilink_with_alias_preserved() {
        let line = "See [[sub/note|My Note]] here";
        let span = span_from(line, 0);
        let r = LinkWriter::rewrite(
            &span,
            line,
            "sub/renamed.md",
            "a.md",
            PreserveForm::Preserve,
            None,
        )
        .unwrap();
        assert_eq!(r.new_text, "[[sub/renamed|My Note]]");
    }

    #[test]
    fn rewrite_wikilink_with_fragment_preserved() {
        let line = "See [[sub/note#section]] here";
        let span = span_from(line, 0);
        let r = LinkWriter::rewrite(
            &span,
            line,
            "sub/renamed.md",
            "a.md",
            PreserveForm::Preserve,
            None,
        )
        .unwrap();
        assert_eq!(r.new_text, "[[sub/renamed#section]]");
    }

    #[test]
    fn rewrite_bare_policy_always_emits_basename() {
        let line = "See [[sub/note]] here";
        let span = span_from(line, 0);
        let r = LinkWriter::rewrite(
            &span,
            line,
            "sub/renamed.md",
            "a.md",
            PreserveForm::Bare,
            None,
        )
        .unwrap();
        // Bare policy always emits just the basename stem.
        assert_eq!(r.new_text, "[[renamed]]");
    }

    #[test]
    fn rewrite_markdown_relative_link() {
        let line = "See [text](notes/old.md) here";
        let span = span_from(line, 0);
        assert_eq!(span.kind, LinkKind::Markdown);
        let r = LinkWriter::rewrite(
            &span,
            line,
            "notes/renamed.md",
            "a.md",
            PreserveForm::Preserve,
            None,
        )
        .unwrap();
        assert_eq!(r.new_text, "[text](notes/renamed.md)");
    }

    #[test]
    fn rewrite_no_change_returns_none() {
        // If new target produces the same text, return None.
        let line = "See [[note]] here";
        let span = span_from(line, 0);
        // new_vault_rel resolves to same stem "note"
        let r = LinkWriter::rewrite(&span, line, "note.md", "a.md", PreserveForm::Preserve, None);
        assert!(r.is_none(), "no change should return None");
    }

    #[test]
    fn rewrite_vault_absolute_markdown_link() {
        let line = "See [text](/docs/notes/old.md) here";
        let span = span_from(line, 0);
        let r = LinkWriter::rewrite(
            &span,
            line,
            "notes/renamed.md",
            "a.md",
            PreserveForm::Preserve,
            Some("docs"),
        )
        .unwrap();
        assert_eq!(r.new_text, "[text](/docs/notes/renamed.md)");
    }

    // ---------------------------------------------------------------------------
    // Round-trip tests (iter-151): every WrittenForm × topology combination
    // ---------------------------------------------------------------------------
    //
    // For each (form, linker_dir, old target, new target) tuple we assert:
    // 1. The writer emits a string.
    // 2. The emitted string, when re-parsed by extract_link_spans, produces a
    //    link target that resolves to new_vault_rel (round-trip invariant).

    /// Helper: parse the full link text, extract the target stem (strip any
    /// fragment and .md suffix), and return it for comparison.
    fn round_trip_target(full_link: &str) -> String {
        use crate::links::extract_link_spans;
        let spans = extract_link_spans(full_link);
        let span = spans.into_iter().next().expect("no link in emitted text");
        span.link.target
    }

    #[test]
    fn roundtrip_bare_sibling() {
        // [[b]] in root, target moves b.md → renamed.md
        let line = "[[b]]";
        let span = span_from(line, 0);
        let r = LinkWriter::rewrite(
            &span,
            line,
            "renamed.md",
            "a.md",
            PreserveForm::Preserve,
            None,
        )
        .unwrap();
        // Bare form: emits "renamed" (basename)
        assert_eq!(r.new_text, "[[renamed]]");
        assert_eq!(round_trip_target(&r.new_text), "renamed");
    }

    #[test]
    fn roundtrip_path_relative_same_subdir() {
        // [[sub/b]] in root, target moves sub/b.md → sub/renamed.md
        let line = "[[sub/b]]";
        let span = span_from(line, 0);
        let r = LinkWriter::rewrite(
            &span,
            line,
            "sub/renamed.md",
            "a.md",
            PreserveForm::Preserve,
            None,
        )
        .unwrap();
        assert_eq!(r.new_text, "[[sub/renamed]]");
        assert_eq!(round_trip_target(&r.new_text), "sub/renamed");
    }

    #[test]
    fn roundtrip_dot_relative_same_dir() {
        // [[./b]] in notes/, target moves notes/b.md → notes/renamed.md
        let line = "[[./b]]";
        let span = span_from(line, 0);
        let r = LinkWriter::rewrite(
            &span,
            line,
            "notes/renamed.md",
            "notes/a.md",
            PreserveForm::Preserve,
            None,
        )
        .unwrap();
        assert_eq!(r.new_text, "[[./renamed]]");
        assert_eq!(round_trip_target(&r.new_text), "./renamed");
    }

    #[test]
    fn roundtrip_dot_relative_root_linker_cross_dir() {
        // [[./b]] in root, target moves b.md → sub/b.md (NEW-2 case)
        let line = "[[./b]]";
        let span = span_from(line, 0);
        let r = LinkWriter::rewrite(
            &span,
            line,
            "sub/b.md",
            "a.md",
            PreserveForm::Preserve,
            None,
        )
        .unwrap();
        // Root linker → ./sub/b
        assert_eq!(r.new_text, "[[./sub/b]]");
        assert_eq!(round_trip_target(&r.new_text), "./sub/b");
    }

    #[test]
    fn roundtrip_dot_relative_nested_linker_different_dir() {
        // [[./b]] in notes/, target moves to other/b.md — upgrade to PathRelative
        let line = "[[./b]]";
        let span = span_from(line, 0);
        let r = LinkWriter::rewrite(
            &span,
            line,
            "other/b.md",
            "notes/a.md",
            PreserveForm::Preserve,
            None,
        )
        .unwrap();
        // notes/ does not prefix other/b → upgrade to path-relative
        assert_eq!(r.new_text, "[[other/b]]");
        assert_eq!(round_trip_target(&r.new_text), "other/b");
    }

    #[test]
    fn roundtrip_md_suffix_sibling() {
        // [[b.md]] in root, target moves b.md → renamed.md
        let line = "[[b.md]]";
        let span = span_from(line, 0);
        let r = LinkWriter::rewrite(
            &span,
            line,
            "renamed.md",
            "a.md",
            PreserveForm::Preserve,
            None,
        )
        .unwrap();
        // MdSuffixed: emits basename.md
        assert_eq!(r.new_text, "[[renamed.md]]");
        assert_eq!(round_trip_target(&r.new_text), "renamed");
    }

    #[test]
    fn roundtrip_md_suffix_preserves_md_after_cross_dir_rename() {
        // [[f.md]] in root, target moves sub/f.md → sub/g.md
        // detect_wikilink_form treats "f.md" as MdSuffixed (no slash), emits basename.md
        let line = "[[f.md]]";
        let span = span_from(line, 0);
        let r = LinkWriter::rewrite(
            &span,
            line,
            "sub/g.md",
            "a.md",
            PreserveForm::Preserve,
            None,
        )
        .unwrap();
        assert_eq!(r.new_text, "[[g.md]]");
        assert_eq!(round_trip_target(&r.new_text), "g");
    }

    #[test]
    fn roundtrip_path_relative_cross_dir() {
        // [[bulk/f]] in root, target moves bulk/f.md → bulk/g.md
        let line = "[[bulk/f]]";
        let span = span_from(line, 0);
        let r = LinkWriter::rewrite(
            &span,
            line,
            "bulk/g.md",
            "a.md",
            PreserveForm::Preserve,
            None,
        )
        .unwrap();
        assert_eq!(r.new_text, "[[bulk/g]]");
        assert_eq!(round_trip_target(&r.new_text), "bulk/g");
    }

    #[test]
    fn roundtrip_with_fragment_all_forms() {
        // Fragment is preserved through every form change.
        let cases: &[(&str, &str, &str, &str, &str)] = &[
            (
                "[[b#sec]]",
                "renamed.md",
                "a.md",
                "[[renamed#sec]]",
                "renamed",
            ),
            (
                "[[sub/b#sec]]",
                "sub/r.md",
                "a.md",
                "[[sub/r#sec]]",
                "sub/r",
            ),
            (
                "[[./b#sec]]",
                "renamed.md",
                "a.md",
                "[[./renamed#sec]]",
                "./renamed",
            ),
            (
                "[t](b.md#sec)",
                "renamed.md",
                "a.md",
                "[t](renamed.md#sec)",
                "renamed.md",
            ),
        ];
        for (line, new_vault_rel, source_rel, expected_full, _) in cases {
            let span = span_from(line, 0);
            let r = LinkWriter::rewrite(
                &span,
                line,
                new_vault_rel,
                source_rel,
                PreserveForm::Preserve,
                None,
            )
            .unwrap_or_else(|| panic!("no replacement for {line}"));
            assert_eq!(
                &r.new_text, expected_full,
                "fragment preservation for {line}"
            );
        }
    }

    #[test]
    fn roundtrip_with_alias_all_forms() {
        // Alias is preserved through every form change.
        let cases: &[(&str, &str, &str, &str)] = &[
            ("[[b|alias]]", "renamed.md", "a.md", "[[renamed|alias]]"),
            ("[[sub/b|alias]]", "sub/r.md", "a.md", "[[sub/r|alias]]"),
            ("[[./b|alias]]", "renamed.md", "a.md", "[[./renamed|alias]]"),
            (
                "[[b.md|alias]]",
                "renamed.md",
                "a.md",
                "[[renamed.md|alias]]",
            ),
        ];
        for (line, new_vault_rel, source_rel, expected_full) in cases {
            let span = span_from(line, 0);
            let r = LinkWriter::rewrite(
                &span,
                line,
                new_vault_rel,
                source_rel,
                PreserveForm::Preserve,
                None,
            )
            .unwrap_or_else(|| panic!("no replacement for {line}"));
            assert_eq!(&r.new_text, expected_full, "alias preservation for {line}");
        }
    }

    #[test]
    fn roundtrip_markdown_relative_cross_dir() {
        // [text](old.md) in root → sub/new.md: becomes [text](sub/new.md)
        let line = "[text](old.md)";
        let span = span_from(line, 0);
        let r = LinkWriter::rewrite(
            &span,
            line,
            "sub/new.md",
            "a.md",
            PreserveForm::Preserve,
            None,
        )
        .unwrap();
        assert_eq!(r.new_text, "[text](sub/new.md)");
    }

    #[test]
    fn roundtrip_markdown_relative_nested_source() {
        // [text](b.md) in notes/a.md → bulk/b.md: becomes [text](../bulk/b.md)
        let line = "[text](b.md)";
        let span = span_from(line, 0);
        let r = LinkWriter::rewrite(
            &span,
            line,
            "bulk/b.md",
            "notes/a.md",
            PreserveForm::Preserve,
            None,
        )
        .unwrap();
        assert_eq!(r.new_text, "[text](../bulk/b.md)");
    }
}
