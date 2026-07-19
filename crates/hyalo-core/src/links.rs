#![allow(clippy::missing_errors_doc)]
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// New types for the unified resolver / writer (iter-150)
// ---------------------------------------------------------------------------

/// The syntactic form a user chose when writing a wikilink target.
///
/// Preserved through `mv` and `links fix` so the writer can emit the new
/// target in exactly the same shape the user originally chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrittenForm {
    /// `[[note]]` — bare stem, no directory prefix.
    Bare,
    /// `[[sub/note]]` — vault-relative path (most common for unambiguous refs).
    PathRelative,
    /// `[[./note]]` — explicit current-directory prefix.
    DotRelative,
    /// `[[note.md]]` — bare stem with `.md` suffix.
    MdSuffixed,
    /// `[text](/site/note.md)` — site-absolute path (markdown link only).
    VaultAbsolute,
}

/// How a link target was resolved against the vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// Target resolved to exactly one vault path.
    Hit {
        /// Vault-relative path (forward slashes, `.md` suffix).
        vault_path: String,
    },
    /// Target could not be resolved to any known vault file.
    Broken,
    /// Target matched more than one vault file (ambiguous bare stem).
    Ambiguous(Vec<String>),
}

/// Policy for how `LinkWriter` should emit the new target text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreserveForm {
    /// Re-emit using the same `WrittenForm` the user originally used.
    Preserve,
    /// Always emit as a bare stem (Obsidian short-form).
    Bare,
}

/// A parsed link extracted from a markdown file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    /// Raw target: note name or relative path (without fragment)
    pub target: String,
    /// Display text from `[[target|label]]` or `[label](target)`
    pub label: Option<String>,
    /// The kind of link syntax used in the source text.
    pub kind: LinkKind,
}

/// The kind of link syntax used in the source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkKind {
    Wikilink,
    Markdown,
}

/// A parsed link together with its byte-offset span within the source text.
///
/// All offsets are byte positions into the original `&str` passed to
/// [`extract_link_spans`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LinkSpan {
    /// The resolved link (target without fragment, plus optional label).
    pub link: Link,
    /// Syntax kind (wikilink or markdown).
    pub kind: LinkKind,
    /// Byte offset of the first byte of the target text (i.e. the text that
    /// `link.target` was derived from, before the `#fragment` was stripped).
    pub target_start: usize,
    /// Byte offset one past the last byte of the target text (stops at `#`,
    /// `|`, `]]`, or `)` depending on what follows the target).
    pub target_end: usize,
    /// Byte offset of the opening `!`, `[`, depending on link kind/embed.
    pub full_start: usize,
    /// Byte offset one past the closing `]]` or `)`.
    pub full_end: usize,
}

/// Extract links from a text segment and append them to `out`.
///
/// `text` must already be cleaned of inline code spans (e.g. via
/// [`strip_inline_code`](crate::scanner::strip_inline_code)), otherwise links
/// inside code spans will be incorrectly parsed. Existing contents of `out` are
/// preserved; new links are appended.
///
/// Link labels are read from `text`. If the caller has a raw (un-stripped)
/// version of the same line with the same byte layout, use
/// [`extract_links_from_text_with_original`] to preserve backtick-wrapped
/// label content.
pub fn extract_links_from_text(text: &str, out: &mut Vec<Link>) {
    extract_links_from_text_with_original(text, text, out);
}

/// Like [`extract_links_from_text`] but reads link label text from `original`
/// instead of `cleaned`.
///
/// Use this when `cleaned` has had inline code spans replaced with spaces (via
/// [`strip_inline_code`](crate::scanner::strip_inline_code)) to avoid
/// mistaking links inside code spans as real links, while still preserving the
/// backtick-wrapped content in link labels such as `` [`file.ts`](path) ``.
///
/// `cleaned` and `original` must describe the same line with identical byte
/// lengths and identical byte positions for all link syntax characters (`[`,
/// `]`, `(`, `)`).
pub(crate) fn extract_links_from_text_with_original(
    cleaned: &str,
    original: &str,
    out: &mut Vec<Link>,
) {
    let bytes = cleaned.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Check for wikilink: ![[...]] or [[...]]
        //
        // L-16: a backslash-escaped opener (`\[[…]]`, `\![[…]]`) is literal text
        // per CommonMark / Obsidian and must NOT be extracted.
        if bytes[i] == b'!'
            && i + 3 < len
            && bytes[i + 1] == b'['
            && bytes[i + 2] == b'['
            && !is_escaped(bytes, i)
            && let Some((link, end)) = try_parse_wikilink_at(cleaned, i + 1)
        {
            out.push(link);
            i = end;
            continue;
        }
        if bytes[i] == b'['
            && i + 1 < len
            && bytes[i + 1] == b'['
            && !is_escaped(bytes, i)
            && let Some((link, end)) = try_parse_wikilink_at(cleaned, i)
        {
            out.push(link);
            i = end;
            continue;
        }

        // Check for markdown link: [text](target)
        // Skip if preceded by `!` — that's image syntax: ![alt](img.png)
        // L-16: skip when the `[` is backslash-escaped.
        if bytes[i] == b'['
            && (i == 0 || bytes[i - 1] != b'!')
            && !is_escaped(bytes, i)
            && let Some((link, end)) = try_parse_markdown_link_at(cleaned, original, i)
        {
            out.push(link);
            i = end;
            continue;
        }

        i += 1;
    }
}

