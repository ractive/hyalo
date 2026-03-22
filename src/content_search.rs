use crate::scanner::{FileVisitor, ScanAction};
use crate::types::ContentMatch;

/// Visitor that performs case-insensitive substring search on body lines.
/// Tracks current section heading for context in matches.
pub struct ContentSearchVisitor {
    /// Lowercase search pattern
    pattern: String,
    /// Current section heading (e.g. "## Design")
    current_section: String,
    /// Collected matches
    matches: Vec<ContentMatch>,
}

impl ContentSearchVisitor {
    /// Create a new visitor that searches for `pattern` (case-insensitive).
    #[must_use]
    pub fn new(pattern: &str) -> Self {
        Self {
            pattern: pattern.to_lowercase(),
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
}

impl FileVisitor for ContentSearchVisitor {
    fn on_body_line(&mut self, raw: &str, line_num: usize) -> ScanAction {
        // Check for ATX heading and update section context.
        if let Some((level, heading_text)) = detect_heading(raw) {
            self.current_section = format!("{} {}", "#".repeat(level as usize), heading_text);
        }

        // Check for pattern match (case-insensitive).
        if raw.to_lowercase().contains(&self.pattern) {
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
        // The heading both updates current_section and can itself match.
        let matches = run_visitor("## Design Goals\n", "design");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].text, "## Design Goals");
        // Section is updated before the match check, so the heading
        // reports itself as its own section.
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
        // #NoSpace is not a valid ATX heading
        let matches = run_visitor("#NoSpace\nbody\n", "body");
        // Section should remain empty because #NoSpace is not a heading
        assert_eq!(matches[0].section, "");
    }
}
