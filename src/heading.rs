//! Shared ATX heading parser and section-scoped filtering.
//!
//! Consolidates heading detection (previously duplicated across `content_search`,
//! `tasks`, and `commands::outline`) and provides [`SectionFilter`] for the
//! `--section` CLI flag used by `find` and `read`.

use crate::types::OutlineSection;

// ---------------------------------------------------------------------------
// ATX heading parser
// ---------------------------------------------------------------------------

/// Parse an ATX heading line (`# Heading`, `## Sub`, etc.).
///
/// Returns `(level, heading_text)` where level is 1–6 and heading_text has
/// leading/trailing whitespace and optional closing `#` sequences stripped.
/// Returns `None` if the line is not a valid ATX heading.
pub fn parse_atx_heading(line: &str) -> Option<(u8, &str)> {
    let bytes = line.as_bytes();
    if bytes.first() != Some(&b'#') {
        return None;
    }

    let level = bytes.iter().take_while(|&&b| b == b'#').count();
    if level > 6 {
        return None;
    }

    let rest = &line[level..];

    let text = if rest.is_empty() {
        ""
    } else if rest.starts_with(' ') || rest.starts_with('\t') {
        rest[1..].trim_end_matches('#').trim()
    } else {
        return None;
    };

    #[allow(clippy::cast_possible_truncation)]
    Some((level as u8, text))
}

// ---------------------------------------------------------------------------
// SectionFilter
// ---------------------------------------------------------------------------

/// A parsed `--section` filter value.
///
/// Supports two forms:
/// - `"Foo"` — match heading text case-insensitively at any level
/// - `"## Foo"` — match heading text case-insensitively at exactly level 2
#[derive(Debug, Clone)]
pub struct SectionFilter {
    /// If `Some`, match only headings at this exact level.
    level: Option<u8>,
    /// Heading text to match (lowercased for case-insensitive comparison).
    text: String,
}

impl SectionFilter {
    /// Parse a `--section` value into a `SectionFilter`.
    ///
    /// Returns an error if the value contains a `#` prefix that doesn't parse
    /// as a valid ATX heading (e.g. `"####### Too deep"`).
    pub fn parse(input: &str) -> Result<Self, String> {
        if input.starts_with('#') {
            match parse_atx_heading(input) {
                Some((level, text)) => Ok(Self {
                    level: Some(level),
                    text: text.to_ascii_lowercase(),
                }),
                None => Err(format!(
                    "invalid section filter: {input:?} (starts with '#' but is not a valid heading)"
                )),
            }
        } else {
            let trimmed = input.trim();
            if trimmed.is_empty() {
                return Err("section filter must not be empty".to_owned());
            }
            Ok(Self {
                level: None,
                text: trimmed.to_ascii_lowercase(),
            })
        }
    }

    /// Check if a heading matches this filter.
    ///
    /// - Case-insensitive whole-string match on heading text.
    /// - If `level` is set, heading level must match exactly.
    #[must_use]
    pub fn matches(&self, heading_level: u8, heading_text: &str) -> bool {
        let level_ok = self.level.is_none_or(|l| l == heading_level);
        let text_ok = heading_text.eq_ignore_ascii_case(&self.text);
        level_ok && text_ok
    }
}

// ---------------------------------------------------------------------------
// Section scope builder
// ---------------------------------------------------------------------------

/// An inclusive line range `[start, end]` representing a section's scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionRange {
    /// 1-based start line (the heading line itself).
    pub start: usize,
    /// 1-based end line (inclusive). Content up to and including this line is in scope.
    pub end: usize,
}

impl SectionRange {
    /// Check if a 1-based line number falls within this range.
    #[must_use]
    pub fn contains_line(&self, line: usize) -> bool {
        line >= self.start && line <= self.end
    }
}

/// Build line ranges for all sections matching any of the given filters.
///
/// Walks the outline and, for each heading that matches a filter, opens a scope
/// that extends until the next heading of **equal or higher** level (exclusive)
/// or end-of-file. Child (deeper) headings are included in the parent's scope.
///
/// `total_lines` is the total number of body lines in the file (used to close
/// the final scope). Pass `usize::MAX` if unknown.
#[must_use]
pub fn build_section_scope(
    sections: &[OutlineSection],
    filters: &[SectionFilter],
    total_lines: usize,
) -> Vec<SectionRange> {
    if filters.is_empty() || sections.is_empty() {
        return Vec::new();
    }

    let mut ranges: Vec<SectionRange> = Vec::new();

    for (i, sec) in sections.iter().enumerate() {
        let heading_text = match &sec.heading {
            Some(h) => h.as_str(),
            None => continue, // pre-heading section has no heading to match
        };

        if !filters.iter().any(|f| f.matches(sec.level, heading_text)) {
            continue;
        }

        // Find the end of this section: next heading at equal or higher level
        let end = sections
            .iter()
            .skip(i + 1)
            .find(|s| s.level <= sec.level)
            .map_or(total_lines, |s| s.line.saturating_sub(1));

        ranges.push(SectionRange {
            start: sec.line,
            end,
        });
    }

    ranges
}

