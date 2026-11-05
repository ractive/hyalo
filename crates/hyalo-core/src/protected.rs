//! Protected regions of a Markdown body: the text that is *not* prose.
//!
//! Lint prose rules, the autofixers behind `lint --fix` and `links auto` all
//! answer one question — "may I rewrite or flag these bytes?" — and until
//! iteration 321 each answered it for itself (DEC-381). [`BodySyntax`] knew
//! fences, indented code and HTML comments; every other false positive got its
//! own `if rule_id == …` branch in the lint engine, and `links auto` kept a
//! parallel, narrower scanner that knew none of HTML, math or block ids.
//!
//! This module is the single answer. [`ProtectedRegions`] records, as sorted
//! byte ranges of the body:
//!
//! * **block regions** — whole physical lines that are not Markdown: raw HTML
//!   blocks (CommonMark §4.6, including `<pre>`/`<script>`/`<style>`/
//!   `<textarea>` bodies) and `$$` math blocks. A position is protected when
//!   it lies anywhere in the range, start included.
//! * **inline regions** — spans inside prose lines: code spans, HTML tags
//!   (their attribute values too, across lines), the content of `<a>`,
//!   `<code>`, `<pre>`, `<kbd>`, `<samp>`, `<script>`, `<style>` and
//!   `<textarea>`, inline `$…$` / `$$…$$` math, `{{ … }}` / `{% … %}` macro
//!   calls, HTML comments and Obsidian block ids (`^id`). A *diagnostic*
//!   position is protected only strictly inside such a span, and a
//!   zero-width *insertion* likewise: a bare URL written right before a tag
//!   is still prose, the byte after `href="` is not.
//! * **destinations** — link destinations (`](…)`, `[ref]: …`, `<scheme:…>`
//!   autolinks). A bare URL there is already a link.
//!
//! The scan is a single forward pass that rides on the line loop of
//! [`BodySyntax::new`]; cross-line constructs (a tag whose attribute value
//! wraps, a macro, a math block) carry a *pending* start and either commit a
//! range when they close or retract it at the end of the block, so the cost
//! is linear in the body and an unclosed `<` or `{{` in prose protects
//! nothing.
//!
//! [`BodySyntax`]: crate::body_syntax::BodySyntax
//! [`BodySyntax::new`]: crate::body_syntax::BodySyntax::new

use std::ops::Range;

/// HTML elements whose content is not Markdown prose: a URL or a word inside
/// them must not be rewritten, because the replacement would land in HTML.
const RAW_ELEMENTS: &[&str] = &[
    "a", "code", "pre", "kbd", "samp", "script", "style", "textarea",
];

/// CommonMark §4.6 start condition 1: these end at their closing tag, not at
/// a blank line, and may interrupt a paragraph.
const RAW_BLOCK_ELEMENTS: &[&str] = &["pre", "script", "style", "textarea"];

/// CommonMark §4.6 start condition 6: block-level element names.
const BLOCK_ELEMENTS: &[&str] = &[
    "address",
    "article",
    "aside",
    "base",
    "basefont",
    "blockquote",
    "body",
    "caption",
    "center",
    "col",
    "colgroup",
    "dd",
    "details",
    "dialog",
    "dir",
    "div",
    "dl",
    "dt",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "frame",
    "frameset",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "hr",
    "html",
    "iframe",
    "legend",
    "li",
    "link",
    "main",
    "menu",
    "menuitem",
    "nav",
    "noframes",
    "ol",
    "optgroup",
    "option",
    "p",
    "param",
    "search",
    "section",
    "summary",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "title",
    "tr",
    "track",
    "ul",
];

/// An inline math or macro opener never searches further than this for its
/// closer on the same line (`$…$` only). Bounds the cost of a line of
/// unbalanced `$`.
const MAX_INLINE_MATH_BYTES: usize = 512;

/// The finished, queryable set of protected regions of one body.
#[derive(Debug, Default, Clone)]
pub struct ProtectedRegions {
    blocks: Vec<Range<usize>>,
    inline: Vec<Range<usize>>,
    destinations: Vec<Range<usize>>,
}

impl ProtectedRegions {
    /// Whether a diagnostic that starts at body byte `offset` points into
    /// protected text: anywhere in a block region, or strictly inside an
    /// inline region.
    #[must_use]
    pub fn contains(&self, offset: usize) -> bool {
        covers(&self.blocks, offset)
            || self
                .inline
                .get(upper(&self.inline, offset))
                .is_some_and(|r| r.start < offset && offset < r.end)
    }

