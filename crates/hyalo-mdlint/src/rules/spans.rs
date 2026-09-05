//! One body-wide line classification every rule and post-filter can consult.
//!
//! Iteration 271 (Parts C, D and E) needed three facts about a body line that
//! no single place computed:
//!
//! * **Is it code?** MD019 (and, it turned out, most of the autofixable stock
//!   rules) fired at a `#   three` written *inside* a fenced or indented code
//!   block and rewrote the sample — silent corruption of documentation whose
//!   whole point is the literal text (BUG-28). MD018, MD023 and MD026 already
//!   respected code blocks; the others simply scan lines.
//! * **Does this fence ever close?** MD031 proposed a blank line at the opener
//!   of an unterminated fence, which inserts it *into* the code sample
//!   (BUG-3). Six GitHub Docs files have an odd fence count.
//! * **Is it disabled?** `<!-- markdownlint-disable no-hard-tabs -->` is the
//!   standard escape hatch (MDN uses it around a tab-laden sample); hyalo
//!   recognised neither the rule-id nor the alias form (DEC-294).
//!
//! [`crate::rules::code_fence`] already owned CommonMark §4.5 fence
//! open/close detection for the line-based HYALO rules; this module walks the
//! body once with it and answers all three questions per line, so a rule or a
//! post-filter never re-derives them (and cannot drift).

use super::code_fence::{CodeFence, fence_open, is_fence_close};

/// One `<!-- markdownlint-… -->` directive found in the body.
#[derive(Debug, Clone)]
struct DisableEvent {
    /// 0-based body line the comment sits on.
    line: usize,
    kind: DisableKind,
    /// Rule tokens the directive named — ids (`MD010`) or aliases
    /// (`no-hard-tabs`), exactly as written. Empty means "every rule".
    tokens: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DisableKind {
    /// `markdownlint-disable` — from this line to the end of the file (or the
    /// next matching `enable`).
    Disable,
    /// `markdownlint-enable` — cancels a preceding `disable`.
    Enable,
    /// `markdownlint-disable-line` — this line only.
    DisableLine,
    /// `markdownlint-disable-next-line` — the following line only.
    DisableNextLine,
    /// `markdownlint-disable-file` — the whole file, wherever it appears.
    DisableFile,
    /// `markdownlint-enable-file` — cancels a `disable-file`.
    EnableFile,
}

/// Per-line facts about a lint body.
#[derive(Debug, Default)]
pub struct BodySpans {
    /// The line's content is inside a code block — fenced (`` ``` `` or
    /// `~~~`, terminated or not) or indented four spaces / a tab. The fence
    /// delimiter lines themselves are **not** marked: they are markup, and the
    /// fence rules legitimately act on them.
    in_code: Vec<bool>,
    /// The line is (or is inside) an HTML comment.
    in_html_comment: Vec<bool>,
    /// The line opens a fenced code block that has no closing fence before
    /// end of file.
    unterminated_fence_open: Vec<bool>,
    /// `markdownlint-…` directives, in document order.
    disable_events: Vec<DisableEvent>,
}

impl BodySpans {
    /// Classify every line of `body`.
    #[must_use]
    pub fn new(body: &str) -> Self {
        let lines: Vec<&str> = body.lines().collect();
        let n = lines.len();
        let mut spans = Self {
            in_code: vec![false; n],
            in_html_comment: vec![false; n],
            unterminated_fence_open: vec![false; n],
            disable_events: Vec::new(),
        };

        // Fence character + the line that opened it, while a fence is open.
        let mut open_fence: Option<(CodeFence, usize)> = None;
        // Indented-code-block state. An indented block can only *start* after
        // a blank line (or at the very top of the body); an indented line that
        // continues a paragraph is a lazy continuation, not code.
        let mut prev_blank = true;
        let mut in_indented = false;
        // HTML comment state (`<!--` … `-->` may span lines).
        let mut in_comment = false;

        for (i, line) in lines.iter().enumerate() {
            if let Some((fence, _)) = open_fence.as_ref() {
                if is_fence_close(line, fence) {
                    open_fence = None; // the delimiter itself is markup
                } else {
                    spans.in_code[i] = true;
                }
                prev_blank = false;
                continue;
            }

            if let Some(fence) = fence_open(line) {
                open_fence = Some((fence, i));
                in_indented = false;
                prev_blank = false;
                continue;
            }

            let is_blank = line.trim().is_empty();
            if in_indented {
                if is_blank || is_indented_code_line(line) {
                    spans.in_code[i] = true;
                } else {
                    in_indented = false;
                }
            } else if !is_blank && prev_blank && is_indented_code_line(line) {
                in_indented = true;
                spans.in_code[i] = true;
            }

            if !spans.in_code[i] {
                scan_html_comment(line, i, &mut in_comment, &mut spans);
            }
            prev_blank = is_blank;
        }

        if let Some((_, opener)) = open_fence {
            spans.unterminated_fence_open[opener] = true;
        }

        spans
    }

