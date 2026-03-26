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
    let mut summary = String::new();

    // ------------------------------------------------------------------
    // Step 1: create .hyalo.toml
    // ------------------------------------------------------------------
    let toml_path = cwd.join(".hyalo.toml");
    if toml_path.exists() {
        writeln!(summary, "skipped  .hyalo.toml (already exists)").unwrap();
    } else {
        let dir_value = match dir {
            Some(d) => d.to_owned(),
            None => auto_detect_dir(&cwd),
        };
        let escaped = dir_value.replace('\\', "\\\\").replace('"', "\\\"");
        let toml_content = format!("dir = \"{escaped}\"\n");
        fs::write(&toml_path, &toml_content)
            .with_context(|| format!("failed to write {}", toml_path.display()))?;
        writeln!(summary, "created  .hyalo.toml  (dir = \"{dir_value}\")").unwrap();
    }

    if !claude {
        return Ok(CommandOutcome::Success(summary.trim_end().to_owned()));
    }

    // ------------------------------------------------------------------
    // Step 2: create .claude/skills/hyalo/SKILL.md
    // ------------------------------------------------------------------
    let skill_path = cwd
        .join(".claude")
        .join("skills")
        .join("hyalo")
        .join("SKILL.md");
    if skill_path.exists() {
        writeln!(
            summary,
            "skipped  .claude/skills/hyalo/SKILL.md (already exists)"
        )
        .unwrap();
    } else {
        let skill_dir = skill_path
            .parent()
            .context("skill path has no parent directory")?;
        fs::create_dir_all(skill_dir)
            .with_context(|| format!("failed to create directory {}", skill_dir.display()))?;
        fs::write(&skill_path, SKILL_CONTENT)
            .with_context(|| format!("failed to write {}", skill_path.display()))?;
        writeln!(summary, "created  .claude/skills/hyalo/SKILL.md").unwrap();
    }

    // ------------------------------------------------------------------
    // Step 3: create .claude/skills/hyalo-dream/SKILL.md
    // ------------------------------------------------------------------
    let dream_skill_path = cwd
        .join(".claude")
        .join("skills")
        .join("hyalo-dream")
        .join("SKILL.md");
    if dream_skill_path.exists() {
        writeln!(
            summary,
            "skipped  .claude/skills/hyalo-dream/SKILL.md (already exists)"
        )
        .unwrap();
    } else {
        let dream_skill_dir = dream_skill_path
            .parent()
            .context("dream skill path has no parent directory")?;
        fs::create_dir_all(dream_skill_dir)
            .with_context(|| format!("failed to create directory {}", dream_skill_dir.display()))?;
        fs::write(&dream_skill_path, DREAM_SKILL_CONTENT)
            .with_context(|| format!("failed to write {}", dream_skill_path.display()))?;
        writeln!(summary, "created  .claude/skills/hyalo-dream/SKILL.md").unwrap();
    }

    // ------------------------------------------------------------------
    // Step 4: append hyalo hint to .claude/CLAUDE.md (no duplicates)
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

/// Scan CWD for the first common doc directory that exists and contains `.md` files.
/// Falls back to `"."` if none found.
fn auto_detect_dir(cwd: &Path) -> String {
    for candidate in CANDIDATE_DIRS {
        let candidate_path = cwd.join(candidate);
        if candidate_path.is_dir() && dir_contains_md(&candidate_path) {
            return (*candidate).to_owned();
        }
    }
    ".".to_owned()
}

/// Returns `true` if `dir` contains at least one `.md` file (non-recursive).
fn dir_contains_md(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("md"))
    })
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
    fn dir_contains_md_detects_md_files() {
        let tmp = tempfile::TempDir::new().unwrap();
        assert!(!dir_contains_md(tmp.path()));
        fs::write(tmp.path().join("note.md"), "# Hello").unwrap();
        assert!(dir_contains_md(tmp.path()));
    }

    #[test]
    fn auto_detect_dir_falls_back_to_dot() {
        let tmp = tempfile::TempDir::new().unwrap();
        let result = auto_detect_dir(tmp.path());
        assert_eq!(result, ".");
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
}
