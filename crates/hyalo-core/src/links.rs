#![allow(clippy::missing_errors_doc)]
use std::collections::HashMap;

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
    /// `true` when the link was written as an embed — `![[target]]`.
    ///
    /// iter-261 (UX-6): an embed is still a `Wikilink` for every resolution
    /// purpose, but the reported link `kind` distinguishes it so a caller can
    /// bucket `![[img.png]]` apart from `[[note]]` without re-reading the file.
    /// Skipped from JSON when `false`, and defaulted on load, so an index
    /// written by an older hyalo keeps deserializing.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub embed: bool,
    /// `true` when the target carries a URI scheme (`https:`, `obsidian://`,
    /// `mailto:`, `file://`, `zotero:`) and therefore names something outside
    /// the vault.
    ///
    /// iter-261 (BUG-2): such a link is inventoried so `--fields links` can
    /// report it with `kind: "external"`, but it is never resolved, never
    /// counted broken, never a graph edge and never rewritten. `target` holds
    /// the URI verbatim — no fragment or query split — so the reported text
    /// matches the source byte for byte.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub external: bool,
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
            && let Some((mut link, end)) = try_parse_wikilink_at(cleaned, i + 1)
        {
            // iter-261 (UX-6): `![[…]]` is an embed. Resolution is identical to
            // a plain wikilink; only the reported `kind` differs.
            link.embed = true;
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

/// Find the absolute byte offset of the first unescaped occurrence of
/// `needle` in `bytes` at or after `start`, skipping backslash-escaped
/// bytes the same way [`is_escaped`] defines escaping.
///
/// A plain `bytes[start..].find(needle)` (or `str::find`) stops at the
/// first literal occurrence regardless of a preceding `\`, which is wrong
/// for any escapable delimiter — e.g. a reference-definition title
/// `"A \"Gamma\" title"` closes at the first `\"` instead of the real
/// closing quote (iter-217 review C4).
fn find_unescaped_byte(bytes: &[u8], start: usize, needle: u8) -> Option<usize> {
    let mut i = start;
    while i < bytes.len() {
        if bytes[i] == needle && !is_escaped(bytes, i) {
            return Some(i);
        }
        i += 1;
    }
    None
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
            span.link.embed = true;
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
    // The target ends at the alias pipe or at `#` (fragment), whichever comes
    // first. `split_wikilink_alias` owns the pipe half so a table-escaped
    // `\|` leaves its backslash *outside* the rewritable span (BUG-7): a
    // rewrite splices only the target bytes and the `\|alias` tail survives
    // byte-for-byte, keeping the markdown table row valid.
    let (alias_target_end, _) = split_wikilink_alias(inner);
    let target_end_in_inner = [Some(alias_target_end), inner.find('#')]
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
    let target_end_in_raw = target_raw.find(['#', '?']).unwrap_or(target_raw.len());

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

/// Split a wikilink's inner text at the alias pipe.
///
/// Returns `(target_end, alias_start)` as byte offsets into `inner`:
/// `inner[..target_end]` is the target text as written (fragment included) and
/// `inner[alias_start..]` is the alias, when there is one.
///
/// iter-261 / BUG-7: Obsidian escapes the alias pipe as `\|` when a wikilink is
/// written inside a markdown table, because a bare `|` would end the table
/// cell. `[[obsidian-advanced-uri\|Advanced URI Plugin]]` therefore has to
/// yield the target `obsidian-advanced-uri`, not `obsidian-advanced-uri\`.
/// The backslash is excluded from the target span, so a rewrite that splices a
/// new target in keeps the `\|` bytes — and the table row — intact.
///
/// A trailing backslash that is *not* followed by a pipe is dropped from the
/// target too: no vault path ends in one, and Obsidian ignores it.
pub(crate) fn split_wikilink_alias(inner: &str) -> (usize, Option<usize>) {
    let (mut target_end, alias_start) = match inner.find('|') {
        Some(pipe_pos) => {
            let escaped = pipe_pos > 0 && inner.as_bytes()[pipe_pos - 1] == b'\\';
            let target_end = if escaped { pipe_pos - 1 } else { pipe_pos };
            (target_end, Some(pipe_pos + 1))
        }
        None => (inner.len(), None),
    };
    while target_end > 0 && inner.as_bytes()[target_end - 1] == b'\\' {
        target_end -= 1;
    }
    (target_end, alias_start)
}

/// Parse the inner content of a wikilink (between [[ and ]]).
/// Handles: target, target|label, target\|label, target#heading, target#^block-id
#[must_use]
pub(crate) fn parse_wikilink(inner: &str) -> Option<Link> {
    if inner.is_empty() {
        return None;
    }

    // Split on the alias pipe (escaped `\|` included — see BUG-7).
    let (target_end, alias_start) = split_wikilink_alias(inner);
    let target_part = &inner[..target_end];
    let label = alias_start.map(|start| inner[start..].to_string());

    // A `scheme:` target inside `[[…]]` is a URI, not a vault path: keep it
    // verbatim (no fragment split, no `.md` strip) and flag it external.
    if is_external_target(target_part) {
        return Some(Link {
            target: target_part.to_string(),
            label,
            kind: LinkKind::Wikilink,
            fragment: None,
            query: None,
            embed: false,
            external: true,
        });
    }

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
        embed: false,
        external: false,
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

    // Skip empty targets
    if target_raw.is_empty() {
        return None;
    }

    // iter-261 / BUG-2: an external destination is *inventoried*, not dropped.
    // It keeps the URI verbatim — splitting the `?query` off truncated
    // `obsidian://show-plugin?id=x` to `obsidian://show-plugin` in every
    // report — and is flagged so no consumer tries to resolve it.
    if is_external(target_raw) {
        return Some((
            Link {
                target: target_raw.to_owned(),
                label: (!label_text.is_empty()).then(|| label_text.to_owned()),
                kind: LinkKind::Markdown,
                fragment: None,
                query: None,
                embed: false,
                external: true,
            },
            paren_start + dest.end,
        ));
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
        embed: false,
        external: false,
    })
}

/// Check if a target names something outside the vault, i.e. it starts with a
/// URI scheme.
///
/// iter-261 / BUG-2: this used to accept only `http://`, `https://` and
/// `mailto:`, so an Obsidian vault's `obsidian://show-plugin?id=x`,
/// `file:///x` or `zotero://select/...` destinations were parsed as vault
/// paths and reported broken — 2897 of them on the Obsidian Hub vault alone.
/// The rule is now the RFC 3986 grammar for a scheme:
///
/// ```text
/// scheme = ALPHA *( ALPHA / DIGIT / "+" / "-" / "." ) ":"
/// ```
///
/// with one deliberate deviation: a **single-letter** scheme is rejected, so a
/// Windows drive letter (`C:\notes\x.md`, `c:/notes/x.md`) stays a path. RFC
/// 3986 does allow one-letter schemes, but none is registered and every real
/// one-letter `x:` in a vault is a drive letter.
#[must_use]
pub fn is_external_target(target: &str) -> bool {
    let bytes = target.as_bytes();
    let Some(&first) = bytes.first() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    let mut i = 1;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b':' {
            // Single-letter scheme → Windows drive letter, not a URI.
            return i >= 2;
        }
        if !(b.is_ascii_alphanumeric() || b == b'+' || b == b'-' || b == b'.') {
            return false;
        }
        i += 1;
    }
    false
}

