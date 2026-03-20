use anyhow::Result;
use std::path::Path;

use crate::scanner::{self, ScanAction};

/// Whether the link was written in wiki or markdown style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkStyle {
    Wiki,
    Markdown,
}

/// A parsed link extracted from a markdown file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// Raw target: "Note Name" or "sub/note.md"
    pub target: String,
    /// Display text from [[target|display]] or [display](target)
    pub display: Option<String>,
    /// Heading fragment from [[target#Heading]] or target.md#heading
    pub heading: Option<String>,
    /// Block reference from [[target#^block-id]]
    pub block_ref: Option<String>,
    /// Whether this is an embed (![[...]]) — always false for markdown links
    pub is_embed: bool,
    /// Wiki or markdown style
    pub style: LinkStyle,
    /// 1-based line number in the source file
    pub line: usize,
}

/// Extract all internal links from a markdown file.
pub fn extract_links_from_file(path: &Path) -> Result<Vec<Link>> {
    let mut links = Vec::new();
    scanner::scan_file(path, |text, line| {
        extract_links_from_text(text, line, &mut links);
        ScanAction::Continue
    })?;
    Ok(links)
}

/// Extract links from a text segment (already cleaned of inline code spans).
fn extract_links_from_text(text: &str, line: usize, out: &mut Vec<Link>) {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Check for wikilink: ![[...]] or [[...]]
        if bytes[i] == b'!'
            && i + 3 < len
            && bytes[i + 1] == b'['
            && bytes[i + 2] == b'['
            && let Some((link, end)) = try_parse_wikilink_at(text, i + 1, true)
        {
            out.push(Link { line, ..link });
            i = end;
            continue;
        }
        if bytes[i] == b'['
            && i + 1 < len
            && bytes[i + 1] == b'['
            && let Some((link, end)) = try_parse_wikilink_at(text, i, false)
        {
            out.push(Link { line, ..link });
            i = end;
            continue;
        }

        // Check for markdown link: [text](target)
        if bytes[i] == b'[' {
            // Make sure it's not preceded by ! (that's an image, but could be an internal embed)
            let is_image = i > 0 && bytes[i - 1] == b'!';
            if let Some((link, end)) = try_parse_markdown_link_at(text, i, is_image) {
                out.push(Link { line, ..link });
                i = end;
                continue;
            }
        }

        i += 1;
    }
}

/// Try to parse a wikilink starting at position `start` (the first `[`).
/// Returns the parsed Link and the position after the closing `]]`.
fn try_parse_wikilink_at(text: &str, start: usize, is_embed: bool) -> Option<(Link, usize)> {
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

    let link = parse_wikilink(inner, is_embed)?;
    let end_pos = content_start + close + 2;
    Some((link, end_pos))
}

/// Parse the inner content of a wikilink (between [[ and ]]).
/// Handles: target, target|display, target#heading, target#^block-id
pub fn parse_wikilink(inner: &str, is_embed: bool) -> Option<Link> {
    if inner.is_empty() {
        return None;
    }

    // Split on pipe for display text
    let (target_part, display) = if let Some(pipe_pos) = inner.find('|') {
        (&inner[..pipe_pos], Some(inner[pipe_pos + 1..].to_string()))
    } else {
        (inner, None)
    };

    // Split target on # for heading/block ref
    let (target, heading, block_ref) = parse_fragment(target_part);

    Some(Link {
        target: target.to_string(),
        display,
        heading,
        block_ref,
        is_embed,
        style: LinkStyle::Wiki,
        line: 0, // caller sets this
    })
}

/// Try to parse a markdown-style link [text](target) at position `start`.
fn try_parse_markdown_link_at(text: &str, start: usize, _is_image: bool) -> Option<(Link, usize)> {
    let rest = &text[start..];

    // Find the closing ]
    let close_bracket = rest.find(']')?;
    let display_text = &rest[1..close_bracket];

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

    let link = parse_markdown_link(display_text, target_raw)?;
    let end_pos = paren_start + close_paren + 1;
    Some((link, end_pos))
}

/// Parse a markdown link's display text and target into a Link.
pub fn parse_markdown_link(display_text: &str, target_raw: &str) -> Option<Link> {
    if target_raw.is_empty() {
        return None;
    }

    if is_external(target_raw) {
        return None;
    }

    let (target, heading, block_ref) = parse_fragment(target_raw);

    Some(Link {
        target: target.to_string(),
        display: if display_text.is_empty() {
            None
        } else {
            Some(display_text.to_string())
        },
        heading,
        block_ref,
        is_embed: false,
        style: LinkStyle::Markdown,
        line: 0, // caller sets this
    })
}

/// Check if a target is an external link (http, https, mailto).
fn is_external(target: &str) -> bool {
    let lower = target.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
}

