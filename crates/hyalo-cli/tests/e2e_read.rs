mod common;

use common::{hyalo, md, write_md};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

fn setup() -> TempDir {
    let tmp = TempDir::new().unwrap();

    write_md(
        tmp.path(),
        "note.md",
        md!(r#"
---
title: Test Note
status: draft
tags:
  - cli
  - rust
---
# Heading One

First paragraph.

## Problem

Problem text line 1.
Problem text line 2.

## Solution

Solution text.

### Details

Nested details.

## Problem

Second problem section.
"#),
    );

    write_md(
        tmp.path(),
        "no-frontmatter.md",
        md!(r#"
# Just a file

No frontmatter here.
"#),
    );

    write_md(tmp.path(), "empty.md", "");

    tmp
}

// ---------------------------------------------------------------------------
// Basic read
// ---------------------------------------------------------------------------

#[test]
fn read_full_body_text() {
    let tmp = setup();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("# Heading One"));
    assert!(stdout.contains("First paragraph."));
    assert!(stdout.contains("## Problem"));
    assert!(stdout.contains("## Solution"));
    // Should NOT contain frontmatter
    assert!(!stdout.contains("title: Test Note"));
    assert!(!stdout.contains("---"));
}

#[test]
fn read_full_body_json() {
    let tmp = setup();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "--file", "note.md", "--format", "json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["file"], "note.md");
    let content = json["content"].as_str().unwrap();
    assert!(content.contains("# Heading One"));
    assert!(content.contains("Problem text"));
    // No frontmatter key unless --frontmatter
    assert!(json.get("frontmatter").is_none());
}

#[test]
fn read_defaults_to_text_format() {
    let tmp = setup();
    // Without --format, read should output plain text (not JSON)
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Text output should NOT be valid JSON
    assert!(serde_json::from_str::<serde_json::Value>(&stdout).is_err());
    assert!(stdout.contains("# Heading One"));
}

// ---------------------------------------------------------------------------
// --section
// ---------------------------------------------------------------------------

#[test]
fn read_section_exact_match() {
    let tmp = setup();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "--file", "note.md", "--section", "Solution"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("## Solution"));
    assert!(stdout.contains("Solution text."));
    assert!(stdout.contains("### Details"));
    assert!(stdout.contains("Nested details."));
    // Should NOT contain Problem section
    assert!(!stdout.contains("Problem text"));
}

#[test]
fn read_section_with_hashes() {
    let tmp = setup();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "--file", "note.md", "--section", "## Solution"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("## Solution"));
    assert!(stdout.contains("Solution text."));
}

#[test]
fn read_section_case_insensitive() {
    let tmp = setup();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "--file", "note.md", "--section", "solution"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("## Solution"));
}

#[test]
fn read_section_multiple_matches() {
    let tmp = setup();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "--file", "note.md", "--section", "Problem"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Problem text line 1."));
    assert!(stdout.contains("Second problem section."));
}

#[test]
fn read_section_no_match() {
    let tmp = setup();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "--file", "note.md", "--section", "Nonexistent"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("section not found"));
}

#[test]
fn read_section_no_substring_match() {
    let tmp = setup();
    // "XYZ" is not a substring of any heading in note.md
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "--file", "note.md", "--section", "XYZ"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("section not found"));
}

#[test]
fn read_section_substring_match() {
    let tmp = setup();
    // "Prob" is a substring of "Problem" — should now succeed
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "--file", "note.md", "--section", "Prob"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let body = String::from_utf8(output.stdout).unwrap();
    assert!(body.contains("Problem text line 1"));
}

#[test]
fn read_section_with_count_suffix() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "tasks.md",
        md!(r"
# My File

## Tasks [4/4]

- [x] Done task
"),
    );
    // 'Tasks' is a substring of 'Tasks [4/4]'
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "--file", "tasks.md", "--section", "Tasks"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let body = String::from_utf8(output.stdout).unwrap();
    assert!(body.contains("Done task"));
}

// ---------------------------------------------------------------------------
// --lines
// ---------------------------------------------------------------------------

#[test]
fn read_lines_range() {
    let tmp = setup();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "read", "--file", "note.md", "--lines", "1:3", "--format", "json",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let content = json["content"].as_str().unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines[0], "# Heading One");
    assert_eq!(lines[2], "First paragraph.");
    assert_eq!(lines.len(), 3);
}

#[test]
fn read_lines_single() {
    let tmp = setup();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "read", "--file", "note.md", "--lines", "1", "--format", "json",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let content = json["content"].as_str().unwrap();
    assert_eq!(content, "# Heading One");
}