    /// Whether the 1-based body line is code — fenced or indented — and so
    /// must not be rewritten by a rule that lints prose.
    #[must_use]
    pub fn line_is_code(&self, line_1based: usize) -> bool {
        Self::at(line_1based, &self.in_code)
    }

    /// Whether the 1-based body line sits inside an HTML comment.
    #[must_use]
    pub fn line_is_html_comment(&self, line_1based: usize) -> bool {
        Self::at(line_1based, &self.in_html_comment)
    }

    /// Whether the 1-based body line opens a fenced code block that is never
    /// closed. A "blank line after this fence" proposal there would land
    /// inside the sample, so MD031 must stay quiet (BUG-3).
    #[must_use]
    pub fn opens_unterminated_fence(&self, line_1based: usize) -> bool {
        Self::at(line_1based, &self.unterminated_fence_open)
    }

    /// Whether the body carries any `markdownlint-…` directive at all — the
    /// cheap early-out that keeps [`Self::rule_disabled_at`] off the hot path
    /// for the overwhelming majority of files.
    #[must_use]
    pub fn has_disable_directives(&self) -> bool {
        !self.disable_events.is_empty()
    }

    /// Whether a rule is disabled at the 1-based body line by a
    /// `markdownlint-…` comment.
    ///
    /// `matches_token` decides whether one directive token names this rule;
    /// the caller supplies it because only the engine knows a rule's id *and*
    /// its alias (`MD010` / `no-hard-tabs`), and markdownlint accepts either.
    #[must_use]
    pub fn rule_disabled_at(
        &self,
        line_1based: usize,
        matches_token: impl Fn(&str) -> bool,
    ) -> bool {
        if self.disable_events.is_empty() {
            return false;
        }
        let Some(line) = line_1based.checked_sub(1) else {
            return false;
        };
        let mut region_disabled = false;
        let mut file_disabled = false;
        for ev in &self.disable_events {
            let applies = ev.tokens.is_empty() || ev.tokens.iter().any(|t| matches_token(t));
            if !applies {
                continue;
            }
            match ev.kind {
                // File-scoped directives hold wherever they appear.
                DisableKind::DisableFile => file_disabled = true,
                DisableKind::EnableFile => file_disabled = false,
                DisableKind::Disable if ev.line <= line => region_disabled = true,
                DisableKind::Enable if ev.line <= line => region_disabled = false,
                DisableKind::DisableLine if ev.line == line => return true,
                DisableKind::DisableNextLine if ev.line + 1 == line => return true,
                _ => {}
            }
        }
        region_disabled || file_disabled
    }

