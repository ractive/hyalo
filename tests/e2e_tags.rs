mod common;

use common::{hyalo, md, write_md, write_tagged};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// `hyalo tags` — aggregate summary (now the only tags subcommand)
// ---------------------------------------------------------------------------

#[test]
fn tags_bare_returns_summary() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "a.md", &["rust", "cli"]);
    write_tagged(tmp.path(), "b.md", &["rust", "iteration"]);
    write_md(tmp.path(), "c.md", "No frontmatter.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .arg("tags")
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 3); // rust, cli, iteration
    let tags = json["tags"].as_array().unwrap();
    let rust = tags.iter().find(|t| t["name"] == "rust").unwrap();
    assert_eq!(rust["count"], 2);
    let cli = tags.iter().find(|t| t["name"] == "cli").unwrap();
    assert_eq!(cli["count"], 1);
}

#[test]
fn tags_empty_vault() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .arg("tags")
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 0);
    assert!(json["tags"].as_array().unwrap().is_empty());
}

#[test]
fn tags_with_glob() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "sub/a.md", &["alpha"]);
    write_tagged(tmp.path(), "sub/b.md", &["beta"]);
    write_tagged(tmp.path(), "root.md", &["gamma"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "--glob", "sub/*.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 2);
    let names: Vec<&str> = json["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"alpha"));
    assert!(names.contains(&"beta"));
    assert!(!names.contains(&"gamma"));
}

#[test]
fn tags_glob_no_match() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["rust"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "--glob", "nonexistent/*.md"])
        .output()
        .unwrap();

    assert!(!output.status.success());
}

#[test]
fn tags_text_format() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["rust"]);
    write_tagged(tmp.path(), "other.md", &["rust", "cli"]);

    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "--format",
            "text",
            "tags",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("unique tags"));
    assert!(stdout.contains("rust"));
    assert!(stdout.contains("2 files"));
    assert!(stdout.contains("cli"));
    assert!(stdout.contains("1 file"));
    assert!(!stdout.contains("1 files"));
}

#[test]
fn tags_file_without_frontmatter() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "plain.md", "Just a plain markdown file.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .arg("tags")
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 0);
}

#[test]
fn tags_scalar_string_tag() {
    let tmp = TempDir::new().unwrap();
    // tags as a scalar string (not a list)
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Note
tags: rust
---
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .arg("tags")
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 1);
    assert_eq!(json["tags"][0]["name"], "rust");
}

// ---------------------------------------------------------------------------
// `hyalo find --tag` — find files containing a specific tag
// ---------------------------------------------------------------------------

#[test]
fn find_tag_exact_match() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "a.md", &["iteration"]);
    write_tagged(tmp.path(), "b.md", &["links"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--tag", "iteration", "--fields", "tags"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json.len(), 1);
    assert!(json[0]["file"].as_str().unwrap().contains("a.md"));
}

#[test]
fn find_tag_nested_match() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "a.md", &["inbox/processing"]);
    write_tagged(tmp.path(), "b.md", &["inbox/to-read"]);
    write_tagged(tmp.path(), "c.md", &["inbox"]);
    write_tagged(tmp.path(), "d.md", &["inboxes"]); // must NOT match

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--tag", "inbox"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json.len(), 3);
    let paths: Vec<&str> = json.iter().map(|e| e["file"].as_str().unwrap()).collect();
    assert!(paths.iter().any(|f| f.contains("a.md")));
    assert!(paths.iter().any(|f| f.contains("b.md")));
    assert!(paths.iter().any(|f| f.contains("c.md")));
    assert!(!paths.iter().any(|f| f.contains("d.md")));
}

#[test]
fn find_tag_no_match_returns_empty_array() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["rust"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--tag", "nonexistent"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json.is_empty());
}

