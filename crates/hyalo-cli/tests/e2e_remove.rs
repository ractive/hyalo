mod common;

use common::{hyalo, md, write_md};
use std::fs;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helper: run `hyalo remove` and return (status, parsed JSON, stderr)
// ---------------------------------------------------------------------------

fn remove_json(
    tmp: &TempDir,
    extra_args: &[&str],
) -> (std::process::ExitStatus, serde_json::Value, String) {
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.arg("remove");
    cmd.args(extra_args);
    let output = cmd.output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let json: serde_json::Value = if output.status.success() {
        serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
            let stdout = String::from_utf8_lossy(&output.stdout);
            panic!("invalid JSON: {e}\nstdout: {stdout}\nstderr: {stderr}")
        })
    } else {
        serde_json::Value::Null
    };
    (output.status, json, stderr)
}

// ---------------------------------------------------------------------------
// --property K: remove entire key
// ---------------------------------------------------------------------------

#[test]
fn remove_property_key_removes_entirely() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Note
status: draft
---
"),
    );

    let (status, json, stderr) = remove_json(&tmp, &["--property", "status", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["property"], "status");
    // No "value" field when removing the whole key
    assert!(
        json.get("value").is_none() || json["value"].is_null(),
        "value field should be absent: {json}"
    );
    assert_eq!(json["modified"].as_array().unwrap().len(), 1);
    assert_eq!(json["skipped"].as_array().unwrap().len(), 0);

    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(
        !content.contains("status:"),
        "status key still present:\n{content}"
    );
    assert!(
        content.contains("title:"),
        "title was removed unexpectedly:\n{content}"
    );
}

