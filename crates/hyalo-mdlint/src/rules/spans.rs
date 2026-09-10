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
    syntax: hyalo_core::body_syntax::BodySyntax,
    /// `markdownlint-…` directives, in document order.
    disable_events: Vec<DisableEvent>,
}

/// A rule token a directive named, with the 1-based body line of its comment.
///
/// Reported so the caller can check it against the rule catalogue: markdownlint
/// warns about an unknown id or alias in a suppression comment, and a silently
/// ignored typo means the region the author meant to protect is still linted
/// (BUG-43, dogfood v0.22.0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectiveToken {
    /// 1-based body line the `<!-- markdownlint-… -->` comment sits on.
    pub line: usize,
    /// The token exactly as written.
    pub token: String,
}

impl BodySpans {
    /// Classify every line of `body`.
    #[must_use]
    pub fn new(body: &str) -> Self {
        let syntax = hyalo_core::body_syntax::BodySyntax::new(body);
        let mut spans = Self {
            syntax,
            disable_events: Vec::new(),
        };
        for comment in spans.syntax.html_comments() {
            if let Some(inner) = body.get(comment.content_span())
                && let Some(event) = parse_directive(inner, comment.line() - 1)
            {
                spans.disable_events.push(event);
            }
        }
        spans
    }

    /// Whether the 1-based body line is code — fenced or indented — and so
    /// must not be rewritten by a rule that lints prose.
    #[must_use]
    pub fn line_is_code(&self, line_1based: usize) -> bool {
        self.syntax.line_is_code(line_1based)
    }

    /// Whether the 1-based body line sits inside an HTML comment.
    #[must_use]
    pub fn line_is_html_comment(&self, line_1based: usize) -> bool {
        self.syntax.line_is_html_comment(line_1based)
    }

    /// Whether a converted autofix span reaches into code or a wholly
    /// commented line, even when the diagnostic itself starts in prose.
    #[must_use]
    pub fn fix_intersects_protected(&self, start: usize, end: usize) -> bool {
        self.syntax.range_intersects_protected(start..end)
    }