#[test]
fn read_lines_open_end() {
    let tmp = setup();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "--file", "note.md", "--lines", "1:"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Should contain all body lines
    assert!(stdout.contains("# Heading One"));
    assert!(stdout.contains("Second problem section."));
}

#[test]
fn read_lines_open_start() {
    let tmp = setup();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "--file", "note.md", "--lines", ":2"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2);
}

#[test]
fn read_lines_invalid() {
    let tmp = setup();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "--file", "note.md", "--lines", "abc"])
        .output()
        .unwrap();

    assert!(!output.status.success());
}

// ---------------------------------------------------------------------------
// --frontmatter
// ---------------------------------------------------------------------------

#[test]
fn read_frontmatter_only() {
    let tmp = setup();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "--file", "note.md", "--frontmatter"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("---"));
    assert!(stdout.contains("title: Test Note"));
    // Should NOT contain body
    assert!(!stdout.contains("# Heading One"));
}

#[test]
fn read_frontmatter_with_section() {
    let tmp = setup();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "read",
            "--file",
            "note.md",
            "--frontmatter",
            "--section",
            "Solution",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("title: Test Note"));
    assert!(stdout.contains("## Solution"));
}

#[test]
fn read_frontmatter_json() {
    let tmp = setup();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "read",
            "--file",
            "note.md",
            "--frontmatter",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["frontmatter"]["title"], "Test Note");
    assert_eq!(json["frontmatter"]["status"], "draft");
}

#[test]
fn read_frontmatter_with_lines() {
    let tmp = setup();
    // --frontmatter + --lines should show frontmatter + sliced body
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "read",
            "--file",
            "note.md",
            "--frontmatter",
            "--lines",
            "1:2",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("title: Test Note"));
    assert!(stdout.contains("# Heading One"));
}

// ---------------------------------------------------------------------------
// Error cases
// ---------------------------------------------------------------------------

#[test]
fn read_file_not_found() {
    let tmp = setup();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "--file", "missing.md"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("file not found"));
}

#[test]
fn read_file_requires_file_flag() {
    let tmp = setup();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read"])
        .output()
        .unwrap();

    // clap should reject this
    assert!(!output.status.success());
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

#[test]
fn read_no_frontmatter_file() {
    let tmp = setup();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "--file", "no-frontmatter.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("# Just a file"));
    assert!(stdout.contains("No frontmatter here."));
}

#[test]
fn read_empty_file() {
    let tmp = setup();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "--file", "empty.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
}

#[test]
fn read_frontmatter_only_file_returns_empty_body() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "fm-only.md",
        "---\ntitle: Frontmatter Only\nstatus: draft\n---\n",
    );

    // Text output: body should be empty
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "--file", "fm-only.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.trim().is_empty(),
        "expected empty body for frontmatter-only file, got: {stdout:?}"
    );

    // JSON output: content field should be empty
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "--file", "fm-only.md", "--format", "json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "invalid JSON: {e}\nstdout: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    let content = json["content"]
        .as_str()
        .expect("field 'content' should be a string");
    assert!(
        content.trim().is_empty(),
        "expected empty content for frontmatter-only file, got: {content:?}"
    );
}

#[test]
fn read_with_jq_explicit_json() {
    let tmp = setup();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "read", "--file", "note.md", "--format", "json", "--jq", ".file",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.trim(), "note.md");
}

#[test]
fn read_with_jq_auto_promotes_to_json() {
    let tmp = setup();
    // --jq without --format json should auto-promote to JSON (not error)
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "--file", "note.md", "--jq", ".file"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.trim(), "note.md");
}

#[test]
fn read_section_json_output() {
    let tmp = setup();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "read",
            "--file",
            "note.md",
            "--section",
            "Solution",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let content = json["content"].as_str().unwrap();
    assert!(content.contains("Solution text."));
    assert!(content.contains("Nested details."));
}

// ---------------------------------------------------------------------------
// --frontmatter with broken frontmatter
// ---------------------------------------------------------------------------

/// `read --frontmatter` on a file with no closing `---` must error, not silently
/// fabricate a result.
#[test]
fn read_frontmatter_broken_errors() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "broken.md",
        "---\ntitle: Unclosed\nNo closing delimiter here\n",
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "--file", "broken.md", "--frontmatter"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected failure for broken frontmatter; stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("unclosed frontmatter"),
        "expected 'unclosed frontmatter' error; got: {stderr}"
    );
}

/// `read --frontmatter` on a file with valid frontmatter must still work correctly.
#[test]
fn read_frontmatter_valid_works() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "valid.md",
        "---\ntitle: Good File\nstatus: ok\n---\n# Body\n",
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "--file", "valid.md", "--frontmatter"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "expected success for valid frontmatter; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("title: Good File"),
        "expected frontmatter in output; got: {stdout}"
    );
}
