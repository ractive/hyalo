#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use crate::output::CommandOutcome;

// ---------------------------------------------------------------------------
// Embedded skill content
// ---------------------------------------------------------------------------

const SKILL_CONTENT: &str = r#"---
name: hyalo
user_invocable: false
description: >
  Use the hyalo CLI instead of Read/Edit/Grep/Glob when working with markdown (.md) files
  that have YAML frontmatter. This skill MUST be consulted whenever Claude is working with
  markdown documentation directories, knowledgebases, wikis, notes, Obsidian-compatible
  collections, Zettelkasten systems, iteration plans, or any collection of .md files with
  frontmatter. Trigger this skill when: searching or filtering markdown files by content,
  tags, or properties; reading or modifying YAML frontmatter; managing tags or metadata
  across documents; toggling task checkboxes in markdown; getting an overview of a
  documentation directory; querying document properties or status fields; bulk-updating
  metadata across many markdown files; or when you find yourself repeatedly using
  Grep/Glob/Read on .md files. Even if the user does not mention "hyalo" by name, use this
  skill whenever the task involves structured markdown documents with frontmatter.
---

# Hyalo CLI — Preferred Tool for Markdown with Frontmatter

Hyalo is a fast CLI for querying and mutating YAML frontmatter, tags, tasks, and structure
in directories of markdown files. Its killer features are combined filtering (e.g.
`hyalo find -e "regex" --property status!=done --tag feature`) which you can't easily
replicate with Grep/Glob, and bulk mutations (`hyalo set --where-property`) that replace
multiple Read + Edit calls.

Filters combine freely — content regex + property conditions + tag + section + task status
in a single call, something impossible with Grep/Glob alone:

```bash
hyalo find -e "pattern" --property status!=completed --tag iteration --section "Tasks" --task todo
```

Pipe through `--jq` to reshape output into anything — dashboards, burndowns, reports:

```bash
hyalo find --property status=in-progress --fields tasks \
  --jq 'map({file, done: ([.tasks[] | select(.status == "x")] | length), total: (.tasks | length)})'
```

**Run `hyalo --help` and `hyalo <command> --help` to learn the full API.**

## Setup (run once per project)

ALWAYS run `which hyalo` as your very first step. Do not skip this.

- **Not on PATH?** Inform the user: "The `hyalo` CLI is not installed. You can install it
  from https://github.com/ractive/hyalo." Fall back to Read/Edit/Grep/Glob.
- **On PATH?** Check for `.hyalo.toml` in the project root. If it exists, hyalo is
  configured — the `dir` setting means you don't need `--dir` on every command.
- **No `.hyalo.toml` but a directory with many `.md` files?** (e.g. `docs/`, `knowledgebase/`,
  `wiki/`, `notes/`, `content/`, or any folder with 10+ markdown files) Suggest creating one:
  ```toml
  dir = "docs"
  ```

**After confirming hyalo works**, add a line to the project's `CLAUDE.md` so future
conversations use hyalo without needing this skill:

```
Use `hyalo` CLI (not Read/Grep/Glob) for all markdown knowledgebase operations (frontmatter, tags, tasks, search). Run `hyalo --help` for usage. Use `--format text` for compact LLM-friendly output.
```

This one-line instruction saves tokens in every future conversation.

## When to use hyalo vs. built-in tools

- **hyalo:** queries, frontmatter reads/mutations, tag management, task toggling, bulk updates
- **Edit tool:** body prose changes (rewriting paragraphs) that hyalo can't handle
- **Write tool:** creating brand new markdown files

Start with `hyalo summary --format text` to orient yourself in a new directory.

## Available commands

- **find** — search/filter by text, regex, property, tag, task status
- **read** — extract body content, a section, or line range
- **summary** — directory overview: file counts, tags, tasks, recent files
- **properties** — list property names and types
- **tags** — list tags with counts
- **set** — create/overwrite frontmatter properties, add tags (supports `--where-property`/`--where-tag` for conditional bulk updates)
- **remove** — delete properties or tags
- **append** — add to list properties
- **task** — read, toggle, or set status on a single task checkbox

## The --format text flag

Use `--format text` for compact, low-token output designed for LLM consumption — less noise
than JSON, fewer tokens. Reach for it when orienting yourself or scanning results.
"#;

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
/// - `claude`: when `true`, also installs the hyalo skill and appends a hint
///   line to `.claude/CLAUDE.md`.
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
        let toml_content = format!("dir = \"{dir_value}\"\n");
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
    // Step 3: append hyalo hint to .claude/CLAUDE.md (no duplicates)
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

/// Append `line` to `content`, ensuring there is exactly one trailing newline before it
/// and one after it.
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
