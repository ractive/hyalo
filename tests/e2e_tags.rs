mod common;

use common::{hyalo, md, write_md, write_tagged};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// `hyalo tags` (bare) — defaults to summary
// ---------------------------------------------------------------------------

#[test]
fn tags_bare_defaults_to_summary() {
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

// ---------------------------------------------------------------------------
// `hyalo tags summary` — explicit summary subcommand
// ---------------------------------------------------------------------------

#[test]
fn tags_summary_all_files() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "a.md", &["rust", "cli"]);
    write_tagged(tmp.path(), "b.md", &["rust", "iteration"]);
    write_md(tmp.path(), "c.md", "No frontmatter.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "summary"])
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
fn tags_summary_with_glob() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "sub/a.md", &["alpha"]);
    write_tagged(tmp.path(), "sub/b.md", &["beta"]);
    write_tagged(tmp.path(), "root.md", &["gamma"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "summary", "--glob", "sub/*.md"])
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
fn tags_summary_with_file() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["rust", "cli"]);
    write_tagged(tmp.path(), "other.md", &["python"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "summary", "--file", "note.md"])
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
    assert!(names.contains(&"rust"));
    assert!(!names.contains(&"python"));
}

#[test]
fn tags_summary_empty_vault() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "summary"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 0);
    assert!(json["tags"].as_array().unwrap().is_empty());
}

#[test]
fn tags_summary_text_format() {
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
            "summary",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Shows total unique tags
    assert!(stdout.contains("unique tags"));
    // Shows tag names with file counts
    assert!(stdout.contains("rust"));
    assert!(stdout.contains("2 files"));
    assert!(stdout.contains("cli"));
    assert!(stdout.contains("1 file"));
    assert!(!stdout.contains("1 files"));
}

// ---------------------------------------------------------------------------
// `hyalo tags list` — per-file detail subcommand (new capability)
// ---------------------------------------------------------------------------

#[test]
fn tags_list_all_files() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "a.md", &["rust", "cli"]);
    write_tagged(tmp.path(), "b.md", &["iteration"]);
    write_md(tmp.path(), "c.md", "No frontmatter.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "list"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    // 3 files
    assert_eq!(json.len(), 3);
    // Each entry has path and tags
    assert!(json.iter().all(|e| e["path"].is_string()));
    assert!(json.iter().all(|e| e["tags"].is_array()));

    let a = json
        .iter()
        .find(|e| e["path"].as_str().unwrap().ends_with("a.md"))
        .unwrap();
    let a_tags: Vec<&str> = a["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(a_tags.contains(&"rust"));
    assert!(a_tags.contains(&"cli"));
}

#[test]
fn tags_list_with_glob() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "sub/a.md", &["alpha"]);
    write_tagged(tmp.path(), "sub/b.md", &["beta"]);
    write_tagged(tmp.path(), "root.md", &["gamma"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "list", "--glob", "sub/*.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    // Only sub/ files
    assert_eq!(json.len(), 2);
    let paths: Vec<&str> = json.iter().map(|e| e["path"].as_str().unwrap()).collect();
    assert!(paths.iter().all(|p| p.starts_with("sub/")));
    assert!(!paths.iter().any(|p| p.contains("root.md")));
}

#[test]
fn tags_list_with_file() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["rust", "cli"]);
    write_tagged(tmp.path(), "other.md", &["python"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "list", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json["path"].as_str().unwrap().ends_with("note.md"));
    let tags: Vec<&str> = json["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(tags.contains(&"rust"));
    assert!(tags.contains(&"cli"));
    assert!(!tags.contains(&"python"));
}

#[test]
fn tags_list_file_without_frontmatter() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "plain.md", "Just a plain markdown file.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "list", "--file", "plain.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["path"], "plain.md");
    assert!(json["tags"].as_array().unwrap().is_empty());
}

#[test]
fn tags_list_text_format() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["rust", "cli"]);

    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "--format",
            "text",
            "tags",
            "list",
            "--file",
            "note.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    // File path and tags shown together
    assert!(stdout.contains("note.md"));
    assert!(stdout.contains("rust"));
    assert!(stdout.contains("cli"));
}

#[test]
fn tags_list_glob_no_match() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["rust"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "list", "--glob", "nonexistent/*.md"])
        .output()
        .unwrap();

    // Should exit with error status (user error: no files match pattern)
    assert!(!output.status.success());
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

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

