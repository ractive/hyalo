mod common;

use common::{hyalo, md, write_md};
use std::fs;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// `hyalo mv` — move/rename file with link updates
// ---------------------------------------------------------------------------

#[test]
fn mv_updates_inbound_wikilink() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        md!(r"
---
title: A
---
See [[b]] for details.
"),
    );
    write_md(
        tmp.path(),
        "b.md",
        md!(r"
---
title: B
---
Content.
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "archive/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["from"], "b.md");
    assert_eq!(json["to"], "archive/b.md");
    assert_eq!(json["dry_run"], false);
    assert_eq!(json["total_links_updated"], 1);

    // Verify file was moved
    assert!(!tmp.path().join("b.md").exists());
    assert!(tmp.path().join("archive/b.md").exists());

    // Verify link was updated
    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(content.contains("[[archive/b]]"), "a.md content: {content}");
    assert!(!content.contains("[[b]]"));
}

#[test]
fn mv_preserves_wikilink_alias() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "See [[b|my note]] here.\n");
    write_md(tmp.path(), "b.md", "Content.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "sub/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(
        content.contains("[[sub/b|my note]]"),
        "a.md content: {content}"
    );
}

#[test]
fn mv_preserves_wikilink_fragment() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "See [[b#section]] here.\n");
    write_md(tmp.path(), "b.md", "Content.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "sub/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(
        content.contains("[[sub/b#section]]"),
        "a.md content: {content}"
    );
}

#[test]
fn mv_updates_inbound_markdown_link() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "See [note](b.md) here.\n");
    write_md(tmp.path(), "b.md", "Content.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "sub/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(
        content.contains("[note](sub/b.md)"),
        "a.md content: {content}"
    );
}

#[test]
fn mv_updates_outbound_relative_link() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "Target.\n");
    write_md(tmp.path(), "b.md", "See [note](a.md) here.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "sub/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let content = fs::read_to_string(tmp.path().join("sub/b.md")).unwrap();
    assert!(
        content.contains("[note](../a.md)"),
        "sub/b.md content: {content}"
    );
}

#[test]
fn mv_outbound_wikilink_unchanged() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "Target.\n");
    write_md(tmp.path(), "b.md", "See [[a]] here.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "sub/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let content = fs::read_to_string(tmp.path().join("sub/b.md")).unwrap();
    // Wikilinks are vault-relative, should NOT change
    assert!(content.contains("[[a]]"), "sub/b.md content: {content}");
}

#[test]
fn mv_skips_links_in_code_blocks() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        md!(r"
---
title: A
---
```
[[b]]
```
Real [[b]] here.
"),
    );
    write_md(tmp.path(), "b.md", "Content.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "sub/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total_links_updated"], 1);

    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    // The code block should still have [[b]], not [[sub/b]]
    assert!(
        content.contains("```\n[[b]]\n```"),
        "code block was modified: {content}"
    );
    assert!(
        content.contains("Real [[sub/b]] here."),
        "real link not updated: {content}"
    );
}

#[test]
fn mv_skips_links_in_inline_code() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "Use `[[b]]` syntax and real [[b]]\n");
    write_md(tmp.path(), "b.md", "Content.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "sub/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(
        content.contains("`[[b]]`"),
        "inline code was modified: {content}"
    );
    assert!(
        content.contains("real [[sub/b]]"),
        "real link not updated: {content}"
    );
}

#[test]
fn mv_dry_run_does_not_modify() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "See [[b]] here.\n");
    write_md(tmp.path(), "b.md", "Content.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "sub/b.md", "--dry-run"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["dry_run"], true);
    assert_eq!(json["total_links_updated"], 1);

    // File should NOT have been moved
    assert!(tmp.path().join("b.md").exists());
    assert!(!tmp.path().join("sub/b.md").exists());

    // Link should NOT have been updated
    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(
        content.contains("[[b]]"),
        "a.md was modified during dry-run: {content}"
    );
}

#[test]
fn mv_target_already_exists_error() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "Content A.\n");
    write_md(tmp.path(), "b.md", "Content B.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "a.md", "--to", "b.md"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("already exists"), "stderr: {stderr}");
}

#[test]
fn mv_source_not_found_error() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "Content.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "nonexistent.md", "--to", "new.md"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"), "stderr: {stderr}");
}

#[test]
fn mv_creates_parent_directory() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "Content.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "a.md", "--to", "deep/nested/a.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!tmp.path().join("a.md").exists());
    assert!(tmp.path().join("deep/nested/a.md").exists());
}

#[test]
fn mv_text_format() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "See [[b]] here.\n");
    write_md(tmp.path(), "b.md", "Content.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "text"])
        .args(["mv", "--file", "b.md", "--to", "sub/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Moved b.md"), "stdout: {stdout}");
    assert!(stdout.contains("sub/b.md"), "stdout: {stdout}");
    assert!(stdout.contains("[[b]]"), "stdout: {stdout}");
    assert!(stdout.contains("[[sub/b]]"), "stdout: {stdout}");
}

#[test]
fn mv_no_links_to_update() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "No links here.\n");
    write_md(tmp.path(), "b.md", "Also no links.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "a.md", "--to", "sub/a.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total_links_updated"], 0);
    assert_eq!(json["total_files_updated"], 0);

    assert!(!tmp.path().join("a.md").exists());
    assert!(tmp.path().join("sub/a.md").exists());
}

#[test]
fn mv_multiple_links_same_file() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "See [[b]] and also [[b|alias]] here.\n");
    write_md(tmp.path(), "b.md", "Content.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "sub/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total_links_updated"], 2);

    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(content.contains("[[sub/b]]"), "a.md content: {content}");
    assert!(
        content.contains("[[sub/b|alias]]"),
        "a.md content: {content}"
    );
}

#[test]
fn mv_target_must_end_with_md() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "Content.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "a.md", "--to", "b.txt"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(".md"), "stderr: {stderr}");
}

#[test]
fn mv_markdown_link_with_fragment() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "See [note](b.md#section) here.\n");
    write_md(tmp.path(), "b.md", "Content.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "sub/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(
        content.contains("[note](sub/b.md#section)"),
        "a.md content: {content}"
    );
}

#[test]
fn mv_cross_directory_markdown_link() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "sub/a.md", "See [note](../b.md) here.\n");
    write_md(tmp.path(), "b.md", "Content.\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "other/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let content = fs::read_to_string(tmp.path().join("sub/a.md")).unwrap();
    assert!(
        content.contains("[note](../other/b.md)"),
        "sub/a.md content: {content}"
    );
}
