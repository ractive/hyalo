mod common;

use common::{hyalo_no_hints, md, sample_frontmatter, write_md};
use std::fs;
use tempfile::TempDir;

/// Helper: extract the results array from a `{total, results}` envelope.
fn unwrap_results(json: &serde_json::Value) -> &Vec<serde_json::Value> {
    json["results"]
        .as_array()
        .expect("expected {total, results} envelope")
}

// ---------------------------------------------------------------------------
// `hyalo properties summary` — aggregate property summary
// ---------------------------------------------------------------------------

#[test]
fn properties_aggregate() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        md!(r"
---
title: A
status: draft
---
# A
"),
    );
    write_md(
        tmp.path(),
        "b.md",
        md!(r"
---
title: B
priority: 1
---
# B
"),
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "summary"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();

    let names: Vec<&str> = json
        .iter()
        .map(|v| v["name"].as_str().expect("field 'name' should be a string"))
        .collect();
    assert!(names.contains(&"title"));
    assert!(names.contains(&"status"));
    assert!(names.contains(&"priority"));

    let title_entry = json
        .iter()
        .find(|v| v["name"] == "title")
        .expect("'title' property should be present");
    assert_eq!(title_entry["count"], 2);
    assert_eq!(title_entry["type"], "text");

    let status_entry = json.iter().find(|v| v["name"] == "status").unwrap();
    assert_eq!(status_entry["count"], 1);
}

#[test]
fn properties_empty_dir() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "summary"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json.is_empty());
}

#[test]
fn properties_with_glob() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "root.md",
        md!(r"
---
title: Root
---
"),
    );
    write_md(
        tmp.path(),
        "sub/a.md",
        md!(r"
---
title: Sub A
only_in_sub: yes
---
"),
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "summary", "--glob", "sub/*.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    let names: Vec<&str> = json
        .iter()
        .map(|v| v["name"].as_str().expect("field 'name' should be a string"))
        .collect();
    assert!(names.contains(&"only_in_sub"));
}

#[test]
fn properties_text_format() {
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

    let output = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "--format",
            "text",
            "properties",
            "summary",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("title"));
    assert!(stdout.contains("text"));
    assert!(stdout.contains("2 files"));
    assert!(stdout.contains("status"));
}

#[test]
fn properties_glob_no_match() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", sample_frontmatter());

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "summary", "--glob", "nonexistent/*.md"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "non-matching glob should exit 0, not error; stderr: {stderr}"
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        json.as_array().is_some_and(std::vec::Vec::is_empty),
        "non-matching glob should return empty array; got: {json}"
    );
    assert!(
        stderr.is_empty(),
        "non-matching glob should produce no stderr output; got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// `hyalo find --fields properties` — per-file property detail
// ---------------------------------------------------------------------------

#[test]
fn find_properties_single_file() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "file.md", sample_frontmatter());

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--file", "file.md", "--fields", "properties"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let results = json["results"]
        .as_array()
        .expect("{total, results} envelope");
    assert_eq!(results.len(), 1);
    let entry = &results[0];
    assert_eq!(entry["file"], "file.md");

    let props = entry["properties"]
        .as_object()
        .expect("field 'properties' should be an object");
    // Values are present with correct scalar types
    assert_eq!(props["title"], "My Note");
    assert_eq!(props["priority"], 3);
    assert_eq!(props["draft"], true);
    assert!(
        props.contains_key("created"),
        "created property should be present"
    );
    // "tags" should not appear as a property (it has its own dedicated field)
    assert!(
        !props.contains_key("tags"),
        "tags should not be in properties: {props:?}"
    );
}

