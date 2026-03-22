mod common;

use common::{hyalo, md, write_md};
use std::fs;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helper: run `hyalo append` and return (status, parsed JSON, stderr)
// ---------------------------------------------------------------------------

fn append_json(
    tmp: &TempDir,
    extra_args: &[&str],
) -> (std::process::ExitStatus, serde_json::Value, String) {
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.arg("append");
    cmd.args(extra_args);
    let output = cmd.output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let json: serde_json::Value = if output.status.success() {
        serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Null)
    } else {
        serde_json::Value::Null
    };
    (output.status, json, stderr)
}

// ---------------------------------------------------------------------------
// --property K=V: create new list when property is absent
// ---------------------------------------------------------------------------

#[test]
fn append_property_creates_new_list() {
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

    let (status, json, stderr) = append_json(
        &tmp,
        &["--property", "aliases=my-note", "--file", "note.md"],
    );
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["property"], "aliases");
    assert_eq!(json["value"], "my-note");
    assert_eq!(json["modified"].as_array().unwrap().len(), 1);
    assert_eq!(json["skipped"].as_array().unwrap().len(), 0);

    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains("my-note"), "value not written:\n{content}");
    // Should be serialized as a YAML list
    assert!(content.contains("- "), "expected list syntax:\n{content}");
}

// ---------------------------------------------------------------------------
// --property K=V: append to existing list
// ---------------------------------------------------------------------------

#[test]
fn append_property_appends_to_existing_list() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
aliases:
  - old-name
---
"),
    );

    let (status, json, stderr) = append_json(
        &tmp,
        &["--property", "aliases=new-name", "--file", "note.md"],
    );
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["modified"].as_array().unwrap().len(), 1);

    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(
        content.contains("old-name"),
        "existing value removed:\n{content}"
    );
    assert!(
        content.contains("new-name"),
        "new value not appended:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// --property K=V: skip duplicate in existing list
// ---------------------------------------------------------------------------

#[test]
fn append_property_skips_duplicate_in_list() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
aliases:
  - my-note
---
"),
    );

    let (status, json, stderr) = append_json(
        &tmp,
        &["--property", "aliases=my-note", "--file", "note.md"],
    );
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["modified"].as_array().unwrap().len(), 0);
    assert_eq!(json["skipped"].as_array().unwrap().len(), 1);
}

// ---------------------------------------------------------------------------
// --property K=V: promote scalar to list
// ---------------------------------------------------------------------------

#[test]
fn append_property_promotes_scalar_to_list() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
author: Alice
---
"),
    );

    let (status, json, stderr) =
        append_json(&tmp, &["--property", "author=Bob", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["modified"].as_array().unwrap().len(), 1);

    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(
        content.contains("Alice"),
        "original scalar removed:\n{content}"
    );
    assert!(
        content.contains("Bob"),
        "new value not appended:\n{content}"
    );
    assert!(
        content.contains("- "),
        "expected list syntax after promotion:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// Guard: --file or --glob required
// ---------------------------------------------------------------------------

#[test]
fn append_requires_file_or_glob() {
    let tmp = TempDir::new().unwrap();
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["append", "--property", "aliases=x"]);
    let output = cmd.output().unwrap();
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

// ---------------------------------------------------------------------------
// Guard: at least one --property required
// Note: `--property` is a required clap argument for `append`, so the CLI
// exits with code 2 (clap usage error) before reaching the application guard.
// ---------------------------------------------------------------------------

#[test]
fn append_requires_at_least_one_property() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["append", "--file", "note.md"]);
    let output = cmd.output().unwrap();
    assert!(!output.status.success());
}

// ---------------------------------------------------------------------------
// Guard: invalid K=V (no `=`) returns error
// ---------------------------------------------------------------------------

#[test]
fn append_invalid_kv_no_equals_returns_error() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args([
        "append",
        "--property",
        "no-equals-sign",
        "--file",
        "note.md",
    ]);
    let output = cmd.output().unwrap();
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

// ---------------------------------------------------------------------------
// Guard: invalid K=V (empty key) returns error
// ---------------------------------------------------------------------------

#[test]
fn append_empty_property_name_returns_error() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["append", "--property", "=value", "--file", "note.md"]);
    let output = cmd.output().unwrap();
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

// ---------------------------------------------------------------------------
// Body content is preserved after append
// ---------------------------------------------------------------------------

#[test]
fn append_preserves_file_body() {
    let tmp = TempDir::new().unwrap();
    let body = "# Heading\n\nSome body content here.\n";
    write_md(
        tmp.path(),
        "note.md",
        &format!("---\ntitle: Note\n---\n{body}"),
    );

    let (status, _json, stderr) = append_json(
        &tmp,
        &["--property", "aliases=my-note", "--file", "note.md"],
    );
    assert!(status.success(), "stderr: {stderr}");

    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains(body), "body was corrupted:\n{content}");
}

// ---------------------------------------------------------------------------
// --format text produces structured mutation output
// ---------------------------------------------------------------------------

#[test]
fn append_format_text_shows_counts() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Note
tags:
  - rust
---
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "text"])
        .args(["append", "--property", "tags=cli", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Should show modified count
    assert!(stdout.contains("1/1 modified"), "counts: {stdout}");
    // Should list the file
    assert!(stdout.contains("note.md"), "modified file: {stdout}");
}
