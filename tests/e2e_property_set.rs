mod common;

use common::{hyalo, write_md};
use tempfile::TempDir;

#[test]
fn set_new_property() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: Existing\n---\n# Body\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property", "set", "--name", "author", "--value", "Alice", "--path", "note.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["name"], "author");
    assert_eq!(json["value"], "Alice");
    assert_eq!(json["type"], "text");

    // Verify the property is persisted by reading it back
    let read_output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "read", "--name", "author", "--path", "note.md"])
        .output()
        .unwrap();
    assert!(read_output.status.success());
    let read_json: serde_json::Value = serde_json::from_slice(&read_output.stdout).unwrap();
    assert_eq!(read_json["value"], "Alice");
}

#[test]
fn set_overwrite_property() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        "---\ntitle: Old Title\n---\n# Body\n",
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property",
            "set",
            "--name",
            "title",
            "--value",
            "New Title",
            "--path",
            "note.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["value"], "New Title");

    // Verify it persisted
    let read_output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "read", "--name", "title", "--path", "note.md"])
        .output()
        .unwrap();
    let read_json: serde_json::Value = serde_json::from_slice(&read_output.stdout).unwrap();
    assert_eq!(read_json["value"], "New Title");
}

#[test]
fn set_creates_frontmatter() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "plain.md", "Just plain text.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property", "set", "--name", "status", "--value", "new", "--path", "plain.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());

    // Verify file now has frontmatter
    let content = std::fs::read_to_string(tmp.path().join("plain.md")).unwrap();
    assert!(content.starts_with("---\n"));
    assert!(content.contains("status:"));
}

#[test]
fn set_infers_number_type() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: Test\n---\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property", "set", "--name", "count", "--value", "42", "--path", "note.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["value"], 42);
    assert_eq!(json["type"], "number");
}

#[test]
fn set_infers_bool_type() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: Test\n---\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property", "set", "--name", "draft", "--value", "true", "--path", "note.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["value"], true);
    assert_eq!(json["type"], "checkbox");
}

#[test]
fn set_infers_date_type() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: Test\n---\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property",
            "set",
            "--name",
            "due",
            "--value",
            "2026-03-20",
            "--path",
            "note.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["value"], "2026-03-20");
    assert_eq!(json["type"], "date");
}

#[test]
fn set_explicit_type() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: Test\n---\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property", "set", "--name", "code", "--value", "42", "--type", "text", "--path",
            "note.md",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["value"], "42");
    assert_eq!(json["type"], "text");
}

#[test]
fn set_preserves_body() {
    let tmp = TempDir::new().unwrap();
    let body = "# My Heading\n\nSome paragraph content.\n\n- Item 1\n- Item 2\n";
    let content = format!("---\ntitle: Test\n---\n{body}");
    write_md(tmp.path(), "note.md", &content);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property", "set", "--name", "author", "--value", "Bob", "--path", "note.md",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());

    let file_content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(file_content.contains("# My Heading"));
    assert!(file_content.contains("Some paragraph content."));
    assert!(file_content.contains("- Item 1"));
    assert!(file_content.contains("- Item 2"));
}

#[test]
fn set_preserves_other_properties() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        "---\ntitle: Keep Me\nstatus: draft\n---\n# Body\n",
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "property", "set", "--name", "author", "--value", "Eve", "--path", "note.md",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());

    // Verify original properties still exist
    let read_title = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "read", "--name", "title", "--path", "note.md"])
        .output()
        .unwrap();
    let title_json: serde_json::Value = serde_json::from_slice(&read_title.stdout).unwrap();
    assert_eq!(title_json["value"], "Keep Me");

    let read_status = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["property", "read", "--name", "status", "--path", "note.md"])
        .output()
        .unwrap();
    let status_json: serde_json::Value = serde_json::from_slice(&read_status.stdout).unwrap();
    assert_eq!(status_json["value"], "draft");
}
