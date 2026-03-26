#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use crate::output::CommandOutcome;

// ---------------------------------------------------------------------------
// Embedded skill content
// ---------------------------------------------------------------------------

const SKILL_CONTENT: &str = include_str!("../../../../.claude/skills/hyalo/SKILL.md");
const DREAM_SKILL_CONTENT: &str = include_str!("../../../../.claude/skills/hyalo-dream/SKILL.md");
const RULE_TEMPLATE: &str = include_str!("../../../../.claude/rules/knowledgebase.md");

const CLAUDE_MD_HINT: &str = "Use `hyalo` CLI (not Read/Grep/Glob) for all markdown knowledgebase operations (frontmatter, tags, tasks, search). Run `hyalo --help` for usage. Use `--format text` for compact LLM-friendly output.";

/// Common documentation directory names to scan for when no `--dir` is given.
const CANDIDATE_DIRS: &[&str] = &["docs", "knowledgebase", "wiki", "notes", "content", "pages"];

// ---------------------------------------------------------------------------
// Public command entry point
// ---------------------------------------------------------------------------

/// Initialize hyalo configuration and optional Claude Code integration.
///
/// - `dir`: explicit value for the `dir` key in `.hyalo.toml`; when `None` the
///   function auto-detects a common doc directory.
/// - `claude`: when `true`, also installs the hyalo and hyalo-dream skills and
///   appends a hint line to `.claude/CLAUDE.md`.
pub fn run_init(dir: Option<&str>, claude: bool) -> Result<CommandOutcome> {
    let cwd = std::env::current_dir().context("failed to determine current working directory")?;
    run_init_in(dir, claude, &cwd)
}

