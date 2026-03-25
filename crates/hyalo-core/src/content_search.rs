use anyhow::{Context, Result};
use regex::{Regex, RegexBuilder};

use crate::heading::parse_atx_heading;
use crate::scanner::{FileVisitor, ScanAction};
use crate::types::ContentMatch;

/// How the visitor matches body lines.
#[derive(Debug)]
enum SearchMode {
    /// Case-insensitive substring (lowercased pattern).
    Substring(String),
    /// Compiled regular expression.
    Regex(Regex),
}

/// Visitor that searches body lines for content matches.
/// Tracks current section heading for context in matches.
#[derive(Debug)]
pub struct ContentSearchVisitor {
    mode: SearchMode,
    /// Current section heading (e.g. "## Design")
    current_section: String,
    /// Collected matches
    matches: Vec<ContentMatch>,
}

impl ContentSearchVisitor {
    /// Create a visitor that does **case-insensitive substring** search.
    #[must_use]
    pub fn new(pattern: &str) -> Self {
        Self {
            mode: SearchMode::Substring(pattern.to_ascii_lowercase()),
            current_section: String::new(),
            matches: Vec::new(),
        }
    }

    /// Compile a **regular expression** for content search.
    ///
    /// The pattern is always prefixed with `(?i)` to make it case-insensitive
    /// by default. Users can override this for all or part of their pattern
    /// with `(?-i)` — the regex crate resolves nested flags correctly.
    #[must_use = "returns a compiled regex visitor; call has no side effects"]
    pub fn regex(pattern: &str) -> Result<Self> {
        let effective = format!("(?i){pattern}");
        let re = RegexBuilder::new(&effective)
            .size_limit(1 << 20) // 1 MiB — generous for real patterns, prevents pathological ones
            .build()
            .with_context(|| format!("invalid regular expression: {pattern}"))?;
        Ok(Self {
            mode: SearchMode::Regex(re),
            current_section: String::new(),
            matches: Vec::new(),
        })
    }

    /// Create a visitor from an already-compiled `Regex`.
    ///
    /// Use this to avoid recompiling the same regex for each file.
    /// The `Regex` is internally reference-counted, so cloning is cheap.
    #[must_use]
    pub fn from_compiled(re: Regex) -> Self {
        Self {
            mode: SearchMode::Regex(re),
            current_section: String::new(),
            matches: Vec::new(),
        }
    }

    /// Returns `true` if any matches were collected.
    #[must_use]
    pub fn has_matches(&self) -> bool {
        !self.matches.is_empty()
    }

    /// Consume the visitor and return all collected matches.
    #[must_use]
    pub fn into_matches(self) -> Vec<ContentMatch> {
        self.matches
    }

    /// Check whether a line matches the current mode.
    fn is_match(&self, line: &str) -> bool {
        match &self.mode {
            SearchMode::Substring(pat) => contains_ignore_ascii_case(line, pat),
            SearchMode::Regex(re) => re.is_match(line),
        }
    }
}

/// Case-insensitive ASCII substring check without allocation.
///
/// Uses ASCII-only case folding (`to_ascii_lowercase`). For Unicode case
/// folding (e.g. `ß` → `ss`), use the regex search mode instead.
///
/// `needle` must already be lowercased. Each byte of the haystack window is
/// folded to lowercase before comparison, so no temporary `String` is created.
fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let needle_bytes = needle.as_bytes();
    let haystack_bytes = haystack.as_bytes();
    if haystack_bytes.len() < needle_bytes.len() {
        return false;
    }
    for i in 0..=(haystack_bytes.len() - needle_bytes.len()) {
        if haystack_bytes[i..i + needle_bytes.len()]
            .iter()
            .zip(needle_bytes)
            .all(|(h, n)| h.to_ascii_lowercase() == *n)
        {
            return true;
        }
    }
    false
}

impl FileVisitor for ContentSearchVisitor {
    fn on_body_line(&mut self, raw: &str, _cleaned: &str, line_num: usize) -> ScanAction {
        // Use raw text for heading detection so that code spans in headings
        // (e.g. `## The \`versions\` field`) are preserved in section context.
        if let Some((level, heading_text)) = parse_atx_heading(raw) {
            self.current_section = format!("{} {}", "#".repeat(level as usize), heading_text);
        }

        // Match against raw text so that users can search for backtick content.
        if self.is_match(raw) {
            self.matches.push(ContentMatch {
                line: line_num,
                section: self.current_section.clone(),
                text: raw.to_owned(),
            });
        }

        ScanAction::Continue
    }

    fn on_code_block_line(&mut self, raw: &str, line_num: usize) -> ScanAction {
        // Do NOT parse headings here — `#` inside code blocks is code, not structure.
        if self.is_match(raw) {
            self.matches.push(ContentMatch {
                line: line_num,
                section: self.current_section.clone(),
                text: raw.to_owned(),
            });
        }
        ScanAction::Continue
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn run_visitor(content: &str, pattern: &str) -> Vec<ContentMatch> {
        let mut visitor = ContentSearchVisitor::new(pattern);
        let lines: Vec<&str> = content.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            // Test helpers pass raw == cleaned (no stripping needed in unit tests).
            visitor.on_body_line(line, line, i + 1);
        }
        visitor.into_matches()
    }

