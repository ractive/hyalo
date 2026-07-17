//! Agent Skills conformance lint rules (`hyalo lint --profile skills`).
//!
//! The conformance half of the `skills` profile. The Agent Skills hard rules
//! that the schema layer *can* express (`name` regex/length, `description`
//! length bounds) are covered by the schema pass once the `skills` fragment is
//! overlaid and the `skill` type is bound to `**/SKILL.md`. What lives here are
//! the checks the schema layer cannot express — one hard (**error**) and two
//! advisory (**warn**):
//!
//! - `SKILL-RESERVED-NAME` — the `name` must not be a reserved word
//!   (`anthropic` / `claude`). This is a hard spec violation, so the finding is
//!   *error*-severity (the slug pattern in the schema cannot express it without
//!   look-around, which Rust's `regex` crate lacks).
//! - `SKILL-NAME-DIRNAME` — the `name` frontmatter value should equal the parent
//!   directory name (the spec identifies a skill by its `<name>/SKILL.md` dir).
//! - `SKILL-LINE-BUDGET` — the SKILL.md body should stay under 500 lines (the
//!   spec recommends keeping instructions concise; longer bodies belong in
//!   `references/`).
//!
//! Rules dispatch only on files whose *effective* type is `skill` (via the path
//! binding or explicit frontmatter). The dirname check reads only the file's own
//! path components — no directory walk, no network.

/// The recommended maximum number of body lines for a SKILL.md file. Above this
/// the `SKILL-LINE-BUDGET` rule warns.
pub(crate) const SKILL_LINE_BUDGET: usize = 500;

/// Reserved skill names the spec forbids (a skill must not be named after the
/// vendor). Compared case-insensitively.
pub(crate) const RESERVED_NAMES: &[&str] = &["anthropic", "claude"];

/// A single Agent-Skills advisory finding, in the shape the lint pipeline
/// converts into an `InternalViolation`.
pub(crate) struct SkillFinding {
    pub(crate) rule_id: &'static str,
    /// Severity to use when `[lint.rules.<id>]` does not override it. Most
    /// skills rules are advisory (`"warn"`); `SKILL-RESERVED-NAME` is a hard
    /// spec violation (`"error"`).
    pub(crate) default_severity: &'static str,
    /// 1-based line within the file, for display.
    pub(crate) line: usize,
    pub(crate) message: String,
}

/// Rule IDs exposed by the skills profile. Kept in one place so the catalog
/// (`lint-rules list`) and the runtime stay in lock-step; the parity test in
/// this module asserts every emitted id is listed here.
#[cfg(test)]
pub(crate) const SKILL_RULE_IDS: &[&str] = &[
    "SKILL-RESERVED-NAME",
    "SKILL-NAME-DIRNAME",
    "SKILL-LINE-BUDGET",
];

/// Run every enabled Agent-Skills rule against one SKILL.md file.
///
/// * `rel_path` — vault-relative path (used to derive the parent directory).
/// * `effective_type` — the resolved type (explicit frontmatter or path
///   binding). Rules run only when this is `skill`.
/// * `name` — the frontmatter `name` value, if any.
/// * `body_line_count` — the number of lines in the markdown body (frontmatter
///   excluded), as counted by the caller.
/// * `is_enabled` — predicate deciding whether a given rule id runs (honors
///   `[lint.rules]` overrides and `--rule`/`--rule-prefix` filters).
pub(crate) fn run_skill_rules(
    rel_path: &str,
    effective_type: Option<&str>,
    name: Option<&str>,
    body_line_count: usize,
    is_enabled: &dyn Fn(&str) -> bool,
) -> Vec<SkillFinding> {
    let mut out = Vec::new();

    // Only skill files participate.
    if !matches!(effective_type, Some(t) if t.eq_ignore_ascii_case("skill")) {
        return out;
    }

    if is_enabled("SKILL-RESERVED-NAME")
        && let Some(name) = name
        && RESERVED_NAMES.iter().any(|r| name.eq_ignore_ascii_case(r))
    {
        out.push(SkillFinding {
            rule_id: "SKILL-RESERVED-NAME",
            default_severity: "error",
            line: 1,
            message: format!(
                "`name: {name}` is a reserved word ({}); choose a different skill name",
                RESERVED_NAMES.join(", ")
            ),
        });
    }

    if is_enabled("SKILL-NAME-DIRNAME")
        && let Some(name) = name
        && let Some(dir) = parent_dir_name(rel_path)
        && name != dir
    {
        out.push(SkillFinding {
            rule_id: "SKILL-NAME-DIRNAME",
            default_severity: "warn",
            line: 1,
            message: format!(
                "`name: {name}` does not match the parent directory `{dir}`; a skill is identified by its `<name>/SKILL.md` directory"
            ),
        });
    }

    if is_enabled("SKILL-LINE-BUDGET") && body_line_count > SKILL_LINE_BUDGET {
        out.push(SkillFinding {
            rule_id: "SKILL-LINE-BUDGET",
            default_severity: "warn",
            line: 1,
            message: format!(
                "SKILL.md body is {body_line_count} lines; keep it under {SKILL_LINE_BUDGET} (move detail into `references/`)"
            ),
        });
    }

    out
}

