mod common;

use common::{hyalo, md, write_md};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Write a markdown file with the given YAML frontmatter string (without delimiters).
fn write_with_frontmatter(dir: &std::path::Path, name: &str, yaml: &str) {
    write_md(dir, name, &format!("---\n{yaml}---\n# Body\n"));
}

// ---------------------------------------------------------------------------
// Happy path tests
// ---------------------------------------------------------------------------

#[test]
fn property_find_by_existence() {
    let tmp = TempDir::new().unwrap();
    write_with_frontmatter(tmp.path(), "a.md", "status: draft\ntitle: A\n");
    write_with_frontmatter(tmp.path(), "b.md", "title: B\n"); // no status
    write_md(tmp.path(), "c.md", "No frontmatter.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "find", "--name", "status"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 1);
    let files = json["files"].as_array().unwrap();
    assert!(files[0].as_str().unwrap().contains("a.md"));
}

#[test]
fn property_find_by_value_string() {
    let tmp = TempDir::new().unwrap();
    write_with_frontmatter(tmp.path(), "a.md", "status: draft\n");
    write_with_frontmatter(tmp.path(), "b.md", "status: done\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "find", "--name", "status", "--value", "draft"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 1);
    assert!(json["files"][0].as_str().unwrap().contains("a.md"));
}

#[test]
fn property_find_by_value_number() {
    let tmp = TempDir::new().unwrap();
    write_with_frontmatter(tmp.path(), "a.md", "priority: 3\n");
    write_with_frontmatter(tmp.path(), "b.md", "priority: 5\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "find", "--name", "priority", "--value", "3"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 1);
    assert!(json["files"][0].as_str().unwrap().contains("a.md"));
}

#[test]
fn property_find_by_value_boolean() {
    let tmp = TempDir::new().unwrap();
    write_with_frontmatter(tmp.path(), "a.md", "draft: true\n");
    write_with_frontmatter(tmp.path(), "b.md", "draft: false\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "find", "--name", "draft", "--value", "true"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 1);
    assert!(json["files"][0].as_str().unwrap().contains("a.md"));
}

#[test]
fn property_find_in_list_value() {
    let tmp = TempDir::new().unwrap();
    write_with_frontmatter(tmp.path(), "a.md", "tags:\n  - rust\n  - cli\n");
    write_with_frontmatter(tmp.path(), "b.md", "tags:\n  - python\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "find", "--name", "tags", "--value", "rust"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 1);
    assert!(json["files"][0].as_str().unwrap().contains("a.md"));
}

#[test]
fn property_find_case_insensitive_value() {
    let tmp = TempDir::new().unwrap();
    write_with_frontmatter(tmp.path(), "a.md", "status: Draft\n");
    write_with_frontmatter(tmp.path(), "b.md", "status: done\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "find", "--name", "status", "--value", "draft"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 1);
    assert!(json["files"][0].as_str().unwrap().contains("a.md"));
}

#[test]
fn property_find_with_glob_filter() {
    let tmp = TempDir::new().unwrap();
    write_with_frontmatter(tmp.path(), "sub/a.md", "status: draft\n");
    write_with_frontmatter(tmp.path(), "root.md", "status: draft\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "find", "--name", "status", "--glob", "sub/*.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 1);
    let files: Vec<&str> = json["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(files.iter().any(|f| f.contains("sub/a.md")));
    assert!(!files.iter().any(|f| f.contains("root.md")));
}

#[test]
fn property_find_with_file_filter() {
    let tmp = TempDir::new().unwrap();
    write_with_frontmatter(tmp.path(), "a.md", "status: draft\n");
    write_with_frontmatter(tmp.path(), "b.md", "status: draft\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "find", "--name", "status", "--file", "a.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 1);
    assert!(json["files"][0].as_str().unwrap().contains("a.md"));
}

#[test]
fn property_find_text_format() {
    let tmp = TempDir::new().unwrap();
    write_with_frontmatter(tmp.path(), "note.md", "status: draft\n");

    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "--format",
            "text",
            "property",
            "find",
            "--name",
            "status",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("note.md"));
}

#[test]
fn property_find_json_format_structure() {
    let tmp = TempDir::new().unwrap();
    write_with_frontmatter(tmp.path(), "note.md", "status: draft\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "find", "--name", "status", "--value", "draft"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // Verify all required fields are present
    assert!(json["property"].is_string());
    assert_eq!(json["property"], "status");
    assert!(json["value"].is_string());
    assert_eq!(json["value"], "draft");
    assert!(json["files"].is_array());
    assert!(json["total"].is_number());
}

// ---------------------------------------------------------------------------
// Unhappy path tests
// ---------------------------------------------------------------------------

#[test]
fn property_find_no_match_returns_success() {
    let tmp = TempDir::new().unwrap();
    write_with_frontmatter(tmp.path(), "note.md", "status: done\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property",
            "find",
            "--name",
            "status",
            "--value",
            "nonexistent",
        ])
        .output()
        .unwrap();

    // No match is still exit 0 with total: 0
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 0);
    assert!(json["files"].as_array().unwrap().is_empty());
}

#[test]
fn property_find_nonexistent_file() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "find", "--name", "status", "--file", "nope.md"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("file not found") || stderr.contains("not found"),
        "stderr: {stderr}"
    );
}

#[test]
fn property_find_glob_no_match() {
    let tmp = TempDir::new().unwrap();
    write_with_frontmatter(tmp.path(), "note.md", "status: draft\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property",
            "find",
            "--name",
            "status",
            "--glob",
            "nonexistent/*.md",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
}

#[test]
fn property_find_empty_vault() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "find", "--name", "status"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 0);
    assert!(json["files"].as_array().unwrap().is_empty());
}

#[test]
fn property_find_no_frontmatter_files() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "plain.md", "No frontmatter here.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "find", "--name", "status"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 0);
}

#[test]
fn property_find_value_type_mismatch() {
    let tmp = TempDir::new().unwrap();
    // priority is a number, searching with a non-numeric string → no match (not an error)
    write_with_frontmatter(tmp.path(), "note.md", "priority: 3\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "find", "--name", "priority", "--value", "high"])
        .output()
        .unwrap();

    // Not an error — just no match
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 0);
}

#[test]
fn property_find_json_value_is_null_when_no_value_filter() {
    let tmp = TempDir::new().unwrap();
    write_with_frontmatter(tmp.path(), "note.md", "status: draft\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "find", "--name", "status"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // value field should be null when no --value was given
    assert!(
        json["value"].is_null(),
        "expected null, got: {}",
        json["value"]
    );
}

#[test]
fn property_find_multiple_files_matching() {
    let tmp = TempDir::new().unwrap();
    write_with_frontmatter(tmp.path(), "a.md", "status: draft\n");
    write_with_frontmatter(tmp.path(), "b.md", "status: draft\n");
    write_with_frontmatter(tmp.path(), "c.md", "status: done\n");
    write_md(tmp.path(), "d.md", "No frontmatter.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "find", "--name", "status", "--value", "draft"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 2);
    let files: Vec<&str> = json["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(files.iter().any(|f| f.contains("a.md")));
    assert!(files.iter().any(|f| f.contains("b.md")));
    assert!(!files.iter().any(|f| f.contains("c.md")));
    assert!(!files.iter().any(|f| f.contains("d.md")));
}

// ---------------------------------------------------------------------------
// Verify the md! macro is available (it's used in some e2e tests)
// ---------------------------------------------------------------------------

#[test]
fn property_find_with_complex_frontmatter() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: My Note
status: draft
priority: 2
draft: true
tags:
  - rust
  - cli
---
# Body

Some content.
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "find", "--name", "tags", "--value", "cli"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 1);
}
