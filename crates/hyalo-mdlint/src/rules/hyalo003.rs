//! HYALO003 — `status: completed` requires all task checkboxes ticked.
//!
//! Only fires when the `.hyalo.toml` schema declares `status` with `completed`
//! in its enum values. Otherwise the rule is a no-op.

use comrak::nodes::AstNode;
use mdbook_lint_core::{
    Document, Violation,
    rule::{Rule, RuleCategory, RuleMetadata},
    violation::Severity,
};

/// HYALO003: `status: completed` → all tasks must be checked.
pub struct Hyalo003 {
    /// Whether the schema declares `status` with `completed` in its enum.
    /// If `false`, the rule is a no-op.
    schema_has_completed: bool,
    /// Frontmatter `status` value for the current file.
    frontmatter_status: Option<String>,
}

impl Hyalo003 {
    pub fn new(schema_has_completed: bool, frontmatter_status: Option<String>) -> Self {
        Self {
            schema_has_completed,
            frontmatter_status,
        }
    }
}

impl Rule for Hyalo003 {
    fn id(&self) -> &'static str {
        "HYALO003"
    }

    fn name(&self) -> &'static str {
        "completed-tasks"
    }

    fn description(&self) -> &'static str {
        "`status: completed` requires all task checkboxes to be ticked"
    }

    fn metadata(&self) -> RuleMetadata {
        RuleMetadata::stable(RuleCategory::Content)
    }

    fn check_with_ast<'a>(
        &self,
        document: &Document,
        _ast: Option<&'a AstNode<'a>>,
    ) -> mdbook_lint_core::error::Result<Vec<Violation>> {
        // Only active when schema has `completed` in the `status` enum.
        if !self.schema_has_completed {
            return Ok(vec![]);
        }

        // Only fires for `status: completed` files.
        let status = match &self.frontmatter_status {
            Some(s) => s.as_str(),
            None => return Ok(vec![]),
        };
        if status != "completed" {
            return Ok(vec![]);
        }

        // Scan for unchecked task items: lines matching `- [ ]` or `* [ ]`.
        let mut open_tasks: Vec<usize> = Vec::new();
        for (idx, line) in document.lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if is_open_task(trimmed) {
                open_tasks.push(idx + 1); // 1-based line number
            }
        }

        if open_tasks.is_empty() {
            return Ok(vec![]);
        }

        let count = open_tasks.len();
        let first_line = open_tasks[0];
        Ok(vec![self.create_violation(
            format!(
                "status is `completed` but {count} task{} remain unchecked (first at line {first_line})",
                if count == 1 { "" } else { "s" }
            ),
            first_line,
            1,
            Severity::Error,
        )])
    }
}

/// Returns true if the line (after left-trimming) is an open task item.
fn is_open_task(trimmed: &str) -> bool {
    trimmed.starts_with("- [ ]") || trimmed.starts_with("* [ ]") || trimmed.starts_with("+ [ ]")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn check(content: &str, schema_has_completed: bool, status: Option<&str>) -> Vec<Violation> {
        let doc = Document::new(content.to_string(), PathBuf::from("test.md")).unwrap();
        let rule = Hyalo003::new(schema_has_completed, status.map(str::to_owned));
        rule.check(&doc).unwrap()
    }

    #[test]
    fn noop_when_schema_lacks_completed() {
        let violations = check(
            "- [ ] Open task\n",
            false, // schema_has_completed = false
            Some("completed"),
        );
        assert!(violations.is_empty());
    }

    #[test]
    fn noop_when_status_not_completed() {
        let violations = check("- [ ] Open task\n", true, Some("in-progress"));
        assert!(violations.is_empty());
    }

    #[test]
    fn noop_when_no_status() {
        let violations = check("- [ ] Open task\n", true, None);
        assert!(violations.is_empty());
    }

    #[test]
    fn fires_when_completed_and_open_tasks() {
        let violations = check("- [x] Done\n- [ ] Still open\n", true, Some("completed"));
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule_id, "HYALO003");
        assert!(violations[0].message.contains("1 task"));
    }

    #[test]
    fn no_violation_when_all_tasks_checked() {
        let violations = check("- [x] Done\n- [x] Also done\n", true, Some("completed"));
        assert!(violations.is_empty());
    }

    #[test]
    fn no_violation_when_no_tasks() {
        let violations = check("# Title\n\nSome body text.\n", true, Some("completed"));
        assert!(violations.is_empty());
    }
}
