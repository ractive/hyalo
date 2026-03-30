mod common;

use common::{hyalo, md, write_md};
use tempfile::TempDir;

#[test]
fn error_nonexistent_file() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--file", "missing.md"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stderr).unwrap();
    assert_eq!(json["error"], "file not found");
    assert!(json.get("path").is_some());
}

#[test]
fn error_nonexistent_dir() {
    let tmp = TempDir::new().unwrap();
    let nonexistent = tmp.path().join("does_not_exist");

    let output = hyalo()
        .args(["--dir", nonexistent.to_str().unwrap()])
        .args(["properties"])
        .output()
        .unwrap();

    // Should fail with exit code 2 (anyhow error path)
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(!stderr.is_empty());
}

#[test]
fn error_invalid_yaml() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "bad.md",
        md!(r"
---
: invalid yaml [[[{
---
# Body
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "summary", "--glob", "bad.md"])
        .output()
        .unwrap();

    // Malformed YAML is now gracefully skipped: command succeeds, warning on stderr.
    assert!(
        output.status.success(),
        "expected graceful skip; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("warning: skipping"),
        "expected warning on stderr; got: {stderr}"
    );
    assert!(
        stderr.contains("bad.md"),
        "warning should name the bad file; got: {stderr}"
    );
}

/// `hyalo find` uses the multi-visitor scan path (`scan_file_multi`). Malformed
/// frontmatter must be gracefully skipped: exit 0, warning on stderr.
#[test]
fn error_find_malformed_yaml_graceful_skip() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "bad.md",
        md!(r"
---
: invalid yaml [[[{
---
# Body
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "expected graceful skip; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("warning: skipping"),
        "expected warning on stderr; got: {stderr}"
    );
    assert!(
        stderr.contains("bad.md"),
        "warning should name the bad file; got: {stderr}"
    );
}

/// `hyalo summary` also uses the multi-visitor scan path. Malformed frontmatter
/// must be gracefully skipped: exit 0, warning on stderr, and directory counts
/// must not include the skipped file.
#[test]
fn error_summary_malformed_yaml_graceful_skip() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "bad.md",
        md!(r"
---
: invalid yaml [[[{
---
# Body
"),
    );
    write_md(
        tmp.path(),
        "good.md",
        md!(r"
---
title: OK
---
# Body
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap(), "--no-hints"])
        .args(["summary"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "expected graceful skip; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("warning: skipping"),
        "expected warning on stderr; got: {stderr}"
    );
    assert!(
        stderr.contains("bad.md"),
        "warning should name the bad file; got: {stderr}"
    );

    // Directory counts must exclude the skipped file
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    let total = json["files"]["total"].as_u64().unwrap();
    assert_eq!(total, 1, "only the good file should be counted");
    let by_dir = json["files"]["by_directory"].as_array().unwrap();
    let root_count: u64 = by_dir
        .iter()
        .map(|d| d["count"].as_u64().unwrap_or(0))
        .sum();
    assert_eq!(
        root_count, total,
        "by_directory counts must sum to total files"
    );
}

/// `hyalo backlinks` must gracefully skip files with broken frontmatter
/// instead of fatally erroring. Exit 0, warning on stderr, good files indexed.
#[test]
fn error_backlinks_unclosed_frontmatter_graceful_skip() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "bad.md",
        "---\nunclosed frontmatter without closing delimiter\n",
    );
    write_md(
        tmp.path(),
        "source.md",
        md!(r"
---
title: Source
---
See [[target]]
"),
    );
    write_md(
        tmp.path(),
        "target.md",
        md!(r"
---
title: Target
---
# Target
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["backlinks", "--file", "target.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "expected graceful skip; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("warning: skipping"),
        "expected warning on stderr; got: {stderr}"
    );
    assert!(
        stderr.contains("bad.md"),
        "warning should name the bad file; got: {stderr}"
    );

    // The good link should still be found
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON output");
    let backlinks = json["backlinks"].as_array().unwrap();
    assert_eq!(backlinks.len(), 1, "source.md should link to target.md");
}

/// `hyalo find --fields links,backlinks` must warn exactly once for a broken file,
/// not twice (once from link graph build, once from the per-file scan loop).
#[test]
fn error_find_links_backlinks_warning_deduplicated() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "bad.md",
        "---\nunclosed frontmatter without closing delimiter\n",
    );
    write_md(
        tmp.path(),
        "good.md",
        md!(r"
---
title: Good
---
# Good
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--fields", "links,backlinks"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "expected graceful skip; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8(output.stderr).unwrap();

    // The warning for bad.md must appear exactly once
    let warning_count = stderr.matches("bad.md").count();
    assert_eq!(
        warning_count, 1,
        "expected exactly one warning for bad.md, got {warning_count}; full stderr:\n{stderr}"
    );
}

/// `hyalo find --fields backlinks` must also gracefully skip broken files
/// during link graph construction.
#[test]
fn error_find_backlinks_field_unclosed_frontmatter_graceful_skip() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "bad.md",
        "---\nunclosed frontmatter without closing delimiter\n",
    );
    write_md(
        tmp.path(),
        "good.md",
        md!(r"
---
title: Good
---
# Good
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--fields", "backlinks"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "expected graceful skip; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("warning: skipping"),
        "expected warning on stderr; got: {stderr}"
    );
}

#[test]
fn error_missing_md_extension() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Test
---
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--file", "note"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stderr).unwrap();
    assert_eq!(json["error"], "file not found");
    assert!(json["hint"].as_str().unwrap().contains("note.md"));
}

#[test]
fn error_json_structure() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--file", "nope.md"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stderr).unwrap();

    // Error JSON must have an "error" field
    assert!(json.get("error").is_some());
    assert!(json["error"].is_string());
}

#[test]
fn error_text_format() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "--format",
            "text",
            "--no-hints",
            "find",
            "--file",
            "nope.md",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("Error:"));
    assert!(stderr.contains("file not found"));
}

#[test]
fn error_dir_pointing_to_file() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Test
---
# Body
"),
    );
    let file_path = tmp.path().join("note.md");

    let output = hyalo()
        .args(["--dir", file_path.to_str().unwrap()])
        .args(["find"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(1),
        "expected exit code 1; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("Error:"),
        "expected 'Error:' in stderr; got: {stderr}"
    );
    assert!(
        stderr.contains("is a file, not a directory"),
        "expected helpful message; got: {stderr}"
    );
    assert!(
        stderr.contains("--file"),
        "expected hint about --file; got: {stderr}"
    );
}
