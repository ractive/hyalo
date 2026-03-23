use anyhow::{Context, Result};
use regex::Regex;

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
            mode: SearchMode::Substring(pattern.to_lowercase()),
            current_section: String::new(),
            matches: Vec::new(),
        }
    }

    /// Create a visitor that searches with a **regular expression**.
    ///
    /// The pattern is automatically made case-insensitive (prefixed with
    /// `(?i)`) unless it already contains an inline flag group.
    pub fn regex(pattern: &str) -> Result<Self> {
        let effective = if pattern.starts_with("(?") {
            pattern.to_owned()
        } else {
            format!("(?i){pattern}")
        };
        let re = Regex::new(&effective)
            .with_context(|| format!("invalid regular expression: {pattern}"))?;
        Ok(Self {
            mode: SearchMode::Regex(re),
            current_section: String::new(),
            matches: Vec::new(),
        })
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
            SearchMode::Substring(pat) => line.to_lowercase().contains(pat),
            SearchMode::Regex(re) => re.is_match(line),
        }
    }
}

impl FileVisitor for ContentSearchVisitor {
    fn on_body_line(&mut self, raw: &str, line_num: usize) -> ScanAction {
        // Check for ATX heading and update section context.
        if let Some((level, heading_text)) = detect_heading(raw) {
            self.current_section = format!("{} {}", "#".repeat(level as usize), heading_text);
        }

        // Check for match.
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

/// Parse an ATX heading line and return `(level, heading_text)`.
/// Returns `None` if the line is not a valid ATX heading.
fn detect_heading(line: &str) -> Option<(u8, &str)> {
    if !line.starts_with('#') {
        return None;
    }
    let level = line.bytes().take_while(|&b| b == b'#').count();
    if level > 6 {
        return None;
    }
    let rest = &line[level..];
    if rest.is_empty() {
        Some((level as u8, ""))
    } else if rest.starts_with(' ') || rest.starts_with('\t') {
        Some((level as u8, rest[1..].trim_end_matches('#').trim()))
    } else {
        None
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
            visitor.on_body_line(line, i + 1);
        }
        visitor.into_matches()
    }

    fn run_regex_visitor(content: &str, pattern: &str) -> Vec<ContentMatch> {
        let mut visitor = ContentSearchVisitor::regex(pattern).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            visitor.on_body_line(line, i + 1);
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
        visitor.on_body_line("say hello", 1);
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
        visitor.on_body_line("say hello", 1);
        visitor.on_body_line("hello again", 2);
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
    fn regex_preserves_user_flags() {
        // Pattern already starts with (? — we don't double-prefix
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
    fn regex_no_match_returns_empty() {
        let matches = run_regex_visitor("Nothing here\n", r"\d{4}-\d{2}-\d{2}");
        assert!(matches.is_empty());
    }
}
