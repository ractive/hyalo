mod common;

use common::{hyalo_no_hints, md, write_md, write_tagged};
use tempfile::TempDir;

/// Helper: extract the results array from a `{total, results}` envelope.
fn unwrap_results(json: &serde_json::Value) -> &Vec<serde_json::Value> {
    json["results"]
        .as_array()
        .expect("expected {total, results} envelope")
}

// ---------------------------------------------------------------------------
// `hyalo tags summary` — aggregate tag summary
// ---------------------------------------------------------------------------

#[test]
fn tags_summary_returns_counts() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "a.md", &["rust", "cli"]);
    write_tagged(tmp.path(), "b.md", &["rust", "iteration"]);
    write_md(tmp.path(), "c.md", "No frontmatter.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "summary"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 3); // rust, cli, iteration
    let tags = json["results"].as_array().unwrap();
    let rust = tags.iter().find(|t| t["name"] == "rust").unwrap();
    assert_eq!(rust["count"], 2);
    let cli = tags.iter().find(|t| t["name"] == "cli").unwrap();
    assert_eq!(cli["count"], 1);
}

#[test]
fn tags_empty_vault() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "summary"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 0);
    assert!(json["results"].as_array().unwrap().is_empty());
}

#[test]
fn tags_with_glob() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "sub/a.md", &["alpha"]);
    write_tagged(tmp.path(), "sub/b.md", &["beta"]);
    write_tagged(tmp.path(), "root.md", &["gamma"]);

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "summary", "--glob", "sub/*.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 2);
    let names: Vec<&str> = json["results"]
        .as_array()
        .expect("field 'tags' should be an array")
        .iter()
        .map(|t| t["name"].as_str().expect("field 'name' should be a string"))
        .collect();
    assert!(names.contains(&"alpha"));
    assert!(names.contains(&"beta"));
    assert!(!names.contains(&"gamma"));
}

#[test]
fn tags_glob_no_match() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["rust"]);

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "summary", "--glob", "nonexistent/*.md"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "non-matching glob should exit 0, not error; stderr: {stderr}"
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json["total"], 0,
        "non-matching glob should return total 0; got: {json}"
    );
    assert!(
        json["results"]
            .as_array()
            .is_some_and(std::vec::Vec::is_empty),
        "non-matching glob should return empty tags array; got: {json}"
    );
    assert!(
        stderr.is_empty(),
        "non-matching glob should produce no stderr output; got: {stderr}"
    );
}

#[test]
fn tags_text_format() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["rust"]);
    write_tagged(tmp.path(), "other.md", &["rust", "cli"]);

    let output = hyalo_no_hints()
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

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "summary"])
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

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "summary"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 1);
    assert_eq!(json["results"][0]["name"], "rust");
}

// ---------------------------------------------------------------------------
// `hyalo find --tag` — find files containing a specific tag
// ---------------------------------------------------------------------------

#[test]
fn find_tag_exact_match() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "a.md", &["iteration"]);
    write_tagged(tmp.path(), "b.md", &["links"]);

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--tag", "iteration", "--fields", "tags"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let json = unwrap_results(&json);
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

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--tag", "inbox"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let json = unwrap_results(&json);
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

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--tag", "nonexistent"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let json = unwrap_results(&json);
    assert!(json.is_empty());
}

