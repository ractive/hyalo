mod common;

use common::{hyalo, md, sample_frontmatter, write_md};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// `hyalo properties` (bare) — defaults to summary
// ---------------------------------------------------------------------------

#[test]
fn properties_aggregate() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        md!(r"
---
title: A
status: draft
---
# A
"),
    );
    write_md(
        tmp.path(),
        "b.md",
        md!(r"
---
title: B
priority: 1
---
# B
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();

    // Should have aggregated properties: title (2), status (1), priority (1)
    let names: Vec<&str> = json.iter().map(|v| v["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"title"));
    assert!(names.contains(&"status"));
    assert!(names.contains(&"priority"));

    // title appears in both files
    let title_entry = json.iter().find(|v| v["name"] == "title").unwrap();
    assert_eq!(title_entry["count"], 2);
    assert_eq!(title_entry["type"], "text");

    // status appears in one file
    let status_entry = json.iter().find(|v| v["name"] == "status").unwrap();
    assert_eq!(status_entry["count"], 1);
}

#[test]
fn properties_empty_dir() {
    let tmp = TempDir::new().unwrap();
    // No .md files at all

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json.is_empty());
}

// ---------------------------------------------------------------------------
// `hyalo properties summary` — explicit summary subcommand
// ---------------------------------------------------------------------------

#[test]
fn properties_summary_explicit() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        md!(r"
---
title: A
status: draft
---
"),
    );
    write_md(
        tmp.path(),
        "b.md",
        md!(r"
---
title: B
priority: 1
---
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "summary"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    let names: Vec<&str> = json.iter().map(|v| v["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"title"));
    assert!(names.contains(&"status"));
}

#[test]
fn properties_summary_with_glob() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "root.md",
        md!(r"
---
title: Root
---
"),
    );
    write_md(
        tmp.path(),
        "sub/a.md",
        md!(r"
---
title: Sub A
only_in_sub: yes
---
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "summary", "--glob", "sub/*.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    let names: Vec<&str> = json.iter().map(|v| v["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"only_in_sub"));
    // root.md was excluded, so root-only properties should not appear
    // (both files share "title", so it may still appear)
}

#[test]
fn properties_summary_with_file() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", sample_frontmatter());
    write_md(
        tmp.path(),
        "other.md",
        md!(r"
---
title: Other
only_in_other: true
---
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "summary", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    let names: Vec<&str> = json.iter().map(|v| v["name"].as_str().unwrap()).collect();
    // only_in_other should not appear — note.md was the only file scanned
    assert!(!names.contains(&"only_in_other"));
    assert!(names.contains(&"title"));
}

// ---------------------------------------------------------------------------
// `hyalo properties list` — per-file detail subcommand
// ---------------------------------------------------------------------------

#[test]
fn properties_list_single_file() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "file.md", sample_frontmatter());

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "list", "--file", "file.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["path"], "file.md");

    let props = json["properties"].as_array().unwrap();
    let find_prop = |name: &str| props.iter().find(|p| p["name"] == name).unwrap();
    assert_eq!(find_prop("title")["type"], "text");
    assert_eq!(find_prop("title")["value"], "My Note");
    assert_eq!(find_prop("priority")["type"], "number");
    assert_eq!(find_prop("priority")["value"], 3);
    assert_eq!(find_prop("draft")["type"], "checkbox");
    assert_eq!(find_prop("draft")["value"], true);
    assert_eq!(find_prop("created")["type"], "date");
    assert_eq!(find_prop("tags")["type"], "list");
}

#[test]
fn properties_list_with_glob() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "root.md",
        md!(r"
---
title: Root
---
"),
    );
    write_md(
        tmp.path(),
        "sub/a.md",
        md!(r"
---
title: Sub A
---
"),
    );
    write_md(
        tmp.path(),
        "sub/b.md",
        md!(r"
---
title: Sub B
---
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "list", "--glob", "sub/*.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json.len(), 2);

    let paths: Vec<&str> = json.iter().map(|v| v["path"].as_str().unwrap()).collect();
    assert!(paths.iter().all(|p| p.starts_with("sub/")));
}

#[test]
fn properties_list_all_files() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        md!(r"
---
title: A
---
"),
    );
    write_md(
        tmp.path(),
        "b.md",
        md!(r"
---
status: draft
---
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "list"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json.len(), 2);
    // Each entry has path and properties
    assert!(json.iter().all(|e| e["path"].is_string()));
    assert!(json.iter().all(|e| e["properties"].is_array()));
}

#[test]
fn properties_list_file_without_frontmatter() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "plain.md", "Just a plain markdown file.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "list", "--file", "plain.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["path"], "plain.md");
    let props = json["properties"].as_array().unwrap();
    assert!(props.is_empty());
}

#[test]
fn properties_list_text_format() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Hello
status: draft
---
"),
    );

    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "--format",
            "text",
            "properties",
            "list",
            "--file",
            "note.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    // File path header
    assert!(stdout.contains("note.md"));
    // Properties are listed with name (type): value format
    assert!(stdout.contains("title"));
    assert!(stdout.contains("Hello"));
    assert!(stdout.contains("status"));
    assert!(stdout.contains("draft"));
}

#[test]
fn properties_summary_text_format() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        md!(r"
---
title: A
status: draft
---
"),
    );
    write_md(
        tmp.path(),
        "b.md",
        md!(r"
---
title: B
---
"),
    );

    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "--format",
            "text",
            "properties",
            "summary",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Properties listed with name, type, and count
    assert!(stdout.contains("title"));
    assert!(stdout.contains("text"));
    assert!(stdout.contains("2 files"));
    assert!(stdout.contains("status"));
}

#[test]
fn properties_rejects_parent_glob_with_subcommand() {
    let tmp = TempDir::new().unwrap();
    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "properties",
            "--glob",
            "*.md",
            "list",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("--file/--glob must be placed after the subcommand"));
}