    /// Whether an edit of the half-open body range `range` touches protected
    /// text. A zero-width range is an insertion point and follows the same
    /// strict-inside rule as [`Self::contains`] for inline regions.
    #[must_use]
    pub fn overlaps(&self, range: &Range<usize>) -> bool {
        if range.start >= range.end {
            return self.contains(range.start);
        }
        overlaps_any(&self.blocks, range) || overlaps_any(&self.inline, range)
    }

    /// Like [`Self::overlaps`], but a link destination counts too — what
    /// `links auto` needs: it must not put a wikilink inside `](…)`.
    #[must_use]
    pub fn overlaps_or_destination(&self, range: &Range<usize>) -> bool {
        self.overlaps(range) || overlaps_any(&self.destinations, range)
    }

    /// Whether body byte `offset` lies inside a link destination.
    #[must_use]
    pub fn in_destination(&self, offset: usize) -> bool {
        covers(&self.destinations, offset)
    }

    /// Whether body byte `offset` lies in a *whole-line* region (raw HTML or
    /// math block), as opposed to an inline span.
    #[must_use]
    pub fn in_block(&self, offset: usize) -> bool {
        covers(&self.blocks, offset)
    }

    /// The merged inline regions, ascending. Exposed for the independent
    /// checker in the test-suite.
    #[must_use]
    pub fn inline_regions(&self) -> &[Range<usize>] {
        &self.inline
    }

    /// The merged block regions, ascending.
    #[must_use]
    pub fn block_regions(&self) -> &[Range<usize>] {
        &self.blocks
    }
}

/// Index of the first range whose `end` is greater than `offset`.
fn upper(ranges: &[Range<usize>], offset: usize) -> usize {
    ranges.partition_point(|r| r.end <= offset)
}

fn covers(ranges: &[Range<usize>], offset: usize) -> bool {
    ranges
        .get(upper(ranges, offset))
        .is_some_and(|r| r.start <= offset)
}

fn overlaps_any(ranges: &[Range<usize>], range: &Range<usize>) -> bool {
    ranges
        .get(upper(ranges, range.start))
        .is_some_and(|r| r.start < range.end)
}

/// Sort and merge touching or overlapping ranges.
fn normalise(ranges: &mut Vec<Range<usize>>) {
    ranges.retain(|r| r.start < r.end);
    ranges.sort_by_key(|r| (r.start, r.end));
    let mut out: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for r in ranges.drain(..) {
        match out.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => out.push(r),
        }
    }
    *ranges = out;
}

/// How a raw HTML block ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HtmlBlockEnd {
    /// At the next blank line (start conditions 6 and 7).
    BlankLine,
    /// At the first line containing this lowercase text (conditions 1, 3-5).
    Marker(&'static str),
}

/// An HTML tag found by [`scan_tag`].
struct Tag {
    /// Byte index of the closing `>`.
    end: usize,
    name_start: usize,
    name_end: usize,
    closing: bool,
    self_closing: bool,
}

/// What [`scan_tag`] made of the text at a `<`.
enum TagScan {
    /// Not an HTML tag (an autolink, a comparison, prose).
    No,
    /// A complete tag.
    Complete(Tag),
    /// The line ended inside the tag; it may continue on the next line.
    Open {
        name_start: usize,
        name_end: usize,
        closing: bool,
        quote: Option<u8>,
        prev: u8,
    },
}

/// The pending (not yet closed) multi-line construct, if any.
#[derive(Debug, Clone)]
struct PendingTag {
    start: usize,
    name: String,
    closing: bool,
    quote: Option<u8>,
    prev: u8,
}

#[derive(Debug, Clone)]
struct PendingRaw {
    start: usize,
    name: String,
}

#[derive(Debug, Clone)]
struct PendingMacro {
    start: usize,
    closer: [u8; 2],
}

/// Builder driven by the line loop of [`crate::body_syntax::BodySyntax`].
#[derive(Debug, Default)]
pub(crate) struct ProtectedBuilder {
    blocks: Vec<Range<usize>>,
    inline: Vec<Range<usize>>,
    destinations: Vec<Range<usize>>,
    tag: Option<PendingTag>,
    raw: Option<PendingRaw>,
    macro_call: Option<PendingMacro>,
    /// Start offset of an open `$$` block and its opening line's end.
    math_block: Option<(usize, usize)>,
    /// A raw HTML block in progress, with the offset it started at.
    html_block: Option<(HtmlBlockEnd, usize)>,
    /// Offsets of `[` still unmatched at the end of the previous line, so a
    /// link label wrapping over lines is recognised when its `](` arrives.
    open_labels: Vec<usize>,
}