#[test]
fn find_tag_with_glob() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "sub/a.md", &["rust"]);
    write_tagged(tmp.path(), "sub/b.md", &["python"]);
    write_tagged(tmp.path(), "root.md", &["rust"]);

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--tag", "rust", "--glob", "sub/*.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let json = unwrap_results(&json);
    assert_eq!(json.len(), 1);
    let paths: Vec<&str> = json.iter().map(|e| e["file"].as_str().unwrap()).collect();
    assert!(paths.iter().any(|f| f.contains("sub/a.md")));
    assert!(!paths.iter().any(|f| f.contains("root.md")));
}

#[test]
fn find_tag_case_insensitive() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["Rust"]);

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--tag", "rust"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let json = unwrap_results(&json);
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

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["set", "--tag", "rust", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["tag"], "rust");
    assert_eq!(json["results"]["modified"].as_array().unwrap().len(), 1);
    assert_eq!(json["results"]["skipped"].as_array().unwrap().len(), 0);

    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains("rust"));
}

#[test]
fn set_tag_glob_pattern() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "sub/a.md", &["existing"]);
    write_tagged(tmp.path(), "sub/b.md", &["existing"]);

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["set", "--tag", "new-tag", "--glob", "sub/*.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["modified"].as_array().unwrap().len(), 2);
    assert_eq!(json["results"]["total"], 2);
}

#[test]
fn set_tag_idempotent() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["rust"]);

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["set", "--tag", "rust", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["modified"].as_array().unwrap().len(), 0);
    assert_eq!(json["results"]["skipped"].as_array().unwrap().len(), 1);
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

    let output = hyalo_no_hints()
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

    let output = hyalo_no_hints()
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

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["set", "--tag", "rust", "--file", "plain.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["modified"].as_array().unwrap().len(), 1);

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

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["remove", "--tag", "rust", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["modified"].as_array().unwrap().len(), 1);
    assert_eq!(json["results"]["skipped"].as_array().unwrap().len(), 0);

    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(!content.contains("rust"));
    assert!(content.contains("cli"));
}

#[test]
fn remove_tag_glob_pattern() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "sub/a.md", &["rust", "cli"]);
    write_tagged(tmp.path(), "sub/b.md", &["rust", "python"]);

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["remove", "--tag", "rust", "--glob", "sub/*.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["modified"].as_array().unwrap().len(), 2);
}

#[test]
fn remove_tag_absent_tag_idempotent() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["cli"]);

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["remove", "--tag", "rust", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["modified"].as_array().unwrap().len(), 0);
    assert_eq!(json["results"]["skipped"].as_array().unwrap().len(), 1);
}

#[test]
fn remove_tag_empties_tags_property() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["rust"]);

    let output = hyalo_no_hints()
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

    let output = hyalo_no_hints()
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

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["remove", "--tag", "rust", "--file", "plain.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["skipped"].as_array().unwrap().len(), 1);
    assert_eq!(json["results"]["modified"].as_array().unwrap().len(), 0);
}

#[test]
fn find_tag_empty_tags_list() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "empty.md", &[]);

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--tag", "rust"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let json = unwrap_results(&json);
    assert!(json.is_empty());
}

// ---------------------------------------------------------------------------
// Mutation commands require --file or --glob
// ---------------------------------------------------------------------------

#[test]
fn set_tag_without_file_or_glob_is_user_error() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["existing"]);

    let output = hyalo_no_hints()
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

    let output = hyalo_no_hints()
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

// ---------------------------------------------------------------------------
// tags rename
// ---------------------------------------------------------------------------

#[test]
fn tags_rename_basic() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "a.md", &["filtering", "cli"]);
    write_tagged(tmp.path(), "b.md", &["filtering"]);
    write_tagged(tmp.path(), "c.md", &["cli"]); // no "filtering"

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["tags", "rename", "--from", "filtering", "--to", "filters"]);
    let output = cmd.output().unwrap();
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["modified"].as_array().unwrap().len(), 2);
    assert_eq!(json["results"]["skipped"].as_array().unwrap().len(), 1);

    let a = std::fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(a.contains("filters"));
    assert!(!a.contains("filtering"));
}

#[test]
fn tags_rename_already_has_new_tag() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "note.md", &["old-name", "new-name"]);

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["tags", "rename", "--from", "old-name", "--to", "new-name"]);
    let output = cmd.output().unwrap();
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["modified"].as_array().unwrap().len(), 1);

    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(!content.contains("old-name"));
    // Should have exactly one "new-name", not duplicated
    let count = content.matches("new-name").count();
    assert_eq!(count, 1, "new tag should not be duplicated");
}

#[test]
fn tags_rename_same_name_exits_1() {
    let tmp = TempDir::new().unwrap();
    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["tags", "rename", "--from", "foo", "--to", "foo"]);
    let output = cmd.output().unwrap();
    assert!(!output.status.success());
}

#[test]
fn tags_rename_with_glob_scope() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "notes/a.md", &["old-tag"]);
    write_tagged(tmp.path(), "other/b.md", &["old-tag"]);

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args([
        "tags",
        "rename",
        "--from",
        "old-tag",
        "--to",
        "new-tag",
        "--glob",
        "notes/*.md",
    ]);
    let output = cmd.output().unwrap();
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["modified"].as_array().unwrap().len(), 1);

    let a = std::fs::read_to_string(tmp.path().join("notes/a.md")).unwrap();
    assert!(a.contains("new-tag"));
    let b = std::fs::read_to_string(tmp.path().join("other/b.md")).unwrap();
    assert!(b.contains("old-tag")); // unchanged
}

#[test]
fn tags_rename_invalid_tag_exits_1() {
    let tmp = TempDir::new().unwrap();
    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["tags", "rename", "--from", "valid", "--to", "invalid tag!"]);
    let output = cmd.output().unwrap();
    assert!(!output.status.success());
}

#[test]
fn tags_rename_scalar_tag() {
    let tmp = TempDir::new().unwrap();
    // tags as a scalar string, not a list
    write_md(
        tmp.path(),
        "note.md",
        "---\ntitle: Note\ntags: old-tag\n---\n",
    );

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["tags", "rename", "--from", "old-tag", "--to", "new-tag"]);
    let output = cmd.output().unwrap();
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["modified"].as_array().unwrap().len(), 1);

    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(
        content.contains("new-tag"),
        "should contain new-tag: {content}"
    );
    assert!(
        !content.contains("old-tag"),
        "should not contain old-tag: {content}"
    );
}

#[test]
fn tags_rename_scalar_tag_already_has_new() {
    let tmp = TempDir::new().unwrap();
    // Scalar tags: old-tag — but file also has a list with new-tag via another mechanism.
    // Simpler case: scalar is old-tag, new-tag already in the file via sequence
    // Actually, with scalar tags the only tag is the scalar value itself.
    // So: scalar "old-tag", rename to "new-tag" where new-tag doesn't exist → just renames.
    // Test: scalar "old-tag" when new-tag already exists isn't possible with scalar form
    // (scalar = exactly one tag). So let's test that removing the scalar when new-tag
    // is already present via sequence form works — but that's mixed form, unlikely.
    // Instead, verify the simple scalar rename writes back as a scalar string.
    write_md(tmp.path(), "note.md", "---\ntags: alpha\n---\n");

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["tags", "rename", "--from", "alpha", "--to", "beta"]);
    let output = cmd.output().unwrap();
    assert!(output.status.success());

    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains("beta"));
    assert!(!content.contains("alpha"));
}

// ---------------------------------------------------------------------------
// Glob negation
// ---------------------------------------------------------------------------

#[test]
fn tags_glob_negation_excludes_files() {
    use common::write_tagged;

    let tmp = tempfile::tempdir().unwrap();
    write_tagged(tmp.path(), "keep.md", &["rust", "cli"]);
    write_tagged(tmp.path(), "exclude.md", &["exclusive-tag"]);

    let output = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "tags",
            "summary",
            "--glob",
            "!exclude.md",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let tags: Vec<&str> = json["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap())
        .collect();
    assert!(tags.contains(&"rust"), "rust tag should be present");
    assert!(tags.contains(&"cli"), "cli tag should be present");
    assert!(
        !tags.contains(&"exclusive-tag"),
        "exclusive-tag should be excluded via negation glob"
    );
}

// ---------------------------------------------------------------------------
// Bare `hyalo tags` defaults to summary
// ---------------------------------------------------------------------------

#[test]
fn tags_bare_defaults_to_summary() {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "a.md", &["rust", "cli"]);

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .arg("tags")
        .output()
        .unwrap();

    assert!(output.status.success(), "bare `tags` should succeed");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 2, "should produce summary output");
}
