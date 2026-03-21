mod common;

use common::{hyalo, md, write_md};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn write_with_list(dir: &std::path::Path, name: &str, prop: &str, items: &[&str]) {
    let list_yaml = if items.is_empty() {
        format!("{prop}: []\n")
    } else {
        let rows = items.iter().fold(String::new(), |mut s, v| {
            use std::fmt::Write as _;
            let _ = writeln!(s, "  - {v}");
            s
        });
        format!("{prop}:\n{rows}")
    };
    write_md(dir, name, &format!("---\ntitle: {name}\n{list_yaml}---\n"));
}

// ---------------------------------------------------------------------------
// `hyalo property add-to-list` — happy paths
// ---------------------------------------------------------------------------

#[test]
fn add_to_list_creates_property() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Note
---
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property",
            "add-to-list",
            "--name",
            "aliases",
            "--value",
            "my-alias",
            "--file",
            "note.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["property"], "aliases");
    assert_eq!(json["modified"].as_array().unwrap().len(), 1);
    assert_eq!(json["skipped"].as_array().unwrap().len(), 0);

    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains("my-alias"));
}

#[test]
fn add_to_list_appends_values() {
    let tmp = TempDir::new().unwrap();
    write_with_list(tmp.path(), "note.md", "aliases", &["existing"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property",
            "add-to-list",
            "--name",
            "aliases",
            "--value",
            "new-one",
            "--file",
            "note.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains("existing"));
    assert!(content.contains("new-one"));
}

#[test]
fn add_to_list_skips_duplicates() {
    let tmp = TempDir::new().unwrap();
    write_with_list(tmp.path(), "note.md", "aliases", &["AlreadyHere"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property",
            "add-to-list",
            "--name",
            "aliases",
            "--value",
            "alreadyhere", // different case
            "--file",
            "note.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["modified"].as_array().unwrap().len(), 0);
    assert_eq!(json["skipped"].as_array().unwrap().len(), 1);
}

#[test]
fn add_to_list_multiple_values() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Note
---
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property",
            "add-to-list",
            "--name",
            "authors",
            "--value",
            "alice",
            "--value",
            "bob",
            "--file",
            "note.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains("alice"));
    assert!(content.contains("bob"));
}

#[test]
fn add_to_list_with_glob() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "sub/a.md",
        md!(r"
---
title: A
---
"),
    );
    write_md(
        tmp.path(),
        "sub/b.md",
        md!(r"
---
title: B
---
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property",
            "add-to-list",
            "--name",
            "tags",
            "--value",
            "batch",
            "--glob",
            "sub/*.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["modified"].as_array().unwrap().len(), 2);
    assert_eq!(json["total"], 2);
}

#[test]
fn add_to_list_text_format() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Note
---
"),
    );

    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "--format",
            "text",
            "property",
            "add-to-list",
            "--name",
            "aliases",
            "--value",
            "alias1",
            "--file",
            "note.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("aliases") || stdout.contains("alias1"));
}

// ---------------------------------------------------------------------------
// `hyalo property remove-from-list` — happy paths
// ---------------------------------------------------------------------------

#[test]
fn remove_from_list_removes_values() {
    let tmp = TempDir::new().unwrap();
    write_with_list(tmp.path(), "note.md", "aliases", &["keep", "remove-me"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property",
            "remove-from-list",
            "--name",
            "aliases",
            "--value",
            "remove-me",
            "--file",
            "note.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["modified"].as_array().unwrap().len(), 1);

    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains("keep"));
    assert!(!content.contains("remove-me"));
}

#[test]
fn remove_from_list_removes_key_when_empty() {
    let tmp = TempDir::new().unwrap();
    write_with_list(tmp.path(), "note.md", "aliases", &["only-one"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property",
            "remove-from-list",
            "--name",
            "aliases",
            "--value",
            "only-one",
            "--file",
            "note.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(!content.contains("aliases:"));
    assert!(content.contains("title:"));
}

#[test]
fn remove_from_list_with_glob() {
    let tmp = TempDir::new().unwrap();
    write_with_list(tmp.path(), "sub/a.md", "aliases", &["shared", "extra"]);
    write_with_list(tmp.path(), "sub/b.md", "aliases", &["shared"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property",
            "remove-from-list",
            "--name",
            "aliases",
            "--value",
            "shared",
            "--glob",
            "sub/*.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["modified"].as_array().unwrap().len(), 2);
}

#[test]
fn remove_from_list_text_format() {
    let tmp = TempDir::new().unwrap();
    write_with_list(tmp.path(), "note.md", "aliases", &["foo"]);

    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "--format",
            "text",
            "property",
            "remove-from-list",
            "--name",
            "aliases",
            "--value",
            "foo",
            "--file",
            "note.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("aliases") || stdout.contains("foo"));
}

// ---------------------------------------------------------------------------
// Unhappy paths
// ---------------------------------------------------------------------------

#[test]
fn add_to_list_without_file_or_glob() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Note
---
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property",
            "add-to-list",
            "--name",
            "aliases",
            "--value",
            "x",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
}

#[test]
fn remove_from_list_without_file_or_glob() {
    let tmp = TempDir::new().unwrap();
    write_with_list(tmp.path(), "note.md", "aliases", &["foo"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property",
            "remove-from-list",
            "--name",
            "aliases",
            "--value",
            "foo",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
}

#[test]
fn add_to_list_file_not_found() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property",
            "add-to-list",
            "--name",
            "aliases",
            "--value",
            "x",
            "--file",
            "nonexistent.md",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
}

#[test]
fn remove_from_list_file_not_found() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property",
            "remove-from-list",
            "--name",
            "aliases",
            "--value",
            "x",
            "--file",
            "nonexistent.md",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
}

#[test]
fn remove_from_list_absent_value() {
    let tmp = TempDir::new().unwrap();
    write_with_list(tmp.path(), "note.md", "aliases", &["bar"]);

    // Value "foo" is not in the list — file should be skipped, exit 0
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property",
            "remove-from-list",
            "--name",
            "aliases",
            "--value",
            "foo",
            "--file",
            "note.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["modified"].as_array().unwrap().len(), 0);
    assert_eq!(json["skipped"].as_array().unwrap().len(), 1);
}

#[test]
fn add_to_list_no_values_provided() {
    // Clap enforces --value is required; omitting it should produce a clap error (exit 2)
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Note
---
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property",
            "add-to-list",
            "--name",
            "aliases",
            "--file",
            "note.md",
            // No --value provided
        ])
        .output()
        .unwrap();

    // Clap should reject this with non-zero exit
    assert!(!output.status.success());
}