/// How many unmatched `[` a block remembers across lines. Bounds memory on a
/// body of nothing but brackets.
const MAX_OPEN_LABELS: usize = 8;

impl ProtectedBuilder {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn push_inline(&mut self, range: Range<usize>) {
        self.inline.push(range);
    }

    /// An inline construct cannot cross a block boundary: drop whatever was
    /// still waiting for its closer. Ranges already committed stay.
    pub(crate) fn end_block(&mut self) {
        self.tag = None;
        self.raw = None;
        self.macro_call = None;
        self.open_labels.clear();
    }

    /// Whether a `$$` block is open (as opposed to a raw HTML block).
    pub(crate) fn is_math_open(&self) -> bool {
        self.math_block.is_some()
    }

    /// Whether a raw HTML block or `$$` block owns the current line.
    pub(crate) fn in_block_region(&self) -> bool {
        self.html_block.is_some() || self.math_block.is_some()
    }

    // ------------------------------------------------------------------
    // Raw HTML blocks (CommonMark §4.6)
    // ------------------------------------------------------------------

    /// Classify `line` (container prefix already removed) as the first line of
    /// a raw HTML block. `relative_indent` is the indentation beyond the
    /// enclosing list item's content column.
    pub(crate) fn html_block_start(
        line: &str,
        relative_indent: usize,
        previous_blank: bool,
    ) -> Option<HtmlBlockEnd> {
        if relative_indent > 3 {
            return None;
        }
        let rest = line.trim_start_matches([' ', '\t']);
        let bytes = rest.as_bytes();
        if bytes.first() != Some(&b'<') {
            return None;
        }
        let lower_head: String = rest
            .chars()
            .take(12)
            .map(|c| c.to_ascii_lowercase())
            .collect();
        // Condition 1.
        for name in RAW_BLOCK_ELEMENTS {
            if let Some(after) = lower_head
                .strip_prefix('<')
                .and_then(|s| s.strip_prefix(name))
                && (after.is_empty() || after.starts_with([' ', '\t', '>']))
            {
                return Some(HtmlBlockEnd::Marker(match *name {
                    "pre" => "</pre>",
                    "script" => "</script>",
                    "style" => "</style>",
                    _ => "</textarea>",
                }));
            }
        }
        // Conditions 3, 4, 5 (condition 2, the comment, is masked elsewhere).
        if lower_head.starts_with("<?") {
            return Some(HtmlBlockEnd::Marker("?>"));
        }
        if lower_head.starts_with("<![cdata[") {
            return Some(HtmlBlockEnd::Marker("]]>"));
        }
        if lower_head.starts_with("<!") && bytes.get(2).is_some_and(u8::is_ascii_alphabetic) {
            return Some(HtmlBlockEnd::Marker(">"));
        }
        // Condition 6.
        let name_at = if bytes.get(1) == Some(&b'/') { 2 } else { 1 };
        let name_len = bytes[name_at.min(bytes.len())..]
            .iter()
            .take_while(|b| b.is_ascii_alphanumeric())
            .count();
        if name_len > 0 {
            let name = rest[name_at..name_at + name_len].to_ascii_lowercase();
            let after = &rest[name_at + name_len..];
            if BLOCK_ELEMENTS.contains(&name.as_str())
                && (after.is_empty()
                    || after.starts_with([' ', '\t', '>'])
                    || after.starts_with("/>"))
            {
                return Some(HtmlBlockEnd::BlankLine);
            }
        }
        // Condition 7: one complete open or close tag alone on the line; it
        // cannot interrupt a paragraph.
        if previous_blank
            && let TagScan::Complete(tag) = scan_tag(bytes, 0, None, 0)
            && bytes[tag.end + 1..]
                .iter()
                .all(|b| matches!(b, b' ' | b'\t' | b'\r'))
        {
            let name = rest[tag.name_start..tag.name_end].to_ascii_lowercase();
            if !RAW_BLOCK_ELEMENTS.contains(&name.as_str()) {
                return Some(HtmlBlockEnd::BlankLine);
            }
        }
        None
    }

