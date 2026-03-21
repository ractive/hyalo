mod common;

use common::{hyalo, md, write_md};
use tempfile::TempDir;

#[test]
fn error_nonexistent_file() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property",
            "read",
            "--name",
            "title",
            "--file",
            "missing.md",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stderr).unwrap();
    assert_eq!(json["error"], "file not found");
    assert!(json.get("path").is_some());
}

#[test]
fn error_nonexistent_dir() {
    let tmp = TempDir::new().unwrap();
    let nonexistent = tmp.path().join("does_not_exist");

    let output = hyalo()
        .args(["--dir", nonexistent.to_str().unwrap()])
        .args(["properties"])
        .output()
        .unwrap();

    // Should fail with exit code 2 (anyhow error path)
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(!stderr.is_empty());
}

#[test]
fn error_invalid_yaml() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "bad.md",
        md!(r"
---
: invalid yaml [[[{
---
# Body
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "--glob", "bad.md"])
        .output()
        .unwrap();

    // Malformed YAML should cause an error
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(!stderr.is_empty());
}

#[test]
fn error_missing_md_extension() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Test
---
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "read", "--name", "title", "--file", "note"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stderr).unwrap();
    assert_eq!(json["error"], "file not found");
    assert!(json["hint"].as_str().unwrap().contains("note.md"));
}

#[test]
fn error_json_structure() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "read", "--name", "title", "--file", "nope.md"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stderr).unwrap();

    // Error JSON must have an "error" field
    assert!(json.get("error").is_some());
    assert!(json["error"].is_string());
}

#[test]
fn error_text_format() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "--format",
            "text",
            "property",
            "read",
            "--name",
            "title",
            "--file",
            "nope.md",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("Error:"));
    assert!(stderr.contains("file not found"));
}