/// The name of the directory immediately containing `rel_path`'s file, e.g.
/// `.claude/skills/my-skill/SKILL.md` → `my-skill`. Returns `None` when the
/// file has no parent directory component (e.g. a bare `SKILL.md` at the vault
/// root). Normalises Windows-style backslash separators first so a rel_path
/// carrying `\\` still resolves the parent correctly.
fn parent_dir_name(rel_path: &str) -> Option<&str> {
    // `rel_path` is already forward-slash-normalised by the scanner, but be
    // defensive: split on both separators so a stray backslash cannot hide the
    // parent directory on Windows.
    let trimmed = rel_path.trim_end_matches(['/', '\\']);
    let parent = &trimmed[..trimmed.rfind(['/', '\\'])?];
    let start = parent.rfind(['/', '\\']).map_or(0, |i| i + 1);
    let name = &parent[start..];
    if name.is_empty() { None } else { Some(name) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn always_enabled(_: &str) -> bool {
        true
    }

    #[test]
    fn parent_dir_extraction() {
        assert_eq!(
            parent_dir_name(".claude/skills/my-skill/SKILL.md"),
            Some("my-skill")
        );
        assert_eq!(parent_dir_name("my-skill/SKILL.md"), Some("my-skill"));
        assert_eq!(parent_dir_name("SKILL.md"), None, "no parent dir");
        assert_eq!(
            parent_dir_name(".claude\\skills\\win-skill\\SKILL.md"),
            Some("win-skill"),
            "backslash separators resolve"
        );
    }

    #[test]
    fn non_skill_type_skips_all_rules() {
        let out = run_skill_rules(
            "notes/n.md",
            Some("note"),
            Some("mismatch"),
            10_000,
            &always_enabled,
        );
        assert!(out.is_empty(), "non-skill files must not trigger rules");
    }

    #[test]
    fn reserved_name_errors() {
        // `claude` matches its own dir, so only the reserved-name rule fires.
        let out = run_skill_rules(
            ".claude/skills/claude/SKILL.md",
            Some("skill"),
            Some("claude"),
            10,
            &always_enabled,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "SKILL-RESERVED-NAME");
        assert_eq!(out[0].default_severity, "error");
    }

    #[test]
    fn reserved_name_is_case_insensitive() {
        let out = run_skill_rules(
            ".claude/skills/Anthropic/SKILL.md",
            Some("skill"),
            Some("Anthropic"),
            10,
            &always_enabled,
        );
        assert!(
            out.iter().any(|f| f.rule_id == "SKILL-RESERVED-NAME"),
            "mixed-case reserved word must still be caught"
        );
    }

    #[test]
    fn dirname_mismatch_warns() {
        let out = run_skill_rules(
            ".claude/skills/my-skill/SKILL.md",
            Some("skill"),
            Some("other-name"),
            10,
            &always_enabled,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "SKILL-NAME-DIRNAME");
        assert!(out[0].message.contains("my-skill"));
    }

    #[test]
    fn dirname_match_is_clean() {
        let out = run_skill_rules(
            ".claude/skills/my-skill/SKILL.md",
            Some("skill"),
            Some("my-skill"),
            10,
            &always_enabled,
        );
        assert!(out.is_empty(), "matching name/dir must not warn: {:?}", {
            out.iter().map(|f| &f.message).collect::<Vec<_>>()
        });
    }

    #[test]
    fn line_budget_warns_above_500() {
        let out = run_skill_rules(
            ".claude/skills/big/SKILL.md",
            Some("skill"),
            Some("big"),
            SKILL_LINE_BUDGET + 1,
            &always_enabled,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "SKILL-LINE-BUDGET");
    }

    #[test]
    fn line_budget_at_limit_is_clean() {
        let out = run_skill_rules(
            ".claude/skills/ok/SKILL.md",
            Some("skill"),
            Some("ok"),
            SKILL_LINE_BUDGET,
            &always_enabled,
        );
        assert!(out.is_empty(), "exactly at the budget must not warn");
    }

    #[test]
    fn missing_name_skips_dirname_rule() {
        // A missing `name` is the schema's concern (required); the advisory
        // dirname coupling has nothing to compare, so it must stay silent.
        let out = run_skill_rules(
            ".claude/skills/my-skill/SKILL.md",
            Some("skill"),
            None,
            10,
            &always_enabled,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn every_emitted_rule_id_is_registered() {
        let out = run_skill_rules(
            ".claude/skills/my-skill/SKILL.md",
            Some("skill"),
            Some("other-name"),
            SKILL_LINE_BUDGET + 42,
            &always_enabled,
        );
        for f in &out {
            assert!(
                SKILL_RULE_IDS.contains(&f.rule_id),
                "unregistered rule id emitted: {}",
                f.rule_id
            );
        }
        // Both rules should have fired in this scenario.
        assert_eq!(out.len(), 2);
    }
}