    /// Begin a raw HTML block on the line spanning `line_start..line_end`
    /// (physical, newline included). Returns `true` when the block already
    /// ended on this same line.
    pub(crate) fn begin_html_block(
        &mut self,
        end: HtmlBlockEnd,
        text: &str,
        line_start: usize,
        line_end: usize,
    ) -> bool {
        self.end_block();
        let closed_here = match &end {
            HtmlBlockEnd::BlankLine => false,
            HtmlBlockEnd::Marker(marker) => {
                // The marker may not overlap the opener itself: `<pre>` does
                // not contain `</pre>`, but `<?` must be followed by `?>`.
                let opener_len = if *marker == "?>" || *marker == ">" {
                    2
                } else {
                    0
                };
                text.get(opener_len..)
                    .is_some_and(|t| t.to_ascii_lowercase().contains(marker))
            }
        };
        if closed_here {
            self.blocks.push(line_start..line_end);
        } else {
            self.html_block = Some((end, line_start));
        }
        closed_here
    }

    /// Feed a line to the raw HTML block in progress. Returns `true` when the
    /// line belongs to it (and is protected); `false` when the block ended
    /// before this line (a blank line for conditions 6 and 7).
    pub(crate) fn continue_html_block(
        &mut self,
        text: &str,
        blank: bool,
        line_start: usize,
        line_end: usize,
    ) -> bool {
        let Some((end, start)) = self.html_block.take() else {
            return false;
        };
        match end {
            HtmlBlockEnd::BlankLine => {
                if blank {
                    self.blocks.push(start..line_start);
                    false
                } else {
                    self.html_block = Some((HtmlBlockEnd::BlankLine, start));
                    true
                }
            }
            HtmlBlockEnd::Marker(marker) => {
                if text.to_ascii_lowercase().contains(marker) {
                    self.blocks.push(start..line_end);
                } else {
                    self.html_block = Some((HtmlBlockEnd::Marker(marker), start));
                }
                true
            }
        }
    }

    // ------------------------------------------------------------------
    // `$$` math blocks
    // ------------------------------------------------------------------

    /// Handle a `$$` display-math block. Returns `true` when `text` (the line,
    /// container prefix removed) is consumed by one.
    pub(crate) fn math_block_line(
        &mut self,
        text: &str,
        line_start: usize,
        line_end: usize,
    ) -> bool {
        if let Some((start, _)) = self.math_block {
            if text.contains("$$") {
                self.math_block = None;
                self.blocks.push(start..line_end);
            }
            return true;
        }
        let trimmed = text.trim_start_matches([' ', '\t']);
        let Some(after) = trimmed.strip_prefix("$$") else {
            return false;
        };
        if after.contains("$$") {
            // Opens and closes on one line.
            self.blocks.push(line_start..line_end);
        } else {
            self.math_block = Some((line_start, line_end));
        }
        true
    }

    // ------------------------------------------------------------------
    // Inline scan
    // ------------------------------------------------------------------

