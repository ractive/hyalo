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
    /// The `#fragment` (heading anchor or `^block-id`) that followed the
    /// target, WITHOUT the leading `#`. `None` when the link had no fragment.
    ///
    /// For markdown links the fragment is preserved exactly as written (it may
    /// be percent-encoded); anchor matching decodes it. `target` never contains
    /// the fragment, and the rewrite span stops before the `#`, so the written
    /// fragment bytes are always preserved through `mv` / `links fix`.
    ///
    /// L-21 (iter-190): added to carry anchors through resolution and enable
    /// broken-anchor validation in `find --broken-links`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fragment: Option<String>,
    /// The `?query` string that followed the target, WITHOUT the leading `?`.
    /// `None` when the link had no query string; always `None` for wikilinks.
    ///
    /// iter-211 / BUG-12: a query string is a URL component, not part of the
    /// path — `resolve_target` has always stripped it before resolution, but
    /// it used to remain glued to `target`, so the rewrite span covered it and
    /// `[x](/deep/page?x=1)` came back out of `mv` as `[x](/deep/Page)` with
    /// the query silently dropped. It is now split off exactly like
    /// [`fragment`](Self::fragment): the rewrite span stops before the `?`, so
    /// the written query bytes survive every rewrite untouched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
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
    /// `?`, `|`, `]]`, or `)` depending on what follows the target). Stopping
    /// before `#`/`?` is what keeps fragments and query strings byte-identical
    /// across a rewrite.
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
    extract_links_and_anchors(cleaned, original, out, None);
}

/// Like [`extract_links_from_text_with_original`], but also collects
/// **same-file anchors** — `[label](#frag)` and `[[#frag]]`, links that name a
/// heading in the *current* file and carry no target path.
///
/// Those are dropped by the ordinary extraction paths (a `Link` must have a
/// target), which meant `find --broken-links` never checked them at all:
/// `[b](#nope)` in a file with no `## Nope` heading was invisible
/// (iter-211 / BUG-8). They are returned as bare fragment strings (no leading
/// `#`) so the caller can validate them against the file's own headings.
///
/// Block references (`^block-id`) are collected too; the anchor matcher skips
/// them, keeping the "never reported broken" contract in one place.
pub(crate) fn extract_links_and_self_anchors(
    cleaned: &str,
    original: &str,
    out: &mut Vec<Link>,
    anchors: &mut Vec<String>,
) {
    extract_links_and_anchors(cleaned, original, out, Some(anchors));
}

fn extract_links_and_anchors(
    cleaned: &str,
    original: &str,
    out: &mut Vec<Link>,
    mut anchors: Option<&mut Vec<String>>,
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
        // `[[#Heading]]` — a same-file anchor, not a file reference.
        if let Some(anchors) = anchors.as_deref_mut()
            && bytes[i] == b'['
            && i + 1 < len
            && bytes[i + 1] == b'['
            && !is_escaped(bytes, i)
            && let Some((frag, end)) = try_parse_self_anchor_wikilink_at(cleaned, i)
        {
            anchors.push(frag);
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
        // `[label](#Heading)` — a same-file anchor, not a file reference.
        if let Some(anchors) = anchors.as_deref_mut()
            && bytes[i] == b'['
            && (i == 0 || bytes[i - 1] != b'!')
            && !is_escaped(bytes, i)
            && let Some((frag, end)) = try_parse_self_anchor_markdown_at(cleaned, i)
        {
            anchors.push(frag);
            i = end;
            continue;
        }

        i += 1;
    }
}

/// Parse `[label](#fragment)` at `start`, returning the fragment (without the
/// leading `#`) and the byte offset just past the closing `)`.
///
/// Returns `None` for anything that is not a fragment-only markdown link —
/// including `[a](p.md#frag)`, which is a real file reference and is handled
/// by [`try_parse_markdown_link_at`].
fn try_parse_self_anchor_markdown_at(text: &str, start: usize) -> Option<(String, usize)> {
    let rest = &text[start..];
    let close_bracket = find_label_close_bracket(rest)?;
    let after_bracket = start + close_bracket + 1;
    if text.as_bytes().get(after_bracket).copied() != Some(b'(') {
        return None;
    }
    let paren_start = after_bracket + 1;
    let dest = parse_destination(&text[paren_start..])?;
    let frag = dest.target_raw.strip_prefix('#')?;
    if frag.is_empty() {
        return None;
    }
    Some((frag.to_owned(), paren_start + dest.end))
}