/// Whether the byte at `pos` is backslash-escaped, i.e. preceded by an odd
/// number of consecutive `\` bytes (CommonMark / Obsidian escaping).
///
/// `\[[foo]]` → the `[` is escaped (one backslash), so the link is literal.
/// `\\[[foo]]` → two backslashes render as one literal `\`, the `[` is *not*
/// escaped, so the link is real. Used by both extraction paths (L-16).
fn is_escaped(bytes: &[u8], pos: usize) -> bool {
    let mut backslashes = 0usize;
    let mut j = pos;
    while j > 0 && bytes[j - 1] == b'\\' {
        backslashes += 1;
        j -= 1;
    }
    backslashes % 2 == 1
}

/// Find the byte offset (relative to `s`) of the closing `]` that terminates
/// a markdown link label, skipping over backslash-escaped `\]`/`\[` (L-A2).
///
/// Unlike a plain `s.find(']')`, this does not stop early on labels like
/// `[Contains \[test\] brackets](dest.md)` — the escaped brackets are part of
/// the label text, not delimiters. Returns `None` if no unescaped `]` exists.
fn find_label_close_bracket(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b']' if !is_escaped(bytes, i) => return Some(i),
            _ => i += 1,
        }
    }
    None
}

/// Result of parsing a markdown link destination starting right after `(`.
struct ParsedDestination<'a> {
    /// The raw target text (angle brackets stripped if the destination used
    /// the `<...>` form; verbatim otherwise).
    target_raw: &'a str,
    /// Byte offset (relative to the start of the destination, i.e. right
    /// after `(`) of the first byte past the destination (and any title),
    /// up to and including the closing `)`.
    end: usize,
}

/// Parse a markdown link destination starting at `rest` (the text right
/// after the opening `(`).
///
/// Handles both bare destinations (`dest.md`, up to the first `)`) and
/// CommonMark angle-bracket destinations (`<my dest.md>`, which may contain
/// spaces and literal `)`), per L-A1. For the angle form, the destination is
/// closed by the first unescaped `>`; anything between it and the closing
/// `)` (e.g. a `"title"`) is ignored since this file does not otherwise
/// track link titles. Returns `None` if no closing `)` can be found for the
/// destination.
fn parse_destination(rest: &str) -> Option<ParsedDestination<'_>> {
    let bytes = rest.as_bytes();
    if bytes.first() == Some(&b'<') {
        // Angle-bracket destination: scan for the first unescaped `>`.
        let mut i = 1;
        let mut close_angle = None;
        while i < bytes.len() {
            match bytes[i] {
                b'>' if !is_escaped(bytes, i) => {
                    close_angle = Some(i);
                    break;
                }
                _ => i += 1,
            }
        }
        let close_angle = close_angle?;
        let target_raw = &rest[1..close_angle];

        // Whatever follows `>` up to `)` (whitespace, optional title) is not
        // part of the target; just locate the closing `)`.
        let after_angle = &rest[close_angle + 1..];
        let close_paren = after_angle.find(')')?;
        let end = close_angle + 1 + close_paren + 1;

        Some(ParsedDestination { target_raw, end })
    } else {
        // Bare destination: up to the first `)`.
        let close_paren = rest.find(')')?;
        Some(ParsedDestination {
            target_raw: &rest[..close_paren],
            end: close_paren + 1,
        })
    }
}

/// Extract all internal links with byte-offset spans from a text segment.
///
/// Works exactly like [`extract_links_from_text`] but returns [`LinkSpan`]
/// values that carry byte positions for both the full link syntax and the
/// target substring.  `text` must already be cleaned of inline code spans.
///
/// Link labels are read from `text`. If the caller has a raw (un-stripped)
/// version of the same line with the same byte layout, use
/// [`extract_link_spans_with_original`] to preserve backtick-wrapped label
/// content.
#[allow(dead_code)] // Used in tests only
pub(crate) fn extract_link_spans(text: &str) -> Vec<LinkSpan> {
    extract_link_spans_with_original(text, text)
}

