#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use toml::Value as TomlValue;

use crate::output::CommandOutcome;

// ---------------------------------------------------------------------------
// Embedded skill content
// ---------------------------------------------------------------------------

const SKILL_CONTENT: &str = include_str!("../../../../.claude/skills/hyalo/SKILL.md");
const DREAM_SKILL_CONTENT: &str = include_str!("../../../../.claude/skills/hyalo-dream/SKILL.md");
const RULE_TEMPLATE: &str = include_str!("../../../../.claude/rules/knowledgebase.md");

const CLAUDE_MD_HINT: &str = "Use `hyalo` CLI (not Read/Grep/Glob) for all markdown knowledgebase operations (frontmatter, tags, tasks, search). Run `hyalo --help` for usage. Use `--format text` for compact LLM-friendly output.";

const SECTION_START: &str = "<!-- hyalo:start -->";
const SECTION_END: &str = "<!-- hyalo:end -->";

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
///   writes a managed section to `.claude/CLAUDE.md`.
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
        let toml_content = if toml_existed {
            // Read and parse the existing file; update only the `dir` key so
            // that other user config (format, hints, site_prefix …) is preserved.
            // If the file is malformed, fall back to overwriting with just `dir`.
            let existing_raw = fs::read_to_string(&toml_path)
                .with_context(|| format!("failed to read {}", toml_path.display()))?;
            match existing_raw.parse::<TomlValue>() {
                Ok(mut table) => {
                    if let Some(map) = table.as_table_mut() {
                        map.insert("dir".to_owned(), TomlValue::String(dir_value.clone()));
                        // Serialise back; toml::to_string always produces valid TOML.
                        match toml::to_string(&table) {
                            Ok(s) => s,
                            Err(_) => {
                                writeln!(
                                    summary,
                                    "warning  .hyalo.toml was malformed; existing content replaced"
                                )
                                .unwrap();
                                minimal_toml_dir(&dir_value)
                            }
                        }
                    } else {
                        // Valid TOML but not a table (e.g. bare string) — overwrite.
                        writeln!(
                            summary,
                            "warning  .hyalo.toml was malformed; existing content replaced"
                        )
                        .unwrap();
                        minimal_toml_dir(&dir_value)
                    }
                }
                Err(_) => {
                    // Malformed existing file — overwrite with just dir and note it.
                    writeln!(
                        summary,
                        "warning  .hyalo.toml was malformed; existing content replaced"
                    )
                    .unwrap();
                    minimal_toml_dir(&dir_value)
                }
            }
        } else {
            minimal_toml_dir(&dir_value)
        };
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
    fs::write(
        &skill_path,
        parameterize_template(SKILL_CONTENT, &dir_value),
    )
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
    fs::write(
        &dream_skill_path,
        parameterize_template(DREAM_SKILL_CONTENT, &dir_value),
    )
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
    // Step 5: upsert the hyalo managed section in .claude/CLAUDE.md
    // ------------------------------------------------------------------
    let claude_md_path = cwd.join(".claude").join("CLAUDE.md");
    let managed_section = format!("{SECTION_START}\n{CLAUDE_MD_HINT}\n{SECTION_END}");
    if claude_md_path.exists() {
        let existing = fs::read_to_string(&claude_md_path)
            .with_context(|| format!("failed to read {}", claude_md_path.display()))?;
        let (new_content, action) = upsert_managed_section(&existing, &managed_section);
        fs::write(&claude_md_path, &new_content)
            .with_context(|| format!("failed to write {}", claude_md_path.display()))?;
        writeln!(summary, "updated  .claude/CLAUDE.md ({action})").unwrap();
    } else {
        // Create the file and any parent directories.
        let claude_dir = claude_md_path
            .parent()
            .context("CLAUDE.md path has no parent directory")?;
        fs::create_dir_all(claude_dir)
            .with_context(|| format!("failed to create directory {}", claude_dir.display()))?;
        let content = format!("{managed_section}\n");
        fs::write(&claude_md_path, content)
            .with_context(|| format!("failed to write {}", claude_md_path.display()))?;
        writeln!(summary, "created  .claude/CLAUDE.md (with managed section)").unwrap();
    }

    Ok(CommandOutcome::Success(summary.trim_end().to_owned()))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Produce a minimal valid TOML string containing only `dir = "<value>"`.