#[test]
fn tags_summary_glob_no_match() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["rust"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "summary", "--glob", "nonexistent/*.md"])
        .output()
        .unwrap();

    // Should exit with error status (user error: no files match pattern)
    assert!(!output.status.success());
}

// ---------------------------------------------------------------------------
// `hyalo tag find` — find files containing a specific tag
// ---------------------------------------------------------------------------

#[test]
fn tag_find_exact_match() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "a.md", &["iteration"]);
    write_tagged(tmp.path(), "b.md", &["links"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tag", "find", "--name", "iteration"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["tag"], "iteration");
    assert_eq!(json["total"], 1);
    let files = json["files"].as_array().unwrap();
    assert!(files[0].as_str().unwrap().contains("a.md"));
}

#[test]
fn tag_find_nested_match() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "a.md", &["inbox/processing"]);
    write_tagged(tmp.path(), "b.md", &["inbox/to-read"]);
    write_tagged(tmp.path(), "c.md", &["inbox"]);
    write_tagged(tmp.path(), "d.md", &["inboxes"]); // must NOT match

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tag", "find", "--name", "inbox"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 3);
    let files: Vec<&str> = json["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(files.iter().any(|f| f.contains("a.md")));
    assert!(files.iter().any(|f| f.contains("b.md")));
    assert!(files.iter().any(|f| f.contains("c.md")));
    // d.md has "inboxes" — should NOT be in results
    assert!(!files.iter().any(|f| f.contains("d.md")));
}

#[test]
fn tag_find_no_match() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["rust"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tag", "find", "--name", "nonexistent"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 0);
    assert!(json["files"].as_array().unwrap().is_empty());
}

