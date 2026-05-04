//! HYALO002 — frontmatter `title` ↔ first H1 agreement.
//!
//! Mode controls:
//! - `either` (default) — no violation if either title or H1 is absent;
//!   violation if both present and differ.
//! - `match`  — violation if both present and differ (same as `either` in practice).
//! - `off`    — rule disabled.

use comrak::nodes::{AstNode, NodeValue};
use mdbook_lint_core::{
    Document, Violation,
    rule::{AstRule, RuleCategory, RuleMetadata},
    violation::Severity,
};

/// Mode for HYALO002.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleMode {
    Either,
    Match,
    Off,
}

impl TitleMode {
    /// Parse from a config string. Unknown values fall back to `Either`.
    pub fn from_config_str(s: &str) -> Self {
        match s {
            "match" => Self::Match,
            "off" => Self::Off,
            _ => Self::Either,
        }
    }
}

/// HYALO002: frontmatter `title` ↔ first H1 agreement.
pub struct Hyalo002 {
    pub mode: TitleMode,
    /// Frontmatter title to compare against (extracted before linting).
    pub frontmatter_title: Option<String>,
}

impl Hyalo002 {
    pub fn new(mode: TitleMode, frontmatter_title: Option<String>) -> Self {
        Self {
            mode,
            frontmatter_title,
        }
    }
}

impl AstRule for Hyalo002 {
    fn id(&self) -> &'static str {
        "HYALO002"
    }

    fn name(&self) -> &'static str {
        "title-h1-agreement"
    }

    fn description(&self) -> &'static str {
        "Frontmatter `title` and first H1 heading should agree (when both present)"
    }

    fn metadata(&self) -> RuleMetadata {
        RuleMetadata::stable(RuleCategory::Structure)
    }

    fn check_ast<'a>(
        &self,
        document: &Document,
        ast: &'a AstNode<'a>,
    ) -> mdbook_lint_core::error::Result<Vec<Violation>> {
        if self.mode == TitleMode::Off {
            return Ok(vec![]);
        }

        let fm_title = match &self.frontmatter_title {
            Some(t) if !t.is_empty() => t.as_str(),
            _ => return Ok(vec![]), // no frontmatter title — either mode = no violation
        };

        // Find the first H1 heading in the AST.
        let Some(h1_text) = find_first_h1(document, ast) else {
            return Ok(vec![]); // no H1 — either mode = no violation
        };

        if fm_title == h1_text {
            return Ok(vec![]);
        }

        Ok(vec![self.create_violation(
            format!("frontmatter `title` ({fm_title:?}) does not match first H1 ({h1_text:?})"),
            1,
            1,
            Severity::Warning,
        )])
    }
}

/// Extract the text content of the first H1 heading node.
fn find_first_h1<'a>(document: &Document, ast: &'a AstNode<'a>) -> Option<String> {
    for node in ast.descendants() {
        if let NodeValue::Heading(heading) = &node.data.borrow().value
            && heading.level == 1
        {
            return Some(document.node_text(node).trim().to_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdbook_lint_core::Rule;
    use std::path::PathBuf;

    fn check_with(content: &str, title: Option<&str>, mode: TitleMode) -> Vec<Violation> {
        let doc = Document::new(content.to_string(), PathBuf::from("test.md")).unwrap();
        let rule = Hyalo002::new(mode, title.map(str::to_owned));
        rule.check(&doc).unwrap()
    }

    #[test]
    fn no_violation_when_titles_match() {
        let violations = check_with("# My Title\n\nBody\n", Some("My Title"), TitleMode::Either);
        assert!(violations.is_empty());
    }

    #[test]
    fn violation_when_titles_differ() {
        let violations = check_with(
            "# H1 Title\n\nBody\n",
            Some("Front Title"),
            TitleMode::Either,
        );
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule_id, "HYALO002");
    }

    #[test]
    fn no_violation_when_no_frontmatter_title() {
        // No frontmatter title → either mode = no violation
        let violations = check_with("# H1 Title\n\nBody\n", None, TitleMode::Either);
        assert!(violations.is_empty());
    }

    #[test]
    fn no_violation_when_no_h1() {
        let violations = check_with(
            "## Section\n\nBody\n",
            Some("Front Title"),
            TitleMode::Either,
        );
        assert!(violations.is_empty());
    }

    #[test]
    fn off_mode_no_violation() {
        let violations = check_with("# H1 Title\n\nBody\n", Some("Different"), TitleMode::Off);
        assert!(violations.is_empty());
    }

    #[test]
    fn match_mode_fires_when_different() {
        let violations = check_with("# H1 Title\n\nBody\n", Some("Other"), TitleMode::Match);
        assert_eq!(violations.len(), 1);
    }
}