/// Internal alias kept for the parser's own call sites.
fn is_external(target: &str) -> bool {
    is_external_target(target)
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
///
/// End offset of the physical line containing byte offset `start` within
/// `line` — the position of the next `\n` at or after `start`, or
/// `line.len()` if `start` is on the last line.
///
/// Used as the "unterminated construct" fallback for Liquid/HTML spans
/// (iter-217 review #4): `line` may be a whole paragraph block's `\n`-joined
/// text, not a single physical line, so falling back to `line.len()`
/// blanked every *following* line in the paragraph too — a real recall
/// loss for something as ordinary as a stray `<` (`"Compare a <b and
/// c.\nMention target here."` matched nothing at all). A genuinely
/// multi-line-wrapped construct is unaffected: its closer is found by the
/// caller's own search, and this fallback is only reached when no closer
/// exists anywhere in the rest of the block — at that point "blank to the
/// end of the block" and "blank to the end of this line" are equally
/// unable to find the (nonexistent) real closer, so the smaller, safer
/// scope is strictly better.
fn end_of_line_from(line: &str, start: usize) -> usize {
    line[start..]
        .find('\n')
        .map_or(line.len(), |rel| start + rel)
}

/// End offset (exclusive) of a Liquid/Jinja template expression starting at
/// `start`, which must point at a `{` byte (iter-207, BUG-2).
///
/// Recognizes `{% … %}` (tags) and `{{ … }}` (output expressions). An
/// unterminated marker makes the rest of the *physical line* inert (see
/// [`end_of_line_from`]) — a template expression that continues onto the
/// next line is still not prose, and blanking too much only costs a missed
/// auto-link candidate.
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
        None => end_of_line_from(line, start),
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
/// An unterminated tag makes the rest of the *physical line* inert (see
/// [`end_of_line_from`]) — a tag that wraps onto the next line is still
/// markup, and is handled by the caller's own search finding the closer
/// there. Returns `None` when `<` is ordinary prose (`a < b`), so
/// comparison operators stay linkable.
fn html_span_end(line: &str, start: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    debug_assert_eq!(bytes.get(start), Some(&b'<'));

    // `<!-- … -->` comment: closed only by the literal `-->`.
    if line[start..].starts_with("<!--") {
        return Some(match line[start + 4..].find("-->") {
            Some(rel) => start + 4 + rel + 3,
            None => end_of_line_from(line, start),
        });
    }
    // `<? … ?>` processing instruction.
    if line[start..].starts_with("<?") {
        return Some(match line[start + 2..].find("?>") {
            Some(rel) => start + 2 + rel + 2,
            None => end_of_line_from(line, start),
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
    // Unterminated: treat the rest of the physical line as markup.
    Some(end_of_line_from(line, start))
}

/// Map every `[` byte in `line` to its nesting-balanced matching `]`, if
/// any (iter-217 review #5/#6), for [`inert_link_zones`]'s generic bracket
/// fallback.
///
/// Brackets are matched by character alone — escaped `\[`/`\]` count too.
/// That is deliberate and differs from [`find_label_close_bracket`]: this
/// map answers "does inserting markup near this position touch
/// bracket-shaped text a user wrote" (so an escaped `\[widget\]` is exactly
/// as off-limits as an unescaped one), not "is this valid CommonMark link
/// syntax" — the wikilink and markdown-link-destination checks earlier in
/// `inert_link_zones` already handle *that* distinction and do respect
/// escaping. A single stack-based pass, O(n) regardless of how many
/// brackets never close or how deeply they nest — the naive per-`[`
/// rescan this replaced was O(n²) on a block with many unclosed openers.
fn match_brackets(line: &str) -> HashMap<usize, usize> {
    let bytes = line.as_bytes();
    if !bytes.contains(&b'[') {
        return HashMap::new();
    }
    let mut stack: Vec<usize> = Vec::new();
    let mut matches = HashMap::new();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'[' => stack.push(i),
            b']' => {
                if let Some(open) = stack.pop() {
                    matches.insert(open, i);
                }
            }
            _ => {}
        }
    }
    matches
}

#[must_use]
pub fn inert_link_zones(line: &str) -> Vec<(usize, usize)> {
    let bytes = line.as_bytes();
    let mut zones: Vec<(usize, usize)> = Vec::new();
    let bracket_matches = match_brackets(line);
    let mut i = 0usize;
    // iter-217 review #2: `find_label_close_bracket`/`.find("]]")` scan to
    // the end of `line` when nothing closes. Once a search from some
    // position finds no closer anywhere ahead, no later (further-right)
    // starting position can find one either — the suffix only shrinks.
    // These flags let every subsequent `[`/`[[` skip straight past the
    // rescan, making the wikilink and markdown-link-destination checks
    // O(n) instead of O(n²) on a block with many never-closing openers
    // (measured 11.5s -> effectively 0 on a 32k-line single block).
    let mut no_label_close_ahead = false;
    let mut no_wikilink_close_ahead = false;

    while i < bytes.len() {
        if !line.is_char_boundary(i) {
            i += 1;
            continue;
        }
        match bytes[i] {
            b'[' => {
                // `[[wikilink]]` — inert as a whole, alias and all.
                if !no_wikilink_close_ahead && bytes.get(i + 1) == Some(&b'[') {
                    match line[i + 2..].find("]]") {
                        Some(rel) => {
                            let end = i + 2 + rel + 2;
                            zones.push((i, end));
                            i = end;
                            continue;
                        }
                        None => no_wikilink_close_ahead = true,
                    }
                }
                let close = if no_label_close_ahead {
                    None
                } else {
                    let found = find_label_close_bracket(&line[i..]);
                    if found.is_none() {
                        no_label_close_ahead = true;
                    }
                    found
                };
                // `[label](destination)` — internal *or* external.
                if let Some(close) = close
                    && bytes.get(i + close + 1) == Some(&b'(')
                    && let Some(dest) = parse_destination(&line[i + close + 2..])
                {
                    let end = i + close + 2 + dest.end;
                    zones.push((i, end));
                    i = end;
                    continue;
                }
                // Any other well-formed `[...]` span (iter-217): GitHub-Docs-
                // and vscode-docs-style bracket conventions — style-guide
                // placeholders (`[ACCOUNT ROLE]`), PR area tags
                // (`[typescript-language-features]`), undefined CommonMark
                // shortcut references, even escaped `\[…\]` — are not
                // links, but injecting `[[target]]` immediately inside or
                // across one of them produces nested bracket soup
                // (`[[[typescript]]-language-…`) that hyalo's own wikilink
                // parser then misreads as a malformed link. Nesting is
                // balanced (`[outer [inner] more]` closes at the final
                // `]`, review #5), so nothing between the inner and outer
                // close is left unprotected either. Real corpora: without
                // this, GH Docs and vscode-docs `broken` counts both
                // increased after `--apply`. Whatever is inside stays
                // un-auto-linked; a missed candidate costs nothing,
                // corrupted brackets do.
                if let Some(&close_pos) = bracket_matches.get(&i) {
                    let end = close_pos + 1;
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
///
/// Requires `zones` to be sorted ascending by start and non-overlapping —
/// [`inert_link_zones`]'s own output already satisfies this, but a caller
/// merging zones from more than one source must restore it first (e.g. via
/// [`merge_zones`]) before calling this. Given that invariant, `end`s are
/// ascending too, so a single [`slice::partition_point`] binary search
/// suffices: find the first zone whose end is past `start`, then check
/// whether *that* zone also starts before `end` (iter-217 review #3 — the
/// previous linear scan cost O(zones) per candidate match).
#[must_use]
pub fn overlaps_zone(zones: &[(usize, usize)], start: usize, end: usize) -> bool {
    let idx = zones.partition_point(|&(_, ze)| ze <= start);
    zones.get(idx).is_some_and(|&(zs, _)| zs < end)
}

/// Sort `zones` and coalesce any that touch or overlap, in place, so the
/// result satisfies [`overlaps_zone`]'s ascending-non-overlapping
/// precondition (iter-217 review #3).
///
/// Needed whenever zones from more than one source are combined — e.g.
/// [`inert_link_zones`]'s own output (already sorted and non-overlapping)
/// plus a caller's own whole-line zones inserted afterward, which are only
/// individually sorted, not sorted *together*, and can also nest inside an
/// existing zone rather than sit beside it.
pub fn merge_zones(zones: &mut Vec<(usize, usize)>) {
    if zones.len() < 2 {
        return;
    }
    zones.sort_unstable_by_key(|&(s, _)| s);
    let mut write = 0;
    for read in 1..zones.len() {
        let (rs, re) = zones[read];
        if rs <= zones[write].1 {
            zones[write].1 = zones[write].1.max(re);
        } else {
            write += 1;
            zones[write] = (rs, re);
        }
    }
    zones.truncate(write + 1);
}

// ---------------------------------------------------------------------------
// CommonMark reference-link inert zones (iter-217, NEW-1)
// ---------------------------------------------------------------------------

/// Whether `line` is a CommonMark link reference definition
/// (`[label]: destination "title"`).
///
/// Single-line only: the destination and optional title must appear on the
/// same line as the label. A definition whose destination or title
/// continues onto a following line is not recognised (out of scope for
/// iter-217 — uncommon in practice and the corpora exercised here always
/// write definitions on one line). Up to three leading space/tab *bytes*
/// are tolerated as indent (not CommonMark's column-based block-indent
/// rule, which treats a tab as advancing to the next multiple of 4 — a tab
/// costs 1 here, so `\t\t\t\t[ref]: url` is accepted when strict CommonMark
/// would reject it as an indented code block; review #13, harmless in
/// practice since real definitions are not tab-indented). Trailing garbage
/// after a well-formed title (or after the destination, if there is no
/// title) means this is not a clean definition line — return `false`
/// rather than guess, so real prose containing a bracket is not blanked.
///
/// The caller only needs a yes/no answer (whether to treat the whole line
/// as inert), so this returns `bool` rather than the parsed label — review
/// #7, following the removal of the only consumer that needed the label
/// text itself.
#[must_use]
pub(crate) fn parse_reference_definition_label(line: &str) -> bool {
    // `line` comes from a `\n`-split iterator, so on a CRLF file it still
    // carries a trailing `\r` — left in, the final `i != bytes.len()` check
    // always fails (the trailing-whitespace loops below only skip space/tab,
    // never `\r`), so no CRLF file's definition lines were ever recognised;
    // review finding #1.
    let line = line.trim_end_matches(['\r', '\n']);
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    if indent > 3 {
        return false;
    }
    let rest = &line[indent..];
    let bytes = rest.as_bytes();
    if bytes.first() != Some(&b'[') {
        return false;
    }
    let Some(close) = find_label_close_bracket(&rest[1..]) else {
        return false;
    };
    // iter-217 review C1: for an empty label (`[]: url`, `close == 0`) this
    // is `rest[1..=0]`. Verified empirically (both a standalone slice-index
    // test and running `links auto` against a real `[]: url` line) that
    // Rust's `RangeInclusive` slice indexing handles this safely — it
    // converts to the empty range `1..1`, not a panic — so `label_raw` is
    // simply `""`, correctly rejected by the empty-check right below.
    let label_raw = &rest[1..=close];
    if label_raw.trim().is_empty() {
        return false; // CommonMark: an empty link label is invalid
    }
    let after_close = 1 + close + 1; // byte offset right after ']'
    if bytes.get(after_close) != Some(&b':') {
        return false;
    }

    let mut i = after_close + 1;
    while matches!(bytes.get(i), Some(b' ' | b'\t')) {
        i += 1;
    }
    if i >= bytes.len() {
        // No destination on this line — a multi-line definition, out of scope.
        return false;
    }

    // Destination: angle-bracket form or a bare run up to the next whitespace.
    if bytes[i] == b'<' {
        let Some(close_angle) = rest[i + 1..].find('>') else {
            return false;
        };
        i += 1 + close_angle + 1;
    } else {
        let dest_start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i == dest_start {
            return false;
        }
    }

    while matches!(bytes.get(i), Some(b' ' | b'\t')) {
        i += 1;
    }

    // Optional title: `"…"`, `'…'`, or `(…)`. The closing delimiter search
    // must skip backslash-escaped occurrences (iter-217 review C4) — a
    // title like `"A \"Gamma\" title"` otherwise closes at the first `\"`,
    // leaving `Gamma\" title"` as trailing garbage that fails the
    // end-of-line check below, so the whole definition line (title text,
    // "Gamma", included) was rejected and left open to being auto-linked.
    if i < bytes.len() {
        match bytes[i] {
            q @ (b'"' | b'\'') => {
                let Some(close_abs) = find_unescaped_byte(bytes, i + 1, q) else {
                    return false;
                };
                i = close_abs + 1;
            }
            b'(' => {
                let Some(close_abs) = find_unescaped_byte(bytes, i + 1, b')') else {
                    return false;
                };
                i = close_abs + 1;
            }
            _ => return false, // trailing content that isn't a title: not a clean definition
        }
        while matches!(bytes.get(i), Some(b' ' | b'\t')) {
            i += 1;
        }
    }

    i == bytes.len() // false if there is trailing garbage after the title
}

// Reference-link *usages* (`[label][ref]`, `[ref][]`, shortcut `[ref]`,
// `![ref][ref]`) do not need their own zone detection: `inert_link_zones`'s
// generic `[...]` fallback (above, in its own `match` arm) already treats
// every well-formed bracket span as inert regardless of whether a matching
// definition exists, which is a superset of "inert when reference-defined".
// See that function's doc comment for why the broader rule replaced a
// definition-gated one. `parse_reference_definition_label` above still
// pulls its own weight: it is what makes a definition line's destination
// and title — the part *outside* the `[label]` brackets — inert too.

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

    // --- iter-211 / BUG-12: query strings and CommonMark titles ---

    fn one_link(text: &str) -> Link {
        let mut out = Vec::new();
        extract_links_from_text(text, &mut out);
        assert_eq!(
            out.len(),
            1,
            "expected exactly one link in {text:?}: {out:?}"
        );
        out.pop().expect("one link")
    }

    #[test]
    fn query_string_is_split_off_the_target() {
        let link = one_link("see [x](/deep/page?x=1) here");
        assert_eq!(link.target, "/deep/page");
        assert_eq!(link.query.as_deref(), Some("x=1"));
        assert_eq!(link.fragment, None);
    }

    #[test]
    fn query_and_fragment_split_independently() {
        let link = one_link("see [x](/deep/page?a=1&b=2#frag) here");
        assert_eq!(link.target, "/deep/page");
        assert_eq!(link.fragment.as_deref(), Some("frag"));
        assert_eq!(link.query.as_deref(), Some("a=1&b=2"));
    }

    #[test]
    fn a_question_mark_inside_a_fragment_stays_in_the_fragment() {
        // Browsers read `#` first, so `page#frag?x` has no query at all.
        let link = one_link("see [x](page.md#frag?x) here");
        assert_eq!(link.target, "page.md");
        assert_eq!(link.fragment.as_deref(), Some("frag?x"));
        assert_eq!(link.query, None);
    }

    #[test]
    fn empty_query_is_dropped_but_trimmed_from_the_path() {
        let link = one_link("see [x](page.md?) here");
        assert_eq!(link.target, "page.md");
        assert_eq!(link.query, None);
    }

    #[test]
    fn query_span_stops_before_the_question_mark() {
        let spans = extract_link_spans("see [x](/deep/page?x=1) here");
        assert_eq!(spans.len(), 1);
        let s = &spans[0];
        assert_eq!(
            &"see [x](/deep/page?x=1) here"[s.target_start..s.target_end],
            "/deep/page"
        );
    }

    #[test]
    fn commonmark_title_is_not_part_of_the_target() {
        let link = one_link(r#"see [a](p.md "The Title") here"#);
        assert_eq!(link.target, "p.md");
        assert_eq!(link.label.as_deref(), Some("a"));
    }

    #[test]
    fn commonmark_title_forms_all_parse() {
        assert_eq!(one_link(r#"[a](p.md "d")"#).target, "p.md");
        assert_eq!(one_link("[a](p.md 'd')").target, "p.md");
        assert_eq!(one_link("[a](p.md (d))").target, "p.md");
    }

    #[test]
    fn a_title_containing_a_paren_does_not_truncate_the_span() {
        let text = r#"[a](p.md "has ) paren") tail"#;
        let spans = extract_link_spans(text);
        assert_eq!(spans.len(), 1);
        let s = &spans[0];
        assert_eq!(&text[s.target_start..s.target_end], "p.md");
        assert_eq!(
            &text[s.full_start..s.full_end],
            r#"[a](p.md "has ) paren")"#
        );
    }

    #[test]
    fn an_unencoded_space_without_a_title_keeps_the_whole_destination() {
        // Not valid CommonMark, but common in hand-written vaults — the title
        // split must not silently truncate these to `my`.
        let link = one_link("[x](my dest.md)");
        assert_eq!(link.target, "my dest.md");
    }

    #[test]
    fn a_leading_space_in_a_destination_is_left_alone() {
        let link = one_link("[x]( p.md )");
        assert_eq!(link.target, " p.md ");
    }

    // --- iter-211 / BUG-8: same-file anchors ---

    fn anchors_of(text: &str) -> Vec<String> {
        let mut links = Vec::new();
        let mut anchors = Vec::new();
        extract_links_and_self_anchors(text, text, &mut links, &mut anchors);
        anchors
    }

    #[test]
    fn same_file_markdown_anchor_is_collected() {
        assert_eq!(anchors_of("see [b](#nope) here"), vec!["nope".to_owned()]);
    }

    #[test]
    fn same_file_wikilink_anchor_is_collected() {
        assert_eq!(anchors_of("see [[#Nope]] here"), vec!["Nope".to_owned()]);
        assert_eq!(
            anchors_of("see [[#Nope|alias]] here"),
            vec!["Nope".to_owned()]
        );
    }

    #[test]
    fn a_targeted_fragment_is_not_a_same_file_anchor() {
        assert!(anchors_of("see [b](p.md#frag) here").is_empty());
        assert!(anchors_of("see [[p#frag]] here").is_empty());
    }

    #[test]
    fn same_file_anchors_do_not_become_links() {
        let mut links = Vec::new();
        let mut anchors = Vec::new();
        extract_links_and_self_anchors(
            "[b](#nope) and [[#other]] and [real](p.md)",
            "[b](#nope) and [[#other]] and [real](p.md)",
            &mut links,
            &mut anchors,
        );
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "p.md");
        assert_eq!(anchors, vec!["nope".to_owned(), "other".to_owned()]);
    }

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
    fn unterminated_tag_or_liquid_does_not_blank_the_next_line_in_a_block() {
        // iter-217 review #4: `inert_link_zones` is now called with a whole
        // paragraph block's `\n`-joined text, not always a single physical
        // line. An unterminated `<b` or `{%` used to fall back to
        // `line.len()` — the end of the *whole block* — silently blanking
        // every following line's real candidates too. The fallback must be
        // clamped to the end of the physical line the marker started on.
        assert!(!needle_is_inert(
            "Compare a <b and c.\nMention net here.",
            "net"
        ));
        assert!(!needle_is_inert(
            "Start {% ifversion x\nMention net here.",
            "net"
        ));
        assert!(!needle_is_inert(
            "<img src=\"x\" alt=\"wraps\nMention net here.",
            "net"
        ));
        // The unterminated marker itself must still be inert on its own line.
        assert!(needle_is_inert("Compare a <b and net here.\nMore.", "net"));
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

    // --- iter-217 review #5/#6: nested and escaped generic bracket spans ---

    #[test]
    fn generic_bracket_zone_balances_nested_brackets() {
        // A title-length word between the inner close and the outer close
        // must stay protected too — the whole `[outer [inner] ...]` run is
        // one construct, not just up to the first `]`.
        assert!(needle_is_inert(
            "See [outer [inner] widget stuff] here.",
            "widget"
        ));
        let line = "See [outer [inner] widget stuff] here.";
        let zones = inert_link_zones(line);
        let outer_start = line.find("[outer").unwrap();
        let outer_end = line.find("stuff]").unwrap() + "stuff]".len();
        assert!(
            zones
                .iter()
                .any(|&(s, e)| s == outer_start && e == outer_end),
            "expected one zone spanning the whole outer bracket: {zones:?}"
        );
    }

    #[test]
    fn generic_bracket_zone_covers_escaped_brackets_too() {
        // `\[...\]` is not real CommonMark link syntax (it renders as
        // literal brackets), but the raw source still has bracket
        // characters a user chose to write there — inserting `[[...]]`
        // markup touching or inside them is exactly the corruption this
        // zone exists to prevent, real link or not.
        assert!(needle_is_inert(r"\[widget config\]", "widget"));
    }

    #[test]
    fn generic_bracket_zone_still_lets_unclosed_brackets_stay_literal() {
        // No matching `]` anywhere: the `[` is just a literal character,
        // not a zone, and the word after it is still a real candidate.
        assert!(!needle_is_inert("stray [ opener with net later", "net"));
    }

    #[test]
    fn generic_bracket_zone_scan_is_not_quadratic_on_many_unclosed_openers() {
        // iter-217 review #2: a block with many `[` that never close used
        // to re-scan to the end of the block for every single one
        // (O(n^2); measured 11.5s on a 32k-line block in review). This
        // must complete quickly and still find the real mention after all
        // the noise.
        let mut line = "[".repeat(50_000);
        line.push_str(" net");
        let start = std::time::Instant::now();
        let zones = inert_link_zones(&line);
        assert!(
            start.elapsed() < std::time::Duration::from_secs(2),
            "inert_link_zones took too long on many unclosed openers: {:?}",
            start.elapsed()
        );
        let net_start = line.rfind("net").unwrap();
        assert!(
            !overlaps_zone(&zones, net_start, net_start + 3),
            "the real mention after 50,000 unclosed '[' must still be a candidate"
        );
    }

    // --- iter-217 review #3: overlaps_zone binary search + merge_zones ---

    #[test]
    fn overlaps_zone_binary_search_matches_linear_semantics() {
        let zones = vec![(0, 5), (10, 20), (25, 30)];
        assert!(overlaps_zone(&zones, 0, 5));
        assert!(overlaps_zone(&zones, 12, 15));
        assert!(overlaps_zone(&zones, 4, 11)); // straddles a gap into a zone
        assert!(!overlaps_zone(&zones, 5, 10)); // exactly the gap
        assert!(!overlaps_zone(&zones, 20, 25));
        assert!(!overlaps_zone(&zones, 30, 40));
    }

    #[test]
    fn merge_zones_coalesces_overlapping_and_nested_ranges() {
        let mut zones = vec![(10, 20), (0, 100), (5, 12), (150, 160)];
        merge_zones(&mut zones);
        assert_eq!(zones, vec![(0, 100), (150, 160)]);
    }

    #[test]
    fn merge_zones_leaves_disjoint_ranges_alone() {
        let mut zones = vec![(10, 20), (0, 5), (30, 40)];
        merge_zones(&mut zones);
        assert_eq!(zones, vec![(0, 5), (10, 20), (30, 40)]);
    }

    #[test]
    fn overlaps_zone_after_merge_finds_a_position_inside_a_formerly_nested_zone() {
        // The exact shape from auto_link.rs: a whole-line definition zone
        // (0, 100) plus a `[ref]` bracket sub-zone (5, 12) that nests
        // inside it, pushed in the "wrong" (unsorted-together) order.
        let mut zones = vec![(5, 12), (0, 100)];
        merge_zones(&mut zones);
        assert!(overlaps_zone(&zones, 7, 9), "must find the nested position");
        assert!(
            overlaps_zone(&zones, 50, 55),
            "must find any other position inside the merged span"
        );
    }

    // --- iter-217 review #1: CRLF definition lines ---

    #[test]
    fn reference_definition_recognised_on_a_crlf_line() {
        assert!(parse_reference_definition_label(
            "[Gamma]: https://example.com/g \"Gamma page\"\r"
        ));
        assert!(parse_reference_definition_label(
            "[Gamma]: https://example.com/g\r\n"
        ));
    }

    // --- direct unit tests for parse_reference_definition_label ---

    #[test]
    fn reference_definition_label_accepts_all_documented_forms() {
        assert!(parse_reference_definition_label("[ref]: /url"));
        assert!(parse_reference_definition_label("[ref]: /url \"title\""));
        assert!(parse_reference_definition_label("[ref]: /url 'title'"));
        assert!(parse_reference_definition_label("[ref]: /url (title)"));
        assert!(parse_reference_definition_label(
            "[ref]: <a url with spaces>"
        ));
        assert!(parse_reference_definition_label("   [ref]: /url")); // 3-space indent
    }

    #[test]
    fn reference_definition_label_empty_label_does_not_panic() {
        // iter-217 review C1: an empty label (`close == 0`) used to slice
        // with the inclusive range `1..=close` == `1..=0`. Verified this
        // never actually panicked (Rust converts it to the empty range
        // `1..1`), but CommonMark still says an empty link label is
        // invalid, so this must return `false`, not treat `[]: url` as a
        // definition with an empty-string label.
        assert!(!parse_reference_definition_label("[]: url"));
        assert!(!parse_reference_definition_label("[   ]: url")); // whitespace-only label
        // A handful of adjacent shapes that must not panic either.
        assert!(!parse_reference_definition_label("[]:"));
        assert!(!parse_reference_definition_label("[]"));
    }

    #[test]
    fn reference_definition_label_title_honors_backslash_escapes() {
        // iter-217 review C4: a plain (non-escape-aware) search for the
        // closing quote stops at the first `\"`, not the real one, leaving
        // trailing garbage that made the whole line fail to parse as a
        // definition — corrupting it, since the un-recognised title text
        // ("Gamma") would then be open to auto-linking.
        assert!(parse_reference_definition_label(
            r#"[ref]: /url "A \"Gamma\" title""#
        ));
        assert!(parse_reference_definition_label(
            "[ref]: /url 'A \\'Gamma\\' title'"
        ));
        assert!(parse_reference_definition_label(
            r"[ref]: /url (A \) paren title)"
        ));
        // An escaped backslash before the delimiter must NOT itself count
        // as escaping it (even number of backslashes = not escaped) — the
        // quote here closes right after the doubled backslash, so
        // everything after it is trailing garbage and the line is
        // correctly rejected.
        assert!(!parse_reference_definition_label(
            r#"[ref]: /url "A \\" trailing garbage"#
        ));
    }

    #[test]
    fn reference_definition_label_rejects_non_definitions() {
        assert!(!parse_reference_definition_label(
            "Mentioned as [Gamma] in prose."
        ));
        assert!(!parse_reference_definition_label(
            "[ref](inline-link-not-a-definition)"
        ));
        assert!(!parse_reference_definition_label("    [ref]: /url")); // 4-space indent: too much
        assert!(!parse_reference_definition_label(
            "[ref]: /url trailing garbage"
        ));
        assert!(!parse_reference_definition_label("[]: /url")); // empty label
        assert!(!parse_reference_definition_label("[ref]:")); // no destination
        assert!(!parse_reference_definition_label("not a definition at all"));
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
    fn external_markdown_links_are_inventoried_and_flagged() {
        // iter-261 / BUG-2: an external destination used to be dropped at parse
        // time. It is now kept — verbatim — and flagged, so `--fields links`
        // can report `kind: "external"` while every resolver skips it.
        let text = "[Google](https://google.com) and [[internal]]";
        let mut links = Vec::new();
        extract_links_from_text(text, &mut links);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target, "https://google.com");
        assert!(links[0].external);
        assert_eq!(links[0].label.as_deref(), Some("Google"));
        assert_eq!(links[1].target, "internal");
        assert!(!links[1].external);
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

    // -----------------------------------------------------------------
    // iter-261 / BUG-2 — external URI schemes
    // -----------------------------------------------------------------

    #[test]
    fn any_uri_scheme_is_external() {
        for target in [
            "https://a.example",
            "HTTP://a.example",
            "obsidian://show-plugin?id=x",
            "mailto:a@b.example",
            "file:///x",
            "zotero://select/items/1",
            "tel:+41000",
            "x-custom.scheme+v1://y",
        ] {
            assert!(is_external_target(target), "{target} should be external");
        }
    }

    #[test]
    fn paths_and_drive_letters_are_not_external() {
        for target in [
            "notes/a.md",
            "a.md#Section: two",
            "./a.md",
            "../out.md",
            "",
            "1scheme://x",
            // A single-letter scheme is a Windows drive letter, not a URI.
            r"C:\notes\x.md",
            "c:/notes/x.md",
        ] {
            assert!(!is_external_target(target), "{target} must not be external");
        }
    }

    #[test]
    fn external_target_keeps_its_query_string_verbatim() {
        // Before iter-261 the `?` split truncated this to
        // `obsidian://show-plugin`, which is what every report printed.
        let mut links = Vec::new();
        extract_links_from_text("[Install](obsidian://show-plugin?id=dataview)", &mut links);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "obsidian://show-plugin?id=dataview");
        assert!(links[0].external);
        assert!(links[0].query.is_none());
        assert!(links[0].fragment.is_none());
    }

    #[test]
    fn mailto_and_file_destinations_are_external_links() {
        let mut links = Vec::new();
        extract_links_from_text("[m](mailto:a@b.example) [f](file:///x)", &mut links);
        assert_eq!(links.len(), 2);
        assert!(links.iter().all(|l| l.external));
        assert_eq!(links[0].target, "mailto:a@b.example");
        assert_eq!(links[1].target, "file:///x");
    }

    #[test]
    fn drive_letter_destination_stays_a_vault_link() {
        let mut links = Vec::new();
        extract_links_from_text(r"[x](C:\notes\x.md)", &mut links);
        assert_eq!(links.len(), 1);
        assert!(!links[0].external, "a drive letter is not a URI scheme");
    }

    #[test]
    fn external_wikilink_is_flagged_and_kept_whole() {
        let mut links = Vec::new();
        extract_links_from_text("[[obsidian://open?vault=v|Open]]", &mut links);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "obsidian://open?vault=v");
        assert!(links[0].external);
        assert_eq!(links[0].label.as_deref(), Some("Open"));
    }

    #[test]
    fn external_markdown_link_produces_no_rewritable_span() {
        // The *span* extractor still drops external destinations, so `mv` and
        // `links fix` can never splice a new target into a URI.
        assert!(extract_link_spans("[x](obsidian://show-plugin?id=y)").is_empty());
    }

    // -----------------------------------------------------------------
    // iter-261 / BUG-7 — table-escaped alias pipe
    // -----------------------------------------------------------------

    #[test]
    fn escaped_alias_pipe_splits_target_and_alias() {
        let mut links = Vec::new();
        extract_links_from_text(r"[[obsidian-advanced-uri\|Advanced URI Plugin]]", &mut links);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "obsidian-advanced-uri");
        assert_eq!(links[0].label.as_deref(), Some("Advanced URI Plugin"));
    }

    #[test]
    fn escaped_alias_pipe_with_fragment_and_embed() {
        let mut links = Vec::new();
        extract_links_from_text(r"[[a#h\|b]] and ![[img.png\|200]]", &mut links);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target, "a");
        assert_eq!(links[0].fragment.as_deref(), Some("h"));
        assert_eq!(links[0].label.as_deref(), Some("b"));
        assert_eq!(links[1].target, "img.png");
        assert_eq!(links[1].label.as_deref(), Some("200"));
        assert!(links[1].embed);
    }

    #[test]
    fn unescaped_alias_pipe_is_unchanged() {
        let mut links = Vec::new();
        extract_links_from_text("[[a|b]]", &mut links);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "a");
        assert_eq!(links[0].label.as_deref(), Some("b"));
    }

    #[test]
    fn lone_trailing_backslash_is_never_part_of_the_target() {
        let mut links = Vec::new();
        extract_links_from_text(r"[[note\]]", &mut links);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "note");
    }

    #[test]
    fn escaped_pipe_leaves_its_backslash_outside_the_rewritable_span() {
        let line = r"| [[note\|Alias]] |";
        let spans = extract_link_spans(line);
        assert_eq!(spans.len(), 1);
        // The span covers `note` only, so splicing a new target keeps `\|Alias`
        // — and therefore the table row — byte-for-byte intact.
        assert_eq!(&line[spans[0].target_start..spans[0].target_end], "note");
    }

    // -----------------------------------------------------------------
    // iter-261 / UX-6 — embed flag
    // -----------------------------------------------------------------

    #[test]
    fn embeds_are_flagged_plain_wikilinks_are_not() {
        let mut links = Vec::new();
        extract_links_from_text("![[img.png]] and [[note]]", &mut links);
        assert_eq!(links.len(), 2);
        assert!(links[0].embed);
        assert!(!links[1].embed);
    }
}