///
/// Uses the `toml` crate to correctly escape the value (handles Unicode,
/// backslashes, double-quotes) rather than relying on Rust's `{:?}` debug
/// format which produces `\u{NNNN}` — invalid TOML escape sequences.
fn minimal_toml_dir(dir_value: &str) -> String {
    let table =
        toml::map::Map::from_iter([("dir".to_owned(), TomlValue::String(dir_value.to_owned()))]);
    toml::to_string(&table).unwrap_or_else(|_| {
        // Fallback: manual escaping of backslash and double-quote only.
        format!(
            "dir = \"{}\"\n",
            dir_value.replace('\\', "\\\\").replace('"', "\\\"")
        )
    })
}

/// Recursively count `.md` files under `dir`.
///
/// Uses `DirEntry::file_type()` instead of `Path::is_dir()` to avoid following
/// symlinks, which could cause infinite loops on circular symlink structures.
fn count_md_files_recursive(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let mut count = 0;
    for entry in entries.flatten() {
        let is_real_dir = entry
            .file_type()
            .is_ok_and(|ft| ft.is_dir() && !ft.is_symlink());
        if is_real_dir {
            count += count_md_files_recursive(&entry.path());
        } else if entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        {
            count += 1;
        }
    }
    count
}

/// Returns `true` if the directory name is an exact candidate match or
/// contains a candidate substring (case-insensitive).
fn is_fuzzy_candidate(dir_name: &str) -> bool {
    let lower = dir_name.to_ascii_lowercase();
    CANDIDATE_DIRS.iter().any(|c| lower.contains(*c))
}

/// Scan CWD for the candidate or root dir that has the most `.md` files (recursive).
/// Falls back to `"."` if none found.
///
/// Exact candidate names (e.g. `docs`) are tried first. Then all non-hidden
/// subdirectories whose names *contain* a candidate substring (e.g.
/// `my-knowledgebase`) are also considered, so common naming variants are
/// auto-detected without an explicit `--dir` flag.
fn auto_detect_dir(cwd: &Path) -> String {
    let mut best_dir: Option<String> = None;
    let mut best_count = 0usize;

    // 1. Exact candidate matches.
    for candidate in CANDIDATE_DIRS {
        let candidate_path = cwd.join(candidate);
        if candidate_path.is_dir() {
            let count = count_md_files_recursive(&candidate_path);
            if count > best_count {
                best_count = count;
                best_dir = Some((*candidate).to_owned());
            }
        }
    }

    // 2. Fuzzy matches: non-hidden subdirectories whose names contain a
    //    candidate substring but are not themselves exact candidates.
    if let Ok(entries) = fs::read_dir(cwd) {
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            if !ft.is_dir() || ft.is_symlink() {
                continue;
            }
            let path = entry.path();
            let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // Skip hidden dirs and exact candidates (already handled above).
            if dir_name.starts_with('.') || CANDIDATE_DIRS.contains(&dir_name) {
                continue;
            }
            if is_fuzzy_candidate(dir_name) {
                let count = count_md_files_recursive(&path);
                if count > best_count {
                    best_count = count;
                    best_dir = Some(dir_name.to_owned());
                }
            }
        }
    }

    // 3. Also consider root "." — but only count .md files that are NOT inside
    //    a candidate directory, so we compare apples to apples.
    let root_count = count_md_root_only(cwd);
    if root_count > best_count {
        return ".".to_owned();
    }

    best_dir.unwrap_or_else(|| ".".to_owned())
}