    /// Scan one prose line. `line` is the visible (code spans and comments
    /// already blanked) text of the line, container prefix removed, and
    /// `abs` the body offset of its first byte.
    pub(crate) fn scan_inline_line(&mut self, line: &[u8], abs: usize) {
        let n = line.len();
        let mut i = 0usize;
        let mut dest_search = true;
        if let Some(def) = reference_definition_destination(line) {
            self.destinations.push(abs + def.start..abs + def.end);
        }
        while i < n {
            if let Some(pending) = self.raw.clone() {
                match find_closing_tag(line, i, &pending.name) {
                    Some(end) => {
                        self.inline.push(pending.start..abs + end);
                        self.raw = None;
                        i = end;
                    }
                    None => return,
                }
                continue;
            }
            if let Some(pending) = self.macro_call.clone() {
                match find_pair(line, i, pending.closer) {
                    Some(j) => {
                        self.inline.push(pending.start..abs + j + 2);
                        self.macro_call = None;
                        i = j + 2;
                    }
                    None => return,
                }
                continue;
            }
            if let Some(pending) = self.tag.clone() {
                match continue_tag(line, i, pending.quote, pending.prev) {
                    TagScan::Complete(tag) => {
                        let tag = Tag {
                            closing: pending.closing,
                            ..tag
                        };
                        self.finish_tag(line, abs, pending.start, &pending.name, &tag);
                        self.tag = None;
                        i = tag.end + 1;
                    }
                    TagScan::Open { quote, prev, .. } => {
                        self.tag = Some(PendingTag {
                            quote,
                            prev,
                            ..pending
                        });
                        return;
                    }
                    TagScan::No => {
                        self.tag = None;
                    }
                }
                continue;
            }
            match line[i] {
                b'\\' => i += 2,
                b'<' => match scan_tag(line, i, None, 0) {
                    TagScan::Complete(tag) => {
                        let name = ascii_lower(&line[tag.name_start..tag.name_end]);
                        self.finish_tag(line, abs, abs + i, &name, &tag);
                        i = tag.end + 1;
                    }
                    TagScan::Open {
                        name_start,
                        name_end,
                        closing,
                        quote,
                        prev,
                    } => {
                        self.tag = Some(PendingTag {
                            start: abs + i,
                            name: ascii_lower(&line[name_start..name_end]),
                            closing,
                            quote,
                            prev,
                        });
                        return;
                    }
                    TagScan::No => {
                        if let Some(end) = autolink_end(line, i) {
                            self.destinations.push(abs + i + 1..abs + end);
                            i = end + 1;
                        } else {
                            i += 1;
                        }
                    }
                },
                b'{' if matches!(line.get(i + 1), Some(b'{' | b'%')) => {
                    let closer = if line[i + 1] == b'{' { *b"}}" } else { *b"%}" };
                    if let Some(j) = find_pair(line, i + 2, closer) {
                        self.inline.push(abs + i..abs + j + 2);
                        i = j + 2;
                    } else {
                        self.macro_call = Some(PendingMacro {
                            start: abs + i,
                            closer,
                        });
                        return;
                    }
                }
                b'$' => {
                    if let Some(end) = inline_math_end(line, i) {
                        self.inline.push(abs + i..abs + end);
                        i = end;
                    } else {
                        i += 1 + usize::from(line.get(i + 1) == Some(&b'$'));
                    }
                }
                b'[' => {
                    if self.open_labels.len() < MAX_OPEN_LABELS {
                        self.open_labels.push(abs + i);
                    }
                    i += 1;
                }
                b']' if dest_search && line.get(i + 1) == Some(&b'(') => {
                    // A label that began on an earlier line is text of a
                    // link, not prose to rewrite.
                    if let Some(open) = self.open_labels.pop()
                        && open < abs
                    {
                        self.inline.push(open..abs + i);
                    }
                    if let Some(close) = destination_end(line, i + 2) {
                        self.destinations.push(abs + i + 2..abs + close);
                        i = close + 1;
                    } else {
                        // No `)` after this `](` means none after a later
                        // one either: stop looking on this line.
                        dest_search = false;
                        i += 1;
                    }
                }
                b']' => {
                    self.open_labels.pop();
                    i += 1;
                }
                _ => i += 1,
            }
        }
        if let Some(id) = trailing_block_id(line) {
            self.inline.push(abs + id.start..abs + id.end);
        }
    }

    /// Record a completed tag and, for an opening raw-text element, start
    /// looking for its closing tag.
    fn finish_tag(&mut self, line: &[u8], abs: usize, tag_start_abs: usize, name: &str, tag: &Tag) {
        let end = tag.end;
        self.inline.push(tag_start_abs..abs + end + 1);
        if !tag.closing && !tag.self_closing && RAW_ELEMENTS.contains(&name) {
            match find_closing_tag(line, end + 1, name) {
                Some(close_end) => self.inline.push(abs + end + 1..abs + close_end),
                None => {
                    self.raw = Some(PendingRaw {
                        start: abs + end + 1,
                        name: name.to_owned(),
                    });
                }
            }
        }
    }

    /// Finish the body: an unclosed `$$` block protects its opening line
    /// only, and every other pending construct is dropped.
    pub(crate) fn finish(mut self) -> ProtectedRegions {
        if let Some((start, end)) = self.math_block.take() {
            self.blocks.push(start..end);
        }
        if let Some((_, start)) = self.html_block.take() {
            // A condition 1/3-5 block with no closer runs to the end of the
            // document; a condition 6/7 block ends there too.
            self.blocks.push(start..usize::MAX);
        }
        normalise(&mut self.blocks);
        normalise(&mut self.inline);
        normalise(&mut self.destinations);
        ProtectedRegions {
            blocks: self.blocks,
            inline: self.inline,
            destinations: self.destinations,
        }
    }
}

// ----------------------------------------------------------------------
// Scanners over a single line of bytes
// ----------------------------------------------------------------------

fn ascii_lower(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| char::from(b.to_ascii_lowercase()))
        .collect()
}

