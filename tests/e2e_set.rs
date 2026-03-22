mod common;

use common::{hyalo, md, write_md};
use std::fs;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helper: run `hyalo set` and return (status, parsed JSON, stderr)
// ---------------------------------------------------------------------------

fn set_json(
    tmp: &TempDir,
    extra_args: &[&str],
) -> (std::process::ExitStatus, serde_json::Value, String) {
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.arg("set");
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
// --property K=V: create, overwrite, skip identical
// ---------------------------------------------------------------------------

#[test]
fn set_property_creates_new() {
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

    let (status, json, stderr) =
        set_json(&tmp, &["--property", "status=done", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["property"], "status");
    assert_eq!(json["value"], "done");
    assert_eq!(json["modified"].as_array().unwrap().len(), 1);
    assert_eq!(json["skipped"].as_array().unwrap().len(), 0);

    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains("status: done"), "content:\n{content}");
}

#[test]
fn set_property_overwrites_existing() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
status: draft
---
"),
    );

    let (status, json, stderr) = set_json(
        &tmp,
        &["--property", "status=published", "--file", "note.md"],
    );
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["modified"].as_array().unwrap().len(), 1);

    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains("status: published"), "content:\n{content}");
    assert!(
        !content.contains("draft"),
        "old value still present:\n{content}"
    );
}

#[test]
fn set_property_skips_when_identical() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
status: done
---
"),
    );

    let (status, json, stderr) =
        set_json(&tmp, &["--property", "status=done", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["modified"].as_array().unwrap().len(), 0);
    assert_eq!(json["skipped"].as_array().unwrap().len(), 1);
    assert_eq!(
        json["skipped"].as_array().unwrap()[0].as_str().unwrap(),
        "note.md"
    );
}

// ---------------------------------------------------------------------------
// --tag T: add new, idempotent
// ---------------------------------------------------------------------------

#[test]
fn set_tag_adds_new_tag() {
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

    let (status, json, stderr) = set_json(&tmp, &["--tag", "rust", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["tag"], "rust");
    assert_eq!(json["modified"].as_array().unwrap().len(), 1);
    assert_eq!(json["skipped"].as_array().unwrap().len(), 0);

    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains("rust"), "tag not written:\n{content}");
}

#[test]
fn set_tag_idempotent_when_already_present() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
tags:
  - rust
---
"),
    );

    let (status, json, stderr) = set_json(&tmp, &["--tag", "rust", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["modified"].as_array().unwrap().len(), 0);
    assert_eq!(json["skipped"].as_array().unwrap().len(), 1);
}

// ---------------------------------------------------------------------------
// Multiple --property and --tag returns an array
// ---------------------------------------------------------------------------

#[test]
fn set_multiple_mutations_returns_array() {
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

    let (status, json, stderr) = set_json(
        &tmp,
        &[
            "--property",
            "status=done",
            "--tag",
            "rust",
            "--file",
            "note.md",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");

    assert!(
        json.is_array(),
        "expected array for multiple mutations: {json}"
    );
    assert_eq!(json.as_array().unwrap().len(), 2);
}

// ---------------------------------------------------------------------------
// Guard: --file or --glob required
// ---------------------------------------------------------------------------

#[test]
fn set_requires_file_or_glob() {
    let tmp = TempDir::new().unwrap();
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["set", "--property", "status=done"]);
    let output = cmd.output().unwrap();
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

// ---------------------------------------------------------------------------
// Guard: at least one --property or --tag required
// ---------------------------------------------------------------------------

#[test]
fn set_requires_at_least_one_mutation() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["set", "--file", "note.md"]);
    let output = cmd.output().unwrap();
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

// ---------------------------------------------------------------------------
// Guard: invalid K=V (no `=`) returns error
// ---------------------------------------------------------------------------

#[test]
fn set_invalid_kv_no_equals_returns_error() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["set", "--property", "no-equals-sign", "--file", "note.md"]);
    let output = cmd.output().unwrap();
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

// ---------------------------------------------------------------------------
// Body content is preserved after mutation
// ---------------------------------------------------------------------------

#[test]
fn set_preserves_file_body() {
    let tmp = TempDir::new().unwrap();
    let body = "# Heading\n\nSome body content here.\n";
    write_md(
        tmp.path(),
        "note.md",
        &format!("---\ntitle: Note\n---\n{body}"),
    );

    let (status, _json, stderr) =
        set_json(&tmp, &["--property", "status=done", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");

    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains(body), "body was corrupted:\n{content}");
}

// ---------------------------------------------------------------------------
// --glob modifies multiple files
// ---------------------------------------------------------------------------

#[test]
fn set_with_glob_modifies_multiple_files() {
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
title: B
---
"),
    );

    let (status, json, stderr) = set_json(&tmp, &["--property", "status=done", "--glob", "*.md"]);
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["total"], 2, "expected total=2: {json}");
    assert_eq!(json["modified"].as_array().unwrap().len(), 2);

    let a = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    let b = fs::read_to_string(tmp.path().join("b.md")).unwrap();
    assert!(a.contains("status: done"), "a.md:\n{a}");
    assert!(b.contains("status: done"), "b.md:\n{b}");
}

// ---------------------------------------------------------------------------
// --format text produces non-empty output
// ---------------------------------------------------------------------------

#[test]
fn set_format_text_produces_output() {
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
        .args(["--format", "text"])
        .args(["set", "--property", "status=done", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.trim().is_empty(), "expected non-empty text output");
}
