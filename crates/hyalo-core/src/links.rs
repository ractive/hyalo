#![allow(clippy::missing_errors_doc)]
use serde::{Deserialize, Serialize};

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
        if bytes[i] == b'!'
            && i + 3 < len
            && bytes[i + 1] == b'['
            && bytes[i + 2] == b'['
            && let Some((link, end)) = try_parse_wikilink_at(cleaned, i + 1)
        {
            out.push(link);
            i = end;
            continue;
        }
        if bytes[i] == b'['
            && i + 1 < len
            && bytes[i + 1] == b'['
            && let Some((link, end)) = try_parse_wikilink_at(cleaned, i)
        {
            out.push(link);
            i = end;
            continue;
        }

        // Check for markdown link: [text](target)
        // Skip if preceded by `!` — that's image syntax: ![alt](img.png)
        if bytes[i] == b'['
            && (i == 0 || bytes[i - 1] != b'!')
            && let Some((link, end)) = try_parse_markdown_link_at(cleaned, original, i)
        {
            out.push(link);
            i = end;
            continue;
        }

        i += 1;
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
        if bytes[i] == b'!'
            && i + 3 < len
            && bytes[i + 1] == b'['
            && bytes[i + 2] == b'['
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
            && let Some((span, end)) = try_parse_wikilink_span_at(cleaned, i)
        {
            out.push(span);
            i = end;
            continue;
        }

        // [text](target) — skip if preceded by `!` (image)
        if bytes[i] == b'['
            && (i == 0 || bytes[i - 1] != b'!')
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

    let close_bracket = rest.find(']')?;
    // Read label from `original` so backtick-wrapped content is not lost when
    // `text` has had inline code spans replaced with spaces.
    // Use `.get()` to avoid panic if `original` has a different byte layout.
    let label_text = original.get(start + 1..start + close_bracket)?;

    let after_bracket = start + close_bracket + 1;
    if text.as_bytes().get(after_bracket).copied() != Some(b'(') {
        return None;
    }

    let paren_start = after_bracket + 1; // first byte of raw target
    let rest_after_paren = &text[paren_start..];
    let close_paren = rest_after_paren.find(')')?;
    let target_raw = &rest_after_paren[..close_paren];

    if is_external(target_raw) || target_raw.is_empty() {
        return None;
    }

    let link = parse_markdown_link(label_text, target_raw)?;

    // target_end stops at `#` if a fragment is present, otherwise at `)`.
    let target_end_in_raw = target_raw.find('#').unwrap_or(target_raw.len());

    let full_end = paren_start + close_paren + 1;

    Some((
        LinkSpan {
            link,
            kind: LinkKind::Markdown,
            target_start: paren_start,
            target_end: paren_start + target_end_in_raw,
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

    Some(Link {
        target: target.to_string(),
        label,
        kind: LinkKind::Wikilink,
    })
}

/// Try to parse a markdown-style link [text](target) at position `start`.
///
/// `text` drives structural parsing; `original` provides the label text so
/// that backtick-wrapped content is preserved when `text` has been
/// inline-code-stripped.
fn try_parse_markdown_link_at(text: &str, original: &str, start: usize) -> Option<(Link, usize)> {
    let rest = &text[start..];

    // Find the closing ]
    let close_bracket = rest.find(']')?;
    // Read label from `original` so backtick-wrapped content is not lost when
    // `text` has had inline code spans replaced with spaces.
    // Use `.get()` to avoid panic if `original` has a different byte layout.
    let label_text = original.get(start + 1..start + close_bracket)?;

    // Must be immediately followed by (
    let after_bracket = start + close_bracket + 1;
    if text.as_bytes().get(after_bracket).copied() != Some(b'(') {
        return None;
    }

    // Find closing )
    let paren_start = after_bracket + 1;
    let rest_after_paren = &text[paren_start..];
    let close_paren = rest_after_paren.find(')')?;
    let target_raw = &rest_after_paren[..close_paren];

    // Skip external links
    if is_external(target_raw) {
        return None;
    }

    // Skip empty targets
    if target_raw.is_empty() {
        return None;
    }

    let link = parse_markdown_link(label_text, target_raw)?;
    let end_pos = paren_start + close_paren + 1;
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
fn is_external(target: &str) -> bool {
    let lower = target.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
}

/// Strip the fragment (#heading or #^block-id) from a target string,
/// returning only the base target name.
fn strip_fragment(target: &str) -> &str {
    target.split('#').next().unwrap_or(target)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