#[test]
fn find_properties_with_glob() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "root.md",
        md!(r"
---
title: Root
---
"),
    );
    write_md(
        tmp.path(),
        "sub/a.md",
        md!(r"
---
title: Sub A
---
"),
    );
    write_md(
        tmp.path(),
        "sub/b.md",
        md!(r"
---
title: Sub B
---
"),
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--glob", "sub/*.md", "--fields", "properties"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let results = unwrap_results(&json);
    assert_eq!(results.len(), 2);

    let paths: Vec<&str> = results
        .iter()
        .map(|v| v["file"].as_str().expect("field 'file' should be a string"))
        .collect();
    assert!(paths.iter().all(|p| p.starts_with("sub/")));
}

#[test]
fn find_properties_file_without_frontmatter() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "plain.md", "Just a plain markdown file.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--file", "plain.md", "--fields", "properties"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let results = unwrap_results(&json);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["file"], "plain.md");
    let props = results[0]["properties"].as_object().unwrap();
    assert!(props.is_empty());
}

// ---------------------------------------------------------------------------
// Malformed YAML resilience
// ---------------------------------------------------------------------------

/// `properties` skips a file with malformed YAML and still returns results
/// for valid files. The warning is emitted to stderr but the command succeeds.
#[test]
fn properties_skips_malformed_yaml_file() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "good.md",
        md!(r"
---
title: Good
status: draft
---
# Good
"),
    );
    // Bare colon key: rejected by serde_yaml_ng.
    write_md(
        tmp.path(),
        "bad.md",
        "---\n: invalid yaml [[[{\n---\n# Bad\n",
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "summary"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "expected success; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    let names: Vec<&str> = json.iter().map(|v| v["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"title"), "missing 'title' in {names:?}");
    assert!(names.contains(&"status"), "missing 'status' in {names:?}");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("warning: skipping"),
        "expected warning on stderr; got: {stderr}"
    );
    assert!(
        stderr.contains("bad.md"),
        "warning should name the bad file; got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// properties rename
// ---------------------------------------------------------------------------

#[test]
fn properties_rename_basic() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "---\ntitle: A\nkeywords: test\n---\n");
    write_md(tmp.path(), "b.md", "---\ntitle: B\nkeywords: other\n---\n");
    write_md(tmp.path(), "c.md", "---\ntitle: C\n---\n"); // no keywords

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args([
        "properties",
        "rename",
        "--from",
        "keywords",
        "--to",
        "Keywords",
    ]);
    let output = cmd.output().unwrap();
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["modified"].as_array().unwrap().len(), 2);
    assert_eq!(json["skipped"].as_array().unwrap().len(), 1);

    let a = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(a.contains("Keywords:"));
    assert!(!a.contains("keywords:"));
}

#[test]
fn properties_rename_conflict_skips() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        "---\ntitle: Note\nfoo: old\nbar: existing\n---\n",
    );

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["properties", "rename", "--from", "foo", "--to", "bar"]);
    let output = cmd.output().unwrap();
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["conflicts"].as_array().unwrap().len(), 1);
    assert_eq!(json["modified"].as_array().unwrap().len(), 0);

    // File should be unchanged
    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains("foo: old"));
    assert!(content.contains("bar: existing"));
}

#[test]
fn properties_rename_with_glob_scope() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "notes/a.md", "---\nkeywords: test\n---\n");
    write_md(tmp.path(), "other/b.md", "---\nkeywords: test\n---\n");

    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args([
        "properties",
        "rename",
        "--from",
        "keywords",
        "--to",
        "Keywords",
        "--glob",
        "notes/*.md",
    ]);
    let output = cmd.output().unwrap();
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["modified"].as_array().unwrap().len(), 1);

    // Only notes/a.md should be renamed
    let a = fs::read_to_string(tmp.path().join("notes/a.md")).unwrap();
    assert!(a.contains("Keywords:"));
    let b = fs::read_to_string(tmp.path().join("other/b.md")).unwrap();
    assert!(b.contains("keywords:")); // unchanged
}

#[test]
fn properties_rename_same_name_exits_1() {
    let tmp = TempDir::new().unwrap();
    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["properties", "rename", "--from", "foo", "--to", "foo"]);
    let output = cmd.output().unwrap();
    assert!(!output.status.success());
}

// ---------------------------------------------------------------------------
// Glob negation
// ---------------------------------------------------------------------------

#[test]
fn properties_glob_negation_excludes_files() {
    let tmp = tempfile::tempdir().unwrap();
    write_md(
        tmp.path(),
        "keep.md",
        "---\ntitle: Keep\nstatus: active\n---\n",
    );
    write_md(
        tmp.path(),
        "exclude.md",
        "---\ntitle: Exclude\nexclusive_prop: only_here\n---\n",
    );

    let output = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "properties",
            "summary",
            "--glob",
            "!exclude.md",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let names: Vec<&str> = json
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"title"));
    assert!(names.contains(&"status"));
    assert!(
        !names.contains(&"exclusive_prop"),
        "exclusive_prop should be excluded via negation glob"
    );
}

// ---------------------------------------------------------------------------
// Bare `hyalo properties` defaults to summary
// ---------------------------------------------------------------------------

#[test]
fn properties_bare_defaults_to_summary() {
    let tmp = tempfile::tempdir().unwrap();
    common::write_md(
        tmp.path(),
        "a.md",
        "---\ntitle: A\nstatus: draft\n---\n# A\n",
    );

    let output = common::hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .arg("properties")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "bare `properties` should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // properties summary returns a JSON array of {name, type, count}
    let arr = json
        .as_array()
        .expect("should produce summary array output");
    assert!(!arr.is_empty(), "should have properties in summary");
}
