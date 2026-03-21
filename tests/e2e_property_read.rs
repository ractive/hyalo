mod common;

use common::{hyalo, sample_frontmatter, write_md};
use tempfile::TempDir;

#[test]
fn read_text_property() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", sample_frontmatter());

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "read", "--name", "title", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["name"], "title");
    assert_eq!(json["value"], "My Note");
    assert_eq!(json["type"], "text");
}

#[test]
fn read_number_property() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", sample_frontmatter());

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property", "read", "--name", "priority", "--file", "note.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["name"], "priority");
    assert_eq!(json["value"], 3);
    assert_eq!(json["type"], "number");
}

#[test]
fn read_bool_property() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", sample_frontmatter());

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "read", "--name", "draft", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["name"], "draft");
    assert_eq!(json["value"], true);
    assert_eq!(json["type"], "checkbox");
}

#[test]
fn read_date_property() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", sample_frontmatter());

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "read", "--name", "created", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["name"], "created");
    assert_eq!(json["value"], "2026-03-20");
    assert_eq!(json["type"], "date");
}

#[test]
fn read_datetime_property() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", sample_frontmatter());

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "read", "--name", "updated", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["name"], "updated");
    assert_eq!(json["value"], "2026-03-20T14:30:00");
    assert_eq!(json["type"], "datetime");
}

#[test]
fn read_list_property() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", sample_frontmatter());

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "read", "--name", "tags", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["name"], "tags");
    assert_eq!(json["type"], "list");
    let values = json["value"].as_array().unwrap();
    assert_eq!(values.len(), 2);
    assert_eq!(values[0], "rust");
    assert_eq!(values[1], "cli");
}

#[test]
fn read_missing_property() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", sample_frontmatter());

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property",
            "read",
            "--name",
            "nonexistent",
            "--file",
            "note.md",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stderr).unwrap();
    assert_eq!(json["error"], "property not found");
}

#[test]
fn read_missing_file() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property",
            "read",
            "--name",
            "title",
            "--file",
            "nonexistent.md",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stderr).unwrap();
    assert_eq!(json["error"], "file not found");
}

#[test]
fn read_missing_extension_hint() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", sample_frontmatter());

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "read", "--name", "title", "--file", "note"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stderr).unwrap();
    assert_eq!(json["error"], "file not found");
    let hint = json["hint"].as_str().unwrap();
    assert!(hint.contains("note.md"));
}

#[test]
fn read_text_format() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", sample_frontmatter());

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
            "note.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Text format shows: name (type): value
    assert!(stdout.contains("title"));
    assert!(stdout.contains("My Note"));
}