/// Scan an HTML tag starting at `line[i] == b'<'`.
fn scan_tag(line: &[u8], i: usize, quote: Option<u8>, prev: u8) -> TagScan {
    let mut j = i + 1;
    let closing = line.get(j) == Some(&b'/');
    if closing {
        j += 1;
    }
    if !line.get(j).is_some_and(u8::is_ascii_alphabetic) {
        return TagScan::No;
    }
    let name_start = j;
    while line
        .get(j)
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'-')
    {
        j += 1;
    }
    let name_end = j;
    // After the name: whitespace, `/`, `>` or the end of the line. A `:` (an
    // autolink `<https://…>`) or any other byte means this is not a tag.
    if let Some(b) = line.get(j)
        && !matches!(b, b' ' | b'\t' | b'/' | b'>' | b'\r')
    {
        return TagScan::No;
    }
    match continue_tag(line, j, quote, prev) {
        TagScan::Open { quote, prev, .. } => TagScan::Open {
            name_start,
            name_end,
            closing,
            quote,
            prev,
        },
        TagScan::Complete(mut tag) => {
            tag.name_start = name_start;
            tag.name_end = name_end;
            tag.closing = closing;
            TagScan::Complete(tag)
        }
        TagScan::No => TagScan::No,
    }
}

/// Continue scanning a tag's attributes from `line[from..]`. `prev` is the
/// last significant byte seen so far (`=` makes the next quote an attribute
/// value, `/` before `>` a self-closing tag).
fn continue_tag(line: &[u8], from: usize, mut quote: Option<u8>, mut prev: u8) -> TagScan {
    for (j, &b) in line.iter().enumerate().skip(from) {
        if let Some(q) = quote {
            if b == q {
                quote = None;
                prev = b;
            }
            continue;
        }
        match b {
            b'>' => {
                return TagScan::Complete(Tag {
                    end: j,
                    name_start: 0,
                    name_end: 0,
                    closing: false,
                    self_closing: prev == b'/',
                });
            }
            b'"' | b'\'' if prev == b'=' => quote = Some(b),
            b'"' | b'\'' | b'<' => return TagScan::No,
            b' ' | b'\t' | b'\r' => {}
            _ => prev = b,
        }
    }
    TagScan::Open {
        name_start: 0,
        name_end: 0,
        closing: false,
        quote,
        prev,
    }
}

/// Byte index just past the first `</name …>` at or after `from`, matched
/// ASCII case-insensitively.
fn find_closing_tag(line: &[u8], from: usize, name: &str) -> Option<usize> {
    let mut j = from;
    while j + 1 < line.len() {
        if line[j] == b'<' && line[j + 1] == b'/' {
            let name_at = j + 2;
            let matches = line
                .get(name_at..name_at + name.len())
                .is_some_and(|s| s.eq_ignore_ascii_case(name.as_bytes()))
                && line
                    .get(name_at + name.len())
                    .is_none_or(|b| matches!(b, b'>' | b' ' | b'\t' | b'\r'));
            if matches {
                let gt = line[name_at + name.len()..].iter().position(|b| *b == b'>');
                return gt.map(|g| name_at + name.len() + g + 1);
            }
        }
        j += 1;
    }
    None
}

/// Index of the first `pair` at or after `from`.
fn find_pair(line: &[u8], from: usize, pair: [u8; 2]) -> Option<usize> {
    line.get(from..)?
        .windows(2)
        .position(|w| w == pair)
        .map(|p| from + p)
}

/// End (exclusive) of an inline math span opened by the `$` at `line[i]`.
fn inline_math_end(line: &[u8], i: usize) -> Option<usize> {
    if i > 0 && line[i - 1] == b'\\' {
        return None;
    }
    if line.get(i + 1) == Some(&b'$') {
        // `$$ … $$` on one line.
        return find_pair(line, i + 2, *b"$$").map(|j| j + 2);
    }
    let next = *line.get(i + 1)?;
    if next.is_ascii_whitespace() {
        return None;
    }
    let limit = (i + 1 + MAX_INLINE_MATH_BYTES).min(line.len());
    let mut j = i + 1;
    while j < limit {
        if line[j] == b'\\' {
            j += 2;
            continue;
        }
        if line[j] == b'$'
            && !line[j - 1].is_ascii_whitespace()
            && !line.get(j + 1).is_some_and(u8::is_ascii_digit)
        {
            return Some(j + 1);
        }
        j += 1;
    }
    None
}

