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
            LinkKind::Wikilink => {
                Self::compute_wikilink_target(span, line, new_vault_rel, new_stem, policy)
            }
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
                Self::emit_wikilink_with_form(form, new_vault_rel, new_stem)
            }
        }
    }

    /// Emit a wikilink target in the given form.
    fn emit_wikilink_with_form(form: WrittenForm, _new_vault_rel: &str, new_stem: &str) -> String {
        let new_basename_stem = new_stem.rsplit('/').next().unwrap_or(new_stem);
        match form {
            WrittenForm::Bare => new_basename_stem.to_string(),
            WrittenForm::PathRelative => new_stem.to_string(),
            WrittenForm::DotRelative => {
                // DotRelative (`[[./foo]]`) is only valid when the target is in
                // the same directory as the source.  After a move the target
                // may be in a different directory, so we upgrade to PathRelative
                // (full vault-relative stem) which is always unambiguous.
                new_stem.to_string()
            }
            WrittenForm::MdSuffixed => {
                // Preserve the `.md` suffix on the stem (no directory).
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
    fn rewrite_dot_relative_wikilink_upgrades_to_path_relative() {
        // [[./b]] — dot-relative form. After a mv the target may be in a
        // different directory, so DotRelative upgrades to PathRelative.
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
        assert_eq!(r.new_text, "[[notes/renamed]]");
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
}