#[test]
fn find_tag_with_glob() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "sub/a.md", &["rust"]);
    write_tagged(tmp.path(), "sub/b.md", &["python"]);
    write_tagged(tmp.path(), "root.md", &["rust"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--tag", "rust", "--glob", "sub/*.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json.len(), 1);
    let paths: Vec<&str> = json.iter().map(|e| e["file"].as_str().unwrap()).collect();
    assert!(paths.iter().any(|f| f.contains("sub/a.md")));
    assert!(!paths.iter().any(|f| f.contains("root.md")));
}

#[test]
fn find_tag_case_insensitive() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["Rust"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--tag", "rust"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json.len(), 1);
}

// ---------------------------------------------------------------------------
// `hyalo set --tag` — add a tag to file(s)
// ---------------------------------------------------------------------------

#[test]
fn set_tag_single_file() {
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
        .args(["set", "--tag", "rust", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["tag"], "rust");
    assert_eq!(json["modified"].as_array().unwrap().len(), 1);
    assert_eq!(json["skipped"].as_array().unwrap().len(), 0);

    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains("rust"));
}

#[test]
fn set_tag_glob_pattern() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "sub/a.md", &["existing"]);
    write_tagged(tmp.path(), "sub/b.md", &["existing"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["set", "--tag", "new-tag", "--glob", "sub/*.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["modified"].as_array().unwrap().len(), 2);
    assert_eq!(json["total"], 2);
}

#[test]
fn set_tag_idempotent() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["rust"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["set", "--tag", "rust", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["modified"].as_array().unwrap().len(), 0);
    assert_eq!(json["skipped"].as_array().unwrap().len(), 1);
}

#[test]
fn set_tag_creates_tags_property() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Note
---
# Body
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["set", "--tag", "plan", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains("tags:"));
    assert!(content.contains("plan"));
    assert!(content.contains("# Body"));
}

#[test]
fn set_tag_file_not_found() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["set", "--tag", "rust", "--file", "nonexistent.md"])
        .output()
        .unwrap();

    assert!(!output.status.success());
}

#[test]
fn set_tag_to_file_with_no_frontmatter() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "plain.md",
        md!(r"
No frontmatter here.
# Content
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["set", "--tag", "rust", "--file", "plain.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["modified"].as_array().unwrap().len(), 1);

    let content = std::fs::read_to_string(tmp.path().join("plain.md")).unwrap();
    assert!(content.starts_with("---\n"));
    assert!(content.contains("rust"));
    assert!(content.contains("# Content"));
}

// ---------------------------------------------------------------------------
// `hyalo remove --tag` — remove a tag from file(s)
// ---------------------------------------------------------------------------

#[test]
fn remove_tag_single_file() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["rust", "cli"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["remove", "--tag", "rust", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["modified"].as_array().unwrap().len(), 1);
    assert_eq!(json["skipped"].as_array().unwrap().len(), 0);

    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(!content.contains("rust"));
    assert!(content.contains("cli"));
}

#[test]
fn remove_tag_glob_pattern() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "sub/a.md", &["rust", "cli"]);
    write_tagged(tmp.path(), "sub/b.md", &["rust", "python"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["remove", "--tag", "rust", "--glob", "sub/*.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["modified"].as_array().unwrap().len(), 2);
}

#[test]
fn remove_tag_absent_tag_idempotent() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["cli"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["remove", "--tag", "rust", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["modified"].as_array().unwrap().len(), 0);
    assert_eq!(json["skipped"].as_array().unwrap().len(), 1);
}

#[test]
fn remove_tag_empties_tags_property() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["rust"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["remove", "--tag", "rust", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(!content.contains("tags:"));
    assert!(content.contains("title:"));
}

#[test]
fn remove_tag_file_not_found() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["remove", "--tag", "rust", "--file", "nonexistent.md"])
        .output()
        .unwrap();

    assert!(!output.status.success());
}

#[test]
fn remove_tag_from_file_with_no_frontmatter() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "plain.md", "No frontmatter here.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["remove", "--tag", "rust", "--file", "plain.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["skipped"].as_array().unwrap().len(), 1);
    assert_eq!(json["modified"].as_array().unwrap().len(), 0);
}

#[test]
fn find_tag_empty_tags_list() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "empty.md", &[]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--tag", "rust"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json.is_empty());
}

// ---------------------------------------------------------------------------
// Mutation commands require --file or --glob
// ---------------------------------------------------------------------------

#[test]
fn set_tag_without_file_or_glob_is_user_error() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["existing"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["set", "--tag", "new-tag"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("--file") || stderr.contains("--glob"),
        "expected hint about --file/--glob in stderr: {stderr}"
    );
    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(!content.contains("new-tag"));
}

#[test]
fn remove_tag_without_file_or_glob_is_user_error() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["rust"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["remove", "--tag", "rust"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("--file") || stderr.contains("--glob"),
        "expected hint about --file/--glob in stderr: {stderr}"
    );
    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains("rust"));
}
