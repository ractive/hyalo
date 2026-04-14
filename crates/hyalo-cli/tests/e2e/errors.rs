use super::common::{hyalo_no_hints, md, write_md};
use tempfile::TempDir;

#[test]
fn error_nonexistent_file() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo_no_hints()
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

    let output = hyalo_no_hints()
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

    let output = hyalo_no_hints()
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

    let output = hyalo_no_hints()
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

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
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
    let total = json["results"]["files"]["total"].as_u64().unwrap();
    assert_eq!(total, 1, "only the good file should be counted");
    let by_dir = json["results"]["files"]["directories"].as_array().unwrap();
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

    let output = hyalo_no_hints()
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
    let backlinks = json["results"]["backlinks"].as_array().unwrap();
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

    let output = hyalo_no_hints()
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

    let output = hyalo_no_hints()
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

    let output = hyalo_no_hints()
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

    let output = hyalo_no_hints()
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

    let output = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "--format",
            "text",
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

    let output = hyalo_no_hints()
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

// ── CWD-relative path fallback (iteration 97) ─────────────────────

/// When --file includes the dir prefix (CWD-relative), hyalo should strip it
/// and resolve correctly.
#[test]
fn cwd_relative_file_path_resolved() {
    let tmp = TempDir::new().unwrap();
    let kb = tmp.path().join("kb");
    write_md(
        &kb,
        "note.md",
        md!(r"
---
title: Hello
---
# Body
"),
    );

    let output = hyalo_no_hints()
        .args(["--dir", kb.to_str().unwrap()])
        .args(["find", "--file", "kb/note.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "expected CWD-relative path to resolve; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let results = json["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0]["file"], "note.md",
        "returned path should be dir-relative"
    );
}

/// CWD-relative fallback works for nested paths too.
#[test]
fn cwd_relative_nested_file_path_resolved() {
    let tmp = TempDir::new().unwrap();
    let kb = tmp.path().join("kb");
    write_md(
        &kb,
        "sub/deep.md",
        md!(r"
---
title: Deep
---
# Body
"),
    );

    let output = hyalo_no_hints()
        .args(["--dir", kb.to_str().unwrap()])
        .args(["find", "--file", "kb/sub/deep.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "expected CWD-relative nested path to resolve; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let results = json["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["file"], "sub/deep.md");
}

/// When both dir-relative and CWD-relative interpretations exist, the prefix
/// is stripped unconditionally (CWD-relative wins).
#[test]
fn cwd_relative_strips_prefix_unconditionally() {
    let tmp = TempDir::new().unwrap();
    let kb = tmp.path().join("kb");
    // Create kb/note.md (vault-relative: note.md)
    write_md(
        &kb,
        "note.md",
        md!(r"
---
title: Top
---
"),
    );
    // Create kb/kb/note.md (vault-relative: kb/note.md)
    write_md(
        &kb,
        "kb/note.md",
        md!(r"
---
title: Nested
---
"),
    );

    let output = hyalo_no_hints()
        .args(["--dir", kb.to_str().unwrap()])
        .args(["find", "--file", "kb/note.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let results = json["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    // Prefix stripped unconditionally: resolves to note.md (title: Top)
    assert_eq!(results[0]["file"], "note.md");
    assert_eq!(results[0]["properties"]["title"], "Top");
}

/// Mutation commands (set) also accept CWD-relative paths.
#[test]
fn cwd_relative_path_works_with_set() {
    let tmp = TempDir::new().unwrap();
    let kb = tmp.path().join("kb");
    write_md(
        &kb,
        "note.md",
        md!(r"
---
title: Hello
---
# Body
"),
    );

    let output = hyalo_no_hints()
        .args(["--dir", kb.to_str().unwrap()])
        .args(["set", "--file", "kb/note.md", "--property", "status=done"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "set with CWD-relative path should succeed; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify the property was actually set
    let content = std::fs::read_to_string(kb.join("note.md")).unwrap();
    assert!(
        content.contains("status: done"),
        "property should be set in file"
    );
}
