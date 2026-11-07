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
    opaque: Vec<Range<usize>>,
    opaque_inline: Vec<Range<usize>>,
}

impl ProtectedRegions {
    /// The whole-line body byte ranges the *reader* does not parse: raw HTML
    /// blocks and `$$` math blocks. Sorted, merged, may end at `usize::MAX`
    /// for a block that runs to the end of the body.
    #[must_use]
    pub fn opaque_blocks(&self) -> &[Range<usize>] {
        &self.opaque
    }

    /// The inline body byte ranges the reader does not parse: HTML tags, the
    /// content of `<code>`, `<pre>`, `<kbd>`, `<samp>`, `<script>`, `<style>`
    /// and `<textarea>`, and `$$…$$` math. Link text, single-`$` math, macros
    /// and block ids stay readable. The reader marks them rather than
    /// blanking them, so a leading `<b>` cannot turn `- [ ] x` into a task.
    #[must_use]
    pub fn opaque_inline(&self) -> &[Range<usize>] {
        &self.opaque_inline
    }

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
            // An insertion changes none of the bytes around it: it is
            // protected only strictly inside a region, a block's first byte
            // included in the open side (R13).
            let at = range.start;
            return [&self.blocks, &self.inline].iter().any(|ranges| {
                ranges
                    .get(upper(ranges, at))
                    .is_some_and(|r| r.start < at && at < r.end)
            });
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
    /// The part of the regions the reader skips, see
    /// [`ProtectedRegions::opaque_ranges`].
    opaque: Vec<Range<usize>>,
    opaque_inline: Vec<Range<usize>>,
    tag: Option<PendingTag>,
    raw: Option<PendingRaw>,
    macro_call: Option<PendingMacro>,
    /// Start offset of an open `$$` block and its opening line's end.
    math_block: Option<(usize, usize)>,
    /// A raw HTML block in progress, with the offset it started at.
    html_block: Option<HtmlBlock>,
    /// Offsets of `[` still unmatched at the end of the previous line, so a
    /// link label wrapping over lines is recognised when its `](` arrives.
    open_labels: Vec<usize>,
}

/// A raw HTML block in progress.
#[derive(Debug)]
struct HtmlBlock {
    end: HtmlBlockEnd,
    start: usize,
    /// Started inside a block quote.
    quoted: bool,
    /// Content column of the list item the block started in (0: none).
    container_col: usize,
}

/// What the host knows about the line being fed to the block tracker.
pub(crate) struct LineInfo<'a> {
    /// The line, block-quote prefix removed.
    pub(crate) text: &'a str,
    /// The part after a list marker (the whole line when there is none).
    pub(crate) content: &'a str,
    /// Columns of indentation beyond the enclosing list item's content column.
    pub(crate) content_indent: usize,
    /// Indentation columns of `text`.
    pub(crate) indent: usize,
    pub(crate) blank: bool,
    pub(crate) quoted: bool,
    pub(crate) previous_blank: bool,
    /// The host is not inside a fence, comment or code span, so a block may
    /// start here.
    pub(crate) can_start: bool,
    /// Content column of the list item a block starting here belongs to.
    pub(crate) container_col: usize,
    /// Physical line span.
    pub(crate) start: usize,
    pub(crate) end: usize,
    /// The text after this line.
    pub(crate) rest: &'a str,
}

/// How many lines a `$$` opener searches for its closer.
const MAX_MATH_LOOKAHEAD_LINES: usize = 128;