/// Parse a target string into (target, heading, block_ref).
/// Handles: "target", "target#heading", "target#^block-id"
fn parse_fragment(target: &str) -> (&str, Option<String>, Option<String>) {
    if let Some(hash_pos) = target.find('#') {
        let base = &target[..hash_pos];
        let fragment = &target[hash_pos + 1..];
        if let Some(block_id) = fragment.strip_prefix('^') {
            (base, None, Some(block_id.to_string()))
        } else {
            (base, Some(fragment.to_string()), None)
        }
    } else {
        (target, None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_wikilink() {
        let link = parse_wikilink("Note", false).unwrap();
        assert_eq!(link.target, "Note");
        assert_eq!(link.display, None);
        assert_eq!(link.heading, None);
        assert_eq!(link.block_ref, None);
        assert!(!link.is_embed);
        assert_eq!(link.style, LinkStyle::Wiki);
    }

    #[test]
    fn parse_wikilink_with_display() {
        let link = parse_wikilink("Note|My Display", false).unwrap();
        assert_eq!(link.target, "Note");
        assert_eq!(link.display.as_deref(), Some("My Display"));
    }

    #[test]
    fn parse_wikilink_with_heading() {
        let link = parse_wikilink("Note#Section", false).unwrap();
        assert_eq!(link.target, "Note");
        assert_eq!(link.heading.as_deref(), Some("Section"));
    }

    #[test]
    fn parse_wikilink_with_block_ref() {
        let link = parse_wikilink("Note#^abc123", false).unwrap();
        assert_eq!(link.target, "Note");
        assert_eq!(link.block_ref.as_deref(), Some("abc123"));
    }

    #[test]
    fn parse_wikilink_heading_and_display() {
        let link = parse_wikilink("Note#Section|display", false).unwrap();
        assert_eq!(link.target, "Note");
        assert_eq!(link.heading.as_deref(), Some("Section"));
        assert_eq!(link.display.as_deref(), Some("display"));
    }

    #[test]
    fn parse_embed_wikilink() {
        let link = parse_wikilink("image.png", true).unwrap();
        assert!(link.is_embed);
    }

    #[test]
    fn parse_empty_wikilink_returns_none() {
        assert!(parse_wikilink("", false).is_none());
    }

    #[test]
    fn parse_simple_markdown_link() {
        let link = parse_markdown_link("click here", "note.md").unwrap();
        assert_eq!(link.target, "note.md");
        assert_eq!(link.display.as_deref(), Some("click here"));
        assert_eq!(link.style, LinkStyle::Markdown);
        assert!(!link.is_embed);
    }

    #[test]
    fn parse_markdown_link_with_heading() {
        let link = parse_markdown_link("text", "note.md#section").unwrap();
        assert_eq!(link.target, "note.md");
        assert_eq!(link.heading.as_deref(), Some("section"));
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
        extract_links_from_text(text, 5, &mut links);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target, "Note A");
        assert_eq!(links[0].line, 5);
        assert_eq!(links[1].target, "Note B");
        assert_eq!(links[1].display.as_deref(), Some("display"));
    }

    #[test]
    fn extract_embed_from_text() {
        let text = "![[embedded note]]";
        let mut links = Vec::new();
        extract_links_from_text(text, 1, &mut links);
        assert_eq!(links.len(), 1);
        assert!(links[0].is_embed);
        assert_eq!(links[0].target, "embedded note");
    }

    #[test]
    fn extract_markdown_link_from_text() {
        let text = "See [my note](notes/foo.md) for details";
        let mut links = Vec::new();
        extract_links_from_text(text, 3, &mut links);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "notes/foo.md");
        assert_eq!(links[0].display.as_deref(), Some("my note"));
        assert_eq!(links[0].style, LinkStyle::Markdown);
    }

    #[test]
    fn external_markdown_links_skipped() {
        let text = "[Google](https://google.com) and [[internal]]";
        let mut links = Vec::new();
        extract_links_from_text(text, 1, &mut links);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "internal");
    }

    #[test]
    fn multiple_links_on_one_line() {
        let text = "[[A]] then [b](b.md) then [[C#heading]]";
        let mut links = Vec::new();
        extract_links_from_text(text, 1, &mut links);
        assert_eq!(links.len(), 3);
        assert_eq!(links[0].target, "A");
        assert_eq!(links[1].target, "b.md");
        assert_eq!(links[2].target, "C");
        assert_eq!(links[2].heading.as_deref(), Some("heading"));
    }

    #[test]
    fn extract_links_from_text_with_block_ref() {
        let text = "[[Note#^abc123]]";
        let mut links = Vec::new();
        extract_links_from_text(text, 1, &mut links);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].block_ref.as_deref(), Some("abc123"));
    }
}
