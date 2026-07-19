//! Shared, stateful line classifier for the body-scan loops (iter-183 Phase B).
//!
//! Before this module, six independent loops (`link_fix`'s
//! `build_replacements_for_file`, `auto_link`'s `resolve_existing_link_targets`
//! and `scan_file_for_matches`, and `link_rewrite`'s three inbound/outbound
//! planners) each hand-rolled the *same* per-line state machine:
//!
//! 1. frontmatter open / content / close tracking,
//! 2. fenced-code-block tracking ([`FenceTracker`]),
//! 3. Obsidian `%%` comment-fence tracking,
//! 4. per-line inline-code and `%%…%%` comment stripping.
//!
//! Each copy drifted slightly and none handled **cross-line** suppression: a
//! CommonMark code span opened with `` `N` `` backticks and closed several
//! lines later (L-3), or an HTML `<!-- … -->` comment spanning multiple lines
//! (L-15), leaked `[[links]]` to the extractor. [`LineScanner`] centralizes
//! all of that so the fixes land once, everywhere, and the frontmatter
//! delimiter policy is the single canonical one
//! ([`crate::frontmatter::is_opening_delimiter`] /
//! [`crate::frontmatter::is_closing_delimiter`], L-4/L-13).
//!
//! # Usage
//!
//! ```ignore
//! let mut scanner = LineScanner::new();
//! let mut line_num = 0;
//! for line in content.split('\n') {
//!     line_num += 1;
//!     match scanner.classify(line) {
//!         LineClass::FrontmatterOpen | LineClass::FrontmatterClose => continue,
//!         LineClass::Frontmatter => { /* frontmatter YAML line: rewrite or skip */ }
//!         LineClass::Skip => continue, // fence delimiter/body, comment body
//!         LineClass::Body(body) => {
//!             let cleaned = body.cleaned(line);
//!             // extract links from `cleaned`, using `line` for original text
//!         }
//!     }
//! }
//! ```

use std::borrow::Cow;

use super::fence::FenceTracker;
use super::strip::{is_comment_fence, strip_html_comments, strip_inline_code_stateful};
use super::strip_inline_comments;
use crate::frontmatter::{is_closing_delimiter, is_opening_delimiter};

/// Classification of a single physical line produced by [`LineScanner::classify`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineClass {
    /// The opening `---` of a frontmatter block (line 1 only).
    FrontmatterOpen,
    /// A YAML content line inside the frontmatter block. Callers that rewrite
    /// frontmatter wikilinks act on the raw line; others skip it.
    Frontmatter,
    /// The closing `---` of a frontmatter block.
    FrontmatterClose,
    /// A line that carries no extractable body content: a fenced-code-block
    /// delimiter or interior line, or an Obsidian `%%` comment fence /
    /// interior line. Callers skip it.
    Skip,
    /// A normal body line. Use [`BodyLine::cleaned`] to obtain the text with
    /// inline code spans, `%%…%%` comments, cross-line code spans (L-3), and
    /// HTML comments (L-15) blanked to spaces (byte positions preserved).
    Body(BodyLine),
}

/// Marker returned for a body line. Kept as a distinct type (rather than
/// returning the cleaned `Cow` directly) so [`LineClass`] stays `Copy` and the
/// caller decides when to pay for the strip; call [`BodyLine::cleaned`] with
/// the original line to produce the cleaned text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodyLine {
    /// Length of an inline code-span run left open at the start of this line
    /// (carried from a previous line); `None` if no span was open.
    open_code_run: Option<usize>,
    /// Whether an HTML comment was open at the start of this line.
    in_html_comment: bool,
}