/// Like [`extract_link_spans`] but reads link label text from `original`
/// instead of `cleaned`.
///
/// Use this when `cleaned` has had inline code spans replaced with spaces (via
/// [`strip_inline_code`](crate::scanner::strip_inline_code)) to avoid
/// mistaking links inside code spans as real links, while still preserving
/// backtick-wrapped content in link labels such as `` [`file.ts`](path) ``.
///
/// `cleaned` and `original` must describe the same line with identical byte
/// lengths and identical byte positions for all link syntax characters (`[`,
/// `]`, `(`, `)`).
pub(crate) fn extract_link_spans_with_original(cleaned: &str, original: &str) -> Vec<LinkSpan> {
    let bytes = cleaned.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut out = Vec::new();

    while i < len {
        // ![[embed]] — full_start is at the `!`
        // L-16: skip a backslash-escaped opener.
        if bytes[i] == b'!'
            && i + 3 < len
            && bytes[i + 1] == b'['
            && bytes[i + 2] == b'['
            && !is_escaped(bytes, i)
            && let Some((mut span, end)) = try_parse_wikilink_span_at(cleaned, i + 1)
        {
            // Extend full_start back to the `!`
            span.full_start = i;
            out.push(span);
            i = end;
            continue;
        }

        // [[wikilink]]
        if bytes[i] == b'['
            && i + 1 < len
            && bytes[i + 1] == b'['
            && !is_escaped(bytes, i)
            && let Some((span, end)) = try_parse_wikilink_span_at(cleaned, i)
        {
            out.push(span);
            i = end;
            continue;
        }

        // [text](target) — skip if preceded by `!` (image)
        // L-16: skip when the `[` is backslash-escaped.
        if bytes[i] == b'['
            && (i == 0 || bytes[i - 1] != b'!')
            && !is_escaped(bytes, i)
            && let Some((span, end)) = try_parse_markdown_link_span_at(cleaned, original, i)
        {
            out.push(span);
            i = end;
            continue;
        }

        i += 1;
    }

    out
}

/// Try to parse a wikilink span starting at `start` (the first `[` of `[[`).
/// Returns the [`LinkSpan`] and the byte position after the closing `]]`.
fn try_parse_wikilink_span_at(text: &str, start: usize) -> Option<(LinkSpan, usize)> {
    let content_start = start + 2; // skip [[
    let rest = &text[content_start..];

    let close = rest.find("]]")?;
    let inner = &rest[..close];

    if inner.is_empty() || inner.contains('\n') {
        return None;
    }

    // Determine where the target text ends within `inner`.
    // target ends at `|` (alias) or `#` (fragment), whichever comes first.
    let target_end_in_inner = [inner.find('|'), inner.find('#')]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(inner.len());

    let target_part = &inner[..target_end_in_inner];

    // Reuse existing logic to validate and strip the fragment from target_part.
    // We call parse_wikilink on `inner` to get the full Link (handles alias etc.).
    let link = parse_wikilink(inner)?;

    let full_end = content_start + close + 2;

    Some((
        LinkSpan {
            link,
            kind: LinkKind::Wikilink,
            target_start: content_start,
            target_end: content_start + target_part.len(),
            full_start: start,
            full_end,
        },
        full_end,
    ))
}

/// Try to parse a markdown link span `[text](target)` at byte position `start`
/// (the `[`).  Returns the [`LinkSpan`] and the byte position after `)`.
///
/// `text` drives structural parsing; `original` provides the label text so
/// that backtick-wrapped content is preserved when `text` has been
/// inline-code-stripped.
fn try_parse_markdown_link_span_at(
    text: &str,
    original: &str,
    start: usize,
) -> Option<(LinkSpan, usize)> {
    let rest = &text[start..];

    // L-A2: skip escaped `\]`/`\[` so labels like
    // `[Contains \[test\] brackets]` don't terminate the scan early.
    let close_bracket = find_label_close_bracket(rest)?;
    // Read label from `original` so backtick-wrapped content is not lost when
    // `text` has had inline code spans replaced with spaces.
    // Use `.get()` to avoid panic if `original` has a different byte layout.
    let label_text = original.get(start + 1..start + close_bracket)?;

    let after_bracket = start + close_bracket + 1;
    if text.as_bytes().get(after_bracket).copied() != Some(b'(') {
        return None;
    }

    let paren_start = after_bracket + 1; // first byte after `(`
    let rest_after_paren = &text[paren_start..];
    // L-A1: handle both bare and angle-bracket (`<my dest.md>`) destinations.
    let dest = parse_destination(rest_after_paren)?;
    let target_raw = dest.target_raw;

    if is_external(target_raw) || target_raw.is_empty() {
        return None;
    }

    let link = parse_markdown_link(label_text, target_raw)?;

    // target_end stops at `#` if a fragment is present, otherwise at the end
    // of the (unwrapped) target text. `target_start` is offset past the `<`
    // when the angle form was used, so the writer's splice naturally
    // preserves the angle brackets around a rewritten target.
    let target_start = if text.as_bytes().get(paren_start).copied() == Some(b'<') {
        paren_start + 1
    } else {
        paren_start
    };
    let target_end_in_raw = target_raw.find('#').unwrap_or(target_raw.len());

    let full_end = paren_start + dest.end;

    Some((
        LinkSpan {
            link,
            kind: LinkKind::Markdown,
            target_start,
            target_end: target_start + target_end_in_raw,
            full_start: start,
            full_end,
        },
        full_end,
    ))
}

/// Try to parse a wikilink starting at position `start` (the first `[`).
/// Returns the parsed Link and the position after the closing `]]`.
fn try_parse_wikilink_at(text: &str, start: usize) -> Option<(Link, usize)> {
    // start points to first `[`, start+1 is second `[`
    let content_start = start + 2;
    let rest = &text[content_start..];

    // Find closing ]]
    let close = rest.find("]]")?;
    let inner = &rest[..close];

    // Reject empty or multiline
    if inner.is_empty() || inner.contains('\n') {
        return None;
    }

    let link = parse_wikilink(inner)?;
    let end_pos = content_start + close + 2;
    Some((link, end_pos))
}

