//! HYALO002 — `status: completed` requires all task checkboxes ticked.
//!
//! Only fires when the `.hyalo.toml` schema declares `status` with `completed`
//! in its enum values. Otherwise the rule is a no-op.

use comrak::nodes::AstNode;
use mdbook_lint_core::{
    Document, Violation,
    rule::{Rule, RuleCategory, RuleMetadata},
    violation::Severity,
};

use crate::rules::code_fence::{CodeFence, fence_open, is_fence_close};

/// HYALO002: `status: completed` → all tasks must be checked.
pub struct Hyalo002 {
    /// Whether the schema declares `status` with `completed` in its enum.
    /// If `false`, the rule is a no-op.
    schema_has_completed: bool,
    /// Frontmatter `status` value for the current file.
    frontmatter_status: Option<String>,
}

impl Hyalo002 {
    pub fn new(schema_has_completed: bool, frontmatter_status: Option<String>) -> Self {
        Self {
            schema_has_completed,
            frontmatter_status,
        }
    }
}

impl Rule for Hyalo002 {
    fn id(&self) -> &'static str {
        "HYALO002"
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
        // A literal `- [ ]` inside a fenced code block documents task syntax
        // and is not itself an open task, so code-block contents are skipped
        // (BUG-5 — the same fenced-code blindness fixed in HYALO001).
        let mut open_tasks: Vec<usize> = Vec::new();
        let mut fence: Option<CodeFence> = None;
        for (idx, line) in document.lines.iter().enumerate() {
            if let Some(open) = &fence {
                if is_fence_close(line, open) {
                    fence = None;
                }
                continue;
            }
            if let Some(open) = fence_open(line) {
                fence = Some(open);
                continue;
            }
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
        let verb = if count == 1 { "remains" } else { "remain" };
        let plural = if count == 1 { "" } else { "s" };
        Ok(vec![self.create_violation(
            format!(
                "status is `completed` but {count} task{plural} {verb} unchecked (first at line {first_line})"
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
        let rule = Hyalo002::new(schema_has_completed, status.map(str::to_owned));
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
        assert_eq!(violations[0].rule_id, "HYALO002");
        assert!(violations[0].message.contains("1 task"));
    }

    #[test]
    fn singular_grammar_uses_remains() {
        let violations = check("- [x] Done\n- [ ] One open\n", true, Some("completed"));
        assert_eq!(violations.len(), 1);
        let msg = &violations[0].message;
        assert!(
            msg.contains("1 task remains unchecked"),
            "expected singular 'remains' in: {msg}"
        );
    }

    #[test]
    fn plural_grammar_uses_remain() {
        let violations = check(
            "- [ ] First\n- [ ] Second\n- [ ] Third\n",
            true,
            Some("completed"),
        );
        assert_eq!(violations.len(), 1);
        let msg = &violations[0].message;
        assert!(
            msg.contains("3 tasks remain unchecked"),
            "expected plural 'remain' in: {msg}"
        );
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

    #[test]
    fn open_task_inside_fenced_code_is_ignored() {
        // A literal `- [ ]` inside a code fence documents syntax; a completed
        // doc must not be flagged just because its prose shows an example.
        let content = "All done.\n\n```markdown\n- [ ] example open task\n```\n";
        let violations = check(content, true, Some("completed"));
        assert!(
            violations.is_empty(),
            "fenced-code task example must not fire HYALO002, got {violations:?}"
        );
    }
}