impl BodyLine {
    /// Produce the cleaned form of `line`: inline code spans, `%%…%%`
    /// comments, any carried-over cross-line code span (L-3), and any HTML
    /// comment (L-15) are replaced with spaces so `[[links]]` inside them are
    /// not extracted. Byte offsets are preserved for in-place rewriting.
    ///
    /// `rest` is the remaining document text after `line` (the multi-line
    /// code-span lookahead, L-3). Callers that hold the full `content` should
    /// pass the slice after this line; `""` yields single-line semantics for
    /// an unclosed opener.
    #[must_use]
    pub fn cleaned<'a>(&self, line: &'a str, rest: &str) -> Cow<'a, str> {
        // 1. Inline code spans first (carrying the open run), then 2. `%%`
        //    comments, then 3. HTML comments — the same order the multi-visitor
        //    scanner uses, extended with cross-line state.
        let mut open = self.open_code_run;
        let after_code = strip_inline_code_stateful(line, &mut open, rest);
        let after_comment = strip_inline_comments(after_code.as_ref());
        let mut in_html = self.in_html_comment;
        // `strip_html_comments` needs to see the code/comment-blanked text so
        // a `<!--` inside a code span is not treated as a real comment opener.
        match strip_html_comments(after_comment.as_ref(), &mut in_html) {
            Cow::Borrowed(_) => {
                // No HTML change: return whichever earlier form we have,
                // preserving the borrow when possible.
                match after_comment {
                    Cow::Borrowed(_) => after_code,
                    Cow::Owned(s) => Cow::Owned(s),
                }
            }
            Cow::Owned(s) => Cow::Owned(s),
        }
    }
}

/// Stateful, single-pass line classifier shared by every body-scan loop.
///
/// Tracks frontmatter, fenced code blocks, `%%` comment fences, cross-line
/// inline code spans (L-3), and cross-line HTML comments (L-15). Feed it one
/// physical line at a time via [`classify`](Self::classify); it returns a
/// [`LineClass`] describing how the caller should treat the line.
#[derive(Debug)]
pub struct LineScanner {
    line_num: usize,
    fence: FenceTracker,
    in_comment_fence: bool,
    in_frontmatter: bool,
    frontmatter_done: bool,
    /// Open inline code-span run length carried across body lines (L-3).
    open_code_run: Option<usize>,
    /// Whether an HTML comment is open across body lines (L-15).
    in_html_comment: bool,
}

impl Default for LineScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl LineScanner {
    /// Create a fresh scanner positioned before the first line.
    #[must_use]
    pub fn new() -> Self {
        Self {
            line_num: 0,
            fence: FenceTracker::new(),
            in_comment_fence: false,
            in_frontmatter: false,
            frontmatter_done: false,
            open_code_run: None,
            in_html_comment: false,
        }
    }

    /// Classify the next physical line, advancing internal state.
    ///
    /// Lines must be fed in order, exactly once each, with any trailing line
    /// terminator already stripped (as produced by `content.split('\n')`).
    /// `rest` is the remaining document text after `line` — the multi-line
    /// code-span lookahead (L-3). Pass the slice after this line (callers hold
    /// the full `content`), or `""` for single-line semantics.
    pub fn classify(&mut self, line: &str, rest: &str) -> LineClass {
        self.line_num += 1;
        let line_num = self.line_num;

        // ---- Frontmatter ----
        if !self.frontmatter_done {
            // BOM-aware opening delimiter on line 1 (canonical policy, L-13).
            if line_num == 1 && is_opening_delimiter(line) {
                self.in_frontmatter = true;
                return LineClass::FrontmatterOpen;
            }
            if self.in_frontmatter {
                // Canonical (lenient) closing delimiter (L-4/L-13).
                if is_closing_delimiter(line) {
                    self.in_frontmatter = false;
                    self.frontmatter_done = true;
                    return LineClass::FrontmatterClose;
                }
                return LineClass::Frontmatter;
            }
            // Line 1 was not an opening delimiter: no frontmatter block.
            self.frontmatter_done = true;
        }

        // ---- Comment fence (Obsidian %% blocks) ----
        // When already inside a comment block, only the closing `%%` matters;
        // fenced code inside a comment is literal, so it is not processed.
        if self.in_comment_fence {
            if is_comment_fence(line) {
                self.in_comment_fence = false;
            }
            return LineClass::Skip;
        }

        // ---- Fenced code block ----
        // Process fences BEFORE the `%%` toggle so a literal `%%` inside a
        // fenced code block is treated as code, not a comment delimiter (L-8).
        if self.fence.process_line(line) {
            return LineClass::Skip;
        }

        // ---- Comment fence opening (only outside code blocks) ----
        if is_comment_fence(line) {
            self.in_comment_fence = true;
            return LineClass::Skip;
        }

        // ---- Normal body line ----
        let body = BodyLine {
            open_code_run: self.open_code_run,
            in_html_comment: self.in_html_comment,
        };

        // Advance the cross-line suppression state so the NEXT line continues
        // any code span / HTML comment that this line left open (L-3, L-15).
        // We compute the trailing state cheaply by re-running the stateful
        // strippers here (the caller re-runs them in `BodyLine::cleaned`, but
        // that pass is on a snapshot; state must live on the scanner).
        let mut open = self.open_code_run;
        let after_code = strip_inline_code_stateful(line, &mut open, rest);
        self.open_code_run = open;
        let after_comment = strip_inline_comments(after_code.as_ref());
        let mut in_html = self.in_html_comment;
        let _ = strip_html_comments(after_comment.as_ref(), &mut in_html);
        self.in_html_comment = in_html;

        LineClass::Body(body)
    }

