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
use crate::link_target::TargetParts;
use crate::links::{LinkKind, LinkSpan, PreserveForm, WrittenForm};

/// Encode path bytes before inserting them into a link. The catalog decodes once,
/// after splitting authored fragment/query delimiters. Wiki spaces, parentheses,
/// `?`, `%` and `:` stay literal; Markdown destinations additionally encode URL syntax delimiters.
#[must_use]
pub(crate) fn encode_destination(path: &str, kind: LinkKind) -> String {
    use std::fmt::Write as _;
    let mut encoded = String::with_capacity(path.len());
    let chars: Vec<char> = path.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        // A wikilink is a vault path, not a URL (M1, iter-322): `?` and `:`
        // are written literally, and so is a `%` the reader would not take
        // for an escape. Only what a wikilink cannot carry is encoded, plus
        // a `%` that a percent-decoding reader would swallow (`%23`).
        let starts_escape = ch == '%'
            && chars.get(i + 1).is_some_and(char::is_ascii_hexdigit)
            && chars.get(i + 2).is_some_and(char::is_ascii_hexdigit);
        let reserved = ch.is_control()
            || matches!(ch, '#' | '[' | ']' | '|' | '\\')
            || starts_escape
            || (kind == LinkKind::Markdown
                && (ch.is_whitespace()
                    || matches!(ch, '?' | '%' | ':' | '(' | ')' | '<' | '>' | '\"' | '\'')));
        if reserved {
            for byte in ch.to_string().as_bytes() {
                let _ = write!(encoded, "%{byte:02X}");
            }
        } else {
            encoded.push(ch);
        }
    }
    encoded
}

/// The alias separator to write into a wikilink on `line`: `\|` inside a GFM
/// table row, `|` everywhere else.
///
/// An unescaped `|` in a table cell ends the cell, so `[[beta|Beta Doc]]`
/// splits one cell into two (A-3, iteration 321). The reader already treats
/// `[[x\|y]]` as target `x` with label `y`, and `mv`/`links fix` preserve an
/// existing escaped form, so writing it is the matching half. A row is a line
/// whose first non-blank byte, after any block-quote `>` prefixes, is `|`
/// (callouts quote their tables), or a line that carries a `|` and sits next
/// to a delimiter row (`---|:--:`), which is how a table without leading
/// pipes is recognised. `previous` and `next` are the neighbouring lines.
#[must_use]
pub(crate) fn alias_separator(
    line: &str,
    previous: Option<&str>,
    next: Option<&str>,
) -> &'static str {
    let body = strip_quote(line);
    let row = body.trim_start().starts_with('|')
        || (body.contains('|')
            && (previous.is_some_and(is_delimiter_row) || next.is_some_and(is_delimiter_row)));
    if row { "\\|" } else { "|" }
}

fn strip_quote(line: &str) -> &str {
    let mut rest = line.trim_start();
    while let Some(after) = rest.strip_prefix('>') {
        rest = after.trim_start();
    }
    rest
}

/// `| --- | :-: |` or `--- | ---`: only `|`, `-`, `:` and spaces, with at
/// least one `-` and one `|`.
fn is_delimiter_row(line: &str) -> bool {
    let body = strip_quote(line);
    body.contains('-')
        && body.contains('|')
        && body
            .chars()
            .all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t' | '\r'))
}

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

/// How the moved file's **new** path should be spelled in a rewritten link
/// (iter-203).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TargetStyle {
    /// Ordinary file target — emit the new vault path (`bar/note.md`).
    File,
    /// The original link named the *directory* whose `index.md` moved. Emit
    /// the new directory path (`/foo` → `/bar`, not `/bar/index`), keeping the
    /// author's trailing slash when they wrote one.
    ///
    /// Falls back to [`TargetStyle::File`] when the new path is no longer a
    /// directory index — after `mv foo/index.md notes/readme.md` there is no
    /// directory spelling left to preserve.
    DirectoryIndex { trailing_slash: bool },
}

