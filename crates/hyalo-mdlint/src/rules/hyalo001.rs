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

/// Returns `true` when `trimmed` (the line with leading whitespace removed)
/// looks like a bare or malformed checkbox that should be `- [ ]`.
///
/// Detects two families:
///
/// 1. **Prefixed-bullet bare brackets** — a list bullet (`-`, `*`, `+`)
///    followed by one or more spaces and then `[]` (no space inside).
///    Examples: `- [] task`, `* [] done`.
///    Already-correct forms `- [ ]`, `- [x]`, `- [X]` are excluded.
///
/// 2. **Bare prefix-less brackets** — `[]`, `[ ]`, `[x]`, `[X]` at the
///    start of the line, NOT followed by `:` or `(` (which would make them
///    markdown reference-link definitions or inline links).
fn is_bare_checkbox(trimmed: &str) -> bool {
    // Family 1: list bullet + space(s) + `[]`
    // Pattern: `[-*+] +\[\]`
    // Correct forms (`- [ ]`, `- [x]`, `- [X]`, `* [ ]`, etc.) must NOT fire.
    for bullet in ['-', '*', '+'] {
        let rest = match trimmed.strip_prefix(bullet) {
            Some(r) if !r.is_empty() && r.starts_with(' ') => r,
            _ => continue,
        };
        // rest starts with at least one space
        let after_spaces = rest.trim_start_matches(' ');
        // Match bare `[]` only — `[ ]` / `[x]` / `[X]` are proper forms.
        if after_spaces.starts_with("[]") {
            return true;
        }
    }

    // Family 2: no leading bullet — `[]`, `[ ]`, `[x]`, `[X]` at the line start.
    // Already a proper task-list item means it has a bullet prefix already checked above.
    // Candidate prefix length (the closing `]` position).
    let prefix_len = if trimmed.starts_with("[ ]") {
        3
    } else if trimmed.starts_with("[]") || trimmed.starts_with("[x]") || trimmed.starts_with("[X]")
    {
        if trimmed.starts_with("[]") { 2 } else { 3 }
    } else {
        return false;
    };

    // Reject markdown link forms: the next non-space character after `]` is
    // `:` (reference-link definition: `[x]: url`) or `(` (inline link: `[x](url)`).
    let after = trimmed[prefix_len..].trim_start_matches(' ');
    !after.starts_with(':') && !after.starts_with('(')
}

/// Build the replacement string for a bare or malformed checkbox.
///
/// For prefixed-bullet bare brackets (`- [] task`, `* [] done`): insert the
/// missing space so `- []` becomes `- [ ]`.
///
/// For prefix-less bare brackets (`[] task`, `[ ] task`, `[x] task`):
/// prepend `- ` so the line becomes a proper list item.
fn build_replacement(trimmed: &str) -> String {
    // Family 1: prefixed-bullet bare `[]`.
    // e.g. `- [] task` → `- [ ] task`
    for bullet in ['-', '*', '+'] {
        let rest = match trimmed.strip_prefix(bullet) {
            Some(r) if r.starts_with(' ') => r,
            _ => continue,
        };
        let after_spaces = rest.trim_start_matches(' ');
        if let Some(after_bracket) = after_spaces.strip_prefix("[]") {
            // Preserve any spaces between bullet and bracket.
            let spaces = &rest[..rest.len() - after_spaces.len()];
            return format!("{bullet}{spaces}[ ]{after_bracket}");
        }
    }

    // Family 2: prefix-less bare brackets.
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

    #[test]
    fn no_violation_for_reference_link_definition() {
        // `[ref]: url` is a markdown reference-link definition, not a checkbox.
        let violations = check("[x]: https://example.com\n[label]: ./other.md\n");
        assert!(
            violations.is_empty(),
            "reference-link definitions should not trigger HYALO001"
        );
    }

    #[test]
    fn no_violation_for_inline_link() {
        // `[x](url)` is an inline link.
        let violations = check("[x](https://example.com) inline link\n");
        assert!(
            violations.is_empty(),
            "inline links should not trigger HYALO001"
        );
    }

    // --- Prefixed-bullet bare bracket forms (BUG-5) ---

    #[test]
    fn detects_dash_bare_bracket() {
        let violations = check("- [] Task one\n");
        assert_eq!(violations.len(), 1, "- [] should fire HYALO001");
    }

    #[test]
    fn detects_star_bare_bracket() {
        let violations = check("* [] Task one\n");
        assert_eq!(violations.len(), 1, "* [] should fire HYALO001");
    }

    #[test]
    fn detects_plus_bare_bracket() {
        let violations = check("+ [] Task one\n");
        assert_eq!(violations.len(), 1, "+ [] should fire HYALO001");
    }

    #[test]
    fn no_violation_for_dash_proper_task() {
        // These are already correct; none should fire.
        let violations = check("- [ ] Task\n- [x] Done\n- [X] Also done\n");
        assert!(
            violations.is_empty(),
            "proper task-list forms must not trigger HYALO001"
        );
    }

    #[test]
    fn no_violation_for_star_proper_task() {
        let violations = check("* [ ] Task\n* [x] Done\n");
        assert!(
            violations.is_empty(),
            "* [ ] forms must not trigger HYALO001"
        );
    }

    #[test]
    fn autofix_dash_bare_bracket() {
        // `- [] task` → `- [ ] task`
        let violations = check("- [] Task one\n");
        assert_eq!(violations.len(), 1);
        let fix = violations[0].fix.as_ref().expect("fix should be present");
        assert_eq!(
            fix.replacement.as_deref(),
            Some("- [ ] Task one"),
            "fix should insert space inside brackets"
        );
    }

    #[test]
    fn autofix_star_bare_bracket() {
        let violations = check("* [] Task two\n");
        assert_eq!(violations.len(), 1);
        let fix = violations[0].fix.as_ref().unwrap();
        assert_eq!(fix.replacement.as_deref(), Some("* [ ] Task two"));
    }

    #[test]
    fn autofix_dash_bare_bracket_idempotent() {
        let content = "- [] Task one\n";
        let violations = check(content);
        let fix = violations[0].fix.as_ref().unwrap();
        let fixed = fix.replacement.as_deref().unwrap_or("");
        let v2 = check(fixed);
        assert!(v2.is_empty(), "- [] fix should be idempotent");
    }

    #[test]
    fn indented_dash_bare_bracket() {
        // Indented lists should also be caught.
        let violations = check("  - [] indented task\n");
        assert_eq!(violations.len(), 1, "indented - [] should fire HYALO001");
        let fix = violations[0].fix.as_ref().unwrap();
        // The replacement covers the trimmed portion: `- [] indented task` → `- [ ] indented task`
        assert_eq!(
            fix.replacement.as_deref(),
            Some("- [ ] indented task"),
            "fix should insert space inside brackets"
        );
    }
}