/// Count `.md` files directly in `dir` and in non-candidate subdirectories (recursive).
///
/// Excludes:
/// - Exact candidate directories (e.g. `docs`, `wiki`) — counted separately.
/// - Fuzzy-matched directories (e.g. `my-knowledgebase`) — also counted separately.
/// - Hidden directories (names starting with `.`, e.g. `.git`, `.claude`).
///
/// This keeps root-level `.md` count independent so the comparison in
/// `auto_detect_dir` is apples-to-apples.
fn count_md_root_only(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let mut count = 0;
    for entry in entries.flatten() {
        let is_real_dir = entry
            .file_type()
            .is_ok_and(|ft| ft.is_dir() && !ft.is_symlink());
        if is_real_dir {
            let path = entry.path();
            let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // Skip hidden directories and any directory that auto_detect_dir
            // would consider as a candidate (exact or fuzzy).
            if !dir_name.starts_with('.') && !is_fuzzy_candidate(dir_name) {
                count += count_md_files_recursive(&path);
            }
        } else if entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        {
            count += 1;
        }
    }
    count
}

/// The sentinel string used in all templates to represent the docs directory.
/// Occurrences are replaced at install time with the user's actual vault directory.
const DIR_SENTINEL: &str = "hyalo-knowledgebase";

/// Replace all occurrences of `hyalo-knowledgebase` in `template` with `dir`.
///
/// Path separators in `dir` are normalised to `/` (Windows compatibility).
/// Double-quotes in `dir` are escaped so substituted values remain valid inside
/// YAML double-quoted scalars and shell strings.
fn parameterize_template(template: &str, dir: &str) -> String {
    let normalised = dir.replace('\\', "/");
    let escaped = normalised.replace('"', "\\\"");
    template.replace(DIR_SENTINEL, &escaped)
}