#[test]
fn tag_find_with_glob() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "sub/a.md", &["rust"]);
    write_tagged(tmp.path(), "sub/b.md", &["python"]);
    write_tagged(tmp.path(), "root.md", &["rust"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tag", "find", "--name", "rust", "--glob", "sub/*.md"])
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
    // Only sub/a.md; root.md is excluded by glob
    assert!(files.iter().any(|f| f.contains("sub/a.md")));
    assert!(!files.iter().any(|f| f.contains("root.md")));
}

#[test]
fn tag_find_case_insensitive() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["Rust"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tag", "find", "--name", "rust"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 1);
}

#[test]
fn tag_find_text_format() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["rust"]);
    write_tagged(tmp.path(), "other.md", &["cli"]);

    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "--format",
            "text",
            "tag",
            "find",
            "--name",
            "rust",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Tag name and file count shown
    assert!(stdout.contains("rust"));
    assert!(stdout.contains("1 file"));
    assert!(!stdout.contains("1 files"));
    // Matching file listed
    assert!(stdout.contains("note.md"));
}

// ---------------------------------------------------------------------------
// `hyalo tag add` — add a tag to file(s)
// ---------------------------------------------------------------------------

#[test]
fn tag_add_single_file() {
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
        .args(["tag", "add", "--name", "rust", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["tag"], "rust");
    assert_eq!(json["modified"].as_array().unwrap().len(), 1);
    assert_eq!(json["skipped"].as_array().unwrap().len(), 0);

    // Verify file was modified
    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains("rust"));
}

#[test]
fn tag_add_glob_pattern() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "sub/a.md", &["existing"]);
    write_tagged(tmp.path(), "sub/b.md", &["existing"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tag", "add", "--name", "new-tag", "--glob", "sub/*.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["modified"].as_array().unwrap().len(), 2);
    assert_eq!(json["total"], 2);
}

#[test]
fn tag_add_idempotent() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["rust"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tag", "add", "--name", "rust", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["modified"].as_array().unwrap().len(), 0);
    assert_eq!(json["skipped"].as_array().unwrap().len(), 1);
}

#[test]
fn tag_add_creates_tags_property() {
    let tmp = TempDir::new().unwrap();
    // File has no tags property at all
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
        .args(["tag", "add", "--name", "plan", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains("tags:"));
    assert!(content.contains("plan"));
    // Body must still be present
    assert!(content.contains("# Body"));
}

#[test]
fn tag_add_invalid_name_numeric() {
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
        .args(["tag", "add", "--name", "1984", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    // Should mention numeric/non-numeric constraint
    assert!(
        stderr.contains("non-numeric") || stderr.contains("numeric"),
        "stderr: {stderr}"
    );
}

#[test]
fn tag_add_invalid_name_with_space() {
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
        .args(["tag", "add", "--name", "my tag", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(!output.status.success());
}

#[test]
fn tag_add_invalid_name_empty() {
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
        .args(["tag", "add", "--name", "", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(!output.status.success());
}

#[test]
fn tag_add_nested_tag_valid() {
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
            "tag",
            "add",
            "--name",
            "inbox/processing",
            "--file",
            "note.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["modified"].as_array().unwrap().len(), 1);
}

#[test]
fn tag_add_file_not_found() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tag", "add", "--name", "rust", "--file", "nonexistent.md"])
        .output()
        .unwrap();

    assert!(!output.status.success());
}

#[test]
fn tag_add_json_format() {
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
            "json",
            "tag",
            "add",
            "--name",
            "rust",
            "--file",
            "note.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["tag"], "rust");
    assert!(json["modified"].is_array());
    assert!(json["skipped"].is_array());
    assert!(json["total"].is_number());
}

// ---------------------------------------------------------------------------
// `hyalo tag remove` — remove a tag from file(s)
// ---------------------------------------------------------------------------

#[test]
fn tag_remove_single_file() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["rust", "cli"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tag", "remove", "--name", "rust", "--file", "note.md"])
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
fn tag_remove_glob_pattern() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "sub/a.md", &["rust", "cli"]);
    write_tagged(tmp.path(), "sub/b.md", &["rust", "python"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tag", "remove", "--name", "rust", "--glob", "sub/*.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["modified"].as_array().unwrap().len(), 2);
}

#[test]
fn tag_remove_absent_tag_idempotent() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["cli"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tag", "remove", "--name", "rust", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["modified"].as_array().unwrap().len(), 0);
    assert_eq!(json["skipped"].as_array().unwrap().len(), 1);
}

#[test]
fn tag_remove_empties_tags_property() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["rust"]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tag", "remove", "--name", "rust", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    // tags property should be gone entirely
    assert!(!content.contains("tags:"));
    // Other properties should be untouched
    assert!(content.contains("title:"));
}

#[test]
fn tag_remove_file_not_found() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "tag",
            "remove",
            "--name",
            "rust",
            "--file",
            "nonexistent.md",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
}

#[test]
fn tag_remove_text_format() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["rust"]);

    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "--format",
            "text",
            "tag",
            "remove",
            "--name",
            "rust",
            "--file",
            "note.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Tag name shown in output
    assert!(stdout.contains("rust"));
    // Modified count shown
    assert!(stdout.contains("1 modified"));
}

#[test]
fn tag_add_text_format() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["cli"]);

    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "--format",
            "text",
            "tag",
            "add",
            "--name",
            "rust",
            "--file",
            "note.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Tag name shown in output
    assert!(stdout.contains("rust"));
    // Modified count shown
    assert!(stdout.contains("1 modified"));
}

// ---------------------------------------------------------------------------
// More edge cases
// ---------------------------------------------------------------------------

#[test]
fn tag_add_to_file_with_no_frontmatter() {
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
        .args(["tag", "add", "--name", "rust", "--file", "plain.md"])
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

#[test]
fn tag_remove_from_file_with_no_frontmatter() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "plain.md", "No frontmatter here.\n");

    // Remove is a no-op — file goes to skipped
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tag", "remove", "--name", "rust", "--file", "plain.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["skipped"].as_array().unwrap().len(), 1);
    assert_eq!(json["modified"].as_array().unwrap().len(), 0);
}

#[test]
fn tag_find_file_with_empty_tags_list() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "empty.md", &[]);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tag", "find", "--name", "rust"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 0);
}

// ---------------------------------------------------------------------------
// Mutation commands require --file or --glob
// ---------------------------------------------------------------------------

#[test]
fn tag_add_without_file_or_glob_is_user_error() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["existing"]);

    // Omit both --file and --glob → must be a user error (exit 1)
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tag", "add", "--name", "new-tag"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("--file") || stderr.contains("--glob"),
        "expected hint about --file/--glob in stderr: {stderr}"
    );
    // File must be untouched
    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(!content.contains("new-tag"));
}

#[test]
fn tag_remove_without_file_or_glob_is_user_error() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["rust"]);

    // Omit both --file and --glob → must be a user error (exit 1)
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tag", "remove", "--name", "rust"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("--file") || stderr.contains("--glob"),
        "expected hint about --file/--glob in stderr: {stderr}"
    );
    // Tag must still be present
    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains("rust"));
}
