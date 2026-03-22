#![allow(clippy::missing_errors_doc)]
use anyhow::Result;
use std::path::Path;

use crate::scanner::{self, ScanAction};

/// A parsed link extracted from a markdown file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// Raw target: note name or relative path (without fragment)
    pub target: String,
    /// Display text from `[[target|label]]` or `[label](target)`
    pub label: Option<String>,
}

/// Extract all internal links from a markdown file.
pub fn extract_links_from_file(path: &Path) -> Result<Vec<Link>> {
    let mut links = Vec::new();
    scanner::scan_file(path, |text, _line| {
        extract_links_from_text(text, &mut links);
        ScanAction::Continue
    })?;
    Ok(links)
}

/// Extract links from a text segment and append them to `out`.
///
/// `text` must already be cleaned of inline code spans (e.g. via
/// [`strip_inline_code`](crate::scanner::strip_inline_code)), otherwise links
/// inside code spans will be incorrectly parsed. Existing contents of `out` are
/// preserved; new links are appended.
pub fn extract_links_from_text(text: &str, out: &mut Vec<Link>) {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Check for wikilink: ![[...]] or [[...]]
        if bytes[i] == b'!'
            && i + 3 < len
            && bytes[i + 1] == b'['
            && bytes[i + 2] == b'['
            && let Some((link, end)) = try_parse_wikilink_at(text, i + 1)
        {
            out.push(link);
            i = end;
            continue;
        }
        if bytes[i] == b'['
            && i + 1 < len
            && bytes[i + 1] == b'['
            && let Some((link, end)) = try_parse_wikilink_at(text, i)
        {
            out.push(link);
            i = end;
            continue;
        }

        // Check for markdown link: [text](target)
        // Skip if preceded by `!` — that's image syntax: ![alt](img.png)
        if bytes[i] == b'['
            && (i == 0 || bytes[i - 1] != b'!')
            && let Some((link, end)) = try_parse_markdown_link_at(text, i)
        {
            out.push(link);
            i = end;
            continue;
        }

        i += 1;
    }
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
pub fn parse_wikilink(inner: &str) -> Option<Link> {
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
    })
}

/// Try to parse a markdown-style link [text](target) at position `start`.
fn try_parse_markdown_link_at(text: &str, start: usize) -> Option<(Link, usize)> {
    let rest = &text[start..];

    // Find the closing ]
    let close_bracket = rest.find(']')?;
    let label_text = &rest[1..close_bracket];

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
pub fn parse_markdown_link(label_text: &str, target_raw: &str) -> Option<Link> {
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
}