/// Parse the inner content of a wikilink (between [[ and ]]).
/// Handles: target, target|label, target#heading, target#^block-id
#[must_use]
pub(crate) fn parse_wikilink(inner: &str) -> Option<Link> {
    if inner.is_empty() {
        return None;
    }

    // Split on pipe for label text
    let (target_part, label) = if let Some(pipe_pos) = inner.find('|') {
        (&inner[..pipe_pos], Some(inner[pipe_pos + 1..].to_string()))
    } else {
        (inner, None)
    };

    // Strip fragment (heading/block ref) — not surfaced in output
    let target = strip_fragment(target_part);

    // Fragment-only links like [[#heading]] are same-file heading links, not file links
    if target.is_empty() {
        return None;
    }

    // Obsidian compatibility: strip a trailing `.md` (case-insensitive) from
    // wikilink targets so that `[[foo.md]]` resolves identically to `[[foo]]`.
    // Obsidian itself allows but ignores the `.md` suffix; without this strip
    // hyalo would flag links written with the suffix as broken.
    let target = strip_wikilink_md_suffix(target);

    Some(Link {
        target: target.to_string(),
        label,
        kind: LinkKind::Wikilink,
    })
}

/// Strip a trailing `.md` (case-insensitive) from a wikilink target.
///
/// Only removes the suffix when it is preceded by at least one character
/// (prevents turning `.md` alone into an empty string).
/// Markdown link targets are intentionally excluded — they require `.md`.
pub(crate) fn strip_wikilink_md_suffix(target: &str) -> &str {
    if target.len() > 3 {
        let split_at = target.len() - 3;
        // The last three bytes form `.md` (case-insensitive) only when they
        // are all ASCII. Slicing the string with `&target[split_at..]` can
        // panic for non-ASCII targets when `split_at` falls inside a
        // multi-byte char, so compare bytes first.
        let last3 = &target.as_bytes()[split_at..];
        if last3.eq_ignore_ascii_case(b".md") {
            // ASCII `.md` bytes imply a char boundary at `split_at`.
            return &target[..split_at];
        }
    }
    target
}

/// Try to parse a markdown-style link [text](target) at position `start`.
///
/// `text` drives structural parsing; `original` provides the label text so
/// that backtick-wrapped content is preserved when `text` has been
/// inline-code-stripped.
fn try_parse_markdown_link_at(text: &str, original: &str, start: usize) -> Option<(Link, usize)> {
    let rest = &text[start..];

    // Find the closing ] (L-A2: skip escaped `\]`/`\[` so labels like
    // `[Contains \[test\] brackets]` don't terminate the scan early).
    let close_bracket = find_label_close_bracket(rest)?;
    // Read label from `original` so backtick-wrapped content is not lost when
    // `text` has had inline code spans replaced with spaces.
    // Use `.get()` to avoid panic if `original` has a different byte layout.
    let label_text = original.get(start + 1..start + close_bracket)?;

    // Must be immediately followed by (
    let after_bracket = start + close_bracket + 1;
    if text.as_bytes().get(after_bracket).copied() != Some(b'(') {
        return None;
    }

    // Parse the destination, handling both bare and angle-bracket
    // (`<my dest.md>`) forms (L-A1).
    let paren_start = after_bracket + 1;
    let rest_after_paren = &text[paren_start..];
    let dest = parse_destination(rest_after_paren)?;
    let target_raw = dest.target_raw;

    // Skip external links
    if is_external(target_raw) {
        return None;
    }

    // Skip empty targets
    if target_raw.is_empty() {
        return None;
    }

    let link = parse_markdown_link(label_text, target_raw)?;
    let end_pos = paren_start + dest.end;
    Some((link, end_pos))
}

/// Parse a markdown link's label text and target into a Link.
#[must_use]
pub(crate) fn parse_markdown_link(label_text: &str, target_raw: &str) -> Option<Link> {
    if target_raw.is_empty() {
        return None;
    }

    if is_external(target_raw) {
        return None;
    }

    // Strip fragment (heading/block ref) — not surfaced in output
    let target = strip_fragment(target_raw);

    // Fragment-only links like [text](#heading) are same-file heading links, not file links
    if target.is_empty() {
        return None;
    }

    Some(Link {
        target: target.to_string(),
        label: if label_text.is_empty() {
            None
        } else {
            Some(label_text.to_string())
        },
        kind: LinkKind::Markdown,
    })
}

/// Check if a target is an external link (http, https, mailto).
///
/// L-20: compares scheme prefixes with `eq_ignore_ascii_case` on borrowed
/// slices instead of allocating a lowercased copy of the whole target for
/// every candidate.
fn is_external(target: &str) -> bool {
    fn has_prefix_ci(target: &str, prefix: &str) -> bool {
        target.len() >= prefix.len()
            && target.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
    }
    has_prefix_ci(target, "http://")
        || has_prefix_ci(target, "https://")
        || has_prefix_ci(target, "mailto:")
}

