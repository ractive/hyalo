mod common;

use common::{hyalo, sample_frontmatter, write_md};
use tempfile::TempDir;

#[test]
fn properties_single_file() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "file.md", sample_frontmatter());

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "--path", "file.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["path"], "file.md");

    let props = &json["properties"];
    assert_eq!(props["title"]["type"], "text");
    assert_eq!(props["title"]["value"], "My Note");
    assert_eq!(props["priority"]["type"], "number");
    assert_eq!(props["priority"]["value"], 3);
    assert_eq!(props["draft"]["type"], "checkbox");
    assert_eq!(props["draft"]["value"], true);
    assert_eq!(props["created"]["type"], "date");
    assert_eq!(props["tags"]["type"], "list");
}

#[test]
fn properties_aggregate() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        "---\ntitle: A\nstatus: draft\n---\n# A\n",
    );
    write_md(tmp.path(), "b.md", "---\ntitle: B\npriority: 1\n---\n# B\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();

    // Should have aggregated properties: title (2), status (1), priority (1)
    let names: Vec<&str> = json.iter().map(|v| v["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"title"));
    assert!(names.contains(&"status"));
    assert!(names.contains(&"priority"));

    // title appears in both files
    let title_entry = json.iter().find(|v| v["name"] == "title").unwrap();
    assert_eq!(title_entry["count"], 2);
    assert_eq!(title_entry["type"], "text");

    // status appears in one file
    let status_entry = json.iter().find(|v| v["name"] == "status").unwrap();
    assert_eq!(status_entry["count"], 1);
}

#[test]
fn properties_with_glob() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "root.md", "---\ntitle: Root\n---\n");
    write_md(tmp.path(), "sub/a.md", "---\ntitle: Sub A\n---\n");
    write_md(tmp.path(), "sub/b.md", "---\ntitle: Sub B\n---\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "--path", "sub/*.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json.len(), 2);

    let paths: Vec<&str> = json.iter().map(|v| v["path"].as_str().unwrap()).collect();
    assert!(paths.iter().all(|p| p.starts_with("sub/")));
}

#[test]
fn properties_empty_dir() {
    let tmp = TempDir::new().unwrap();
    // No .md files at all

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
fn properties_text_format() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        "---\ntitle: Hello\nstatus: draft\n---\n",
    );

    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "--format",
            "text",
            "properties",
            "--path",
            "note.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Text format outputs key: value lines
    assert!(stdout.contains("path:"));
    assert!(stdout.contains("properties:"));
}

#[test]
fn properties_file_without_frontmatter() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "plain.md", "Just a plain markdown file.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "--path", "plain.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["path"], "plain.md");
    let props = json["properties"].as_object().unwrap();
    assert!(props.is_empty());
}