fn run_init_in(dir: Option<&str>, claude: bool, cwd: &Path) -> Result<CommandOutcome> {
    let mut summary = String::new();

    // Resolve the directory value once, so we can use it both for .hyalo.toml
    // and for the rules file path substitution.
    let dir_explicit = dir.is_some();
    let dir_value = match dir {
        Some(d) => d.to_owned(),
        None => auto_detect_dir(cwd),
    };

    // ------------------------------------------------------------------
    // Step 1: create or update .hyalo.toml
    // ------------------------------------------------------------------
    let toml_path = cwd.join(".hyalo.toml");
    let toml_existed = toml_path.exists();
    if toml_existed && !dir_explicit {
        writeln!(summary, "skipped  .hyalo.toml (already exists)").unwrap();
    } else {
        let escaped = dir_value.replace('\\', "\\\\").replace('"', "\\\"");
        let toml_content = format!("dir = \"{escaped}\"\n");
        fs::write(&toml_path, &toml_content)
            .with_context(|| format!("failed to write {}", toml_path.display()))?;
        if toml_existed {
            writeln!(summary, "updated  .hyalo.toml  (dir = \"{dir_value}\")").unwrap();
        } else {
            writeln!(summary, "created  .hyalo.toml  (dir = \"{dir_value}\")").unwrap();
        }
    }

    if !claude {
        return Ok(CommandOutcome::Success(summary.trim_end().to_owned()));
    }

    // ------------------------------------------------------------------
    // Step 2: write (overwrite) .claude/skills/hyalo/SKILL.md
    // ------------------------------------------------------------------
    let skill_path = cwd
        .join(".claude")
        .join("skills")
        .join("hyalo")
        .join("SKILL.md");
    let skill_existed = skill_path.exists();
    let skill_dir = skill_path
        .parent()
        .context("skill path has no parent directory")?;
    fs::create_dir_all(skill_dir)
        .with_context(|| format!("failed to create directory {}", skill_dir.display()))?;
    fs::write(&skill_path, SKILL_CONTENT)
        .with_context(|| format!("failed to write {}", skill_path.display()))?;
    if skill_existed {
        writeln!(summary, "updated  .claude/skills/hyalo/SKILL.md").unwrap();
    } else {
        writeln!(summary, "created  .claude/skills/hyalo/SKILL.md").unwrap();
    }

    // ------------------------------------------------------------------
    // Step 3: write (overwrite) .claude/skills/hyalo-dream/SKILL.md
    // ------------------------------------------------------------------
    let dream_skill_path = cwd
        .join(".claude")
        .join("skills")
        .join("hyalo-dream")
        .join("SKILL.md");
    let dream_skill_existed = dream_skill_path.exists();
    let dream_skill_dir = dream_skill_path
        .parent()
        .context("dream skill path has no parent directory")?;
    fs::create_dir_all(dream_skill_dir)
        .with_context(|| format!("failed to create directory {}", dream_skill_dir.display()))?;
    fs::write(&dream_skill_path, DREAM_SKILL_CONTENT)
        .with_context(|| format!("failed to write {}", dream_skill_path.display()))?;
    if dream_skill_existed {
        writeln!(summary, "updated  .claude/skills/hyalo-dream/SKILL.md").unwrap();
    } else {
        writeln!(summary, "created  .claude/skills/hyalo-dream/SKILL.md").unwrap();
    }

    // ------------------------------------------------------------------
    // Step 4: write (overwrite) .claude/rules/knowledgebase.md
    // ------------------------------------------------------------------
    let rules_path = cwd.join(".claude").join("rules").join("knowledgebase.md");
    let rules_existed = rules_path.exists();
    let rules_dir = rules_path
        .parent()
        .context("rules path has no parent directory")?;
    fs::create_dir_all(rules_dir)
        .with_context(|| format!("failed to create directory {}", rules_dir.display()))?;
    let rule_content = parameterize_rule(RULE_TEMPLATE, &dir_value);
    fs::write(&rules_path, &rule_content)
        .with_context(|| format!("failed to write {}", rules_path.display()))?;
    if rules_existed {
        writeln!(summary, "updated  .claude/rules/knowledgebase.md").unwrap();
    } else {
        writeln!(summary, "created  .claude/rules/knowledgebase.md").unwrap();
    }

    // ------------------------------------------------------------------
    // Step 5: append hyalo hint to .claude/CLAUDE.md (no duplicates)
    // ------------------------------------------------------------------
    let claude_md_path = cwd.join(".claude").join("CLAUDE.md");
    if claude_md_path.exists() {
        let existing = fs::read_to_string(&claude_md_path)
            .with_context(|| format!("failed to read {}", claude_md_path.display()))?;
        if existing.contains(CLAUDE_MD_HINT) {
            writeln!(summary, "skipped  .claude/CLAUDE.md (hint already present)").unwrap();
        } else {
            let appended = append_line_to_file_content(&existing, CLAUDE_MD_HINT);
            fs::write(&claude_md_path, appended)
                .with_context(|| format!("failed to write {}", claude_md_path.display()))?;
            writeln!(summary, "updated  .claude/CLAUDE.md (appended hyalo hint)").unwrap();
        }
    } else {
        // Create the file and any parent directories.
        let claude_dir = claude_md_path
            .parent()
            .context("CLAUDE.md path has no parent directory")?;
        fs::create_dir_all(claude_dir)
            .with_context(|| format!("failed to create directory {}", claude_dir.display()))?;
        let content = format!("{CLAUDE_MD_HINT}\n");
        fs::write(&claude_md_path, content)
            .with_context(|| format!("failed to write {}", claude_md_path.display()))?;
        writeln!(summary, "created  .claude/CLAUDE.md (with hyalo hint)").unwrap();
    }

    Ok(CommandOutcome::Success(summary.trim_end().to_owned()))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Recursively count `.md` files under `dir`.
fn count_md_files_recursive(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let mut count = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            count += count_md_files_recursive(&path);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        {
            count += 1;
        }
    }
    count
}

/// Scan CWD for the candidate or root dir that has the most `.md` files (recursive).
/// Falls back to `"."` if none found.
fn auto_detect_dir(cwd: &Path) -> String {
    let mut best_dir: Option<&str> = None;
    let mut best_count = 0usize;

    for candidate in CANDIDATE_DIRS {
        let candidate_path = cwd.join(candidate);
        if candidate_path.is_dir() {
            let count = count_md_files_recursive(&candidate_path);
            if count > best_count {
                best_count = count;
                best_dir = Some(candidate);
            }
        }
    }

    // Also consider root "." — but only count .md files that are NOT inside a
    // candidate directory, so we compare apples to apples.
    let root_count = count_md_root_only(cwd);
    if root_count > best_count {
        return ".".to_owned();
    }

    best_dir
        .map(|s| s.to_owned())
        .unwrap_or_else(|| ".".to_owned())
}