/// Replace the `hyalo-knowledgebase/**` path glob in the rule template with
/// `{dir}/**` so the rule targets the actual vault directory.
///
/// Asserts that the sentinel glob is present so template drift is caught early.
fn parameterize_rule(template: &str, dir: &str) -> String {
    let sentinel_glob = format!("{DIR_SENTINEL}/**");
    // The sentinel must be present; its absence means the template was changed
    // without updating this function — a programming error.
    assert!(
        template.contains(&sentinel_glob),
        "rule template does not contain the expected sentinel {sentinel_glob:?}"
    );
    // parameterize_template replaces `hyalo-knowledgebase` with `dir`, leaving
    // the `/**` suffix intact — so the glob path is correctly rewritten.
    parameterize_template(template, dir)
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

/// Replace the hyalo managed section in `content`, or append it if absent.
///
/// Returns `(updated_content, action_description)` where `action_description` is a
/// short human-readable string describing what was done.
///
/// The replacement logic (in priority order):
/// 1. If both `<!-- hyalo:start -->` and `<!-- hyalo:end -->` markers are present,
///    replace everything between them (inclusive) with `section`.
/// 2. If only the start marker is present (no matching end marker), treat as absent.
/// 3. If the bare `CLAUDE_MD_HINT` text appears without markers, replace that line
///    with `section` (migration path from old format).
/// 4. Otherwise, append `section` separated by a blank line.
fn upsert_managed_section(content: &str, section: &str) -> (String, &'static str) {
    let lines: Vec<&str> = content.lines().collect();

    // Find line indices of the start and end markers.
    let start_idx = lines.iter().position(|l| l.contains(SECTION_START));
    let end_idx = lines.iter().position(|l| l.contains(SECTION_END));

    if let (Some(s), Some(e)) = (start_idx, end_idx)
        && s < e
    {
        // Both markers present in correct order — replace from start to end (inclusive).
        let mut result = String::new();
        for line in &lines[..s] {
            result.push_str(line);
            result.push('\n');
        }
        result.push_str(section);
        result.push('\n');
        for line in &lines[e + 1..] {
            result.push_str(line);
            result.push('\n');
        }
        return (result, "replaced managed section");
    }

    // Only start marker without end: treat as absent (fall through).
    // Check for old bare hint line and migrate it.
    if let Some(hint_idx) = lines.iter().position(|l| *l == CLAUDE_MD_HINT) {
        let mut result = String::new();
        for line in &lines[..hint_idx] {
            result.push_str(line);
            result.push('\n');
        }
        result.push_str(section);
        result.push('\n');
        for line in &lines[hint_idx + 1..] {
            result.push_str(line);
            result.push('\n');
        }
        return (result, "migrated to managed section");
    }

    // No markers and no old hint — append.
    let appended = append_line_to_file_content(content, section);
    (appended, "appended managed section")
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
    fn auto_detect_dir_fuzzy_matches_containing_candidate_substring() {
        let tmp = tempfile::TempDir::new().unwrap();
        // "my-knowledgebase" contains "knowledgebase" → should be auto-detected
        fs::create_dir_all(tmp.path().join("my-knowledgebase")).unwrap();
        fs::write(tmp.path().join("my-knowledgebase").join("a.md"), "# A").unwrap();
        let result = auto_detect_dir(tmp.path());
        assert_eq!(result, "my-knowledgebase");
    }

    #[test]
    fn auto_detect_dir_fuzzy_matches_project_wiki() {
        let tmp = tempfile::TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("project-wiki")).unwrap();
        fs::write(
            tmp.path().join("project-wiki").join("readme.md"),
            "# Readme",
        )
        .unwrap();
        let result = auto_detect_dir(tmp.path());
        assert_eq!(result, "project-wiki");
    }

    #[test]
    fn auto_detect_dir_fuzzy_does_not_double_count_in_root() {
        let tmp = tempfile::TempDir::new().unwrap();
        // "my-knowledgebase" contains "knowledgebase" — 2 md files
        fs::create_dir_all(tmp.path().join("my-knowledgebase")).unwrap();
        fs::write(tmp.path().join("my-knowledgebase").join("a.md"), "# A").unwrap();
        fs::write(tmp.path().join("my-knowledgebase").join("b.md"), "# B").unwrap();
        // root-level md files (should NOT include the fuzzy dir's files)
        fs::write(tmp.path().join("readme.md"), "# Root").unwrap();
        let result = auto_detect_dir(tmp.path());
        // fuzzy dir has 2, root-only has 1 → fuzzy dir wins
        assert_eq!(result, "my-knowledgebase");
    }

    #[test]
    fn auto_detect_dir_exact_candidate_beats_fuzzy_with_fewer_files() {
        let tmp = tempfile::TempDir::new().unwrap();
        // exact "docs" with 2 md files
        fs::create_dir_all(tmp.path().join("docs")).unwrap();
        fs::write(tmp.path().join("docs").join("a.md"), "# A").unwrap();
        fs::write(tmp.path().join("docs").join("b.md"), "# B").unwrap();
        // fuzzy "my-docs" with 1 md file
        fs::create_dir_all(tmp.path().join("my-docs")).unwrap();
        fs::write(tmp.path().join("my-docs").join("c.md"), "# C").unwrap();
        let result = auto_detect_dir(tmp.path());
        assert_eq!(result, "docs");
    }

    #[test]
    fn parameterize_template_replaces_sentinel() {
        let template = "Use hyalo-knowledgebase for all docs in hyalo-knowledgebase/\n";
        let result = parameterize_template(template, "my-docs");
        assert_eq!(result, "Use my-docs for all docs in my-docs/\n");
        assert!(!result.contains("hyalo-knowledgebase"));
    }

    #[test]
    fn parameterize_template_normalises_windows_backslashes() {
        let template = "git log -- \"hyalo-knowledgebase/\"\n";
        let result = parameterize_template(template, "my\\notes");
        assert!(result.contains("my/notes"));
        assert!(!result.contains('\\'));
    }

    #[test]
    fn parameterize_template_escapes_double_quotes() {
        let template = "path: hyalo-knowledgebase\n";
        let result = parameterize_template(template, "my\"notes");
        assert!(result.contains("my\\\"notes"));
    }

    #[test]
    fn parameterize_rule_replaces_path() {
        let template = "---\npaths:\n  - \"hyalo-knowledgebase/**\"\n---\nContent here.\n";
        let result = parameterize_rule(template, "docs");
        assert!(result.contains("\"docs/**\""));
        assert!(!result.contains("hyalo-knowledgebase"));
    }

    #[test]
    fn parameterize_rule_with_dot_dir() {
        let template = "---\npaths:\n  - \"hyalo-knowledgebase/**\"\n---\n";
        let result = parameterize_rule(template, ".");
        assert!(result.contains("\"./**\""));
    }

    #[test]
    fn parameterize_rule_normalises_windows_backslashes() {
        let template = "---\npaths:\n  - \"hyalo-knowledgebase/**\"\n---\n";
        let result = parameterize_rule(template, "my\\notes");
        // Backslashes must become forward slashes.
        assert!(result.contains("my/notes/**"));
        assert!(!result.contains('\\'));
    }

    #[test]
    fn parameterize_rule_escapes_double_quotes_in_dir() {
        let template = "---\npaths:\n  - \"hyalo-knowledgebase/**\"\n---\n";
        // A dir name containing a double-quote (unusual but must not corrupt YAML).
        let result = parameterize_rule(template, "my\"notes");
        assert!(result.contains("my\\\"notes/**"));
    }

    #[test]
    fn count_md_root_only_skips_hidden_dirs() {
        let tmp = tempfile::TempDir::new().unwrap();

        // .claude/ with md files — should be skipped
        let claude_dir = tmp.path().join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(claude_dir.join("CLAUDE.md"), "# Hidden").unwrap();
        fs::write(claude_dir.join("SKILL.md"), "# Skill").unwrap();

        // Visible non-candidate dir with md files — should be counted
        let other_dir = tmp.path().join("other");
        fs::create_dir_all(&other_dir).unwrap();
        fs::write(other_dir.join("note.md"), "# Note").unwrap();

        // Root-level md file — should be counted
        fs::write(tmp.path().join("readme.md"), "# Root").unwrap();

        let count = count_md_root_only(tmp.path());
        // readme.md (1) + other/note.md (1) = 2; .claude/*.md are excluded
        assert_eq!(count, 2);
    }

    // ---------------------------------------------------------------------------
    // upsert_managed_section tests
    // ---------------------------------------------------------------------------

    fn make_section() -> String {
        format!("{SECTION_START}\n{CLAUDE_MD_HINT}\n{SECTION_END}")
    }

    #[test]
    fn upsert_managed_section_appends_when_absent() {
        let content = "# Existing\n\nSome content.\n";
        let section = make_section();
        let (result, action) = upsert_managed_section(content, &section);
        assert_eq!(action, "appended managed section");
        assert!(
            result.contains("Some content."),
            "original content preserved"
        );
        assert!(result.contains(SECTION_START), "start marker present");
        assert!(result.contains(SECTION_END), "end marker present");
        assert!(result.contains(CLAUDE_MD_HINT), "hint content present");
        // Verify the section appears after the original content.
        let orig_pos = result.find("Some content.").unwrap();
        let section_pos = result.find(SECTION_START).unwrap();
        assert!(
            section_pos > orig_pos,
            "section appended after original content"
        );
    }

    #[test]
    fn upsert_managed_section_replaces_existing_markers() {
        let section = make_section();
        let old_content =
            format!("# Before\n\n{SECTION_START}\nold hint text\n{SECTION_END}\n\n# After\n");
        let (result, action) = upsert_managed_section(&old_content, &section);
        assert_eq!(action, "replaced managed section");
        assert!(
            result.contains("# Before"),
            "content before markers preserved"
        );
        assert!(
            result.contains("# After"),
            "content after markers preserved"
        );
        assert!(result.contains(CLAUDE_MD_HINT), "new hint content present");
        assert!(!result.contains("old hint text"), "old hint text replaced");
        // Verify only one copy of start/end markers.
        assert_eq!(result.matches(SECTION_START).count(), 1);
        assert_eq!(result.matches(SECTION_END).count(), 1);
    }

    #[test]
    fn upsert_managed_section_migrates_old_hint() {
        let section = make_section();
        let old_content = format!("# Header\n\n{CLAUDE_MD_HINT}\n\n# Footer\n");
        let (result, action) = upsert_managed_section(&old_content, &section);
        assert_eq!(action, "migrated to managed section");
        assert!(result.contains("# Header"), "header preserved");
        assert!(result.contains("# Footer"), "footer preserved");
        assert!(result.contains(SECTION_START), "start marker added");
        assert!(result.contains(SECTION_END), "end marker added");
        assert!(result.contains(CLAUDE_MD_HINT), "hint content present");
        // Should still appear exactly once.
        assert_eq!(result.matches(CLAUDE_MD_HINT).count(), 1);
    }

    #[test]
    fn upsert_managed_section_preserves_surrounding_content() {
        let section = make_section();
        let before = "# Top\n\nFirst paragraph.\n\n";
        let after = "\n\n# Bottom\n\nLast paragraph.\n";
        let old_content = format!("{before}{SECTION_START}\nstale\n{SECTION_END}{after}");
        let (result, _action) = upsert_managed_section(&old_content, &section);
        assert!(
            result.starts_with("# Top\n"),
            "leading content preserved exactly"
        );
        assert!(
            result.contains("First paragraph."),
            "first paragraph preserved"
        );
        assert!(
            result.contains("Last paragraph."),
            "last paragraph preserved"
        );
        assert!(!result.contains("stale"), "stale content replaced");
    }

    #[test]
    fn upsert_managed_section_handles_missing_end_marker() {
        // Only the start marker, no end — treat as absent and append.
        let section = make_section();
        let content = format!("# Existing\n\n{SECTION_START}\norphaned start\n");
        let (result, action) = upsert_managed_section(&content, &section);
        assert_eq!(action, "appended managed section");
        // The fresh section is appended at the end.
        assert!(result.contains(SECTION_END), "end marker now present");
        assert!(result.contains(CLAUDE_MD_HINT), "hint content present");
    }

    #[test]
    fn auto_detect_dir_ignores_hidden_dirs_in_root_count() {
        let tmp = tempfile::TempDir::new().unwrap();

        // docs: 1 md file
        fs::create_dir_all(tmp.path().join("docs")).unwrap();
        fs::write(tmp.path().join("docs").join("a.md"), "# A").unwrap();

        // .claude: 3 md files — must NOT make root win
        let claude_dir = tmp.path().join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(claude_dir.join("CLAUDE.md"), "#").unwrap();
        fs::write(claude_dir.join("SKILL.md"), "#").unwrap();
        fs::write(claude_dir.join("RULE.md"), "#").unwrap();

        // Without the fix, root count would be 3 (from .claude) > 1 (docs) → "."
        // With the fix, root count is 0 → docs wins.
        let result = auto_detect_dir(tmp.path());
        assert_eq!(result, "docs");
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
        // toml::to_string serialises the value; just verify the key is updated.
        assert!(content.contains("dir = \"newdir\""));
    }

    #[test]
    fn run_init_preserves_other_toml_keys_when_updating_dir() {
        let tmp = tempfile::TempDir::new().unwrap();

        // Existing config with extra keys beyond `dir`.
        fs::write(
            tmp.path().join(".hyalo.toml"),
            "dir = \"old\"\nformat = \"text\"\nhints = true\n",
        )
        .unwrap();

        let outcome = run_init_in(Some("newdir"), false, tmp.path()).unwrap();
        assert!(matches!(outcome, CommandOutcome::Success(_)));

        let content = fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
        // dir updated, other keys preserved.
        assert!(content.contains("dir = \"newdir\""));
        assert!(content.contains("format = \"text\""));
        assert!(content.contains("hints = true"));
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

    #[test]
    fn upsert_managed_section_inverted_markers_treated_as_absent() {
        // End marker appears before start marker (malformed file) — must not
        // produce garbled output; instead treat as absent and append.
        let section = make_section();
        let content = format!("# File\n\n{SECTION_END}\nsome text\n{SECTION_START}\n# After\n");
        let (result, action) = upsert_managed_section(&content, &section);
        assert_eq!(
            action, "appended managed section",
            "inverted markers fall through to append"
        );
        // Original content preserved and new section appended at the end.
        assert!(result.contains("# File"), "leading content preserved");
        assert!(result.contains("# After"), "trailing content preserved");
        assert!(result.contains(CLAUDE_MD_HINT), "hint content present");
        // The section must appear after the original content.
        let orig_pos = result.find("# After").unwrap();
        let section_pos = result.find(CLAUDE_MD_HINT).unwrap();
        assert!(
            section_pos > orig_pos,
            "appended section follows original content"
        );
    }

    #[test]
    fn minimal_toml_dir_produces_valid_toml_for_unicode() {
        // A directory name with a non-ASCII character.
        let output = minimal_toml_dir("my\u{1F4C1}notes");
        // Must parse back as valid TOML with the correct value.
        let parsed: toml::Value = output.parse().expect("must be valid TOML");
        assert_eq!(
            parsed.get("dir").and_then(|v| v.as_str()),
            Some("my\u{1F4C1}notes"),
            "unicode round-trips correctly"
        );
    }

    #[test]
    fn minimal_toml_dir_produces_valid_toml_for_backslash() {
        // Windows-style path — backslashes must be escaped in TOML strings.
        let output = minimal_toml_dir("C:\\Users\\me\\notes");
        let parsed: toml::Value = output.parse().expect("must be valid TOML");
        assert_eq!(
            parsed.get("dir").and_then(|v| v.as_str()),
            Some("C:\\Users\\me\\notes"),
            "backslashes round-trip correctly"
        );
    }

    #[test]
    #[cfg(unix)]
    fn count_md_files_recursive_does_not_follow_symlinks() {
        use std::os::unix::fs::symlink;

        let tmp = tempfile::TempDir::new().unwrap();
        let real_dir = tmp.path().join("real");
        fs::create_dir_all(&real_dir).unwrap();
        fs::write(real_dir.join("a.md"), "# A").unwrap();

        // Create a symlink that points back to the parent — a cycle.
        let link = real_dir.join("loop");
        symlink(tmp.path(), &link).unwrap();

        // Without the fix this would overflow the stack. With the fix it
        // completes and counts only the one real file.
        let count = count_md_files_recursive(tmp.path());
        assert_eq!(count, 1, "only the real file counted, symlink loop ignored");
    }

    #[test]
    fn run_init_overwrites_malformed_toml_non_table() {
        // .hyalo.toml contains valid TOML that is not a table (bare string).
        let tmp = tempfile::TempDir::new().unwrap();
        fs::write(tmp.path().join(".hyalo.toml"), "\"just a string\"\n").unwrap();

        let outcome = run_init_in(Some("docs"), false, tmp.path()).unwrap();
        let CommandOutcome::Success(out) = outcome else {
            panic!("expected success");
        };
        // Warning emitted.
        assert!(
            out.contains("warning  .hyalo.toml was malformed"),
            "malformed warning present"
        );
        // File overwritten with a valid table.
        let content = fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
        let parsed: toml::Value = content
            .parse()
            .expect("overwritten content must be valid TOML");
        assert_eq!(
            parsed.get("dir").and_then(|v| v.as_str()),
            Some("docs"),
            "dir key written correctly"
        );
    }
}
