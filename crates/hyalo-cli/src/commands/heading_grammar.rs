//! Declarative heading-grammar lint mode (generic capability).
//!
//! A *heading grammar* is a small declarative description of the ATX-heading
//! skeleton a document type is expected to follow: the sequence of sections,
//! their level, a pattern the heading text must match, and ordering constraints
//! between sibling sections. iter-166 first hand-rolled these checks for OKF's
//! reserved `index.md` / `log.md` files (`okf_lint.rs`); iter-169 lifts them
//! into this reusable engine so the changelog profile (and later Nygard ADRs,
//! Standard Readme, …) can declare a grammar rather than re-implement a scanner.
//!
//! The engine is intentionally small and self-contained:
//!
//! - [`scan_headings`] is a CRLF-tolerant, fenced-code-aware ATX-heading scanner
//!   returning each heading's level, text, and 1-based line (a trimmed-down
//!   sibling of `okf_lint::scan_sections`; no byte-range bookkeeping is needed
//!   for pure heading-shape rules).
//! - A [`HeadingGrammar`] is a list of [`HeadingRule`]s. Each rule targets
//!   headings of a given level and optionally matches their text against a
//!   [`TextMatcher`]. A grammar is *checked* against the scanned headings and
//!   yields [`GrammarFinding`]s (level + text + ordering violations).
//!
//! Severity is deliberately **per-rule** (`GrammarFinding.rule_id` +
//! `default_severity`): OKF's structure rules are warn-only (permissive), but a
//! changelog grammar is stricter and mixes error-level rules. The consumer maps
//! `rule_id` → catalog entry and applies `[lint.rules.<id>]` overrides exactly
//! like the other profile blocks.
//!
//! Everything here is a pure function over already-read content — no filesystem,
//! no network. The engine expresses *heading-shape* constraints only; anything
//! that needs body content (e.g. the changelog footer link-ref cross-check)
//! stays in the profile's own lint module.

/// A scanned ATX heading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScannedHeading {
    /// Heading level (number of leading `#`, 1..=6).
    pub(crate) level: usize,
    /// Trimmed heading text (everything after the `#`s and the required space).
    pub(crate) text: String,
    /// 1-based file line of the heading (accounting for `line_offset`).
    pub(crate) line: usize,
}

/// Strip a trailing `\r` for CRLF tolerance.
fn trim_cr(line: &str) -> &str {
    line.strip_suffix('\r').unwrap_or(line)
}

/// Scan `content` into a flat, document-order list of ATX headings.
///
/// Fenced code blocks (```` ``` ```` / `~~~`) are skipped so a `#` inside a code
/// fence is never mistaken for a heading. `line_offset` is the 1-based file line
/// on which `content` begins (1 for a whole file; the body offset for a
/// post-frontmatter slice).
pub(crate) fn scan_headings(content: &str, line_offset: usize) -> Vec<ScannedHeading> {
    let mut headings = Vec::new();
    let mut in_fence = false;
    let mut fence_marker = "";
    let mut file_line = line_offset;

    for raw in content.split_inclusive('\n') {
        let line = trim_cr(raw.strip_suffix('\n').unwrap_or(raw));
        let trimmed = line.trim_start();

        // Fenced-code tracking (``` or ~~~). A fence line is never a heading.
        let fence = if trimmed.starts_with("```") {
            Some("```")
        } else if trimmed.starts_with("~~~") {
            Some("~~~")
        } else {
            None
        };
        if let Some(f) = fence {
            if in_fence {
                if f == fence_marker {
                    in_fence = false;
                }
            } else {
                in_fence = true;
                fence_marker = f;
            }
            file_line += 1;
            continue;
        }

        if !in_fence {
            let hashes = line.chars().take_while(|c| *c == '#').count();
            if (1..=6).contains(&hashes) && line[hashes..].starts_with(' ') {
                headings.push(ScannedHeading {
                    level: hashes,
                    text: line[hashes..].trim().to_owned(),
                    line: file_line,
                });
            }
        }

        file_line += 1;
    }
    headings
}

/// How a rule matches a heading's text.
pub(crate) enum TextMatcher {
    /// The text must be exactly this (case-sensitive).
    Exact(&'static str),
    /// The text must be one of these (case-sensitive).
    OneOf(&'static [&'static str]),
    /// The text must satisfy this predicate. The `&'static str` is a
    /// human-readable description of the expected shape, used in messages.
    Predicate(&'static str, fn(&str) -> bool),
}

impl TextMatcher {
    fn matches(&self, text: &str) -> bool {
        match self {
            TextMatcher::Exact(want) => text == *want,
            TextMatcher::OneOf(set) => set.contains(&text),
            TextMatcher::Predicate(_, f) => f(text),
        }
    }

