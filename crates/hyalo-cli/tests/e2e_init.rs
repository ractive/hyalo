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
        ".hyalo.toml should not have been modified"
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
fn init_claude_skips_existing_skill() {
    let tmp = TempDir::new().unwrap();
    let skill_dir = tmp.path().join(".claude").join("skills").join("hyalo");
    fs::create_dir_all(&skill_dir).unwrap();
    let skill_path = skill_dir.join("SKILL.md");
    let original = "---\nname: custom\n---\n";
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
        stdout.contains("skipped"),
        "expected 'skipped' in stdout for existing SKILL.md; got: {stdout}"
    );

    // Content should not have been overwritten
    let content = fs::read_to_string(&skill_path).unwrap();
    assert_eq!(content, original);
}

// ---------------------------------------------------------------------------
// --claude flag: hyalo-dream skill creation
// ---------------------------------------------------------------------------

#[test]
fn init_claude_creates_dream_skill() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["init", "--claude"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    let dream_skill_path = tmp
        .path()
        .join(".claude")
        .join("skills")
        .join("hyalo-dream")
        .join("SKILL.md");
    assert!(
        dream_skill_path.exists(),
        "hyalo-dream SKILL.md should have been created"
    );

    let content = fs::read_to_string(&dream_skill_path).unwrap();
    assert!(
        content.contains("name: hyalo-dream"),
        "SKILL.md should have frontmatter name field; got: {content}"
    );
    assert!(
        content.contains("Knowledgebase Consolidation"),
        "SKILL.md should contain dream skill content"
    );
}

#[test]
fn init_claude_skips_existing_dream_skill() {
    let tmp = TempDir::new().unwrap();
    let dream_skill_dir = tmp
        .path()
        .join(".claude")
        .join("skills")
        .join("hyalo-dream");
    fs::create_dir_all(&dream_skill_dir).unwrap();
    let dream_skill_path = dream_skill_dir.join("SKILL.md");
    let original = "---\nname: custom-dream\n---\n";
    fs::write(&dream_skill_path, original).unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["init", "--claude"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(
        stdout.contains("skipped"),
        "expected 'skipped' for existing dream SKILL.md; got: {stdout}"
    );

    let content = fs::read_to_string(&dream_skill_path).unwrap();
    assert_eq!(
        content, original,
        "dream SKILL.md should not be overwritten"
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
    // Hint must have been appended
    assert!(
        content.contains("hyalo"),
        "hyalo hint should have been appended; got: {content}"
    );
}

#[test]
fn init_claude_no_duplicate_in_claude_md() {
    let tmp = TempDir::new().unwrap();
    let claude_dir = tmp.path().join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    let claude_md_path = claude_dir.join("CLAUDE.md");
    // Pre-populate with the exact hint line
    let original = "Use `hyalo` CLI (not Read/Grep/Glob) for all markdown knowledgebase operations (frontmatter, tags, tasks, search). Run `hyalo --help` for usage. Use `--format text` for compact LLM-friendly output.\n";
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
        stdout.contains("skipped"),
        "expected 'skipped' when hint already present; got: {stdout}"
    );

    // The file should still contain exactly one copy of the hint
    let content = fs::read_to_string(&claude_md_path).unwrap();
    let occurrences = content.matches("hyalo --help").count();
    assert_eq!(
        occurrences, 1,
        "hint should appear exactly once; got {occurrences} times in: {content}"
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
    // Should mention the toml and the skill at minimum
    assert!(
        stdout.contains(".hyalo.toml"),
        "summary should mention .hyalo.toml; got: {stdout}"
    );
    assert!(
        stdout.contains("skills/hyalo/SKILL.md"),
        "summary should mention hyalo SKILL.md; got: {stdout}"
    );
    assert!(
        stdout.contains("skills/hyalo-dream/SKILL.md"),
        "summary should mention hyalo-dream SKILL.md; got: {stdout}"
    );
    assert!(
        stdout.contains("CLAUDE.md"),
        "summary should mention CLAUDE.md; got: {stdout}"
    );
}