/// The `?query` tail of a wikilink target (`[[note?x=1]]` -> `?x=1`) that a
/// rewrite keeps, or `""`. Dropped when the new target itself holds a `?`,
/// because then the whole of it is the file name and a kept tail would be read
/// as part of it.
fn wikilink_query_tail<'a>(span: &LinkSpan, line: &'a str, new_target: &str) -> &'a str {
    if span.kind != LinkKind::Wikilink || new_target.contains('?') {
        return "";
    }
    let raw = &line[span.target_start..span.target_end];
    &raw[TargetParts::split(raw).path.len()..]
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
        Self::rewrite_styled(
            span,
            line,
            new_vault_rel,
            source_rel,
            policy,
            site_prefix,
            TargetStyle::File,
        )
    }

    /// [`LinkWriter::rewrite`] with an explicit [`TargetStyle`].
    ///
    /// `TargetStyle::DirectoryIndex` keeps a directory-spelled link spelled as
    /// a directory after its `index.md` moves: `[x](/foo)` becomes `[x](/bar)`
    /// rather than `[x](/bar/index)`. Everything else — written form,
    /// fragment, alias, delimiters — is preserved exactly as in `rewrite`.
    pub(crate) fn rewrite_styled(
        span: &LinkSpan,
        line: &str,
        new_vault_rel: &str,
        source_rel: &str,
        policy: PreserveForm,
        site_prefix: Option<&str>,
        style: TargetStyle,
    ) -> Option<SpanReplacement> {
        // A directory-spelled link keeps its spelling only while the new path
        // is still a directory index; otherwise the file form is the only
        // correct target.
        let dir_slash = match style {
            TargetStyle::DirectoryIndex { trailing_slash } => {
                crate::discovery::directory_for_index_file(new_vault_rel)
                    .map(|dir| (dir, trailing_slash))
            }
            TargetStyle::File => None,
        };

        let new_target = match dir_slash {
            Some((new_dir, trailing_slash)) => {
                // Feed the *directory* path through the normal machinery: with
                // no `.md` suffix, `new_stem == new_dir`, so every written form
                // (bare, path-relative, vault-absolute, relative markdown)
                // emits the directory spelling.
                let mut target =
                    Self::compute_new_target(span, line, new_dir, source_rel, policy, site_prefix);
                if trailing_slash && !target.ends_with('/') {
                    target.push('/');
                }
                target
            }
            None => {
                Self::compute_new_target(span, line, new_vault_rel, source_rel, policy, site_prefix)
            }
        };

        let new_target = encode_destination(&new_target, span.kind);
        let old_text = line[span.full_start..span.full_end].to_string();
        // iter-322 (DEC-384): only the path portion of a wikilink target is
        // replaced; the `?query` the reader strips before resolving (`[[note?x=1]]`)
        // stays as written, so the rewritten link resolves exactly as the old one.
        let keep_tail = wikilink_query_tail(span, line, &new_target);
        let new_text = format!(
            "{}{}{}{}",
            &line[span.full_start..span.target_start],
            new_target,
            keep_tail,
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
                let raw_target = &raw_target[..TargetParts::split(raw_target).path.len()];
                let form = detect_wikilink_form(raw_target);
                let mut emitted =
                    Self::emit_wikilink_with_form(form, new_vault_rel, new_stem, source_rel);
                // iter-211 / BUG-12: `detect_wikilink_form` classifies any
                // target containing `/` as `PathRelative`, even when the author
                // wrote `[[foo/index.md]]` — so the `.md` was silently dropped
                // on rewrite, breaking iter-203's spelling guarantee on the
                // tenth form. The suffix is a spelling choice orthogonal to the
                // path shape, so re-apply it whenever the original carried one.
                let raw_had_md = raw_target.len() > 3
                    && raw_target.as_bytes()[raw_target.len() - 3..].eq_ignore_ascii_case(b".md");
                let emitted_has_md = emitted.len() > 3
                    && emitted.as_bytes()[emitted.len() - 3..].eq_ignore_ascii_case(b".md");
                if raw_had_md && !emitted_has_md {
                    emitted.push_str(".md");
                }
                emitted
            }
        }
    }

    /// Emit a wikilink target in the given form.
    pub(crate) fn emit_wikilink_with_form(
        form: WrittenForm,
        _new_vault_rel: &str,
        new_stem: &str,
        source_rel: &str,
    ) -> String {
        let new_basename_stem = new_stem.rsplit('/').next().unwrap_or(new_stem);
        match form {
            WrittenForm::Bare => new_basename_stem.to_string(),
            WrittenForm::PathRelative => new_stem.to_string(),
            WrittenForm::DotRelative | WrittenForm::ParentRelative => {
                // A link authored relative to its note's folder (`[[./foo]]`,
                // `[[../foo]]`) stays relative from wherever either end now
                // sits (DEC-391): the path from the linker's directory to the
                // new target, `./` in front when it does not walk up.
                relative_wikilink_path(source_rel, new_stem)
            }
            WrittenForm::MdSuffixed => {
                // MdSuffixed: preserve the `.md` suffix on the basename only.
                // detect_wikilink_form returns PathRelative for paths containing
                // `/` even with `.md` suffix, so MdSuffixed here means the
                // original was a bare `[[stem.md]]` (no slash). Emit just the
                // basename with the suffix re-applied.
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
        // Mirror presence/absence of the `.md` suffix from the original
        // spelling. iter-211 / BUG-12: this used to be applied only on the
        // site-absolute branch, so an extensionless *relative* link
        // (`[f](foo/index)`) came back from `mv` as `[f](bar/index.md)` —
        // a suffix the author never wrote, breaking iter-203's spelling
        // guarantee on one of its ten forms.
        // Encoded punctuation/letters still express an explicit Markdown suffix.
        let had_md = TargetParts::split(&span.link.target)
            .resolution_path()
            .to_ascii_lowercase()
            .ends_with(".md");
        let styled_target = if had_md { new_vault_rel } else { new_stem };

        if span.link.target.starts_with('/') {
            // Preserve absolute-path style (vault-absolute).
            let target = styled_target;
            // L-11: only re-emit the site prefix when the original spelling
            // carried it. The prefix is often *auto-derived* from the vault
            // directory name, and blindly prepending it turns a working
            // `/foo` into `/my-vault/bar` — a link that resolves nowhere.
            match crate::link_graph::matching_site_prefix(
                span.link.target.trim_start_matches('/'),
                site_prefix,
            ) {
                Some(prefix) => format!("/{prefix}/{target}"),
                None => format!("/{target}"),
            }
        } else {
            relative_path_between(source_rel, styled_target)
        }
    }
}

/// The wikilink path from the folder of `source_rel` to `target_stem` (both
/// vault-relative, forward slashes): `../` once per folder walked up, `./` in
/// front when it does not walk up at all.
pub(crate) fn relative_wikilink_path(source_rel: &str, target_stem: &str) -> String {
    let source_dir: Vec<&str> = source_rel
        .rsplit_once('/')
        .map_or(Vec::new(), |(dir, _)| dir.split('/').collect());
    let target: Vec<&str> = target_stem.split('/').collect();
    let (target_dir, name) = target.split_at(target.len() - 1);
    let common = source_dir
        .iter()
        .zip(target_dir)
        .take_while(|(a, b)| a == b)
        .count();
    let ups = source_dir.len() - common;
    let mut out = if ups == 0 {
        String::from("./")
    } else {
        "../".repeat(ups)
    };
    for part in &target_dir[common..] {
        out.push_str(part);
        out.push('/');
    }
    out.push_str(name[0]);
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alias_separator_escapes_the_pipe_only_in_table_rows() {
        assert_eq!(alias_separator("| [[a]] | x |", None, None), "\\|");
        assert_eq!(alias_separator("   |a|b|", None, None), "\\|");
        assert_eq!(alias_separator("> | [[a]] | x |", None, None), "\\|");
        assert_eq!(alias_separator("text [[a]] here", None, None), "|");
        assert_eq!(alias_separator("- item | pipe", None, None), "|");
        let delim = Some("--- | ---");
        assert_eq!(alias_separator("Beta | x", delim, None), "\\|");
        assert_eq!(alias_separator("Name | Note", None, delim), "\\|");
        assert_eq!(alias_separator("a | b", Some("prose"), Some("more")), "|");
    }
    use crate::links::{LinkKind, extract_link_spans};

    #[test]
    fn encoded_markdown_suffix_remains_explicit() {
        for target in [
            "../guides/topic.v1%2Emd",
            "../guides/topic.v1.%6D%64",
            "../guides/topic.v1%2eMD",
        ] {
            let line = format!("[Topic]({target}#heading)");
            let spans = extract_link_spans(&line);
            let replacement = LinkWriter::rewrite(
                &spans[0],
                &line,
                "guides/new.v1.md",
                "archive/a.md",
                PreserveForm::Preserve,
                None,
            )
            .unwrap();
            assert_eq!(replacement.new_text, "[Topic](../guides/new.v1.md#heading)");
        }
    }

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
    fn rewrite_dot_relative_wikilink_stays_relative_when_linker_nested() {
        // [[./b]] — source at sub/a.md (nested linker). New target moves to
        // other/b.md, which is not below sub/ → the link walks up (DEC-391).
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
        assert_eq!(r.new_text, "[[../other/b]]");
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
    fn rewrite_vault_absolute_does_not_inject_an_unwritten_site_prefix() {
        // L-11: `/notes/old.md` was written without the prefix (which is often
        // auto-derived from the vault directory name); the rewrite must not
        // invent one.
        let line = "See [text](/notes/old.md) here";
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
        assert_eq!(r.new_text, "[text](/notes/renamed.md)");
    }

    #[test]
    fn rewrite_directory_index_style_emits_the_directory_path() {
        let line = "See [text](/foo) here";
        let span = span_from(line, 0);
        let r = LinkWriter::rewrite_styled(
            &span,
            line,
            "bar/index.md",
            "a.md",
            PreserveForm::Preserve,
            None,
            TargetStyle::DirectoryIndex {
                trailing_slash: false,
            },
        )
        .unwrap();
        assert_eq!(r.new_text, "[text](/bar)");
    }

    #[test]
    fn rewrite_directory_index_style_keeps_the_trailing_slash() {
        let line = "See [text](/foo/) here";
        let span = span_from(line, 0);
        let r = LinkWriter::rewrite_styled(
            &span,
            line,
            "bar/index.md",
            "a.md",
            PreserveForm::Preserve,
            None,
            TargetStyle::DirectoryIndex {
                trailing_slash: true,
            },
        )
        .unwrap();
        assert_eq!(r.new_text, "[text](/bar/)");
    }

    #[test]
    fn rewrite_directory_index_style_falls_back_when_target_is_not_an_index() {
        let line = "See [text](/foo) here";
        let span = span_from(line, 0);
        let r = LinkWriter::rewrite_styled(
            &span,
            line,
            "notes/readme.md",
            "a.md",
            PreserveForm::Preserve,
            None,
            TargetStyle::DirectoryIndex {
                trailing_slash: false,
            },
        )
        .unwrap();
        assert_eq!(r.new_text, "[text](/notes/readme)");
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
    // 2. The emitted string, when re-parsed by extract_link_spans, yields the
    //    expected target text (a syntactic round-trip — these tests do not run
    //    vault resolution).

    /// Helper: parse the full link text and return the parsed target as-is
    /// (no fragment stripping, no `.md` normalization — callers compare
    /// against the literal expected target text).
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
        // [[./b]] in notes/, target moves to other/b.md — stays relative (DEC-391)
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
        // notes/ does not prefix other/b → the link walks up one folder
        assert_eq!(r.new_text, "[[../other/b]]");
        assert_eq!(round_trip_target(&r.new_text), "../other/b");
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
