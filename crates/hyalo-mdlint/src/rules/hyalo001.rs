//! HYALO001 — bare `[]` should be `- [ ]` (autofix, line-based).
//!
//! Detects lines that contain `[]` as a bare checkbox marker (not inside a
//! proper list item `- [ ]`) and rewrites them.
//!
//! This is purely line-based; no AST parsing is needed.

use comrak::nodes::AstNode;
use mdbook_lint_core::{
    Document, Violation,
    rule::{Rule, RuleCategory, RuleMetadata},
    violation::{Fix, Position, Severity},
};

/// HYALO001: bare `[]` should be `- [ ]`.
pub struct Hyalo001;

impl Rule for Hyalo001 {
    fn id(&self) -> &'static str {
        "HYALO001"
    }

    fn name(&self) -> &'static str {
        "bare-checkbox"
    }

    fn description(&self) -> &'static str {
        "Bare `[]` should be written as `- [ ]` (a proper task-list item)"
    }

    fn metadata(&self) -> RuleMetadata {
        RuleMetadata::stable(RuleCategory::Formatting)
    }

    fn check_with_ast<'a>(
        &self,
        document: &Document,
        _ast: Option<&'a AstNode<'a>>,
    ) -> mdbook_lint_core::error::Result<Vec<Violation>> {
        let mut violations = Vec::new();
        for (line_idx, line) in document.lines.iter().enumerate() {
            let line_no = line_idx + 1;
            // Find occurrences of bare `[]` or `[x]` / `[X]` that are NOT already
            // part of a proper task-list format `- [ ]` / `- [x]`.
            // We look for occurrences of `[]` that appear at the start of the line
            // (possibly with leading whitespace) and are NOT preceded by `- `.
            let trimmed = line.trim_start();
            if !is_bare_checkbox(trimmed) {
                continue;
            }

            let col = line.len() - trimmed.len() + 1;
            let replacement = build_replacement(trimmed);
            let end_col = col + trimmed.len();

            let fix = Fix {
                description: "Replace bare `[]` with `- [ ]`".to_owned(),
                replacement: Some(replacement),
                start: Position {
                    line: line_no,
                    column: col,
                },
                end: Position {
                    line: line_no,
                    column: end_col,
                },
            };

            violations.push(self.create_violation_with_fix(
                format!("bare checkbox `[]` on line {line_no} — should be `- [ ]`"),
                line_no,
                col,
                Severity::Error,
                fix,
            ));
        }
        Ok(violations)
    }

    fn can_fix(&self) -> bool {
        true
    }
}

/// Returns `true` when `trimmed` starts with a bare `[]` or `[ ]` that is
/// NOT already a proper task-list marker (e.g. `- [ ]` or `* [ ]`).
fn is_bare_checkbox(trimmed: &str) -> bool {
    // Already a proper task-list item: starts with `- [ ]`, `- [x]`, `* [ ]`, etc.
    if trimmed.starts_with("- [ ]")
        || trimmed.starts_with("- [x]")
        || trimmed.starts_with("- [X]")
        || trimmed.starts_with("* [ ]")
        || trimmed.starts_with("* [x]")
        || trimmed.starts_with("* [X]")
        || trimmed.starts_with("+ [ ]")
        || trimmed.starts_with("+ [x]")
        || trimmed.starts_with("+ [X]")
    {
        return false;
    }

    // Bare `[]` at start of trimmed line.
    trimmed.starts_with("[]")
        || trimmed.starts_with("[ ]")
        || trimmed.starts_with("[x]")
        || trimmed.starts_with("[X]")
}

/// Build the replacement string: strip the bare checkbox prefix and prepend `- [ ]`.
fn build_replacement(trimmed: &str) -> String {
    // Determine what comes after the bare `[]` / `[ ]` / `[x]` / `[X]`
    let (prefix_len, checked) = if trimmed.starts_with("[ ]") || trimmed.starts_with("[]") {
        let len = if trimmed.starts_with("[ ]") { 3 } else { 2 };
        (len, false)
    } else {
        // [x] or [X]
        (3, true)
    };

    let rest = &trimmed[prefix_len..];
    let task_marker = if checked { "- [x]" } else { "- [ ]" };
    if rest.is_empty() {
        task_marker.to_owned()
    } else {
        format!("{task_marker}{rest}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn check(content: &str) -> Vec<Violation> {
        let doc = Document::new(content.to_string(), PathBuf::from("test.md")).unwrap();
        Hyalo001.check(&doc).unwrap()
    }

    #[test]
    fn detects_bare_bracket() {
        let violations = check("# Title\n\n[] Task one\n");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule_id, "HYALO001");
    }

    #[test]
    fn detects_bare_space_bracket() {
        let violations = check("[ ] Task one\n");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn no_violation_for_proper_task() {
        let violations = check("- [ ] Task one\n- [x] Done\n");
        assert!(violations.is_empty());
    }

    #[test]
    fn autofix_replaces_bare_bracket() {
        let violations = check("[] Task one\n");
        assert_eq!(violations.len(), 1);
        let fix = violations[0].fix.as_ref().expect("fix should be present");
        assert_eq!(fix.replacement.as_deref(), Some("- [ ] Task one"));
    }

    #[test]
    fn autofix_idempotent() {
        // Applying the fix should produce text that no longer triggers the rule.
        let content = "[] Task one\n";
        let violations = check(content);
        assert_eq!(violations.len(), 1);
        let fix = violations[0].fix.as_ref().unwrap();
        let fixed = fix.replacement.as_deref().unwrap_or("");
        let v2 = check(fixed);
        assert!(v2.is_empty(), "fix should be idempotent");
    }

    #[test]
    fn multiple_bare_checkboxes() {
        let content = "[] Task A\n- [ ] OK\n[] Task B\n";
        let violations = check(content);
        assert_eq!(violations.len(), 2, "should fire for Task A and Task B");
    }
}