/// Index of the `)` closing the inline link whose `(` is just before `from`,
/// following CommonMark §6.3: optional whitespace, a destination (`<…>` or a
/// run of non-space bytes with balanced parentheses), an optional quoted
/// title, optional whitespace, `)`. `None` when the text after `](` is not a
/// link destination — `](https://x.org is useful.` is prose that lost its
/// closing parenthesis, not a link, so a URL in it is still bare.
fn destination_end(line: &[u8], from: usize) -> Option<usize> {
    let skip_ws = |mut j: usize| {
        while line.get(j).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            j += 1;
        }
        j
    };
    let mut j = skip_ws(from);
    if line.get(j) == Some(&b'<') {
        j += 1;
        loop {
            match line.get(j)? {
                b'>' => break,
                b'<' | b'\n' => return None,
                b'\\' => j += 2,
                _ => j += 1,
            }
        }
        j += 1;
    } else {
        let mut depth = 0usize;
        while let Some(&b) = line.get(j) {
            match b {
                b'\\' => j += 1,
                b'(' => depth += 1,
                b')' if depth == 0 => break,
                b')' => depth -= 1,
                b' ' | b'\t' | b'\r' => break,
                _ => {}
            }
            j += 1;
        }
    }
    j = skip_ws(j);
    if let Some(&open) = line.get(j)
        && matches!(open, b'"' | b'\'' | b'(')
    {
        let close = if open == b'(' { b')' } else { open };
        j += 1;
        loop {
            match line.get(j)? {
                b'\\' => j += 1,
                b if *b == close => break,
                _ => {}
            }
            j += 1;
        }
        j = skip_ws(j + 1);
    }
    (line.get(j) == Some(&b')')).then_some(j)
}

/// The index of the closing `>` of an autolink `<scheme:…>` at `line[i]`.
fn autolink_end(line: &[u8], i: usize) -> Option<usize> {
    let mut j = i + 1;
    if !line.get(j).is_some_and(u8::is_ascii_alphabetic) {
        return None;
    }
    while line
        .get(j)
        .is_some_and(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'.' | b'-'))
    {
        j += 1;
    }
    if !(3..=33).contains(&(j - i)) || line.get(j) != Some(&b':') {
        return None;
    }
    while let Some(b) = line.get(j) {
        match b {
            b'>' => return Some(j),
            b' ' | b'\t' | b'<' => return None,
            _ => j += 1,
        }
    }
    None
}

/// `[label]: destination "title"`: the destination token's range.
fn reference_definition_destination(line: &[u8]) -> Option<Range<usize>> {
    let indent = line.iter().take_while(|b| **b == b' ').count();
    if indent > 3 || line.get(indent) != Some(&b'[') {
        return None;
    }
    let close = line[indent..].iter().position(|b| *b == b']')? + indent;
    if line.get(close + 1) != Some(&b':') {
        return None;
    }
    let mut start = close + 2;
    while line.get(start).is_some_and(|b| matches!(b, b' ' | b'\t')) {
        start += 1;
    }
    let len = line[start..]
        .iter()
        .take_while(|b| !matches!(b, b' ' | b'\t' | b'\r'))
        .count();
    (len > 0).then_some(start..start + len)
}

/// The `^block-id` that ends the line, when preceded by whitespace or at the
/// line start (Obsidian block identifiers).
fn trailing_block_id(line: &[u8]) -> Option<Range<usize>> {
    let end = line
        .iter()
        .rposition(|b| !matches!(b, b' ' | b'\t' | b'\r'))?
        + 1;
    let id_len = line[..end]
        .iter()
        .rev()
        .take_while(|b| b.is_ascii_alphanumeric() || **b == b'-')
        .count();
    if id_len == 0 {
        return None;
    }
    let caret = end.checked_sub(id_len + 1)?;
    if line[caret] != b'^' {
        return None;
    }
    (caret == 0 || matches!(line[caret - 1], b' ' | b'\t')).then_some(caret..end)
}

#[cfg(test)]
mod tests {
    use crate::body_syntax::BodySyntax;

    fn regions(body: &str) -> super::ProtectedRegions {
        BodySyntax::with_protected(body)
            .protected()
            .cloned()
            .unwrap_or_default()
    }

    /// Whether `needle` (first occurrence) is protected as a diagnostic start.
    fn hit(body: &str, needle: &str) -> bool {
        let at = body.find(needle).expect("needle in body");
        regions(body).contains(at)
    }

    #[test]
    fn attribute_values_and_raw_elements_are_protected() {
        let body = "Text <a href=\"https://a.org/x\">https://b.org/y</a> and https://c.org/z\n";
        assert!(hit(body, "https://a.org"), "attribute value");
        assert!(hit(body, "https://b.org"), "content of <a>");
        assert!(!hit(body, "https://c.org"), "prose after the element");
        let body = "<p><code>https://a.org/some/path/</code> and https://prose.org</p>\n";
        assert!(hit(body, "https://a.org"));
    }