    fn at(line_1based: usize, flags: &[bool]) -> bool {
        line_1based
            .checked_sub(1)
            .and_then(|i| flags.get(i))
            .copied()
            .unwrap_or(false)
    }
}

/// A line that starts an indented code block: four spaces, or a tab
/// (CommonMark §4.4 counts a tab as four columns).
fn is_indented_code_line(line: &str) -> bool {
    line.starts_with("    ") || line.starts_with('\t')
}

/// Track HTML-comment state across `line` and record any `markdownlint-…`
/// directive it carries.
fn scan_html_comment(line: &str, index: usize, in_comment: &mut bool, spans: &mut BodySpans) {
    let mut rest = line;
    loop {
        if *in_comment {
            spans.in_html_comment[index] = true;
            match rest.find("-->") {
                Some(end) => {
                    *in_comment = false;
                    rest = &rest[end + 3..];
                }
                None => return,
            }
        } else {
            let Some(start) = rest.find("<!--") else {
                return;
            };
            spans.in_html_comment[index] = true;
            *in_comment = true;
            rest = &rest[start + 4..];
            // A directive must be wholly contained in one comment; anything
            // spanning lines is not markdownlint syntax.
            if let Some(end) = rest.find("-->")
                && let Some(ev) = parse_directive(&rest[..end], index)
            {
                spans.disable_events.push(ev);
            }
        }
    }
}

/// Parse the inside of an HTML comment as a `markdownlint-…` directive.
///
/// markdownlint semantics: `markdownlint-disable`, `-enable`,
/// `-disable-line`, `-disable-next-line`, `-disable-file` and `-enable-file`,
/// each optionally followed by whitespace- or comma-separated rule ids or
/// aliases; no ids means every rule. `markdownlint-capture`/`-restore` are
/// deliberately not supported (DEC-294) and parse to `None`, as does MDN's
/// `-nolint` info-string convention, which is not markdownlint syntax at all.
fn parse_directive(inner: &str, line: usize) -> Option<DisableEvent> {
    // Longest keyword first: `disable-next-line` must not be read as
    // `disable` followed by a rule named `-next-line`.
    const KEYWORDS: &[(&str, DisableKind)] = &[
        ("disable-next-line", DisableKind::DisableNextLine),
        ("disable-file", DisableKind::DisableFile),
        ("disable-line", DisableKind::DisableLine),
        ("enable-file", DisableKind::EnableFile),
        ("disable", DisableKind::Disable),
        ("enable", DisableKind::Enable),
    ];

    let inner = inner.trim();
    let rest = inner.strip_prefix("markdownlint-")?;
    let (tail, kind) = KEYWORDS.iter().find_map(|(word, kind)| {
        let tail = rest.strip_prefix(word)?;
        // The keyword must end the directive or be followed by whitespace,
        // so `markdownlint-disabled` is not read as `disable`.
        (tail.is_empty() || tail.starts_with(char::is_whitespace)).then_some((tail, *kind))
    })?;
    let tokens = tail
        .split([' ', '\t', ','])
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .collect();
    Some(DisableEvent { line, kind, tokens })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(body: &str) -> BodySpans {
        BodySpans::new(body)
    }

    #[test]
    fn backtick_fence_content_is_code_but_the_delimiters_are_not() {
        let s = spans("a\n\n```text\n#   x\n```\n\nb\n");
        assert!(!s.line_is_code(3), "the opening fence is markup");
        assert!(s.line_is_code(4), "the sample is code");
        assert!(!s.line_is_code(5), "the closing fence is markup");
        assert!(!s.line_is_code(7));
    }

    #[test]
    fn tilde_fences_and_info_strings_count_too() {
        let s = spans("~~~sh\n#   x\n~~~\n");
        assert!(s.line_is_code(2));
        assert!(!s.line_is_code(1));
    }

    #[test]
    fn an_indented_block_is_code_only_after_a_blank_line() {
        let s = spans("intro\n\n    #   indented\n\nafter\n");
        assert!(s.line_is_code(3));
        assert!(!s.line_is_code(5));

        // A lazy continuation of a paragraph is prose, not code.
        let s = spans("intro\n    still the paragraph\n");
        assert!(!s.line_is_code(2));
    }

    #[test]
    fn an_unterminated_fence_is_flagged_at_its_opener() {
        let s = spans("intro\n\n```yaml\n  - uses: x\n  - name: y\n");
        assert!(s.opens_unterminated_fence(3));
        assert!(s.line_is_code(4));
        assert!(!s.opens_unterminated_fence(1));

        let s = spans("```yaml\nx: 1\n```\n");
        assert!(
            !s.opens_unterminated_fence(1),
            "a closed fence is not unterminated"
        );
    }

    #[test]
    fn html_comments_are_tracked_across_lines() {
        let s = spans("a\n<!-- start\nstill inside\nend -->\nb\n");
        assert!(!s.line_is_html_comment(1));
        assert!(s.line_is_html_comment(2));
        assert!(s.line_is_html_comment(3));
        assert!(s.line_is_html_comment(4));
        assert!(!s.line_is_html_comment(5));
    }

    #[test]
    fn disable_and_enable_bracket_a_region() {
        let s = spans(
            "a\n<!-- markdownlint-disable no-hard-tabs -->\nb\n<!-- markdownlint-enable no-hard-tabs -->\nc\n",
        );
        let hard_tabs = |t: &str| t.eq_ignore_ascii_case("MD010") || t == "no-hard-tabs";
        assert!(!s.rule_disabled_at(1, hard_tabs));
        assert!(s.rule_disabled_at(3, hard_tabs));
        assert!(!s.rule_disabled_at(5, hard_tabs));
        // A different rule is unaffected.
        assert!(!s.rule_disabled_at(3, |t| t == "no-bare-urls"));
    }

    #[test]
    fn a_bare_disable_covers_every_rule() {
        let s = spans("<!-- markdownlint-disable -->\nx\n");
        assert!(s.rule_disabled_at(2, |_| false));
    }

    #[test]
    fn disable_next_line_and_disable_line_are_scoped_to_one_line() {
        let s = spans("<!-- markdownlint-disable-next-line MD019 -->\n#   x\n#   y\n");
        let md019 = |t: &str| t == "MD019";
        assert!(s.rule_disabled_at(2, md019));
        assert!(!s.rule_disabled_at(3, md019));

        let s = spans("#   x <!-- markdownlint-disable-line MD019 -->\n#   y\n");
        assert!(s.rule_disabled_at(1, md019));
        assert!(!s.rule_disabled_at(2, md019));
    }

    #[test]
    fn disable_file_holds_everywhere() {
        let s = spans("x\ny\n<!-- markdownlint-disable-file MD019 -->\n");
        assert!(s.rule_disabled_at(1, |t| t == "MD019"));
    }

    #[test]
    fn unsupported_and_lookalike_directives_are_ignored() {
        for body in [
            "<!-- markdownlint-capture -->\nx\n",
            "<!-- markdownlint-disabled MD019 -->\nx\n",
            "<!-- not-markdownlint-disable -->\nx\n",
        ] {
            let s = spans(body);
            assert!(
                !s.has_disable_directives(),
                "{body:?} must not register a directive"
            );
        }
    }

    #[test]
    fn a_directive_inside_a_code_fence_is_a_sample_not_a_directive() {
        let s = spans("```md\n<!-- markdownlint-disable -->\n```\n");
        assert!(!s.has_disable_directives());
    }
}