    fn run_regex_visitor(content: &str, pattern: &str) -> Vec<ContentMatch> {
        let mut visitor = ContentSearchVisitor::regex(pattern).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            // Test helpers pass raw == cleaned (no stripping needed in unit tests).
            visitor.on_body_line(line, line, i + 1);
        }
        visitor.into_matches()
    }

    // --- substring mode (existing behaviour) ---

    #[test]
    fn finds_exact_match() {
        let matches = run_visitor("Hello world\nnothing here\n", "world");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line, 1);
        assert_eq!(matches[0].text, "Hello world");
    }

    #[test]
    fn case_insensitive_match() {
        let matches = run_visitor("Hello WORLD\nGoodbye world\n", "world");
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn uppercase_pattern_matches_lowercase_line() {
        let matches = run_visitor("foo bar baz\n", "BAR");
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn no_match_returns_empty() {
        let matches = run_visitor("Nothing relevant here\n", "zzz");
        assert!(matches.is_empty());
    }

    #[test]
    fn has_matches_false_when_empty() {
        let visitor = ContentSearchVisitor::new("x");
        assert!(!visitor.has_matches());
    }

    #[test]
    fn has_matches_true_after_match() {
        let mut visitor = ContentSearchVisitor::new("hello");
        visitor.on_body_line("say hello", "say hello", 1);
        assert!(visitor.has_matches());
    }

    #[test]
    fn correct_line_numbers() {
        let content = "line one\nline two\nline three\n";
        let matches = run_visitor(content, "two");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line, 2);
    }

    #[test]
    fn section_tracking_updates_on_heading() {
        let content = "## Design\nsome text\n### Sub\nother text\n";
        let matches = run_visitor(content, "text");
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].section, "## Design");
        assert_eq!(matches[1].section, "### Sub");
    }

    #[test]
    fn section_empty_before_first_heading() {
        let matches = run_visitor("intro text\n## Section\n", "intro");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].section, "");
    }

    #[test]
    fn heading_line_itself_can_be_matched() {
        let matches = run_visitor("## Design Goals\n", "design");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].text, "## Design Goals");
        assert_eq!(matches[0].section, "## Design Goals");
    }

    #[test]
    fn heading_not_matched_when_no_pattern() {
        let matches = run_visitor("## Design\nsome content\n", "content");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].section, "## Design");
    }

    #[test]
    fn into_matches_consumes_visitor() {
        let mut visitor = ContentSearchVisitor::new("hello");
        visitor.on_body_line("say hello", "say hello", 1);
        visitor.on_body_line("hello again", "hello again", 2);
        let matches = visitor.into_matches();
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn level_1_heading_tracked() {
        let matches = run_visitor("# Top Level\nbody\n", "body");
        assert_eq!(matches[0].section, "# Top Level");
    }

    #[test]
    fn invalid_atx_heading_not_tracked() {
        let matches = run_visitor("#NoSpace\nbody\n", "body");
        assert_eq!(matches[0].section, "");
    }

    #[test]
    fn heading_with_inline_code_span_preserved_in_section() {
        // The heading `## The \`versions\` field` must appear verbatim in the
        // section context — not with the code span replaced by spaces.
        //
        // This test drives raw text directly to on_body_line to isolate the
        // ContentSearchVisitor from dispatch_body_line's stripping logic.
        let content = "## The `versions` field\nsome text\n";
        let matches = run_visitor(content, "text");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].section, "## The `versions` field");
    }

    #[test]
    fn heading_with_inline_code_span_is_matchable() {
        // Users should be able to search for content inside a code span in a heading.
        let content = "## The `versions` field\n";
        let matches = run_visitor(content, "versions");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].text, "## The `versions` field");
    }

    // --- regex mode ---

    #[test]
    fn regex_simple_match() {
        let matches = run_regex_visitor("Hello world\nnothing here\n", "wor.d");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].text, "Hello world");
    }

    #[test]
    fn regex_case_insensitive_by_default() {
        let matches = run_regex_visitor("Hello WORLD\nGoodbye world\n", "world");
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn regex_alternation() {
        let matches = run_regex_visitor("TODO fix this\nFIXME later\nall good\n", "TODO|FIXME");
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn regex_anchored() {
        let matches = run_regex_visitor("## Design\nnot a heading\n", r"^##\s");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].text, "## Design");
    }

    #[test]
    fn regex_explicit_case_sensitive() {
        // User supplies (?-i) to override default case-insensitivity
        let matches = run_regex_visitor("Hello WORLD\nGoodbye world\n", "(?-i)WORLD");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].text, "Hello WORLD");
    }

    #[test]
    fn regex_user_flag_overrides_default() {
        // (?-i) in the user pattern overrides the auto-prepended (?i)
        let matches = run_regex_visitor("Hello WORLD\nGoodbye world\n", "(?-i)world");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].text, "Goodbye world");
    }

    #[test]
    fn regex_section_tracking() {
        let content = "## Tasks\n- TODO item\n### Done\n- completed\n";
        let matches = run_regex_visitor(content, "TODO|completed");
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].section, "## Tasks");
        assert_eq!(matches[1].section, "### Done");
    }

    #[test]
    fn regex_invalid_returns_error() {
        let result = ContentSearchVisitor::regex("[invalid");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid regular expression"), "got: {err}");
    }

    #[test]
    fn regex_non_capturing_group_still_case_insensitive() {
        // (?:...) is a non-capturing group, not a flag group — must still be case-insensitive
        let matches = run_regex_visitor("Hello WORLD\nGoodbye world\n", "(?:world)");
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn regex_empty_pattern_matches_everything() {
        // Empty regex compiles to (?i) which matches every line
        let matches = run_regex_visitor("line one\nline two\n", "");
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn regex_no_match_returns_empty() {
        let matches = run_regex_visitor("Nothing here\n", r"\d{4}-\d{2}-\d{2}");
        assert!(matches.is_empty());
    }

    #[test]
    fn regex_rejects_oversized_pattern() {
        // A huge alternation that exceeds the 1 MiB compiled-size limit
        let huge = (0..50_000)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join("|");
        let result = ContentSearchVisitor::regex(&huge);
        assert!(result.is_err(), "oversized pattern should be rejected");
    }

    // --- full pipeline (code block coverage) ---

    fn run_full_scan(content: &str, pattern: &str) -> Vec<ContentMatch> {
        use crate::scanner::scan_reader_multi;
        let mut visitor = ContentSearchVisitor::new(pattern);
        scan_reader_multi(content.as_bytes(), &mut [&mut visitor]).unwrap();
        visitor.into_matches()
    }

    #[test]
    fn finds_match_inside_code_block() {
        let content = "---\n---\n## Code\n```rust\nlet typescript = 42;\n```\n";
        let matches = run_full_scan(content, "typescript");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line, 5);
        assert_eq!(matches[0].section, "## Code");
    }

    #[test]
    fn finds_match_inside_code_block_regex() {
        use crate::scanner::scan_reader_multi;
        let content = "---\n---\n```\nfoo_bar_baz\n```\n";
        let mut visitor = ContentSearchVisitor::regex("foo.*baz").unwrap();
        scan_reader_multi(content.as_bytes(), &mut [&mut visitor]).unwrap();
        let matches = visitor.into_matches();
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn code_block_match_outside_and_inside() {
        let content = "---\n---\nhello world\n```\nhello code\n```\n";
        let matches = run_full_scan(content, "hello");
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn heading_inside_code_block_not_tracked_as_section() {
        // A `#` line inside a code block must not change current_section
        let content = "---\n---\n## Real Section\n```\n# not a heading\nfoo\n```\nbar\n";
        let matches = run_full_scan(content, "bar");
        assert_eq!(matches.len(), 1);
        // section should still be the last real heading, not the code block `#`
        assert_eq!(matches[0].section, "## Real Section");
    }

    #[test]
    fn no_match_inside_code_block_when_pattern_absent() {
        let content = "---\n---\n```\nsome code here\n```\n";
        let matches = run_full_scan(content, "zzz");
        assert!(matches.is_empty());
    }

    // --- contains_ignore_ascii_case ---

    #[test]
    fn ascii_case_empty_needle() {
        assert!(super::contains_ignore_ascii_case("anything", ""));
        assert!(super::contains_ignore_ascii_case("", ""));
    }

    #[test]
    fn ascii_case_needle_longer_than_haystack() {
        assert!(!super::contains_ignore_ascii_case("ab", "abc"));
    }

    #[test]
    fn ascii_case_exact_match() {
        assert!(super::contains_ignore_ascii_case("hello", "hello"));
    }

    #[test]
    fn ascii_case_mixed_case_match() {
        assert!(super::contains_ignore_ascii_case("Hello WORLD", "lo wor"));
    }

    #[test]
    fn ascii_case_no_match() {
        assert!(!super::contains_ignore_ascii_case("hello world", "xyz"));
    }

    #[test]
    fn ascii_case_multibyte_utf8_in_haystack() {
        // Multi-byte chars in the haystack should not break the search
        assert!(super::contains_ignore_ascii_case("café latte", "latte"));
        assert!(super::contains_ignore_ascii_case("über cool", "cool"));
    }

    #[test]
    fn ascii_case_match_at_end() {
        assert!(super::contains_ignore_ascii_case("say HELLO", "hello"));
    }

    #[test]
    fn ascii_case_single_char() {
        assert!(super::contains_ignore_ascii_case("A", "a"));
        assert!(!super::contains_ignore_ascii_case("A", "b"));
    }
}
