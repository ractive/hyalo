mod common;

use common::{hyalo, md, write_md};
use tempfile::TempDir;

#[test]
fn remove_existing_property() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!("---
title: Test
status: draft
---
# Body
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property", "remove", "--name", "status", "--file", "note.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["removed"], "status");
    assert_eq!(json["path"], "note.md");

    // Verify it's gone
    let read_output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "read", "--name", "status", "--file", "note.md"])
        .output()
        .unwrap();
    assert_eq!(read_output.status.code(), Some(1));
}

#[test]
fn remove_missing_property() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!("---
title: Test
---
# Body
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property",
            "remove",
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
fn remove_preserves_body() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!("---
title: Test
status: draft
---
# Heading

Paragraph content.
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property", "remove", "--name", "status", "--file", "note.md",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());

    let file_content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(file_content.contains("# Heading"));
    assert!(file_content.contains("Paragraph content."));
}

#[test]
fn remove_preserves_other_properties() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!("---
title: Keep
status: draft
priority: 5
---
# Body
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property", "remove", "--name", "status", "--file", "note.md",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());

    // title should still be there
    let read_title = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "read", "--name", "title", "--file", "note.md"])
        .output()
        .unwrap();
    assert!(read_title.status.success());
    let title_json: serde_json::Value = serde_json::from_slice(&read_title.stdout).unwrap();
    assert_eq!(title_json["value"], "Keep");

    // priority should still be there
    let read_priority = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property", "read", "--name", "priority", "--file", "note.md",
        ])
        .output()
        .unwrap();
    assert!(read_priority.status.success());
    let priority_json: serde_json::Value = serde_json::from_slice(&read_priority.stdout).unwrap();
    assert_eq!(priority_json["value"], 5);
}

#[test]
fn remove_last_property() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!("---
only_prop: value
---
# Body
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property",
            "remove",
            "--name",
            "only_prop",
            "--file",
            "note.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());

    // File should still be readable and have no properties
    let props_output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "--glob", "note.md"])
        .output()
        .unwrap();
    assert!(props_output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&props_output.stdout).unwrap();
    let props = json["properties"].as_object().unwrap();
    assert!(props.is_empty());
}