    #[test]
    fn a_tag_wrapping_over_lines_protects_its_attribute_value() {
        let body = "Intro <img\n  src=\"https://a.org/i.png\"\n  alt=\"x\"> tail https://p.org\n";
        assert!(hit(body, "https://a.org"));
        assert!(!hit(body, "https://p.org"));
    }

    #[test]
    fn an_unclosed_angle_bracket_protects_nothing() {
        let body = "if a <b then\nsee https://p.org/x\n\nnext https://q.org\n";
        assert!(!hit(body, "https://p.org"));
        assert!(!hit(body, "https://q.org"));
    }

    #[test]
    fn raw_html_blocks_are_protected_to_the_blank_line() {
        let body = "<div class=\"x\">\n# not a heading\nhttps://a.org\n</div>\n\nhttps://p.org\n";
        assert!(hit(body, "# not"));
        assert!(hit(body, "https://a.org"));
        assert!(!hit(body, "https://p.org"));
    }

    #[test]
    fn pre_block_survives_blank_lines_and_indent_under_a_list() {
        let body = "1. step\n\n   <pre>\n   [c]\n\n   # ipv6 = 1\n   </pre>\n\nafter\n";
        assert!(hit(body, "# ipv6"));
        assert!(hit(body, "[c]"));
        assert!(!hit(body, "after"));
    }

    #[test]
    fn math_macros_and_block_ids_are_protected() {
        let body = "$$\na = https://m.org/x\n$$\n\ninline $a = b$ and {{ domxref(\"https://g.com\") }} text ^youtube\n";
        assert!(hit(body, "https://m.org"));
        assert!(hit(body, "a = b"));
        assert!(hit(body, "https://g.com"));
        let id = body.find("^youtube").unwrap_or(0);
        assert!(regions(body).overlaps(&(id..id + 8)), "block id");
        assert!(regions(body).overlaps(&(id + 1..id + 8)), "its name alone");
    }

    #[test]
    fn a_link_label_wrapping_over_lines_is_protected() {
        let body =
            "A [two line\nlink text https://t.example/z](https://d.example/q) end https://p.org\n";
        assert!(hit(body, "https://t.example"), "label text");
        assert!(!hit(body, "https://p.org"), "prose after the link");
        assert!(regions(body).in_destination(body.find("https://d.example").unwrap_or(0)));
    }

    #[test]
    fn a_destination_with_a_title_or_angle_brackets_is_recognised() {
        let body = "[a](https://a.org/x \"Title\") and [b](<https://b.org/y z>) and [c](https://c.org/(d))\n";
        let r = regions(body);
        for url in ["https://a.org", "https://b.org", "https://c.org"] {
            assert!(r.in_destination(body.find(url).unwrap_or(0)), "{url}");
        }
    }

    #[test]
    fn text_after_an_unclosed_parenthesis_is_not_a_destination() {
        let body = "[video](https://www.youtube.com/watch?v=x is useful (really).\n";
        let at = body.find("https://").unwrap_or(0);
        assert!(!regions(body).in_destination(at));
    }

    #[test]
    fn an_unclosed_label_protects_nothing() {
        let body = "see [one\nand https://p.org here\n";
        assert!(!hit(body, "https://p.org"));
    }

    #[test]
    fn dollar_amounts_are_not_math() {
        let body = "It costs $5 and $10 today\n";
        assert!(!hit(body, "5 and"));
    }

    #[test]
    fn unclosed_macro_and_math_block_retract() {
        let body = "{{ never closed\nhttps://p.org\n\nhttps://q.org\n";
        assert!(!hit(body, "https://p.org"));
        let body = "$$\nnever closed\n\ntext https://q.org\n";
        assert!(!hit(body, "text"));
    }

    #[test]
    fn link_destinations_are_recorded() {
        let body = "[t](https://a.org/x) and [r]\n\n[r]: https://b.org/y \"T\"\n";
        let r = regions(body);
        assert!(r.in_destination(body.find("https://a.org").unwrap_or(0)));
        assert!(r.in_destination(body.find("https://b.org").unwrap_or(0)));
        assert!(!r.in_destination(body.find(" and").unwrap_or(0)));
    }

    #[test]
    fn scanning_is_linear_on_a_pathological_body() {
        let body = "`a [b](c) $x <y {{ ^\n".repeat(20_000);
        let start = std::time::Instant::now();
        let _ = BodySyntax::with_protected(&body);
        assert!(
            start.elapsed() < std::time::Duration::from_secs(5),
            "protected scan took {:?}",
            start.elapsed()
        );
    }
}