/// Whether a line ending in `$$` follows within the lookahead cap, with no
/// fence delimiter before it. Without one the `$$` is prose (`$$` is the PID
/// in shell, `Energy is $$`), and a block must not swallow the lines after it.
fn math_closer_ahead(rest: &str) -> bool {
    for line in rest.split('\n').take(MAX_MATH_LOOKAHEAD_LINES) {
        let t = line.trim();
        if t.starts_with("```") || t.starts_with("~~~") {
            return false;
        }
        if t.ends_with("$$") {
            return true;
        }
    }
    false
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
            // An opening `<pre>`, `<script>`, `<style>` or `<textarea>` took
            // condition 1 above; a closing `</pre>` alone on a line is a
            // condition 7 block in cmark, commonmark.js and markdown-it.
            return Some(HtmlBlockEnd::BlankLine);
        }
        None
    }

    /// Record a raw HTML or `$$` block: protected, and unread by the reader.
    fn push_raw_block(&mut self, range: Range<usize>) {
        self.opaque.push(range.clone());
        self.blocks.push(range);
    }

    /// Record a whole-line region (a `%%` comment block line). The reader has
    /// its own answer for `%%` and keeps it.
    pub(crate) fn push_block(&mut self, range: Range<usize>) {
        self.blocks.push(range);
    }

    /// Feed one line to the block-level tracker (raw HTML blocks and `$$`
    /// blocks). The tracker is an **overlay**: it never changes how the host
    /// classifies the line, so fences, lists and code agree with a scan made
    /// without protection. Returns `true` when the line is part of a block
    /// region, in which case the host skips the inline scan for it.
    pub(crate) fn feed_line(&mut self, info: &LineInfo<'_>) -> bool {
        // An open block ends with its container: a block quote that stops
        // quoting, or a list item that the line no longer sits inside.
        if let Some(open) = &self.html_block
            && ((open.quoted && !info.quoted)
                || (!info.blank && open.container_col > 0 && info.indent < open.container_col))
        {
            let open = self.html_block.take();
            if let Some(open) = open {
                self.push_raw_block(open.start..info.start);
            }
        }
        if let Some(open) = self.html_block.take() {
            match open.end {
                HtmlBlockEnd::BlankLine => {
                    if info.blank {
                        self.push_raw_block(open.start..info.start);
                    } else {
                        self.html_block = Some(open);
                        return true;
                    }
                }
                HtmlBlockEnd::Marker(marker) => {
                    if info.text.to_ascii_lowercase().contains(marker) {
                        self.push_raw_block(open.start..info.end);
                    } else {
                        self.html_block = Some(HtmlBlock {
                            end: HtmlBlockEnd::Marker(marker),
                            ..open
                        });
                    }
                    return true;
                }
            }
        }
        if let Some((start, _)) = self.math_block {
            if info.text.trim_end().ends_with("$$") {
                self.math_block = None;
                self.push_raw_block(start..info.end);
            }
            return true;
        }
        if info.blank || !info.can_start {
            return false;
        }
        if let Some(end) =
            Self::html_block_start(info.content, info.content_indent, info.previous_blank)
        {
            self.end_block();
            let closed_here = match &end {
                HtmlBlockEnd::BlankLine => false,
                HtmlBlockEnd::Marker(marker) => {
                    let skip = if *marker == "?>" || *marker == ">" {
                        2
                    } else {
                        0
                    };
                    info.content
                        .get(skip..)
                        .is_some_and(|t| t.to_ascii_lowercase().contains(marker))
                }
            };
            if closed_here {
                self.push_raw_block(info.start..info.end);
            } else {
                self.html_block = Some(HtmlBlock {
                    end,
                    start: info.start,
                    quoted: info.quoted,
                    container_col: info.container_col,
                });
            }
            return true;
        }
        let trimmed = info.content.trim_start_matches([' ', '\t']);
        if let Some(after) = trimmed.strip_prefix("$$") {
            // `$$x$$ and prose` closes on its own line: the inline scan
            // protects exactly `$$x$$`.
            if !after.contains("$$") && math_closer_ahead(info.rest) {
                self.end_block();
                self.math_block = Some((info.start, info.end));
                return true;
            }
        }
        false
    }

    // ------------------------------------------------------------------
    // Inline scan
    // ------------------------------------------------------------------

    /// Scan one prose line. `line` is the visible (code spans and comments
    /// already blanked) text of the line, container prefix removed, and
    /// `abs` the body offset of its first byte.
    ///
    /// An opener whose closer is not on the line (`{{`, an unclosed `<code>`)
    /// stays *pending* while the scan carries on, so what follows it keeps
    /// its own protection; the range is committed when the closer arrives
    /// and dropped at the end of the block (R9). Every search that failed on
    /// this line is remembered, so a line of repeated openers is scanned
    /// once, not once per opener (R11).
    pub(crate) fn scan_inline_line(&mut self, line: &[u8], abs: usize) {
        let n = line.len();
        let mut i = 0usize;
        let mut dest_search = true;
        let mut macro_closer_missing = [false; 2];
        let mut raw_missing: Vec<String> = Vec::new();
        // End of the first `</a>` found on this line, so a line of `<a>`
        // openers searches for it once.
        let mut a_closer: Option<usize> = None;
        if let Some(def) = reference_definition_destination(line) {
            self.destinations.push(abs + def.start..abs + def.end);
        }
        // Closers of constructs pending from earlier lines.
        if let Some(pending) = self.raw.clone()
            && let Some(end) = find_closing_tag(line, 0, &pending.name)
        {
            self.inline.push(pending.start..abs + end);
            if pending.name != "a" {
                self.opaque_inline.push(pending.start..abs + end);
            }
            self.raw = None;
        }
        if let Some(pending) = self.macro_call.clone()
            && let Some(j) = find_pair(line, 0, pending.closer)
        {
            self.inline.push(pending.start..abs + j + 2);
            self.macro_call = None;
        }
        while i < n {
            if let Some(pending) = self.tag.clone() {
                match continue_tag(line, i, pending.quote, pending.prev) {
                    TagScan::Complete(tag) => {
                        let tag = Tag {
                            closing: pending.closing,
                            ..tag
                        };
                        let next = self.finish_tag(
                            line,
                            abs,
                            pending.start,
                            &pending.name,
                            &tag,
                            &mut raw_missing,
                            &mut a_closer,
                        );
                        self.tag = None;
                        i = next;
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
                        i = self.finish_tag(
                            line,
                            abs,
                            abs + i,
                            &name,
                            &tag,
                            &mut raw_missing,
                            &mut a_closer,
                        );
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
                    let which = usize::from(line[i + 1] == b'%');
                    let closer = if which == 0 { *b"}}" } else { *b"%}" };
                    let found = if macro_closer_missing[which] {
                        None
                    } else {
                        find_pair(line, i + 2, closer)
                    };
                    if let Some(j) = found {
                        self.inline.push(abs + i..abs + j + 2);
                        i = j + 2;
                    } else {
                        macro_closer_missing[which] = true;
                        if self.macro_call.is_none() {
                            self.macro_call = Some(PendingMacro {
                                start: abs + i,
                                closer,
                            });
                        }
                        i += 2;
                    }
                }
                b'$' => {
                    if let Some(end) = inline_math_end(line, i) {
                        self.inline.push(abs + i..abs + end);
                        if line.get(i + 1) == Some(&b'$') {
                            self.opaque_inline.push(abs + i..abs + end);
                        }
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

    /// Record a completed tag and, for an opening raw-text element, look for
    /// its closing tag. Returns the index the scan resumes at: just past the
    /// closing tag when one was found (everything between is protected
    /// already, so the scan does not walk it again).
    #[allow(clippy::too_many_arguments)]
    fn finish_tag(
        &mut self,
        line: &[u8],
        abs: usize,
        tag_start_abs: usize,
        name: &str,
        tag: &Tag,
        raw_missing: &mut Vec<String>,
        a_closer: &mut Option<usize>,
    ) -> usize {
        let end = tag.end;
        self.inline.push(tag_start_abs..abs + end + 1);
        self.opaque_inline.push(tag_start_abs..abs + end + 1);
        let mut next = end + 1;
        if !tag.closing
            && !tag.self_closing
            && RAW_ELEMENTS.contains(&name)
            && !raw_missing.iter().any(|m| m == name)
        {
            let found = match (name, *a_closer) {
                ("a", Some(close_end)) if close_end > end => Some(close_end),
                _ => find_closing_tag(line, end + 1, name),
            };
            if let Some(close_end) = found {
                if name == "a" {
                    *a_closer = Some(close_end);
                }
                self.inline.push(abs + end + 1..abs + close_end);
                if name == "a" {
                    // Link text can hold code and macros: keep scanning it.
                    next = end + 1;
                } else {
                    self.opaque_inline.push(abs + end + 1..abs + close_end);
                    next = close_end;
                }
            } else {
                raw_missing.push(name.to_owned());
                if self.raw.is_none() {
                    self.raw = Some(PendingRaw {
                        start: abs + end + 1,
                        name: name.to_owned(),
                    });
                }
            }
        }
        next
    }

    /// Finish the body: an unclosed `$$` block protects its opening line
    /// only, and every other pending construct is dropped.
    pub(crate) fn finish(mut self) -> ProtectedRegions {
        if let Some((start, end)) = self.math_block.take() {
            self.push_raw_block(start..end);
        }
        if let Some(open) = self.html_block.take() {
            // A condition 1/3-5 block with no closer runs to the end of the
            // document; a condition 6/7 block ends there too.
            self.push_raw_block(open.start..usize::MAX);
        }
        normalise(&mut self.blocks);
        normalise(&mut self.inline);
        normalise(&mut self.destinations);
        normalise(&mut self.opaque);
        normalise(&mut self.opaque_inline);
        ProtectedRegions {
            blocks: self.blocks,
            inline: self.inline,
            destinations: self.destinations,
            opaque: self.opaque,
            opaque_inline: self.opaque_inline,
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
    let state = if closing { TAG_CLOSING } else { prev };
    match continue_tag(line, j, quote, state) {
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

// States of the CommonMark §6.6 tag grammar, carried across lines in the
// `prev` field of `TagScan::Open` and `PendingTag`.
/// Just after the tag name.
const TAG_NAME_END: u8 = 0;
/// Whitespace seen: an attribute name, `/` or `>` may follow.
const TAG_SPACE: u8 = 1;
/// Inside an attribute name.
const TAG_ATTR_NAME: u8 = 2;
/// After an attribute name and whitespace: `=` or the next attribute.
const TAG_AFTER_NAME: u8 = 3;
/// After `=`: a value must follow.
const TAG_BEFORE_VALUE: u8 = 4;
/// Inside an unquoted value.
const TAG_UNQUOTED: u8 = 5;
/// Inside a quoted value (the quote is the other half of the state).
const TAG_QUOTED: u8 = 6;
/// Just after a closing quote: whitespace, `/` or `>` only.
const TAG_AFTER_QUOTE: u8 = 7;
/// After `/`: only `>` may follow.
const TAG_SLASH: u8 = 8;
/// Inside a closing tag: whitespace or `>` only.
const TAG_CLOSING: u8 = 9;

/// Continue scanning a tag from `line[from..]` in grammar state `state`
/// (CommonMark §6.6: a tag name, then attributes — a name, an optional
/// `= value` — each separated by whitespace). Anything else, such as the
/// `[[x]]` of `a<b see [[x]] and c>d`, is not a tag: the reader would
/// otherwise blank real prose.
fn continue_tag(line: &[u8], from: usize, mut quote: Option<u8>, mut state: u8) -> TagScan {
    for (j, &b) in line.iter().enumerate().skip(from) {
        if let Some(q) = quote {
            if b == q {
                quote = None;
                state = TAG_AFTER_QUOTE;
            }
            continue;
        }
        let ws = matches!(b, b' ' | b'\t' | b'\r');
        let complete = |self_closing| {
            TagScan::Complete(Tag {
                end: j,
                name_start: 0,
                name_end: 0,
                closing: false,
                self_closing,
            })
        };
        let name_start = b.is_ascii_alphabetic() || matches!(b, b'_' | b':');
        let name_char = name_start || b.is_ascii_digit() || matches!(b, b'.' | b'-');
        state = match (state, b) {
            (TAG_CLOSING | TAG_SPACE | TAG_AFTER_NAME | TAG_BEFORE_VALUE, _) if ws => state,
            (TAG_NAME_END | TAG_AFTER_QUOTE | TAG_ATTR_NAME | TAG_UNQUOTED, _) if ws => {
                if state == TAG_ATTR_NAME {
                    TAG_AFTER_NAME
                } else {
                    TAG_SPACE
                }
            }
            (TAG_CLOSING | TAG_UNQUOTED, b'>') => return complete(false),
            (TAG_BEFORE_VALUE, b'"' | b'\'') => {
                quote = Some(b);
                TAG_QUOTED
            }
            (TAG_UNQUOTED | TAG_BEFORE_VALUE, b'"' | b'\'' | b'=' | b'<' | b'`' | b'>') => {
                return TagScan::No;
            }
            (TAG_UNQUOTED | TAG_BEFORE_VALUE, _) => TAG_UNQUOTED,
            (TAG_SLASH, b'>') => return complete(true),
            (TAG_SLASH, _) => return TagScan::No,
            (_, b'>') => return complete(false),
            (_, b'/') => TAG_SLASH,
            (TAG_ATTR_NAME | TAG_AFTER_NAME, b'=') => TAG_BEFORE_VALUE,
            (TAG_ATTR_NAME, _) if name_char => TAG_ATTR_NAME,
            (TAG_SPACE | TAG_AFTER_NAME, _) if name_start => TAG_ATTR_NAME,
            _ => return TagScan::No,
        };
    }
    // A line end is whitespace, except inside a quoted value.
    if quote.is_none() {
        state = match state {
            TAG_ATTR_NAME => TAG_AFTER_NAME,
            TAG_NAME_END | TAG_UNQUOTED | TAG_AFTER_QUOTE => TAG_SPACE,
            other => other,
        };
    }
    TagScan::Open {
        name_start: 0,
        name_end: 0,
        closing: false,
        quote,
        prev: state,
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

    /// Lines whose code classification differs between the plain scan and the
    /// protected one: must be none, protection is an overlay (R1, R2).
    fn code_disagreements(body: &str) -> Vec<usize> {
        let plain = BodySyntax::new(body);
        let prot = BodySyntax::with_protected(body);
        (1..=body.lines().count())
            .filter(|l| plain.line_is_code(*l) != prot.line_is_code(*l))
            .collect()
    }

    #[test]
    fn protection_never_changes_code_classification() {
        for body in [
            "- $$\n  a = b\n  $$\n\nText.\n\n```text\n##  literal sample\n\n\nend\n```\n",
            "Energy is $$\nE = mc^2\n$$ which is famous.\n\n```sh\necho $$\n```\n\nprose\n",
            "$$ expands to the PID.\n\n```text\nx\n```\n",
            "- item\n\n<div>x</div>\n\n    ##  indented code\n    \ttab\n\n\n    more code\n",
            "- <pre>\n  a\n\n  b\n  </pre>\n\n    code here\n",
            "> <pre>\n> x\n\n```\nfenced\n```\n",
        ] {
            assert!(
                code_disagreements(body).is_empty(),
                "{body:?}: {:?}",
                code_disagreements(body)
            );
        }
    }

    #[test]
    fn an_unbalanced_dollar_pair_protects_only_what_it_closes() {
        let body = "Energy is $$\nE = mc^2\n$$ which is famous.\n\n```text\n##  x\n```\n";
        let r = regions(body);
        assert!(
            !r.contains(body.find("##  x").unwrap_or(0)),
            "fence content is code, not math"
        );
        let syntax = BodySyntax::with_protected(body);
        assert!(syntax.line_is_code(6));
        assert!(!hit(
            "$$ expands to the PID.\n\ntext https://p.org\n",
            "text"
        ));
    }

    #[test]
    fn a_display_block_needs_a_real_closer_and_stops_at_it() {
        let body = "$$\na\n$$\ntext https://p.org\n";
        assert!(hit(body, "a\n"));
        assert!(!hit(body, "https://p.org"));
        // a content line that merely contains `$$` does not close the block
        let body = "$$\nx $$ y\nstill math\n$$\ntext\n";
        assert!(hit(body, "still math"));
        assert!(!hit(body, "text\n"));
        // a fence before any closer: not a block
        assert!(!hit("$$\n```\ncode\n```\n$$\n", "$$\n```"));
    }

    #[test]
    fn a_single_line_display_formula_protects_only_itself() {
        let body = "$$x$$ and https://p.org/z\n";
        assert!(hit(body, "x$$"));
        assert!(!hit(body, "https://p.org"));
    }

    #[test]
    fn percent_comment_blocks_are_protected() {
        let body = "%%\nhttps://x.org/a\n##  heading\n%%\n\nhttps://p.org\n";
        assert!(hit(body, "https://x.org"));
        assert!(!hit(body, "https://p.org"));
    }

    #[test]
    fn a_block_opened_on_a_list_marker_line_is_recognised() {
        let body = "- <pre>\n  raw https://x.org/a\n\n  raw https://y.org/b\n  </pre>\n- next https://p.org\n";
        assert!(hit(body, "https://x.org"));
        assert!(hit(body, "https://y.org"));
        assert!(!hit(body, "https://p.org"));
        let body = "1. <div>\n   https://z.org/c\n   </div>\n";
        assert!(hit(body, "https://z.org"));
    }

    #[test]
    fn a_raw_block_ends_with_its_container() {
        let body = "> <pre>\n> never closed\n\n## Next\n\nText  with https://p.org/x here.\n";
        assert!(hit(body, "never closed"));
        assert!(!hit(body, "https://p.org"));
        let body = "- a\n  <div>x</div>\n- b https://x.org/one\n- c https://x.org/two\n";
        assert!(hit(body, "<div>"));
        assert!(!hit(body, "https://x.org/one"));
        assert!(!hit(body, "https://x.org/two"));
    }

    #[test]
    fn a_pending_opener_does_not_unprotect_what_follows() {
        let body = "Use {{ in Jinja. See <a href=\"x\">https://b.org/y</a> for more.\n";
        assert!(hit(body, "https://b.org"));
        let body = "Type Foo<a> here, then <code>https://b.org/y</code> ok.\n";
        assert!(hit(body, "https://b.org"));
        // and the pending macro still closes on a later line
        let body = "{{ start\nhttps://m.org/x }} tail https://p.org\n";
        assert!(hit(body, "https://m.org"));
        assert!(!hit(body, "https://p.org"));
    }

    #[test]
    fn repeated_openers_on_one_line_stay_linear() {
        for line in [
            format!("{}</a>\n", "<a>".repeat(120_000)),
            format!("{}\n", "<code>".repeat(120_000)),
            "{{ ".repeat(120_000) + "\n",
        ] {
            let start = std::time::Instant::now();
            let _ = BodySyntax::with_protected(&line);
            assert!(
                start.elapsed() < std::time::Duration::from_secs(3),
                "{:?}",
                start.elapsed()
            );
        }
    }

    #[test]
    fn inserting_before_a_block_is_not_protected() {
        let body = "## Next\n<div>x</div>\n";
        let at = body.find("<div>").unwrap_or(0);
        assert!(!regions(body).overlaps(&(at..at)));
        assert!(
            regions(body).contains(at),
            "a diagnostic there is still protected"
        );
        assert!(regions(body).overlaps(&(at + 2..at + 2)));
    }
}
