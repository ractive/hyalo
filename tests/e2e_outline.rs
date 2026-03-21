mod common;

use common::{hyalo, md, write_md};

fn setup_vault() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: My Note
tags:
  - rust
  - cli
---
# Introduction

See [[other-note]] for context.

## Tasks

- [ ] Write tests
- [x] Write code
- Regular bullet

```rust
fn main() {}
```
"),
    );
    write_md(
        tmp.path(),
        "plain.md",
        md!(r"
# Plain Heading

Just text here.
"),
    );
    write_md(tmp.path(), "empty.md", "");
    write_md(
        tmp.path(),
        "sub/nested.md",
        md!(r"
---
title: Nested
---
# Nested Section

[[note]]
"),
    );
    tmp
}

// --- outline --file (single file, returns bare object) ---

#[test]
fn outline_single_file_json() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["outline", "--file", "note.md"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    // Bare object, not an array
    assert!(parsed.is_object());
    assert_eq!(parsed["file"], "note.md");

    // Properties use the same shape as `properties list`
    let props = parsed["properties"].as_array().unwrap();
    let title_prop = props.iter().find(|p| p["name"] == "title").unwrap();
    assert_eq!(title_prop["type"], "text");
    assert_eq!(title_prop["value"], "My Note");

    // Tags
    let tags = parsed["tags"].as_array().unwrap();
    assert!(tags.contains(&serde_json::json!("rust")));
    assert!(tags.contains(&serde_json::json!("cli")));

    // Sections
    let sections = parsed["sections"].as_array().unwrap();
    assert_eq!(sections.len(), 2); // Introduction + Tasks

    // Introduction section
    let intro = &sections[0];
    assert_eq!(intro["level"], 1);
    assert_eq!(intro["heading"], "Introduction");
    let intro_links = intro["links"].as_array().unwrap();
    assert_eq!(intro_links.len(), 1);
    assert_eq!(intro_links[0], "[[other-note]]");
    assert!(intro["tasks"].is_null()); // no tasks in this section

    // Tasks section
    let tasks_section = &sections[1];
    assert_eq!(tasks_section["level"], 2);
    assert_eq!(tasks_section["heading"], "Tasks");
    assert_eq!(tasks_section["tasks"]["total"], 2);
    assert_eq!(tasks_section["tasks"]["done"], 1);
    let code_blocks = tasks_section["code_blocks"].as_array().unwrap();
    assert_eq!(code_blocks.len(), 1);
    assert_eq!(code_blocks[0], "rust");
}

// --- outline --glob (multi-file, returns array) ---

#[test]
fn outline_glob_returns_array() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["outline", "--glob", "*.md"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    // Array of file outlines
    assert!(parsed.is_array());
    let arr = parsed.as_array().unwrap();
    // *.md matches only top-level files (literal_separator prevents * crossing /)
    assert_eq!(arr.len(), 3);

    // Each element has the FileOutline shape
    for item in arr {
        assert!(item["file"].is_string());
        assert!(item["properties"].is_array());
        assert!(item["tags"].is_array());
        assert!(item["sections"].is_array());
    }
}

// --- outline vault-wide (no --file or --glob) ---

#[test]
fn outline_vault_wide_returns_array() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["outline"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert!(parsed.is_array());
    let arr = parsed.as_array().unwrap();
    // All 4 files: note.md, plain.md, empty.md, sub/nested.md
    assert_eq!(arr.len(), 4);
}

// --- File not found ---

#[test]
fn outline_file_not_found() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["outline", "--file", "nonexistent.md"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stderr).unwrap();
    assert_eq!(parsed["error"], "file not found");
}

// --- No frontmatter ---

#[test]
fn outline_no_frontmatter() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["outline", "--file", "plain.md"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert!(parsed["properties"].as_array().unwrap().is_empty());
    assert!(parsed["tags"].as_array().unwrap().is_empty());
    let sections = parsed["sections"].as_array().unwrap();
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0]["heading"], "Plain Heading");
}

// --- Empty file ---

#[test]
fn outline_empty_file() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["outline", "--file", "empty.md"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert!(parsed["properties"].as_array().unwrap().is_empty());
    assert!(parsed["tags"].as_array().unwrap().is_empty());
    assert!(parsed["sections"].as_array().unwrap().is_empty());
}

// --- Pre-heading section only emitted with content ---

#[test]
fn outline_pre_heading_section_with_links() {
    let tmp = tempfile::tempdir().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
See [[some-note]] for details.

# Heading
"),
    );
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["outline", "--file", "note.md"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let sections = parsed["sections"].as_array().unwrap();
    assert_eq!(sections.len(), 2); // pre-heading + heading
    assert_eq!(sections[0]["level"], 0);
    assert!(sections[0]["heading"].is_null());
    assert_eq!(sections[0]["links"].as_array().unwrap().len(), 1);
}

#[test]
fn outline_no_pre_heading_section_when_only_text() {
    let tmp = tempfile::tempdir().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Test
---
# Heading

Text here.
"),
    );
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["outline", "--file", "note.md"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let sections = parsed["sections"].as_array().unwrap();
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0]["level"], 1);
}

// --- Text format ---

#[test]
fn outline_text_format() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["--format", "text"])
        .args(["outline", "--file", "note.md"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    // File path appears at the top
    assert!(stdout.contains("note.md"));
    // Tags appear in output
    assert!(stdout.contains("rust"));
    assert!(stdout.contains("cli"));
    // Headings appear with # prefix
    assert!(stdout.contains("# Introduction"));
    assert!(stdout.contains("## Tasks"));
    // Task counts appear
    assert!(stdout.contains("[1/2]"));
}

// --- Glob no match ---

#[test]
fn outline_glob_no_match() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["outline", "--glob", "nonexistent/**/*.md"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stderr).unwrap();
    assert_eq!(parsed["error"], "no files match pattern");
}

// --- Code blocks inside code fences not counted as headings ---

#[test]
fn outline_heading_inside_code_block_ignored() {
    let tmp = tempfile::tempdir().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
# Real Heading

```markdown
# Fake heading inside code block
```
"),
    );
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["outline", "--file", "note.md"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let sections = parsed["sections"].as_array().unwrap();
    assert_eq!(sections.len(), 1); // Only the real heading
    assert_eq!(sections[0]["heading"], "Real Heading");
}