    /// Whether the 1-based body line opens a fenced code block that is never
    /// closed. A "blank line after this fence" proposal there would land
    /// inside the sample, so MD031 must stay quiet (BUG-3).
    #[must_use]
    pub fn opens_unterminated_fence(&self, line_1based: usize) -> bool {
        self.syntax.opens_unterminated_fence(line_1based)
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

    /// Every rule token named by a `markdownlint-…` directive, with the
    /// 1-based body line of the comment it came from, in document order.
    ///
    /// The caller resolves each token against the rule catalogue: hyalo cannot
    /// tell an id from an alias here, and a token that names neither is a typo
    /// worth reporting (BUG-43).
    #[must_use]
    pub fn directive_tokens(&self) -> Vec<DirectiveToken> {
        self.disable_events
            .iter()
            .flat_map(|ev| {
                ev.tokens.iter().map(move |t| DirectiveToken {
                    line: ev.line + 1,
                    token: t.clone(),
                })
            })
            .collect()
    }

    /// Byte offsets at which an autofix must not insert a line break.
    ///
    /// A `markdownlint-disable-next-line` comment binds to the line below it;
    /// pushing the two apart — as MD022's "surround headings with blank lines"
    /// fix did — silently disarms the directive, and the next fix pass then
    /// rewrites the very line the author protected (BUG-4, dogfood v0.22.0).
    /// Each returned offset is the first byte of a guarded line.
    #[must_use]
    pub fn guarded_line_starts(&self) -> Vec<usize> {
        self.disable_events
            .iter()
            .filter(|ev| ev.kind == DisableKind::DisableNextLine)
            .filter_map(|ev| self.syntax.line_start(ev.line + 2))
            .collect()
    }

    /// Mask code, comments, and inline literal spans while preserving every
    /// byte offset and newline. Native line-based rules inspect this view so
    /// their diagnostics and fixes cannot target literal examples.
    #[must_use]
    pub(crate) fn structural_body(&self, body: &str) -> String {
        debug_assert_eq!(self.syntax.visible_body().len(), body.len());
        self.syntax.visible_body().to_owned()
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

    // --- BUG-4 (iter-276): a trailing directive does not comment out its line

    #[test]
    fn a_line_with_a_trailing_comment_is_not_a_comment_line() {
        let s = spans("#   heading <!-- markdownlint-disable-next-line MD019 -->\n#   next\n");
        assert!(
            !s.line_is_html_comment(1),
            "the heading is still a heading, so MD019 must see it"
        );
        let md019 = |t: &str| t == "MD019";
        assert!(
            !s.rule_disabled_at(1, md019),
            "-next-line never protects its own line"
        );
        assert!(s.rule_disabled_at(2, md019), "it protects the one after");
    }

    #[test]
    fn a_standalone_comment_line_is_still_a_comment_line() {
        let s = spans("  <!-- a note -->  \ntext\n");
        assert!(s.line_is_html_comment(1));
        assert!(!s.line_is_html_comment(2));
    }

    #[test]
    fn text_after_a_closing_comment_marker_makes_the_line_prose() {
        let s = spans("a\n<!-- start\nstill inside\nend --> #   tail\n");
        assert!(s.line_is_html_comment(3), "wholly inside the comment");
        assert!(
            !s.line_is_html_comment(4),
            "content after `-->` is real prose"
        );
    }

    // --- BUG-5 (iter-276): fences indented inside list items

    #[test]
    fn a_fence_indented_under_an_ordered_item_is_a_fence() {
        // The GitHub Docs shape: a 4-space-indented `1.` whose fence sits at
        // five columns. Before iter-276 every prose rule fired on line 3.
        let s = spans(
            "    1. Add the registry\n     ```\n     npmRegistryServer: \"https://x\"\n     ```\n",
        );
        assert!(!s.line_is_code(2), "the opening fence is markup");
        assert!(s.line_is_code(3), "the sample is code");
        assert!(!s.line_is_code(4), "the closing fence is markup");
    }

    #[test]
    fn a_fence_indented_under_a_bullet_item_is_a_fence() {
        let s = spans("- item\n\n  ~~~yaml\n  key:   value\n  ~~~\n\nafter\n");
        assert!(s.line_is_code(4));
        assert!(!s.line_is_code(3));
        assert!(!s.line_is_code(7));
    }

    #[test]
    fn a_fence_inside_a_blockquoted_list_is_a_fence() {
        let s = spans("> - item\n>   ```\n>   #   sample\n>   ```\n");
        assert!(s.line_is_code(3));
        assert!(!s.line_is_code(2));
    }

    #[test]
    fn a_nested_list_widens_the_allowance_further() {
        let s = spans("- outer\n  - inner\n    ```\n    #   sample\n    ```\n");
        assert!(s.line_is_code(4));
    }

    #[test]
    fn dedenting_out_of_a_list_restores_the_column_zero_rule() {
        // After the list ends, a four-space fence is an indented code block —
        // which is still code, but not a fence, so it must not swallow the
        // rest of the document.
        let s = spans("- item\n\nprose\n\n    ```\n    x\n\nafter\n");
        assert!(s.line_is_code(5), "indented code block");
        assert!(!s.line_is_code(8), "the block ended at the blank line");
    }

    #[test]
    fn a_lookalike_marker_does_not_open_a_container() {
        // `1.5` and `-fish` are prose, not list markers.
        let s = spans("1.5 metres\n    ```\n    x\n");
        assert!(
            !s.opens_unterminated_fence(2),
            "no container, so a four-space fence is not a fence"
        );
    }

    // --- BUG-43 (iter-276): directive tokens are reported for validation

    #[test]
    fn directive_tokens_carry_their_comment_line() {
        let s = spans("a\n<!-- markdownlint-disable MD019, no-such-rule -->\nb\n");
        let tokens = s.directive_tokens();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].line, 2);
        assert_eq!(tokens[0].token, "MD019");
        assert_eq!(tokens[1].token, "no-such-rule");
    }

    #[test]
    fn inline_code_comment_example_is_not_a_directive() {
        let s = spans("`<!-- markdownlint-disable MD019 -->`\n#   real violation\n");
        assert!(s.directive_tokens().is_empty());
        assert!(!s.rule_disabled_at(2, |token| token.eq_ignore_ascii_case("MD019")));
    }

    #[test]
    fn a_bare_directive_names_no_tokens() {
        assert!(
            spans("<!-- markdownlint-disable -->\nx\n")
                .directive_tokens()
                .is_empty()
        );
    }
}