    fn describe(&self) -> String {
        match self {
            TextMatcher::Exact(want) => format!("`{want}`"),
            TextMatcher::OneOf(set) => set
                .iter()
                .map(|s| format!("`{s}`"))
                .collect::<Vec<_>>()
                .join(" | "),
            TextMatcher::Predicate(desc, _) => (*desc).to_owned(),
        }
    }
}

/// A single heading-grammar rule.
pub(crate) struct HeadingRule {
    /// The rule id reported on a violation (maps to a catalog entry).
    pub(crate) rule_id: &'static str,
    /// Default severity when no `[lint.rules.<id>]` override is set.
    pub(crate) default_severity: &'static str,
    /// The heading level this rule constrains (1..=6).
    pub(crate) level: usize,
    /// The text matcher headings at this level must satisfy.
    pub(crate) matcher: TextMatcher,
}

/// A finding produced by checking a grammar.
pub(crate) struct GrammarFinding {
    pub(crate) rule_id: &'static str,
    pub(crate) default_severity: &'static str,
    pub(crate) line: usize,
    pub(crate) message: String,
}

/// Check every heading of `rule.level` against `rule.matcher`, emitting a
/// finding for each heading whose text does not match. `is_enabled` gates the
/// rule (honoring `[lint.rules]` and `--rule`/`--rule-prefix`).
pub(crate) fn check_level_rule(
    headings: &[ScannedHeading],
    rule: &HeadingRule,
    is_enabled: &dyn Fn(&str) -> bool,
    out: &mut Vec<GrammarFinding>,
) {
    if !is_enabled(rule.rule_id) {
        return;
    }
    for h in headings.iter().filter(|h| h.level == rule.level) {
        if !rule.matcher.matches(&h.text) {
            out.push(GrammarFinding {
                rule_id: rule.rule_id,
                default_severity: rule.default_severity,
                line: h.line,
                message: format!(
                    "heading `{}` at level {} does not match expected {}",
                    h.text,
                    rule.level,
                    rule.matcher.describe()
                ),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn always_enabled(_: &str) -> bool {
        true
    }

    #[test]
    fn scan_simple_headings() {
        let hs = scan_headings("# A\n\n## B\n\n### C\n", 1);
        assert_eq!(hs.len(), 3);
        assert_eq!(
            hs[0],
            ScannedHeading {
                level: 1,
                text: "A".into(),
                line: 1
            }
        );
        assert_eq!(
            hs[1],
            ScannedHeading {
                level: 2,
                text: "B".into(),
                line: 3
            }
        );
        assert_eq!(
            hs[2],
            ScannedHeading {
                level: 3,
                text: "C".into(),
                line: 5
            }
        );
    }

    #[test]
    fn scan_respects_line_offset() {
        let hs = scan_headings("# A\n", 5);
        assert_eq!(hs[0].line, 5);
    }

    #[test]
    fn scan_ignores_headings_in_code_fence() {
        let body = "# Real\n\n```\n# not a heading\n```\n\n## Also\n";
        let hs = scan_headings(body, 1);
        let texts: Vec<_> = hs.iter().map(|h| h.text.as_str()).collect();
        assert_eq!(texts, vec!["Real", "Also"]);
    }

    #[test]
    fn scan_tilde_fence_and_crlf() {
        let body = "# Real\r\n~~~\r\n### fenced\r\n~~~\r\n## After\r\n";
        let hs = scan_headings(body, 1);
        let texts: Vec<_> = hs.iter().map(|h| h.text.as_str()).collect();
        assert_eq!(texts, vec!["Real", "After"]);
    }

    #[test]
    fn exact_matcher_flags_mismatch() {
        let hs = scan_headings("# Wrong\n", 1);
        let rule = HeadingRule {
            rule_id: "T-TITLE",
            default_severity: "error",
            level: 1,
            matcher: TextMatcher::Exact("Changelog"),
        };
        let mut out = Vec::new();
        check_level_rule(&hs, &rule, &always_enabled, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "T-TITLE");
        assert_eq!(out[0].default_severity, "error");
    }

    #[test]
    fn exact_matcher_accepts_match() {
        let hs = scan_headings("# Changelog\n", 1);
        let rule = HeadingRule {
            rule_id: "T-TITLE",
            default_severity: "error",
            level: 1,
            matcher: TextMatcher::Exact("Changelog"),
        };
        let mut out = Vec::new();
        check_level_rule(&hs, &rule, &always_enabled, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn one_of_matcher() {
        let hs = scan_headings("### Added\n\n### Bogus\n", 1);
        let rule = HeadingRule {
            rule_id: "T-CAT",
            default_severity: "error",
            level: 3,
            matcher: TextMatcher::OneOf(&["Added", "Changed", "Fixed"]),
        };
        let mut out = Vec::new();
        check_level_rule(&hs, &rule, &always_enabled, &mut out);
        assert_eq!(out.len(), 1);
        assert!(out[0].message.contains("Bogus"));
    }

    #[test]
    fn predicate_matcher() {
        let hs = scan_headings("## good\n\n## BAD\n", 1);
        let rule = HeadingRule {
            rule_id: "T-PRED",
            default_severity: "warn",
            level: 2,
            matcher: TextMatcher::Predicate("lowercase", |t| {
                t.chars().all(|c| !c.is_ascii_uppercase())
            }),
        };
        let mut out = Vec::new();
        check_level_rule(&hs, &rule, &always_enabled, &mut out);
        assert_eq!(out.len(), 1);
        assert!(out[0].message.contains("BAD"));
    }

    #[test]
    fn disabled_rule_is_skipped() {
        let hs = scan_headings("# Wrong\n", 1);
        let rule = HeadingRule {
            rule_id: "T-TITLE",
            default_severity: "error",
            level: 1,
            matcher: TextMatcher::Exact("Changelog"),
        };
        let mut out = Vec::new();
        check_level_rule(&hs, &rule, &|_| false, &mut out);
        assert!(out.is_empty());
    }
}