#[test]
fn remove_property_key_absent_skips_file() {
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

    let (status, json, stderr) = remove_json(&tmp, &["--property", "status", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["modified"].as_array().unwrap().len(), 0);
    assert_eq!(json["skipped"].as_array().unwrap().len(), 1);
}

// ---------------------------------------------------------------------------
// --property K=V: remove value from a list property
// ---------------------------------------------------------------------------

#[test]
fn remove_property_value_removes_from_list() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
aliases:
  - old-name
  - other
---
"),
    );

    let (status, json, stderr) = remove_json(
        &tmp,
        &["--property", "aliases=old-name", "--file", "note.md"],
    );
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["modified"].as_array().unwrap().len(), 1);

    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(
        !content.contains("old-name"),
        "removed value still present:\n{content}"
    );
    assert!(
        content.contains("other"),
        "other value was removed:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// --property K=V: remove matching scalar removes the key entirely
// ---------------------------------------------------------------------------

#[test]
fn remove_property_value_scalar_matching_removes_key() {
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

    let (status, json, stderr) =
        remove_json(&tmp, &["--property", "status=draft", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["modified"].as_array().unwrap().len(), 1);

    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(
        !content.contains("status:"),
        "status key still present:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// --tag T: remove tag
// ---------------------------------------------------------------------------

#[test]
fn remove_tag_removes_from_list() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
tags:
  - rust
  - cli
---
"),
    );

    let (status, json, stderr) = remove_json(&tmp, &["--tag", "rust", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["tag"], "rust");
    assert_eq!(json["modified"].as_array().unwrap().len(), 1);
    assert_eq!(json["skipped"].as_array().unwrap().len(), 0);

    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(
        !content.contains("rust"),
        "rust tag still present:\n{content}"
    );
    assert!(content.contains("cli"), "cli tag was removed:\n{content}");
}

#[test]
fn remove_tag_absent_skips_file() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
tags:
  - cli
---
"),
    );

    let (status, json, stderr) = remove_json(&tmp, &["--tag", "rust", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["modified"].as_array().unwrap().len(), 0);
    assert_eq!(json["skipped"].as_array().unwrap().len(), 1);
}

// ---------------------------------------------------------------------------
// --where-property / --where-tag filter tests
// ---------------------------------------------------------------------------

#[test]
fn remove_where_property_match() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "target.md",
        md!(r"
---
title: Target
status: draft
legacy: true
---
"),
    );
    write_md(
        tmp.path(),
        "keep.md",
        md!(r"
---
title: Keep
status: published
legacy: true
---
"),
    );

    let (status, json, stderr) = remove_json(
        &tmp,
        &[
            "--property",
            "legacy",
            "--where-property",
            "status=draft",
            "--glob",
            "**/*.md",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(
        json["modified"].as_array().unwrap().len(),
        1,
        "expected 1 modified: {json}"
    );
    assert_eq!(
        json["skipped"].as_array().unwrap().len(),
        0,
        "expected 0 skipped (keep.md was filtered out, not skipped): {json}"
    );

    let target_content = fs::read_to_string(tmp.path().join("target.md")).unwrap();
    assert!(
        !target_content.contains("legacy"),
        "target.md should have legacy removed:\n{target_content}"
    );

    let keep_content = fs::read_to_string(tmp.path().join("keep.md")).unwrap();
    assert!(
        keep_content.contains("legacy: true"),
        "keep.md should be untouched:\n{keep_content}"
    );
}

#[test]
fn remove_where_tag_match() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "tagged.md",
        md!(r"
---
title: Tagged
status: old
tags:
  - rust
  - cli
---
"),
    );
    write_md(
        tmp.path(),
        "untagged.md",
        md!(r"
---
title: Untagged
status: old
---
"),
    );

    let (status, json, stderr) = remove_json(
        &tmp,
        &[
            "--property",
            "status",
            "--where-tag",
            "rust",
            "--glob",
            "**/*.md",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(
        json["modified"].as_array().unwrap().len(),
        1,
        "expected only tagged file to be modified: {json}"
    );

    let tagged_content = fs::read_to_string(tmp.path().join("tagged.md")).unwrap();
    assert!(
        !tagged_content.contains("status:"),
        "tagged.md should have status removed:\n{tagged_content}"
    );

    let untagged_content = fs::read_to_string(tmp.path().join("untagged.md")).unwrap();
    assert!(
        untagged_content.contains("status: old"),
        "untagged.md should be untouched:\n{untagged_content}"
    );
}

// ---------------------------------------------------------------------------
// Guard: --file or --glob required
// ---------------------------------------------------------------------------

#[test]
fn remove_requires_file_or_glob() {
    let tmp = TempDir::new().unwrap();
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["remove", "--property", "status"]);
    let output = cmd.output().unwrap();
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

// ---------------------------------------------------------------------------
// Guard: invalid K=V (empty key) returns error
// ---------------------------------------------------------------------------

#[test]
fn remove_empty_property_name_returns_error() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["remove", "--property", "=value", "--file", "note.md"]);
    let output = cmd.output().unwrap();
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

// ---------------------------------------------------------------------------
// Guard: at least one --property or --tag required
// ---------------------------------------------------------------------------

#[test]
fn remove_requires_at_least_one_mutation() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["remove", "--file", "note.md"]);
    let output = cmd.output().unwrap();
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

// ---------------------------------------------------------------------------
// Body content is preserved after removal
// ---------------------------------------------------------------------------

#[test]
fn remove_preserves_file_body() {
    let tmp = TempDir::new().unwrap();
    let body = "# Heading\n\nSome body content here.\n";
    write_md(
        tmp.path(),
        "note.md",
        &format!("---\ntitle: Note\nstatus: draft\n---\n{body}"),
    );

    let (status, _json, stderr) = remove_json(&tmp, &["--property", "status", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");

    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains(body), "body was corrupted:\n{content}");
}

// ---------------------------------------------------------------------------
// Multi-file --file targeting
// ---------------------------------------------------------------------------

#[test]
fn remove_multi_file_modifies_all() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "---\ntitle: A\nstatus: draft\n---\n");
    write_md(tmp.path(), "b.md", "---\ntitle: B\nstatus: draft\n---\n");

    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args([
        "remove",
        "--property",
        "status",
        "--file",
        "a.md",
        "--file",
        "b.md",
    ]);
    let output = cmd.output().unwrap();
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["modified"].as_array().unwrap().len(), 2);
}

// ---------------------------------------------------------------------------
// --format text produces structured mutation output
// ---------------------------------------------------------------------------

#[test]
fn remove_format_text_shows_counts() {
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
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "text"])
        .args(["remove", "--property", "status", "--glob", "*.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Should show modified count
    assert!(stdout.contains("1/2 modified"), "counts: {stdout}");
    // Should list modified file
    assert!(stdout.contains("a.md"), "modified file: {stdout}");
}

#[test]
fn remove_tag_format_text_shows_counts() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Note
tags:
  - rust
  - cli
---
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "text"])
        .args(["remove", "--tag", "rust", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("rust"), "tag name: {stdout}");
    assert!(stdout.contains("1/1 modified"), "counts: {stdout}");
}