/// Check if a line falls within any of the given section ranges.
#[must_use]
pub fn in_scope(ranges: &[SectionRange], line: usize) -> bool {
    ranges.iter().any(|r| r.contains_line(line))
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TaskCount;

    // --- parse_atx_heading ---

    #[test]
    fn heading_level_1() {
        let (level, text) = parse_atx_heading("# Hello").unwrap();
        assert_eq!(level, 1);
        assert_eq!(text, "Hello");
    }

    #[test]
    fn heading_level_3() {
        let (level, text) = parse_atx_heading("### Sub section").unwrap();
        assert_eq!(level, 3);
        assert_eq!(text, "Sub section");
    }

    #[test]
    fn heading_max_level_6() {
        let (level, text) = parse_atx_heading("###### Deep").unwrap();
        assert_eq!(level, 6);
        assert_eq!(text, "Deep");
    }

    #[test]
    fn heading_7_hashes_not_heading() {
        assert!(parse_atx_heading("####### Too deep").is_none());
    }

    #[test]
    fn heading_no_space_not_heading() {
        assert!(parse_atx_heading("#NoSpace").is_none());
    }

    #[test]
    fn heading_empty() {
        let (level, text) = parse_atx_heading("##").unwrap();
        assert_eq!(level, 2);
        assert_eq!(text, "");
    }

    #[test]
    fn heading_with_closing_hashes() {
        let (level, text) = parse_atx_heading("## Section ##").unwrap();
        assert_eq!(level, 2);
        assert_eq!(text, "Section");
    }

    #[test]
    fn not_a_heading() {
        assert!(parse_atx_heading("Normal text").is_none());
        assert!(parse_atx_heading("").is_none());
    }

    #[test]
    fn heading_with_tab_separator() {
        let (level, text) = parse_atx_heading("#\tTabbed").unwrap();
        assert_eq!(level, 1);
        assert_eq!(text, "Tabbed");
    }

    // --- SectionFilter::parse ---

    #[test]
    fn parse_plain_text() {
        let f = SectionFilter::parse("Tasks").unwrap();
        assert!(f.level.is_none());
        assert_eq!(f.text, "tasks");
    }

    #[test]
    fn parse_with_hashes() {
        let f = SectionFilter::parse("## Design").unwrap();
        assert_eq!(f.level, Some(2));
        assert_eq!(f.text, "design");
    }

    #[test]
    fn parse_level_1() {
        let f = SectionFilter::parse("# Top").unwrap();
        assert_eq!(f.level, Some(1));
        assert_eq!(f.text, "top");
    }

    #[test]
    fn parse_empty_errors() {
        assert!(SectionFilter::parse("").is_err());
        assert!(SectionFilter::parse("  ").is_err());
    }

    #[test]
    fn parse_invalid_heading_errors() {
        assert!(SectionFilter::parse("####### Too deep").is_err());
    }

    #[test]
    fn parse_hash_no_space_errors() {
        assert!(SectionFilter::parse("#NoSpace").is_err());
    }

    // --- SectionFilter::matches ---

    #[test]
    fn matches_any_level() {
        let f = SectionFilter::parse("Tasks").unwrap();
        assert!(f.matches(1, "Tasks"));
        assert!(f.matches(2, "Tasks"));
        assert!(f.matches(3, "tasks"));
        assert!(f.matches(6, "TASKS"));
    }

    #[test]
    fn matches_pinned_level() {
        let f = SectionFilter::parse("## Tasks").unwrap();
        assert!(f.matches(2, "Tasks"));
        assert!(f.matches(2, "tasks"));
        assert!(!f.matches(1, "Tasks"));
        assert!(!f.matches(3, "Tasks"));
    }

    #[test]
    fn no_partial_match() {
        let f = SectionFilter::parse("Task").unwrap();
        assert!(!f.matches(2, "Tasks"));
        assert!(!f.matches(2, "My Task"));
    }

    // --- build_section_scope ---

    fn make_section(level: u8, heading: &str, line: usize) -> OutlineSection {
        OutlineSection {
            level,
            heading: Some(heading.to_owned()),
            line,
            links: Vec::new(),
            tasks: None,
            code_blocks: Vec::new(),
        }
    }

    fn make_pre_heading(line: usize) -> OutlineSection {
        OutlineSection {
            level: 0,
            heading: None,
            line,
            links: Vec::new(),
            tasks: None,
            code_blocks: Vec::new(),
        }
    }

    #[test]
    fn scope_single_match() {
        // ## Tasks (line 5) ... ## Other (line 10) ... EOF at 20
        let sections = vec![make_section(2, "Tasks", 5), make_section(2, "Other", 10)];
        let filters = vec![SectionFilter::parse("Tasks").unwrap()];
        let ranges = build_section_scope(&sections, &filters, 20);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0], SectionRange { start: 5, end: 9 });
    }

    #[test]
    fn scope_includes_children() {
        // ## Tasks (line 5) -> ### Subtasks (line 8) -> ## Other (line 15)
        let sections = vec![
            make_section(2, "Tasks", 5),
            make_section(3, "Subtasks", 8),
            make_section(2, "Other", 15),
        ];
        let filters = vec![SectionFilter::parse("Tasks").unwrap()];
        let ranges = build_section_scope(&sections, &filters, 20);
        assert_eq!(ranges.len(), 1);
        // Tasks scope: 5..14 (up to but not including ## Other at 15)
        assert_eq!(ranges[0], SectionRange { start: 5, end: 14 });
        // Line 8 (### Subtasks) and line 12 are within scope
        assert!(in_scope(&ranges, 8));
        assert!(in_scope(&ranges, 12));
        // Line 15 (## Other) is NOT in scope
        assert!(!in_scope(&ranges, 15));
    }

    #[test]
    fn scope_multiple_matches_in_one_doc() {
        // # Alpha (line 1) -> ## Tasks (line 3) -> # Beta (line 8) -> ## Tasks (line 10) -> EOF 15
        let sections = vec![
            make_section(1, "Alpha", 1),
            make_section(2, "Tasks", 3),
            make_section(1, "Beta", 8),
            make_section(2, "Tasks", 10),
        ];
        let filters = vec![SectionFilter::parse("Tasks").unwrap()];
        let ranges = build_section_scope(&sections, &filters, 15);
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0], SectionRange { start: 3, end: 7 });
        assert_eq!(ranges[1], SectionRange { start: 10, end: 15 });
    }

    #[test]
    fn scope_last_section_extends_to_eof() {
        let sections = vec![make_section(2, "Tasks", 5)];
        let filters = vec![SectionFilter::parse("Tasks").unwrap()];
        let ranges = build_section_scope(&sections, &filters, 50);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0], SectionRange { start: 5, end: 50 });
    }

    #[test]
    fn scope_no_match_returns_empty() {
        let sections = vec![make_section(2, "Other", 5)];
        let filters = vec![SectionFilter::parse("Tasks").unwrap()];
        let ranges = build_section_scope(&sections, &filters, 20);
        assert!(ranges.is_empty());
    }

    #[test]
    fn scope_pre_heading_never_matches() {
        let sections = vec![make_pre_heading(1), make_section(2, "Tasks", 5)];
        let filters = vec![SectionFilter::parse("Tasks").unwrap()];
        let ranges = build_section_scope(&sections, &filters, 20);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start, 5);
    }

    #[test]
    fn scope_or_semantics_multiple_filters() {
        let sections = vec![
            make_section(2, "Tasks", 3),
            make_section(2, "Notes", 8),
            make_section(2, "Other", 12),
        ];
        let filters = vec![
            SectionFilter::parse("Tasks").unwrap(),
            SectionFilter::parse("Notes").unwrap(),
        ];
        let ranges = build_section_scope(&sections, &filters, 20);
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0], SectionRange { start: 3, end: 7 });
        assert_eq!(ranges[1], SectionRange { start: 8, end: 11 });
    }

    #[test]
    fn scope_level_pinned_filter() {
        let sections = vec![make_section(1, "Tasks", 1), make_section(2, "Tasks", 5)];
        let filters = vec![SectionFilter::parse("## Tasks").unwrap()];
        let ranges = build_section_scope(&sections, &filters, 20);
        // Should only match the level-2 heading
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start, 5);
    }

    #[test]
    fn scope_with_task_counts() {
        // Ensure OutlineSection with task counts still works
        let sections = vec![OutlineSection {
            level: 2,
            heading: Some("Tasks".to_owned()),
            line: 5,
            links: Vec::new(),
            tasks: Some(TaskCount { total: 3, done: 1 }),
            code_blocks: Vec::new(),
        }];
        let filters = vec![SectionFilter::parse("Tasks").unwrap()];
        let ranges = build_section_scope(&sections, &filters, 20);
        assert_eq!(ranges.len(), 1);
    }
}
