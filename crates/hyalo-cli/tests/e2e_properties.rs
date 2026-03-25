mod common;

use common::{hyalo, md, sample_frontmatter, write_md};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// `hyalo properties` — aggregate summary (the only mode now)
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

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties"])
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

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties"])
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

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "--glob", "sub/*.md"])
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

    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "--format",
            "text",
            "properties",
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

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "--glob", "nonexistent/*.md"])
        .output()
        .unwrap();

    assert!(!output.status.success());
}

// ---------------------------------------------------------------------------
// `hyalo find --fields properties` — per-file property detail
// ---------------------------------------------------------------------------

#[test]
fn find_properties_single_file() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "file.md", sample_frontmatter());

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--file", "file.md", "--fields", "properties"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json.len(), 1);
    let entry = &json[0];
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

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--glob", "sub/*.md", "--fields", "properties"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json.len(), 2);

    let paths: Vec<&str> = json
        .iter()
        .map(|v| v["file"].as_str().expect("field 'file' should be a string"))
        .collect();
    assert!(paths.iter().all(|p| p.starts_with("sub/")));
}

#[test]
fn find_properties_file_without_frontmatter() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "plain.md", "Just a plain markdown file.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--file", "plain.md", "--fields", "properties"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json.len(), 1);
    assert_eq!(json[0]["file"], "plain.md");
    let props = json[0]["properties"].as_object().unwrap();
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

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties"])
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