/// Count `.md` files directly in `dir` and in non-candidate subdirectories (recursive).
/// Excludes candidate directories so root doesn't double-count their contents.
fn count_md_root_only(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let mut count = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip candidate directories — they're counted separately.
            let is_candidate = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|name| CANDIDATE_DIRS.contains(&name));
            if !is_candidate {
                count += count_md_files_recursive(&path);
            }
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        {
            count += 1;
        }
    }
    count
}

/// Replace the `hyalo-knowledgebase/**` path glob in the rule template with
/// `{dir}/**` so the rule targets the actual vault directory.
fn parameterize_rule(template: &str, dir: &str) -> String {
    template.replace("hyalo-knowledgebase/**", &format!("{dir}/**"))
}

/// Append `line` to `content`, separated by a blank line. Strips trailing newlines from
/// `content` first, then adds `\n\n` (blank-line separator) before `line` and a final `\n`.
fn append_line_to_file_content(content: &str, line: &str) -> String {
    let mut result = content.trim_end_matches('\n').to_owned();
    result.push('\n');
    result.push('\n');
    result.push_str(line);
    result.push('\n');
    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_adds_blank_line_separator() {
        let content = "# Existing\n\nSome content.\n";
        let result = append_line_to_file_content(content, "New hint");
        assert_eq!(result, "# Existing\n\nSome content.\n\nNew hint\n");
    }

    #[test]
    fn append_handles_trailing_newlines() {
        let content = "# Existing\n\n";
        let result = append_line_to_file_content(content, "New hint");
        assert_eq!(result, "# Existing\n\nNew hint\n");
    }

    #[test]
    fn append_handles_empty_content() {
        // Empty content: trim_end_matches('\n') leaves "", then we add \n + \n + hint + \n
        let result = append_line_to_file_content("", "New hint");
        assert_eq!(result, "\n\nNew hint\n");
    }

    #[test]
    fn count_md_files_recursive_counts_nested() {
        let tmp = tempfile::TempDir::new().unwrap();
        assert_eq!(count_md_files_recursive(tmp.path()), 0);

        fs::write(tmp.path().join("top.md"), "# Top").unwrap();
        let sub = tmp.path().join("sub");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("nested.md"), "# Nested").unwrap();
        fs::write(sub.join("not.txt"), "text").unwrap();

        assert_eq!(count_md_files_recursive(tmp.path()), 2);
    }

    #[test]
    fn auto_detect_dir_falls_back_to_dot() {
        let tmp = tempfile::TempDir::new().unwrap();
        let result = auto_detect_dir(tmp.path());
        assert_eq!(result, ".");
    }

    #[test]
    fn auto_detect_dir_picks_most_md_files() {
        let tmp = tempfile::TempDir::new().unwrap();

        // docs: 1 md file
        fs::create_dir_all(tmp.path().join("docs")).unwrap();
        fs::write(tmp.path().join("docs").join("a.md"), "# A").unwrap();

        // knowledgebase: 3 md files (2 nested)
        fs::create_dir_all(tmp.path().join("knowledgebase").join("sub")).unwrap();
        fs::write(tmp.path().join("knowledgebase").join("b.md"), "# B").unwrap();
        fs::write(
            tmp.path().join("knowledgebase").join("sub").join("c.md"),
            "# C",
        )
        .unwrap();
        fs::write(
            tmp.path().join("knowledgebase").join("sub").join("d.md"),
            "# D",
        )
        .unwrap();

        let result = auto_detect_dir(tmp.path());
        assert_eq!(result, "knowledgebase");
    }

    #[test]
    fn auto_detect_dir_finds_docs() {
        let tmp = tempfile::TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("docs")).unwrap();
        fs::write(tmp.path().join("docs").join("note.md"), "# Hello").unwrap();
        let result = auto_detect_dir(tmp.path());
        assert_eq!(result, "docs");
    }

    #[test]
    fn auto_detect_dir_skips_empty_candidate() {
        let tmp = tempfile::TempDir::new().unwrap();
        // docs exists but is empty — should fall through to knowledgebase
        fs::create_dir_all(tmp.path().join("docs")).unwrap();
        fs::create_dir_all(tmp.path().join("knowledgebase")).unwrap();
        fs::write(tmp.path().join("knowledgebase").join("note.md"), "# Hello").unwrap();
        let result = auto_detect_dir(tmp.path());
        assert_eq!(result, "knowledgebase");
    }

    #[test]
    fn auto_detect_dir_falls_back_to_dot_when_root_has_most() {
        let tmp = tempfile::TempDir::new().unwrap();
        // docs: 1 md file
        fs::create_dir_all(tmp.path().join("docs")).unwrap();
        fs::write(tmp.path().join("docs").join("a.md"), "# A").unwrap();
        // root (excluding candidate dirs): 2 md files → root wins
        fs::write(tmp.path().join("x.md"), "# X").unwrap();
        fs::write(tmp.path().join("y.md"), "# Y").unwrap();
        let result = auto_detect_dir(tmp.path());
        assert_eq!(result, ".");
    }

    #[test]
    fn parameterize_rule_replaces_path() {
        let template = "---\npaths:\n  - \"hyalo-knowledgebase/**\"\n---\nContent here.\n";
        let result = parameterize_rule(template, "docs");
        assert!(result.contains("\"docs/**\""));
        assert!(!result.contains("hyalo-knowledgebase/**"));
    }

    #[test]
    fn parameterize_rule_with_dot_dir() {
        let template = "---\npaths:\n  - \"hyalo-knowledgebase/**\"\n---\n";
        let result = parameterize_rule(template, ".");
        assert!(result.contains("\"./**\""));
    }

    #[test]
    fn run_init_overwrites_skills_on_rerun() {
        let tmp = tempfile::TempDir::new().unwrap();

        // First run
        let outcome1 = run_init_in(Some("docs"), true, tmp.path()).unwrap();
        let CommandOutcome::Success(out1) = outcome1 else {
            panic!("expected success");
        };
        assert!(out1.contains("created  .claude/skills/hyalo/SKILL.md"));
        assert!(out1.contains("created  .claude/skills/hyalo-dream/SKILL.md"));
        assert!(out1.contains("created  .claude/rules/knowledgebase.md"));

        // Second run — should say "updated", not "created"
        let outcome2 = run_init_in(Some("docs"), true, tmp.path()).unwrap();
        let CommandOutcome::Success(out2) = outcome2 else {
            panic!("expected success");
        };
        assert!(out2.contains("updated  .claude/skills/hyalo/SKILL.md"));
        assert!(out2.contains("updated  .claude/skills/hyalo-dream/SKILL.md"));
        assert!(out2.contains("updated  .claude/rules/knowledgebase.md"));
    }

    #[test]
    fn run_init_updates_toml_when_dir_explicit() {
        let tmp = tempfile::TempDir::new().unwrap();

        // Create initial .hyalo.toml
        fs::write(tmp.path().join(".hyalo.toml"), "dir = \"old\"\n").unwrap();

        let outcome = run_init_in(Some("newdir"), false, tmp.path()).unwrap();
        let CommandOutcome::Success(out) = outcome else {
            panic!("expected success");
        };
        assert!(out.contains(".hyalo.toml"));

        let content = fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
        assert_eq!(content, "dir = \"newdir\"\n");
    }

    #[test]
    fn run_init_skips_toml_when_exists_and_no_explicit_dir() {
        let tmp = tempfile::TempDir::new().unwrap();

        fs::write(tmp.path().join(".hyalo.toml"), "dir = \"old\"\n").unwrap();

        let outcome = run_init_in(None, false, tmp.path()).unwrap();
        let CommandOutcome::Success(out) = outcome else {
            panic!("expected success");
        };
        assert!(out.contains("skipped  .hyalo.toml"));

        // Content unchanged
        let content = fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
        assert_eq!(content, "dir = \"old\"\n");
    }

    #[test]
    fn run_init_rule_uses_detected_dir() {
        let tmp = tempfile::TempDir::new().unwrap();

        let outcome = run_init_in(Some("my-notes"), true, tmp.path()).unwrap();
        assert!(matches!(outcome, CommandOutcome::Success(_)));

        let rule_content = fs::read_to_string(
            tmp.path()
                .join(".claude")
                .join("rules")
                .join("knowledgebase.md"),
        )
        .unwrap();
        assert!(rule_content.contains("my-notes/**"));
        assert!(!rule_content.contains("hyalo-knowledgebase/**"));
    }
}