/// Parse `[[#fragment]]` (optionally `[[#fragment|alias]]`) at `start`,
/// returning the fragment without the leading `#`.
fn try_parse_self_anchor_wikilink_at(text: &str, start: usize) -> Option<(String, usize)> {
    let content_start = start + 2;
    let rest = &text[content_start..];
    let close = rest.find("]]")?;
    let inner = &rest[..close];
    if inner.is_empty() || inner.contains('\n') {
        return None;
    }
    let target_part = inner.split('|').next().unwrap_or(inner);
    let frag = target_part.strip_prefix('#')?;
    if frag.is_empty() {
        return None;
    }
    Some((frag.to_owned(), content_start + close + 2))
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
    /// after `(`) of the first byte past the `)` that closes the link
    /// (skipping any intervening title, which may itself contain `)`).
    end: usize,
}

/// Locate the `)` that closes a markdown link, given the text immediately
/// after an angle-bracket destination's closing `>`. Skips leading
/// whitespace and an optional CommonMark title (`"…"`, `'…'`, or `(…)`,
/// escape-aware), so a `)` inside the title is not mistaken for the closing
/// paren. Returns the byte offset of the closing `)` within `s`, or `None`
/// if what follows is neither a title nor the closing paren (per CommonMark
/// the construct is then not a link).
fn find_close_paren_after_destination(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    match bytes.get(i) {
        Some(b')') => Some(i),
        Some(&open @ (b'"' | b'\'' | b'(')) => {
            let close = if open == b'(' { b')' } else { open };
            i += 1;
            while i < bytes.len() {
                if bytes[i] == close && !is_escaped(bytes, i) {
                    i += 1;
                    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                        i += 1;
                    }
                    return (bytes.get(i) == Some(&b')')).then_some(i);
                }
                i += 1;
            }
            None
        }
        _ => None,
    }
}

