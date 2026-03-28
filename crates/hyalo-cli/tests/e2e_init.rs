mod common;

use common::hyalo;
use std::fs;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// .hyalo.toml creation
// ---------------------------------------------------------------------------

#[test]
fn init_creates_hyalo_toml() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["init"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    let toml_path = tmp.path().join(".hyalo.toml");
    assert!(toml_path.exists(), ".hyalo.toml should have been created");

    let content = fs::read_to_string(&toml_path).unwrap();
    assert!(
        content.contains("dir ="),
        ".hyalo.toml should contain a dir setting; got: {content}"
    );
}

#[test]
fn init_does_not_overwrite_existing_toml() {
    // Without an explicit --dir flag, .hyalo.toml is skipped if it already exists.
    let tmp = TempDir::new().unwrap();
    let original = "dir = \"my-vault\"\n";
    fs::write(tmp.path().join(".hyalo.toml"), original).unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["init"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(
        stdout.contains("skipped"),
        "expected 'skipped' in stdout; got: {stdout}"
    );

    let content = fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
    assert_eq!(
        content, original,
        ".hyalo.toml should not have been modified when no --dir given"
    );
}

#[test]
fn init_dir_flag_updates_existing_toml() {
    // When --dir is explicitly given, .hyalo.toml is updated even if it already exists.
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \"old\"\n").unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["init", "--dir", "new-dir"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(
        stdout.contains("updated"),
        "expected 'updated' in stdout; got: {stdout}"
    );

    let content = fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
    assert_eq!(
        content, "dir = \"new-dir\"\n",
        ".hyalo.toml should have been updated to new-dir"
    );
}

#[test]
fn init_with_dir_flag() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["init", "--dir", "docs"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    let content = fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
    assert!(
        content.contains("dir = \"docs\""),
        ".hyalo.toml should contain dir = \"docs\"; got: {content}"
    );
}

#[test]
fn init_auto_detects_docs_dir() {
    let tmp = TempDir::new().unwrap();
    // Create docs/ with a .md file so auto-detection picks it up
    fs::create_dir_all(tmp.path().join("docs")).unwrap();
    fs::write(tmp.path().join("docs").join("note.md"), "# Hello").unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["init"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    let content = fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
    assert!(
        content.contains("dir = \"docs\""),
        "should have auto-detected docs/; got: {content}"
    );
}

#[test]
fn init_falls_back_to_dot_when_no_doc_dir() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["init"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    let content = fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
    assert!(
        content.contains("dir = \".\""),
        "should have defaulted to .; got: {content}"
    );
}

#[test]
fn init_smart_detection_picks_dir_with_most_md() {
    let tmp = TempDir::new().unwrap();

    // docs: 1 md file
    fs::create_dir_all(tmp.path().join("docs")).unwrap();
    fs::write(tmp.path().join("docs").join("a.md"), "# A").unwrap();

    // knowledgebase: 3 md files (including nested)
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

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["init"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    let content = fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
    assert!(
        content.contains("dir = \"knowledgebase\""),
        "should have picked knowledgebase (most .md files); got: {content}"
    );
}

// ---------------------------------------------------------------------------
// --claude flag: skill creation
// ---------------------------------------------------------------------------

#[test]
fn init_claude_creates_skill() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["init", "--claude"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    let skill_path = tmp
        .path()
        .join(".claude")
        .join("skills")
        .join("hyalo")
        .join("SKILL.md");
    assert!(skill_path.exists(), "SKILL.md should have been created");

    let content = fs::read_to_string(&skill_path).unwrap();
    assert!(
        content.contains("hyalo"),
        "SKILL.md should contain hyalo content; got: {content}"
    );
    assert!(
        content.contains("name: hyalo"),
        "SKILL.md should have frontmatter name field; got: {content}"
    );
}

#[test]
fn init_claude_overwrites_existing_skill() {
    // Skills are always overwritten on re-run; summary says "updated".
    let tmp = TempDir::new().unwrap();
    let skill_dir = tmp.path().join(".claude").join("skills").join("hyalo");
    fs::create_dir_all(&skill_dir).unwrap();
    let skill_path = skill_dir.join("SKILL.md");
    let original = "---\nname: custom\n---\nstale content\n";
    fs::write(&skill_path, original).unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["init", "--claude"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(
        stdout.contains("updated"),
        "expected 'updated' in stdout for overwritten SKILL.md; got: {stdout}"
    );

    // Content should have been replaced with the canonical skill content
    let content = fs::read_to_string(&skill_path).unwrap();
    assert_ne!(
        content, original,
        "SKILL.md should have been overwritten, not preserved"
    );
    assert!(
        content.contains("name: hyalo"),
        "overwritten SKILL.md should contain canonical name field; got: {content}"
    );
}

// ---------------------------------------------------------------------------
// --claude flag: hyalo-tidy skill creation
// ---------------------------------------------------------------------------

#[test]
fn init_claude_creates_tidy_skill() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["init", "--claude"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    let tidy_skill_path = tmp
        .path()
        .join(".claude")
        .join("skills")
        .join("hyalo-tidy")
        .join("SKILL.md");
    assert!(
        tidy_skill_path.exists(),
        "hyalo-tidy SKILL.md should have been created"
    );

    let content = fs::read_to_string(&tidy_skill_path).unwrap();
    assert!(
        content.contains("name: hyalo-tidy"),
        "SKILL.md should have frontmatter name field; got: {content}"
    );
    assert!(
        content.contains("Knowledgebase Consolidation"),
        "SKILL.md should contain tidy skill content"
    );
}

#[test]
fn init_claude_overwrites_existing_tidy_skill() {
    // Tidy skill is always overwritten on re-run; summary says "updated".
    let tmp = TempDir::new().unwrap();
    let tidy_skill_dir = tmp.path().join(".claude").join("skills").join("hyalo-tidy");
    fs::create_dir_all(&tidy_skill_dir).unwrap();
    let tidy_skill_path = tidy_skill_dir.join("SKILL.md");
    let original = "---\nname: custom-tidy\n---\nstale content\n";
    fs::write(&tidy_skill_path, original).unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["init", "--claude"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(
        stdout.contains("updated"),
        "expected 'updated' for overwritten tidy SKILL.md; got: {stdout}"
    );

    let content = fs::read_to_string(&tidy_skill_path).unwrap();
    assert_ne!(
        content, original,
        "tidy SKILL.md should have been overwritten, not preserved"
    );
    assert!(
        content.contains("name: hyalo-tidy"),
        "overwritten tidy SKILL.md should contain canonical name field; got: {content}"
    );
}

// ---------------------------------------------------------------------------
// --claude flag: knowledgebase rule creation
// ---------------------------------------------------------------------------

#[test]
fn init_claude_creates_rule() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["init", "--claude"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    let rule_path = tmp
        .path()
        .join(".claude")
        .join("rules")
        .join("knowledgebase.md");
    assert!(
        rule_path.exists(),
        ".claude/rules/knowledgebase.md should have been created"
    );

    let content = fs::read_to_string(&rule_path).unwrap();
    assert!(
        content.contains("paths:"),
        "rule should contain a paths: key; got: {content}"
    );
    // Should not contain the template placeholder
    assert!(
        !content.contains("hyalo-knowledgebase/**"),
        "rule should have the placeholder replaced; got: {content}"
    );
}

#[test]
fn init_claude_overwrites_existing_rule() {
    // Rule is always overwritten on re-run; summary says "updated".
    let tmp = TempDir::new().unwrap();
    let rules_dir = tmp.path().join(".claude").join("rules");
    fs::create_dir_all(&rules_dir).unwrap();
    let rule_path = rules_dir.join("knowledgebase.md");
    let original = "---\npaths:\n  - \"old-vault/**\"\n---\nold content\n";
    fs::write(&rule_path, original).unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["init", "--claude"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(
        stdout.contains("updated"),
        "expected 'updated' for overwritten rule; got: {stdout}"
    );

    let content = fs::read_to_string(&rule_path).unwrap();
    assert_ne!(
        content, original,
        "rule should have been overwritten, not preserved"
    );
}

#[test]
fn init_claude_rule_uses_detected_dir() {
    // When a docs/ dir has .md files, the rule paths should reference docs/**.
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("docs")).unwrap();
    fs::write(tmp.path().join("docs").join("note.md"), "# Hello").unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["init", "--claude"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    let rule_path = tmp
        .path()
        .join(".claude")
        .join("rules")
        .join("knowledgebase.md");
    let content = fs::read_to_string(&rule_path).unwrap();
    assert!(
        content.contains("docs/**"),
        "rule paths should reference docs/**; got: {content}"
    );
    assert!(
        !content.contains("hyalo-knowledgebase/**"),
        "rule should not contain the placeholder; got: {content}"
    );
}

#[test]
fn init_claude_rule_uses_explicit_dir() {
    // --dir my-vault should be reflected in the rule paths.
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["init", "--claude", "--dir", "my-vault"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    let rule_path = tmp
        .path()
        .join(".claude")
        .join("rules")
        .join("knowledgebase.md");
    let content = fs::read_to_string(&rule_path).unwrap();
    assert!(
        content.contains("my-vault/**"),
        "rule paths should reference my-vault/**; got: {content}"
    );
    assert!(
        !content.contains("hyalo-knowledgebase/**"),
        "rule should not contain the placeholder; got: {content}"
    );
}

// ---------------------------------------------------------------------------
// --claude flag: CLAUDE.md creation and update
// ---------------------------------------------------------------------------

#[test]
fn init_claude_creates_claude_md() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["init", "--claude"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    let claude_md_path = tmp.path().join(".claude").join("CLAUDE.md");
    assert!(
        claude_md_path.exists(),
        ".claude/CLAUDE.md should have been created"
    );

    let content = fs::read_to_string(&claude_md_path).unwrap();
    assert!(
        content.contains("hyalo"),
        ".claude/CLAUDE.md should contain hyalo hint; got: {content}"
    );
    assert!(
        content.contains("<!-- hyalo:start -->"),
        ".claude/CLAUDE.md should contain start marker; got: {content}"
    );
    assert!(
        content.contains("<!-- hyalo:end -->"),
        ".claude/CLAUDE.md should contain end marker; got: {content}"
    );
}

#[test]
fn init_claude_appends_to_existing_claude_md() {
    let tmp = TempDir::new().unwrap();
    let claude_dir = tmp.path().join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    let claude_md_path = claude_dir.join("CLAUDE.md");
    let original = "# Project Instructions\n\nSome existing content.\n";
    fs::write(&claude_md_path, original).unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["init", "--claude"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(
        stdout.contains("updated"),
        "expected 'updated' in stdout; got: {stdout}"
    );

    let content = fs::read_to_string(&claude_md_path).unwrap();
    // Original content must still be present
    assert!(
        content.contains("Some existing content."),
        "original content should be preserved; got: {content}"
    );
    // Hint must have been appended with markers
    assert!(
        content.contains("hyalo"),
        "hyalo hint should have been appended; got: {content}"
    );
    assert!(
        content.contains("<!-- hyalo:start -->"),
        "start marker should be present; got: {content}"
    );
    assert!(
        content.contains("<!-- hyalo:end -->"),
        "end marker should be present; got: {content}"
    );
}

#[test]
fn init_claude_no_duplicate_in_claude_md() {
    let tmp = TempDir::new().unwrap();
    let claude_dir = tmp.path().join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    let claude_md_path = claude_dir.join("CLAUDE.md");
    // Pre-populate with the marker-wrapped section (as a prior run would have written).
    let hint = "Use `hyalo` CLI (not Read/Grep/Glob) for all markdown knowledgebase operations (frontmatter, tags, tasks, search). Run `hyalo --help` for usage. Use `--format text` for compact LLM-friendly output.";
    let original = format!("<!-- hyalo:start -->\n{hint}\n<!-- hyalo:end -->\n");
    fs::write(&claude_md_path, &original).unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["init", "--claude"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(
        stdout.contains("updated"),
        "expected 'updated' (replaced managed section) in stdout; got: {stdout}"
    );

    // The file should contain exactly one copy of the hint and one pair of markers.
    let content = fs::read_to_string(&claude_md_path).unwrap();
    let occurrences = content.matches("hyalo --help").count();
    assert_eq!(
        occurrences, 1,
        "hint should appear exactly once; got {occurrences} times in: {content}"
    );
    assert_eq!(
        content.matches("<!-- hyalo:start -->").count(),
        1,
        "start marker should appear exactly once; got: {content}"
    );
    assert_eq!(
        content.matches("<!-- hyalo:end -->").count(),
        1,
        "end marker should appear exactly once; got: {content}"
    );
}

#[test]
fn init_claude_updates_managed_section_on_rerun() {
    // Run init twice; verify the section is replaced (not duplicated) and
    // surrounding content is preserved.
    let tmp = TempDir::new().unwrap();
    let claude_dir = tmp.path().join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    let claude_md_path = claude_dir.join("CLAUDE.md");
    let surrounding = "# Project Rules\n\nKeep these instructions.\n";
    fs::write(&claude_md_path, surrounding).unwrap();

    // First run — appends section.
    let out1 = hyalo()
        .current_dir(tmp.path())
        .args(["init", "--claude"])
        .output()
        .unwrap();
    let stderr1 = String::from_utf8_lossy(&out1.stderr);
    assert!(out1.status.success(), "first run stderr: {stderr1}");

    // Second run — should replace, not duplicate.
    let out2 = hyalo()
        .current_dir(tmp.path())
        .args(["init", "--claude"])
        .output()
        .unwrap();
    let stdout2 = String::from_utf8_lossy(&out2.stdout);
    let stderr2 = String::from_utf8_lossy(&out2.stderr);
    assert!(out2.status.success(), "second run stderr: {stderr2}");
    assert!(
        stdout2.contains("updated"),
        "second run should report 'updated'; got: {stdout2}"
    );

    let content = fs::read_to_string(&claude_md_path).unwrap();
    // Surrounding content preserved.
    assert!(
        content.contains("Keep these instructions."),
        "surrounding content should be preserved; got: {content}"
    );
    // Exactly one copy of the managed section.
    assert_eq!(
        content.matches("<!-- hyalo:start -->").count(),
        1,
        "start marker should appear exactly once; got: {content}"
    );
    assert_eq!(
        content.matches("<!-- hyalo:end -->").count(),
        1,
        "end marker should appear exactly once; got: {content}"
    );
    assert_eq!(
        content.matches("hyalo --help").count(),
        1,
        "hint should appear exactly once; got: {content}"
    );
}

#[test]
fn init_claude_appends_section_when_no_markers_exist() {
    // Pre-populate CLAUDE.md with plain content (no markers). Init should append
    // the managed section rather than replacing anything.
    let tmp = TempDir::new().unwrap();
    let claude_dir = tmp.path().join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    let claude_md_path = claude_dir.join("CLAUDE.md");
    let original = "# Header\n\nSome existing instructions.\n\n# Footer\n";
    fs::write(&claude_md_path, original).unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["init", "--claude"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(
        stdout.contains("updated"),
        "expected 'updated' in stdout; got: {stdout}"
    );

    let content = fs::read_to_string(&claude_md_path).unwrap();
    assert!(
        content.contains("# Header"),
        "header preserved; got: {content}"
    );
    assert!(
        content.contains("# Footer"),
        "footer preserved; got: {content}"
    );
    assert!(
        content.contains("<!-- hyalo:start -->"),
        "start marker added; got: {content}"
    );
    assert!(
        content.contains("<!-- hyalo:end -->"),
        "end marker added; got: {content}"
    );
    assert!(
        content.contains("hyalo --help"),
        "hint present; got: {content}"
    );
}

// ---------------------------------------------------------------------------
// Summary output
// ---------------------------------------------------------------------------

#[test]
fn init_prints_summary_of_actions() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["init", "--claude"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(!stdout.is_empty(), "stdout should contain a summary");
    // Should mention the toml and the skills at minimum
    assert!(
        stdout.contains(".hyalo.toml"),
        "summary should mention .hyalo.toml; got: {stdout}"
    );
    assert!(
        stdout.contains("skills/hyalo/SKILL.md"),
        "summary should mention hyalo SKILL.md; got: {stdout}"
    );
    assert!(
        stdout.contains("skills/hyalo-tidy/SKILL.md"),
        "summary should mention hyalo-tidy SKILL.md; got: {stdout}"
    );
    assert!(
        stdout.contains("CLAUDE.md"),
        "summary should mention CLAUDE.md; got: {stdout}"
    );
}

#[test]
fn init_summary_mentions_rule() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["init", "--claude"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(
        stdout.contains("rules/knowledgebase.md"),
        "summary should mention .claude/rules/knowledgebase.md; got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Skill parameterization
// ---------------------------------------------------------------------------

#[test]
fn init_claude_tidy_skill_parameterized_with_dir() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["init", "--claude", "--dir", "my-kb"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    let tidy_skill_path = tmp
        .path()
        .join(".claude")
        .join("skills")
        .join("hyalo-tidy")
        .join("SKILL.md");
    let content = fs::read_to_string(&tidy_skill_path).unwrap();

    // Sentinel must have been replaced
    assert!(
        !content.contains("hyalo-knowledgebase"),
        "tidy SKILL.md should not contain sentinel after parameterization; got: {content}"
    );
    // Actual dir name must be present
    assert!(
        content.contains("my-kb"),
        "tidy SKILL.md should contain the configured dir 'my-kb'; got: {content}"
    );
}

#[test]
fn init_claude_hyalo_skill_has_no_sentinel() {
    // The hyalo SKILL.md template doesn't currently use the sentinel — verify
    // that parameterization is a safe no-op (no hyalo-knowledgebase leaks).
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["init", "--claude", "--dir", "custom-docs"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    let skill_path = tmp
        .path()
        .join(".claude")
        .join("skills")
        .join("hyalo")
        .join("SKILL.md");
    let content = fs::read_to_string(&skill_path).unwrap();
    assert!(
        !content.contains("hyalo-knowledgebase"),
        "hyalo SKILL.md should not contain sentinel; got: {content}"
    );
}

// ---------------------------------------------------------------------------
// Fuzzy directory auto-detection
// ---------------------------------------------------------------------------

#[test]
fn init_auto_detects_fuzzy_knowledgebase_dir() {
    let tmp = TempDir::new().unwrap();

    // "my-knowledgebase" contains "knowledgebase" → should be auto-detected
    fs::create_dir_all(tmp.path().join("my-knowledgebase")).unwrap();
    fs::write(
        tmp.path().join("my-knowledgebase").join("index.md"),
        "# Index",
    )
    .unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["init"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    let content = fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
    assert!(
        content.contains("my-knowledgebase"),
        "should auto-detect 'my-knowledgebase'; stdout: {stdout}, toml: {content}"
    );
}

#[test]
fn init_auto_detects_fuzzy_wiki_dir() {
    let tmp = TempDir::new().unwrap();

    fs::create_dir_all(tmp.path().join("project-wiki")).unwrap();
    fs::write(tmp.path().join("project-wiki").join("home.md"), "# Home").unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["init"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    let content = fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
    assert!(
        content.contains("project-wiki"),
        "should auto-detect 'project-wiki'; toml: {content}"
    );
}