/// Strip the fragment (#heading or #^block-id) from a target string,
/// returning only the base target name.
fn strip_fragment(target: &str) -> &str {
    target.split('#').next().unwrap_or(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- .md suffix stripping (Obsidian compatibility) ---

    #[test]
    fn strip_wikilink_md_suffix_plain() {
        assert_eq!(strip_wikilink_md_suffix("foo.md"), "foo");
        assert_eq!(strip_wikilink_md_suffix("foo.MD"), "foo");
        assert_eq!(strip_wikilink_md_suffix("foo.Md"), "foo");
    }

    #[test]
    fn strip_wikilink_md_suffix_path() {
        assert_eq!(strip_wikilink_md_suffix("path/foo.md"), "path/foo");
        assert_eq!(strip_wikilink_md_suffix("a/b/c.md"), "a/b/c");
    }

    #[test]
    fn strip_wikilink_md_suffix_no_suffix() {
        assert_eq!(strip_wikilink_md_suffix("foo"), "foo");
        assert_eq!(strip_wikilink_md_suffix("foo.txt"), "foo.txt");
    }

    #[test]
    fn strip_wikilink_md_suffix_too_short() {
        // ".md" alone (3 chars) should not be stripped
        assert_eq!(strip_wikilink_md_suffix(".md"), ".md");
        // "x.md" is 4 chars, should be stripped
        assert_eq!(strip_wikilink_md_suffix("x.md"), "x");
    }

    #[test]
    fn strip_wikilink_md_suffix_non_ascii_no_panic() {
        // Multi-byte chars whose bytes straddle `len-3` must not panic.
        // "ab🎉" is 6 bytes; len-3 = 3 falls inside the emoji.
        assert_eq!(strip_wikilink_md_suffix("ab🎉"), "ab🎉");
        // Likewise for a trailing 2-byte char without .md.
        assert_eq!(strip_wikilink_md_suffix("café"), "café");
        // Non-ASCII followed by a real .md suffix still strips correctly.
        assert_eq!(strip_wikilink_md_suffix("café.md"), "café");
    }

    #[test]
    fn parse_wikilink_with_md_suffix() {
        // [[foo.md]] resolves identically to [[foo]]
        let link = parse_wikilink("foo.md").unwrap();
        assert_eq!(link.target, "foo");
        assert_eq!(link.label, None);
    }

    #[test]
    fn parse_wikilink_path_with_md_suffix() {
        // [[path/foo.md]] resolves identically to [[path/foo]]
        let link = parse_wikilink("path/foo.md").unwrap();
        assert_eq!(link.target, "path/foo");
    }

    #[test]
    fn parse_wikilink_md_suffix_with_fragment() {
        // [[foo.md#heading]] — .md stripped, heading preserved
        let link = parse_wikilink("foo.md#heading").unwrap();
        assert_eq!(link.target, "foo");
    }

    #[test]
    fn parse_wikilink_md_suffix_with_alias() {
        // [[foo.md|alias]] — .md stripped, alias preserved
        let link = parse_wikilink("foo.md|my alias").unwrap();
        assert_eq!(link.target, "foo");
        assert_eq!(link.label.as_deref(), Some("my alias"));
    }

    #[test]
    fn parse_wikilink_md_suffix_in_full_text() {
        // Verify extract_links_from_text handles [[foo.md]] correctly
        let text = "See [[foo.md]] and [[bar.md#sec]] and [[baz.md|title]] here.";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert_eq!(links.len(), 3);
        assert_eq!(links[0].target, "foo");
        assert_eq!(links[1].target, "bar");
        assert_eq!(links[2].target, "baz");
        assert_eq!(links[2].label.as_deref(), Some("title"));
    }

    // --- L-16: backslash escape suppresses extraction ---

    #[test]
    fn escaped_wikilink_not_extracted() {
        let text = r"prefix \[[not-a-link]] suffix";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert!(links.is_empty(), "escaped [[…]] must not be extracted");
    }

    #[test]
    fn escaped_embed_wikilink_not_extracted() {
        // Backslash before the `[[` of an embed suppresses the whole link.
        let text = r"!\[[embed]]";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert!(links.is_empty(), "escaped [[…]] must not be extracted");
    }

    #[test]
    fn escaping_only_the_bang_still_yields_a_link() {
        // `\![[embed]]` escapes only the `!`; the `[[embed]]` after it is a
        // normal (non-embed) wikilink and is still extracted.
        let text = r"\![[embed]]";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "embed");
    }

    #[test]
    fn escaped_markdown_link_not_extracted() {
        let text = r"see \[label](note.md) here";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert!(links.is_empty(), "escaped [text](…) must not be extracted");
    }

    #[test]
    fn double_backslash_before_wikilink_is_real() {
        // `\\` renders as a literal backslash, so the `[` is NOT escaped and the
        // link is genuine.
        let text = r"x \\[[real]] y";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "real");
    }

    #[test]
    fn triple_backslash_before_wikilink_is_escaped() {
        // `\\\` = literal backslash + escape, so the `[` IS escaped.
        let text = r"x \\\[[nope]] y";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert!(links.is_empty());
    }

    #[test]
    fn escaped_link_leaves_later_real_link_intact() {
        let text = r"\[[escaped]] but [[real]] here";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "real");
    }

    #[test]
    fn escaped_wikilink_span_not_extracted() {
        let text = r"a \[[nope]] b";
        let spans = extract_link_spans(text);
        assert!(spans.is_empty());
    }

    #[test]
    fn escaped_markdown_link_span_not_extracted() {
        let text = r"a \[t](x.md) b";
        let spans = extract_link_spans(text);
        assert!(spans.is_empty());
    }

    #[test]
    fn parse_simple_wikilink() {
        let link = parse_wikilink("Note").unwrap();
        assert_eq!(link.target, "Note");
        assert_eq!(link.label, None);
    }

    #[test]
    fn parse_wikilink_with_label() {
        let link = parse_wikilink("Note|My Display").unwrap();
        assert_eq!(link.target, "Note");
        assert_eq!(link.label.as_deref(), Some("My Display"));
    }

    #[test]
    fn parse_wikilink_with_heading_strips_fragment() {
        let link = parse_wikilink("Note#Section").unwrap();
        assert_eq!(link.target, "Note");
    }

    #[test]
    fn parse_wikilink_with_block_ref_strips_fragment() {
        let link = parse_wikilink("Note#^abc123").unwrap();
        assert_eq!(link.target, "Note");
    }

    #[test]
    fn parse_wikilink_heading_and_label() {
        let link = parse_wikilink("Note#Section|display").unwrap();
        assert_eq!(link.target, "Note");
        assert_eq!(link.label.as_deref(), Some("display"));
    }

    #[test]
    fn parse_embed_wikilink() {
        let link = parse_wikilink("image.png").unwrap();
        assert_eq!(link.target, "image.png");
    }

    #[test]
    fn parse_empty_wikilink_returns_none() {
        assert!(parse_wikilink("").is_none());
    }

    #[test]
    fn parse_simple_markdown_link() {
        let link = parse_markdown_link("click here", "note.md").unwrap();
        assert_eq!(link.target, "note.md");
        assert_eq!(link.label.as_deref(), Some("click here"));
    }

    #[test]
    fn parse_markdown_link_with_heading_strips_fragment() {
        let link = parse_markdown_link("text", "note.md#section").unwrap();
        assert_eq!(link.target, "note.md");
    }

    #[test]
    fn parse_markdown_link_with_path() {
        let link = parse_markdown_link("text", "sub/dir/note.md").unwrap();
        assert_eq!(link.target, "sub/dir/note.md");
    }

    #[test]
    fn parse_markdown_link_skips_http() {
        assert!(parse_markdown_link("text", "https://example.com").is_none());
        assert!(parse_markdown_link("text", "http://example.com").is_none());
        assert!(parse_markdown_link("text", "mailto:foo@bar.com").is_none());
    }

    #[test]
    fn parse_markdown_link_empty_target() {
        assert!(parse_markdown_link("text", "").is_none());
    }

    #[test]
    fn extract_wikilinks_from_text() {
        let text = "See [[Note A]] and [[Note B|display]]";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target, "Note A");
        assert_eq!(links[1].target, "Note B");
        assert_eq!(links[1].label.as_deref(), Some("display"));
    }

    #[test]
    fn extract_embed_from_text() {
        let text = "![[embedded note]]";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "embedded note");
    }

    #[test]
    fn extract_markdown_link_from_text() {
        let text = "See [my note](notes/foo.md) for details";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "notes/foo.md");
        assert_eq!(links[0].label.as_deref(), Some("my note"));
    }

    #[test]
    fn external_markdown_links_skipped() {
        let text = "[Google](https://google.com) and [[internal]]";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "internal");
    }

    #[test]
    fn multiple_links_on_one_line() {
        let text = "[[A]] then [b](b.md) then [[C#heading]]";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert_eq!(links.len(), 3);
        assert_eq!(links[0].target, "A");
        assert_eq!(links[1].target, "b.md");
        assert_eq!(links[2].target, "C"); // fragment stripped
    }

    #[test]
    fn extract_links_from_text_with_block_ref() {
        let text = "[[Note#^abc123]]";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "Note"); // fragment stripped
    }

    #[test]
    fn unclosed_wikilink_skipped() {
        let text = "See [[broken and more text";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert!(links.is_empty());
    }

    #[test]
    fn unclosed_markdown_link_skipped() {
        let text = "See [text](broken and more";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert!(links.is_empty());
    }

    #[test]
    fn empty_wikilink_label() {
        // [[target|]] — pipe present but label is empty string
        let link = parse_wikilink("target|").unwrap();
        assert_eq!(link.target, "target");
        assert_eq!(link.label, Some(String::new()));
    }

    #[test]
    fn empty_markdown_display() {
        // [](note.md) — empty display text becomes None label
        let link = parse_markdown_link("", "note.md").unwrap();
        assert_eq!(link.target, "note.md");
        assert_eq!(link.label, None);
    }

    #[test]
    fn nested_brackets_wikilink() {
        // [[outer [[inner]]]] — the parser finds the first ]] closing "outer [[inner",
        // so "inner" is parsed as the target after the second [[, stopping at the first ]]
        let text = "[[outer [[inner]]]]";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        // The outer [[ is tried first; rest is "outer [[inner]]]]",
        // find("]]") hits the first ]] → inner = "outer [[inner" → no pipe → target = "outer [[inner"
        // (fragment strip on # only; this is the pinned behavior)
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "outer [[inner");
    }

    #[test]
    fn wikilink_only_fragment() {
        // [[#heading]] — same-file heading link, not a file link
        assert!(parse_wikilink("#heading").is_none());
    }

    #[test]
    fn markdown_link_only_fragment() {
        // [text](#heading) — same-file heading link, not a file link
        assert!(parse_markdown_link("text", "#heading").is_none());
    }

    #[test]
    fn markdown_image_skipped() {
        let text = "![alt text](image.png) and [[real link]]";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "real link");
    }

    // --- LinkSpan / extract_link_spans tests ---

    #[test]
    fn span_simple_wikilink() {
        let text = "See [[Note]] here";
        let spans = extract_link_spans(text);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].link.target, "Note");
        assert_eq!(spans[0].kind, LinkKind::Wikilink);
        assert_eq!(&text[spans[0].target_start..spans[0].target_end], "Note");
        assert_eq!(&text[spans[0].full_start..spans[0].full_end], "[[Note]]");
    }

    #[test]
    fn span_wikilink_with_alias() {
        let text = "[[target|display text]]";
        let spans = extract_link_spans(text);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].link.target, "target");
        assert_eq!(&text[spans[0].target_start..spans[0].target_end], "target");
        assert_eq!(
            &text[spans[0].full_start..spans[0].full_end],
            "[[target|display text]]"
        );
    }

    #[test]
    fn span_wikilink_with_fragment() {
        let text = "[[note#heading]]";
        let spans = extract_link_spans(text);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].link.target, "note");
        assert_eq!(&text[spans[0].target_start..spans[0].target_end], "note");
    }

    #[test]
    fn span_wikilink_with_fragment_and_alias() {
        let text = "[[note#section|display]]";
        let spans = extract_link_spans(text);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].link.target, "note");
        assert_eq!(&text[spans[0].target_start..spans[0].target_end], "note");
    }

    #[test]
    fn span_embed_wikilink() {
        let text = "![[embedded]]";
        let spans = extract_link_spans(text);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].kind, LinkKind::Wikilink);
        assert_eq!(
            &text[spans[0].full_start..spans[0].full_end],
            "![[embedded]]"
        );
        assert_eq!(
            &text[spans[0].target_start..spans[0].target_end],
            "embedded"
        );
    }

    #[test]
    fn span_markdown_link() {
        let text = "See [click](notes/foo.md) here";
        let spans = extract_link_spans(text);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].kind, LinkKind::Markdown);
        assert_eq!(spans[0].link.target, "notes/foo.md");
        assert_eq!(
            &text[spans[0].target_start..spans[0].target_end],
            "notes/foo.md"
        );
        assert_eq!(
            &text[spans[0].full_start..spans[0].full_end],
            "[click](notes/foo.md)"
        );
    }

    #[test]
    fn span_markdown_link_with_fragment() {
        let text = "[text](note.md#section)";
        let spans = extract_link_spans(text);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].link.target, "note.md");
        assert_eq!(&text[spans[0].target_start..spans[0].target_end], "note.md");
    }

    #[test]
    fn span_multiple_links() {
        let text = "[[A]] then [b](b.md) then [[C]]";
        let spans = extract_link_spans(text);
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].kind, LinkKind::Wikilink);
        assert_eq!(spans[1].kind, LinkKind::Markdown);
        assert_eq!(spans[2].kind, LinkKind::Wikilink);
    }

    #[test]
    fn span_external_link_skipped() {
        let text = "[Google](https://google.com) and [[internal]]";
        let spans = extract_link_spans(text);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].link.target, "internal");
    }

    #[test]
    fn span_image_skipped() {
        let text = "![alt](image.png) and [[real]]";
        let spans = extract_link_spans(text);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].link.target, "real");
    }

    #[test]
    fn span_fragment_only_skipped() {
        let text = "[[#heading]] and [[real]]";
        let spans = extract_link_spans(text);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].link.target, "real");
    }

    // --- Backtick-wrapped label preservation ---

    /// Regression: a label like [`lib/frontmatter.ts`](path) was producing
    /// all-whitespace label text when the line had been run through
    /// `strip_inline_code` (which replaces backtick span content with spaces).
    /// `extract_links_from_text_with_original` fixes this by reading the label
    /// from the original (un-stripped) line.
    #[test]
    fn markdown_link_backtick_label_preserved_with_original() {
        use crate::scanner::strip_inline_code;

        let original = "[`lib/frontmatter.ts`](/src/frame/lib/frontmatter.ts)";
        let cleaned = strip_inline_code(original);

        // Sanity-check: strip_inline_code should have replaced the backtick
        // span content with spaces, so `cleaned` should not equal `original`.
        assert_ne!(cleaned.as_ref(), original);

        // Without original: label is whitespace (the bug).
        let mut links_no_orig = Vec::new();
        extract_links_from_text(cleaned.as_ref(), &mut links_no_orig);
        assert_eq!(links_no_orig.len(), 1);
        // The label is all spaces — document the broken behavior for contrast.
        assert!(
            links_no_orig[0]
                .label
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty(),
            "without original the label should be whitespace (confirming the bug)"
        );

        // With original: label is the backtick-wrapped text (the fix).
        let mut links_with_orig = Vec::new();
        extract_links_from_text_with_original(cleaned.as_ref(), original, &mut links_with_orig);
        assert_eq!(links_with_orig.len(), 1);
        assert_eq!(
            links_with_orig[0].label.as_deref(),
            Some("`lib/frontmatter.ts`"),
            "label should preserve the backtick-wrapped content"
        );
        assert_eq!(links_with_orig[0].target, "/src/frame/lib/frontmatter.ts");
    }

    #[test]
    fn markdown_link_backtick_label_span_preserved_with_original() {
        use crate::scanner::strip_inline_code;

        let original = "See [`file.ts`](src/file.ts) for details";
        let cleaned = strip_inline_code(original);

        let spans = extract_link_spans_with_original(cleaned.as_ref(), original);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].link.target, "src/file.ts");
        assert_eq!(
            spans[0].link.label.as_deref(),
            Some("`file.ts`"),
            "span label should preserve backtick-wrapped content"
        );
    }

    #[test]
    fn extract_links_from_text_backtick_label_without_strip_preserved() {
        // When the text has NOT been stripped (e.g. raw line from file),
        // backtick labels should pass through correctly via the regular path.
        let text = "[`mod.rs`](src/mod.rs)";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].label.as_deref(), Some("`mod.rs`"));
        assert_eq!(links[0].target, "src/mod.rs");
    }

    // --- L-A1: angle-bracket destinations ---

    #[test]
    fn angle_bracket_destination_with_spaces_strips_brackets() {
        let text = "[spaced link](<my dest.md>)";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "my dest.md");
        assert_eq!(links[0].label.as_deref(), Some("spaced link"));
    }

    #[test]
    fn angle_bracket_destination_without_spaces_still_works() {
        let text = "[link](<dest.md>)";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "dest.md");
    }

    #[test]
    fn angle_bracket_destination_empty_does_not_panic() {
        // `<>` — empty angle destination. Falls through to the standard
        // empty-target rejection (mirrors bare `()`), so no link is
        // extracted, but parsing must not panic.
        let text = "[link](<>)";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert!(links.is_empty(), "empty angle destination yields no link");
    }

    #[test]
    fn angle_bracket_destination_unclosed_does_not_panic() {
        // No matching `>` — not parseable as an angle destination. There is
        // no closing `)` before end-of-string either (the `)` that follows
        // `dest.md` is inside the unterminated `<...`), so this also fails to
        // parse as a link at all, consistent with the rest of this file
        // treating unparseable link syntax as "no link" rather than a panic
        // or a partial/garbled match.
        let text = "[link](<dest.md) trailing";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert!(
            links.is_empty(),
            "unclosed angle destination must not panic and must not parse as a link"
        );
    }

    #[test]
    fn angle_bracket_destination_with_fragment() {
        let text = "[link](<my dest.md#heading>)";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "my dest.md");
    }

    #[test]
    fn angle_bracket_destination_span_target_excludes_brackets() {
        let text = "[spaced link](<my dest.md>)";
        let spans = extract_link_spans(text);
        assert_eq!(spans.len(), 1);
        let span = &spans[0];
        assert_eq!(span.link.target, "my dest.md");
        // target_start/target_end must point at the unwrapped text so a
        // writer splice re-emits the angle brackets around a new target.
        assert_eq!(&text[span.target_start..span.target_end], "my dest.md");
    }

    // --- L-A2: escaped brackets in link text ---

    #[test]
    fn escaped_brackets_in_label_are_not_terminators() {
        let text = r"[Contains \[test\] brackets](dest.md)";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "dest.md");
        assert_eq!(
            links[0].label.as_deref(),
            Some(r"Contains \[test\] brackets")
        );
    }

    #[test]
    fn escaped_bracket_at_start_of_label() {
        let text = r"[\[leading](dest.md)";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "dest.md");
        assert_eq!(links[0].label.as_deref(), Some(r"\[leading"));
    }

    #[test]
    fn escaped_bracket_at_end_of_label() {
        let text = r"[trailing\]](dest.md)";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "dest.md");
        assert_eq!(links[0].label.as_deref(), Some(r"trailing\]"));
    }

    #[test]
    fn escaped_brackets_in_label_span_variant() {
        let text = r"[Contains \[test\] brackets](dest.md)";
        let spans = extract_link_spans(text);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].link.target, "dest.md");
        assert_eq!(
            spans[0].link.label.as_deref(),
            Some(r"Contains \[test\] brackets")
        );
    }
}