/// Parse a markdown link destination starting at `rest` (the text right
/// after the opening `(`).
///
/// Handles both bare destinations (`dest.md`, up to the first `)`) and
/// CommonMark angle-bracket destinations (`<my dest.md>`, which may contain
/// spaces and literal `)`), per L-A1. For the angle form, the destination is
/// closed by the first unescaped `>`; an optional title between it and the
/// closing `)` is skipped (escape-aware, so a `)` inside the title does not
/// truncate the span) but not stored, since this file does not otherwise
/// track link titles. Returns `None` if no closing `)` can be found for the
/// destination.
///
/// Known CommonMark deviation: an unescaped `<` inside the angle form
/// (`<foo<bar.md>`) should invalidate the destination per spec, but is
/// accepted here; hyalo is not a full CommonMark parser elsewhere either.
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

        // Whatever follows `>` (whitespace, optional title) is not part of
        // the target; locate the `)` that actually closes the link — a bare
        // `find(')')` would stop inside a title containing `)`.
        let after_angle = &rest[close_angle + 1..];
        let close_paren = find_close_paren_after_destination(after_angle)?;
        let end = close_angle + 1 + close_paren + 1;

        Some(ParsedDestination { target_raw, end })
    } else {
        // Bare destination: up to the first `)`.
        let close_paren = rest.find(')')?;
        let raw = &rest[..close_paren];
        // iter-211 / BUG-12: a bare CommonMark destination cannot contain
        // whitespace — anything after the first space/tab is an optional
        // title (`[a](p.md "Title")`). Without this split the title became
        // part of the target, so `p.md "Title"` resolved to nothing: the link
        // was reported broken and never appeared in `backlinks`.
        //
        // The split is applied only when what follows really parses as a
        // title immediately before the closing `)`. That keeps the
        // long-standing tolerance for unencoded spaces in destinations
        // (`[x](my dest.md)`), which is not valid CommonMark but is common in
        // hand-written vaults. A leading space (`[a]( p.md )`) is likewise
        // left alone rather than producing an empty target.
        if let Some(ws) = raw.find([' ', '\t'])
            && ws > 0
            && let Some(cp) = find_close_paren_after_destination(&rest[ws..])
        {
            // `find_close_paren_after_destination` also skips a title that
            // itself contains `)`, so the span end can legitimately land past
            // the `close_paren` found above.
            return Some(ParsedDestination {
                target_raw: &raw[..ws],
                end: ws + cp + 1,
            });
        }
        Some(ParsedDestination {
            target_raw: raw,
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
    // Stop the rewritable span at whichever URL suffix comes first, so both
    // `#fragment` and `?query` bytes are preserved verbatim (iter-211/BUG-12).
    let target_end_in_raw = target_raw
        .find(['#', '?'])
        .unwrap_or(target_raw.len());

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

    // Split the fragment (heading/block ref) off the target. The fragment is
    // carried on the Link (L-21) without the leading `#`.
    let (target, fragment) = split_target_and_fragment(target_part);

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
        fragment,
        // Wikilinks are vault paths, not URLs — they never carry a query
        // string, and a literal `?` in a note name stays part of the target.
        query: None,
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

    // Split the fragment (heading/block ref) off the target. The fragment is
    // carried on the Link (L-21) without the leading `#`, preserving its
    // written form (it may be percent-encoded).
    let (target, fragment) = split_target_and_fragment(target_raw);
    // …then split the `?query` off whatever is left of the path (iter-211 /
    // BUG-12). Order matters: `page?x=1#frag` keeps `x=1` as the query, while
    // `page#frag?x` leaves the `?x` inside the fragment, exactly as a browser
    // would read it.
    let (target, query) = split_target_and_query(target);

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
        fragment,
        query,
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

// ---------------------------------------------------------------------------
// Inert link zones (iter-200)
// ---------------------------------------------------------------------------

/// Prefixes that start a bare URL run in prose.
///
/// Deliberately wider than [`is_external`]: that predicate decides whether a
/// *parsed markdown destination* points off-vault, whereas this list has to
/// recognise a URL sitting in plain prose, where `www.` forms are common.
const BARE_URL_PREFIXES: [&str; 5] = ["https://", "http://", "ftp://", "mailto:", "www."];

/// Whether `c` terminates a bare URL run.
fn is_url_terminator(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '<' | '>' | '(' | ')' | '[' | ']' | '{' | '}' | '"' | '\'' | '`' | '|' | '\\'
        )
}

/// If a bare URL run starts at byte offset `start`, return its exclusive end.
///
/// `start` must be a char boundary. The run only starts on a word boundary so
/// that `foohttps://x` is not treated as a URL, and trailing sentence
/// punctuation is excluded so `see https://a.example.` keeps the final stop
/// outside the zone.
fn bare_url_end(line: &str, start: usize) -> Option<usize> {
    if line[..start]
        .chars()
        .next_back()
        .is_some_and(|c| c.is_alphanumeric() || c == '_')
    {
        return None;
    }
    let rest = &line[start..];
    let matched = BARE_URL_PREFIXES.iter().any(|p| {
        rest.len() >= p.len() && rest.as_bytes()[..p.len()].eq_ignore_ascii_case(p.as_bytes())
    });
    if !matched {
        return None;
    }
    let mut end = line.len();
    for (off, c) in rest.char_indices() {
        if is_url_terminator(c) {
            end = start + off;
            break;
        }
    }
    while end > start {
        let Some(c) = line[..end].chars().next_back() else {
            break;
        };
        if matches!(c, '.' | ',' | ';' | ':' | '!' | '?') {
            end -= c.len_utf8();
        } else {
            break;
        }
    }
    (end > start).then_some(end)
}

/// Byte ranges in `line` that are syntactically part of a link and must never
/// be rewritten by a text-level mutator such as `links auto` (iter-200, H-2).
///
/// [`extract_link_spans`] answers a different question — "which *vault* links
/// does this line contain?" — and therefore drops external destinations
/// (`[x](https://…)`) entirely and never sees bare URLs at all. Auto-linking
/// against those spans alone let a page titled `net` rewrite
/// `[x](https://pkg.go.dev/x/actions.summerwind.net/v1)` into
/// `…summerwind.[[net]]/v1`, destroying a working URL. The zones returned here
/// cover, regardless of whether the destination is internal or external:
///
/// - whole `[label](destination)` constructs (label *and* destination, so a
///   title mention inside a link's own label is left alone);
/// - whole `[[wikilink]]` constructs;
/// - autolinks (`<https://…>`) and bare URLs written in prose;
/// - Liquid/Jinja template expressions — `{% … %}` and `{{ … }}` (iter-207,
///   BUG-2). Injecting a wikilink into `{% data variables.x %}` destroys a
///   variable reference that renders to prose;
/// - raw HTML tag spans, including attribute values — `<img src="…" …>`,
///   `<a name="…">`, HTML comments and processing instructions (iter-207,
///   BUG-3). Text *between* tags stays linkable: in `<div>prose</div>` only
///   the two tags are inert.
///
/// Unterminated Liquid or HTML markers make the rest of the line inert. That
/// is deliberate: a missed auto-link candidate costs nothing, a corrupted
/// file does.
///
/// Ranges are returned in ascending, non-overlapping order. `line` should be
/// the same text the caller matches against (e.g. the inline-code-blanked
/// form), since offsets are relative to it.
/// End offset (exclusive) of a Liquid/Jinja template expression starting at
/// `start`, which must point at a `{` byte (iter-207, BUG-2).
///
/// Recognizes `{% … %}` (tags) and `{{ … }}` (output expressions). An
/// unterminated marker makes the rest of the line inert — a template
/// expression that continues onto the next line is still not prose, and
/// blanking too much only costs a missed auto-link candidate.
///
/// Returns `None` when `start` is a lone `{` (not a template marker).
fn liquid_span_end(line: &str, start: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    debug_assert_eq!(bytes.get(start), Some(&b'{'));
    let closer: &str = match bytes.get(start + 1)? {
        b'%' => "%}",
        b'{' => "}}",
        _ => return None,
    };
    Some(match line[start + 2..].find(closer) {
        Some(rel) => start + 2 + rel + closer.len(),
        None => line.len(),
    })
}

/// End offset (exclusive) of a raw-HTML span starting at `start`, which must
/// point at a `<` byte (iter-207, BUG-3).
///
/// Covers the four raw-HTML shapes markdown lets through inline: open/close
/// tags (`<a href="x">`, `</a>`), comments (`<!-- … -->`), declarations
/// (`<!DOCTYPE …>`) and processing instructions (`<? … ?>`). Quoted attribute
/// values are scanned through, so a `>` inside `alt="a > b"` does not end the
/// tag early.
///
/// An unterminated tag makes the rest of the line inert (a tag that wraps onto
/// the next line is still markup). Returns `None` when `<` is ordinary prose
/// (`a < b`), so comparison operators stay linkable.
fn html_span_end(line: &str, start: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    debug_assert_eq!(bytes.get(start), Some(&b'<'));

    // `<!-- … -->` comment: closed only by the literal `-->`.
    if line[start..].starts_with("<!--") {
        return Some(match line[start + 4..].find("-->") {
            Some(rel) => start + 4 + rel + 3,
            None => line.len(),
        });
    }
    // `<? … ?>` processing instruction.
    if line[start..].starts_with("<?") {
        return Some(match line[start + 2..].find("?>") {
            Some(rel) => start + 2 + rel + 2,
            None => line.len(),
        });
    }

    // Open (`<a …>`) or closing (`</a>`) tag, or a `<!DOCTYPE …>` declaration.
    // All three require a name character right after the opening punctuation;
    // without one this is prose (`a < b`, `5 <10`).
    let mut i = start + 1;
    if matches!(bytes.get(i), Some(b'/' | b'!')) {
        i += 1;
    }
    if !bytes.get(i).is_some_and(u8::is_ascii_alphabetic) {
        return None;
    }

    // Scan to the closing `>`, skipping quoted attribute values.
    while i < bytes.len() {
        match bytes[i] {
            b'>' => return Some(i + 1),
            q @ (b'"' | b'\'') => {
                i += 1;
                while i < bytes.len() && bytes[i] != q {
                    i += 1;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    // Unterminated: treat the rest of the line as markup.
    Some(line.len())
}

#[must_use]
pub fn inert_link_zones(line: &str) -> Vec<(usize, usize)> {
    let bytes = line.as_bytes();
    let mut zones: Vec<(usize, usize)> = Vec::new();
    let mut i = 0usize;

    while i < bytes.len() {
        if !line.is_char_boundary(i) {
            i += 1;
            continue;
        }
        match bytes[i] {
            b'[' => {
                // `[[wikilink]]` — inert as a whole, alias and all.
                if bytes.get(i + 1) == Some(&b'[')
                    && let Some(rel) = line[i + 2..].find("]]")
                {
                    let end = i + 2 + rel + 2;
                    zones.push((i, end));
                    i = end;
                    continue;
                }
                // `[label](destination)` — internal *or* external.
                if let Some(close) = find_label_close_bracket(&line[i..])
                    && bytes.get(i + close + 1) == Some(&b'(')
                    && let Some(dest) = parse_destination(&line[i + close + 2..])
                {
                    let end = i + close + 2 + dest.end;
                    zones.push((i, end));
                    i = end;
                    continue;
                }
                i += 1;
            }
            b'<' => {
                // `<https://…>` autolink.
                if let Some(rel) = line[i + 1..].find('>')
                    && bare_url_end(line, i + 1).is_some()
                {
                    let end = i + 1 + rel + 1;
                    zones.push((i, end));
                    i = end;
                    continue;
                }
                // Raw HTML tag / comment / declaration (iter-207, BUG-3).
                if let Some(end) = html_span_end(line, i) {
                    zones.push((i, end));
                    i = end;
                    continue;
                }
                i += 1;
            }
            b'{' => {
                // Liquid / Jinja expressions (iter-207, BUG-2).
                if let Some(end) = liquid_span_end(line, i) {
                    zones.push((i, end));
                    i = end;
                    continue;
                }
                i += 1;
            }
            b'h' | b'H' | b'f' | b'F' | b'm' | b'M' | b'w' | b'W' => {
                if let Some(end) = bare_url_end(line, i) {
                    zones.push((i, end));
                    i = end;
                    continue;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }

    zones
}

/// Whether `[start, end)` overlaps any range in `zones`.
#[must_use]
pub fn overlaps_zone(zones: &[(usize, usize)], start: usize, end: usize) -> bool {
    zones.iter().any(|&(zs, ze)| start < ze && end > zs)
}

/// Split a target string into its base target and optional `#fragment`.
///
/// The fragment is returned WITHOUT the leading `#`. Only the FIRST `#` is
/// treated as the fragment delimiter; any subsequent `#` bytes stay in the
/// fragment (Obsidian block/heading refs never contain a bare `#`, and this
/// keeps percent-encoded markdown fragments intact). An empty fragment
/// (`target#`) yields `None`.
fn split_target_and_fragment(target: &str) -> (&str, Option<String>) {
    match target.split_once('#') {
        Some((base, frag)) if !frag.is_empty() => (base, Some(frag.to_string())),
        Some((base, _)) => (base, None),
        None => (target, None),
    }
}

/// Split a `?query` off the end of a (fragment-free) markdown destination.
///
/// The query is returned WITHOUT the leading `?`. Only the FIRST `?` delimits
/// it; everything after stays in the query string. An empty query (`page?`)
/// yields `None` while still trimming the `?` from the path, so resolution
/// sees the same path either way.
///
/// iter-211 / BUG-12 — see [`Link::query`].
fn split_target_and_query(target: &str) -> (&str, Option<String>) {
    match target.split_once('?') {
        Some((base, q)) if !q.is_empty() => (base, Some(q.to_string())),
        Some((base, _)) => (base, None),
        None => (target, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- inert link zones (iter-200) ---

    /// Whether `needle`'s first occurrence in `line` falls inside an inert zone.
    fn needle_is_inert(line: &str, needle: &str) -> bool {
        let start = line.find(needle).expect("needle must occur in line");
        let zones = inert_link_zones(line);
        overlaps_zone(&zones, start, start + needle.len())
    }

    #[test]
    fn inert_zone_covers_external_markdown_destination() {
        assert!(needle_is_inert(
            "Link: [x](https://pkg.go.dev/x/actions.summerwind.net/v1)",
            "net"
        ));
    }

    #[test]
    fn inert_zone_covers_internal_markdown_link_and_label() {
        assert!(needle_is_inert(
            "See [read about net](other.md) here",
            "net"
        ));
        assert!(needle_is_inert("See [label](sub/net.md) here", "net.md"));
    }

    #[test]
    fn inert_zone_covers_bare_urls_and_autolinks() {
        assert!(needle_is_inert(
            "Bare: https://example.net/path here",
            "net"
        ));
        assert!(needle_is_inert("Auto: <https://example.net/p>", "net"));
        assert!(needle_is_inert("Mail: mailto:a@example.net now", "net"));
        assert!(needle_is_inert("Site: www.example.net/x", "net"));
    }

    #[test]
    fn inert_zone_covers_wikilinks() {
        assert!(needle_is_inert("See [[sub/net|the net]] here", "net"));
    }

    #[test]
    fn inert_zone_leaves_plain_prose_alone() {
        assert!(!needle_is_inert("A plain net mention", "net"));
        // A mention after a URL on the same line is still linkable.
        assert!(!needle_is_inert(
            "See https://example.com/x then net",
            "net"
        ));
        // Trailing sentence punctuation is outside the URL zone.
        let line = "See https://example.com/x. net follows";
        let start = line.rfind("net").unwrap();
        assert!(!overlaps_zone(&inert_link_zones(line), start, start + 3));
    }

    #[test]
    fn inert_zone_does_not_treat_a_word_ending_in_a_scheme_as_a_url() {
        // `xhttps://` must not start a URL run — the boundary check rejects it.
        let line = "notahttps://example.net";
        let zones = inert_link_zones(line);
        assert!(
            zones.is_empty(),
            "no URL run should start mid-word: {zones:?}"
        );
    }

    #[test]
    fn inert_zone_handles_multibyte_lines() {
        // Byte-indexed scanning must never slice a multibyte char.
        let line = "日本語 テキスト [x](https://example.net/日本) 末尾 net";
        let zones = inert_link_zones(line);
        let tail = line.rfind("net").unwrap();
        assert!(!overlaps_zone(&zones, tail, tail + 3));
        let inside = line.find("example.net").unwrap() + "example.".len();
        assert!(overlaps_zone(&zones, inside, inside + 3));
    }

    // --- iter-207: Liquid and raw-HTML inert zones (BUG-2 / BUG-3) ---

    #[test]
    fn inert_zone_covers_liquid_tags_and_output() {
        assert!(needle_is_inert(
            "Use {% data variables.product.prodname_net %} here",
            "net"
        ));
        assert!(needle_is_inert("Path: {{ site.net.baseurl }}/x", "net"));
        // Prose either side of the expression stays linkable.
        assert!(!needle_is_inert("net before {% x %}", "net"));
        let line = "{% x %} net after";
        let start = line.rfind("net").unwrap();
        assert!(!overlaps_zone(&inert_link_zones(line), start, start + 3));
    }

    #[test]
    fn inert_zone_treats_unterminated_liquid_as_inert_to_end_of_line() {
        assert!(needle_is_inert("Start {% ifversion net", "net"));
        assert!(needle_is_inert("Start {{ net", "net"));
        // A lone brace is not a template marker.
        assert!(!needle_is_inert("A { net } set", "net"));
    }

    #[test]
    fn inert_zone_covers_html_tags_and_attribute_values() {
        assert!(needle_is_inert(r#"<img src="net.png" alt="x">"#, "net.png"));
        assert!(needle_is_inert(r#"<a name="net">x</a>"#, "net"));
        assert!(needle_is_inert("<!-- net comment -->", "net"));
        assert!(needle_is_inert("<?php echo net; ?>", "net"));
        // A `>` inside a quoted attribute must not end the tag early.
        assert!(needle_is_inert(
            r#"<img alt="a > b" src="net.png">"#,
            "net.png"
        ));
    }

    #[test]
    fn inert_zone_leaves_text_between_html_tags_linkable() {
        let line = "<div>net prose</div>";
        let start = line.find("net").unwrap();
        assert!(!overlaps_zone(&inert_link_zones(line), start, start + 3));
    }

    #[test]
    fn inert_zone_does_not_treat_a_comparison_as_an_html_tag() {
        let line = "if a < b then net wins";
        let start = line.find("net").unwrap();
        assert!(!overlaps_zone(&inert_link_zones(line), start, start + 3));
        assert!(!needle_is_inert("5 <10 net", "net"));
    }

    #[test]
    fn inert_zone_treats_unterminated_html_tag_as_inert_to_end_of_line() {
        assert!(needle_is_inert(
            r#"<img src="net.png" alt="wraps"#,
            "net.png"
        ));
    }

    #[test]
    fn inert_zone_still_recognizes_autolinks_before_html_tags() {
        // `<https://…>` must stay an autolink zone, not be eaten as a tag.
        let zones = inert_link_zones("Auto: <https://example.net/p> done");
        assert_eq!(zones.len(), 1);
        let line = "Auto: <https://example.net/p> done";
        assert_eq!(&line[zones[0].0..zones[0].1], "<https://example.net/p>");
    }

    #[test]
    fn inert_zone_ranges_are_ascending_and_disjoint() {
        let line = "[a](x.md) plain https://e.example/z and [[w]] end";
        let zones = inert_link_zones(line);
        assert!(zones.len() >= 3, "expected three zones: {zones:?}");
        for pair in zones.windows(2) {
            assert!(pair[0].1 <= pair[1].0, "zones must be disjoint: {zones:?}");
        }
    }

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
        // L-21: the fragment round-trips onto the Link WITHOUT the leading `#`,
        // while the rewrite span still stops before `#` so the `#heading` bytes
        // are untouched by any splice.
        assert_eq!(spans[0].link.fragment.as_deref(), Some("heading"));
        assert_eq!(&text[spans[0].target_end..spans[0].full_end], "#heading]]");
    }

    #[test]
    fn span_markdown_fragment_roundtrips_and_span_untouched() {
        // L-21: markdown fragment captured (percent-encoded form preserved on
        // the Link) while the rewrite span stops before `#`.
        let text = "[t](note.md#my%20heading)";
        let spans = extract_link_spans(text);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].link.target, "note.md");
        assert_eq!(spans[0].link.fragment.as_deref(), Some("my%20heading"));
        assert_eq!(&text[spans[0].target_start..spans[0].target_end], "note.md");
        assert_eq!(
            &text[spans[0].target_end..spans[0].full_end],
            "#my%20heading)"
        );
    }

    #[test]
    fn parse_wikilink_fragment_and_alias() {
        // Fragment stored without `#`; alias still parsed independently.
        let link = parse_wikilink("note#section|display").unwrap();
        assert_eq!(link.target, "note");
        assert_eq!(link.fragment.as_deref(), Some("section"));
        assert_eq!(link.label.as_deref(), Some("display"));
    }

    #[test]
    fn parse_wikilink_block_ref_fragment() {
        let link = parse_wikilink("note#^block-id").unwrap();
        assert_eq!(link.target, "note");
        assert_eq!(link.fragment.as_deref(), Some("^block-id"));
    }

    #[test]
    fn parse_wikilink_no_fragment() {
        let link = parse_wikilink("note").unwrap();
        assert_eq!(link.fragment, None);
    }

    #[test]
    fn parse_wikilink_trailing_hash_empty_fragment() {
        // `[[note#]]` — empty fragment collapses to None (not a real anchor).
        let link = parse_wikilink("note#").unwrap();
        assert_eq!(link.target, "note");
        assert_eq!(link.fragment, None);
    }

    #[test]
    fn parse_markdown_link_fragment() {
        let link = parse_markdown_link("t", "note.md#Real").unwrap();
        assert_eq!(link.target, "note.md");
        assert_eq!(link.fragment.as_deref(), Some("Real"));
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

    #[test]
    fn angle_bracket_destination_with_title_containing_paren() {
        // A `)` inside the title must not truncate the span: full_end has to
        // land past the real closing paren or a writer splice corrupts the
        // line during `mv`/`links fix --apply`.
        let text = r#"[text](<dest.md> "a (note)") tail"#;
        let spans = extract_link_spans(text);
        assert_eq!(spans.len(), 1);
        let span = &spans[0];
        assert_eq!(span.link.target, "dest.md");
        assert_eq!(
            &text[span.full_start..span.full_end],
            r#"[text](<dest.md> "a (note)")"#
        );
    }

    #[test]
    fn angle_bracket_destination_with_single_quote_and_paren_titles() {
        for text in [
            r"[text](<dest.md> 'a (note)')",
            r"[text](<dest.md> (a note))",
        ] {
            let mut links = Vec::new();
            extract_links_from_text(text, &mut links);
            assert_eq!(links.len(), 1, "failed for {text}");
            assert_eq!(links[0].target, "dest.md", "failed for {text}");
        }
    }

    #[test]
    fn angle_bracket_destination_followed_by_garbage_is_not_a_link() {
        // Per CommonMark, only whitespace and an optional title may sit
        // between the closing `>` and the `)`.
        let text = "[text](<dest.md> junk)";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert!(links.is_empty());
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