    /// The 1-based number of the most recently classified line.
    #[must_use]
    pub fn line_num(&self) -> usize {
        self.line_num
    }
}

/// Iterate `content` as `(line, rest)` pairs, where `line` is one physical
/// line (no trailing terminator) and `rest` is everything after it. Mirrors
/// `content.split('\n')` but also yields the lookahead slice the multi-line
/// code-span rule needs (L-3), so body-scan loops can feed
/// [`LineScanner::classify`] and [`BodyLine::cleaned`] without hand-rolling
/// byte-offset bookkeeping.
pub fn lines_with_rest(content: &str) -> impl Iterator<Item = (&str, &str)> {
    let mut offset = 0usize;
    let total = content.len();
    content.split('\n').map(move |line| {
        let after = offset + line.len();
        let rest = if after < total {
            &content[after + 1..] // +1 skips the '\n'
        } else {
            ""
        };
        offset = after + 1;
        (line, rest)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive the scanner over `content` (splitting into `(line, rest)` pairs so
    /// the multi-line code-span lookahead works) and return the cleaned body
    /// lines with their 1-based line numbers.
    fn body_lines(content: &str) -> Vec<(String, usize)> {
        let mut scanner = LineScanner::new();
        let mut out = Vec::new();
        let mut offset = 0usize;
        let lines: Vec<&str> = content.split('\n').collect();
        for line in &lines {
            // `rest` is everything after this line (past its trailing '\n').
            let after = offset + line.len();
            let rest = if after < content.len() {
                &content[after + 1..] // +1 skips the '\n'
            } else {
                ""
            };
            offset = after + 1;
            let n = scanner.line_num() + 1;
            if let LineClass::Body(body) = scanner.classify(line, rest) {
                out.push((body.cleaned(line, rest).into_owned(), n));
            }
        }
        out
    }

    #[test]
    fn plain_body_line_borrows() {
        let mut s = LineScanner::new();
        match s.classify("hello [[link]] world", "") {
            LineClass::Body(b) => {
                assert!(matches!(
                    b.cleaned("hello [[link]] world", ""),
                    Cow::Borrowed(_)
                ));
            }
            other => panic!("expected Body, got {other:?}"),
        }
    }

    #[test]
    fn frontmatter_open_content_close() {
        let mut s = LineScanner::new();
        assert_eq!(s.classify("---", ""), LineClass::FrontmatterOpen);
        assert_eq!(s.classify("title: X", ""), LineClass::Frontmatter);
        assert_eq!(s.classify("---", ""), LineClass::FrontmatterClose);
        assert!(matches!(s.classify("body", ""), LineClass::Body(_)));
    }

    #[test]
    fn indented_closing_delimiter_closes_frontmatter() {
        // Lenient closing policy (L-4): `  ---` closes the block.
        let mut s = LineScanner::new();
        assert_eq!(s.classify("---", ""), LineClass::FrontmatterOpen);
        assert_eq!(s.classify("title: X", ""), LineClass::Frontmatter);
        assert_eq!(s.classify("  ---", ""), LineClass::FrontmatterClose);
        assert!(matches!(s.classify("body", ""), LineClass::Body(_)));
    }

    #[test]
    fn leading_space_is_not_frontmatter() {
        // Strict opening policy: ` ---` on line 1 does not open frontmatter.
        let mut s = LineScanner::new();
        assert!(matches!(s.classify(" ---", ""), LineClass::Body(_)));
    }

    #[test]
    fn fenced_code_block_skipped() {
        let lines = body_lines("before\n```\n[[nope]]\n```\nafter");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].0, "before");
        assert_eq!(lines[1].0, "after");
    }

    #[test]
    fn comment_fence_skipped() {
        let lines = body_lines("before\n%%\n[[nope]]\n%%\nafter");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].0, "before");
        assert_eq!(lines[1].0, "after");
    }

    #[test]
    fn cross_line_code_span_hides_link() {
        // L-3: a code span opened on one line and closed two lines later must
        // blank the interior `[[link]]` and `[t](x.md)`.
        let content = "start ``code\n[[hidden]] and [t](x.md)\nmore code`` end [[visible]]";
        let lines = body_lines(content);
        assert_eq!(lines.len(), 3);
        assert!(lines[0].0.contains("start"));
        // Line 2 is entirely inside the span → fully blanked.
        assert!(!lines[1].0.contains("hidden"), "line2: {:?}", lines[1].0);
        assert!(!lines[1].0.contains("x.md"), "line2: {:?}", lines[1].0);
        // Line 3: after the closer, the real link survives.
        assert!(
            lines[2].0.contains("[[visible]]"),
            "line3: {:?}",
            lines[2].0
        );
        assert!(!lines[2].0.contains("more code"), "line3: {:?}", lines[2].0);
    }

    #[test]
    fn single_line_unterminated_backtick_is_literal() {
        // CommonMark: a backtick opener with no matching closer anywhere in the
        // document is literal, so the following link stays visible. The
        // lookahead (L-3) sees no closer in `rest` and does NOT open a span,
        // avoiding stray-backtick prose swallowing real links.
        let content = "text `open [[link]]";
        let lines = body_lines(content);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].0.contains("[[link]]"), "{:?}", lines[0].0);
        assert!(lines[0].0.contains("text"));
    }

    #[test]
    fn unterminated_backtick_across_lines_stays_literal_when_never_closed() {
        // A `code opener that never closes must leave later links visible.
        let content = "intro `open\nnext [[keep]] line\nmore text";
        let lines = body_lines(content);
        assert_eq!(lines.len(), 3);
        assert!(lines[1].0.contains("[[keep]]"), "{:?}", lines[1].0);
    }

    #[test]
    fn cross_line_html_comment_hides_link() {
        // L-15: an HTML comment spanning lines blanks interior links.
        let content = "before <!-- open\n[[hidden]]\nclose --> after [[visible]]";
        let lines = body_lines(content);
        assert_eq!(lines.len(), 3);
        assert!(lines[0].0.contains("before"));
        assert!(!lines[1].0.contains("hidden"), "line2: {:?}", lines[1].0);
        assert!(
            lines[2].0.contains("[[visible]]"),
            "line3: {:?}",
            lines[2].0
        );
        assert!(!lines[2].0.contains("close"), "line3: {:?}", lines[2].0);
    }

    #[test]
    fn single_line_html_comment_hides_link() {
        let content = "a <!-- [[nope]] --> b [[yes]]";
        let lines = body_lines(content);
        assert_eq!(lines.len(), 1);
        assert!(!lines[0].0.contains("nope"), "{:?}", lines[0].0);
        assert!(lines[0].0.contains("[[yes]]"));
    }

    #[test]
    fn html_comment_inside_code_span_is_literal_comment_marker() {
        // A `<!--` inside a backtick code span is blanked as code, so it must
        // not open an HTML comment that swallows the following real link.
        let content = "`<!--` [[visible]]";
        let lines = body_lines(content);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].0.contains("[[visible]]"), "{:?}", lines[0].0);
    }

    #[test]
    fn code_fence_not_opened_inside_comment_fence() {
        let lines = body_lines("%%\n```\ncode\n```\n%%\nafter");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].0, "after");
    }
}
