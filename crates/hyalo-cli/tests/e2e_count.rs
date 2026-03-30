mod common;

use common::{hyalo_no_hints, write_md, write_tagged};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn setup_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "a.md", &["rust", "cli"]);
    write_tagged(tmp.path(), "b.md", &["rust", "iteration"]);
    write_md(tmp.path(), "c.md", "No frontmatter.\n");
    tmp
}

// ---------------------------------------------------------------------------
// Basic --count usage
// ---------------------------------------------------------------------------

#[test]
fn count_find_all_files() {
    let tmp = setup_vault();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--count"])
        .args(["find"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "3");
}

#[test]
fn count_find_filtered_by_tag() {
    let tmp = setup_vault();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--count"])
        .args(["find", "--tag", "rust"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "2");
}

#[test]
fn count_tags_summary() {
    let tmp = setup_vault();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--count"])
        .args(["tags", "summary"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "3");
}

#[test]
fn count_zero_results() {
    let tmp = setup_vault();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--count"])
        .args(["find", "--tag", "nonexistent"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "0");
}

// ---------------------------------------------------------------------------
// --count with --format (output is always bare integer)
// ---------------------------------------------------------------------------

#[test]
fn count_with_format_text() {
    let tmp = setup_vault();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "text"])
        .args(["--count"])
        .args(["find"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "3");
}

#[test]
fn count_with_format_json() {
    let tmp = setup_vault();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "json"])
        .args(["--count"])
        .args(["find"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "3");
}

// ---------------------------------------------------------------------------
// Conflict: --count + --jq
// ---------------------------------------------------------------------------

#[test]
fn count_with_jq_errors() {
    let tmp = setup_vault();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--count"])
        .args(["--jq", ".total"])
        .args(["find"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(
        output.status.code(),
        Some(2),
        "expected exit code 2 (usage error)"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--count cannot be combined with --jq"),
        "expected conflict error, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// --count on non-list command
// ---------------------------------------------------------------------------

#[test]
fn count_on_read_command_errors() {
    let tmp = setup_vault();
    write_md(tmp.path(), "note.md", "---\ntitle: Test\n---\nBody\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--count"])
        .args(["read", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(
        output.status.code(),
        Some(1),
        "expected exit code 1 (user error)"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--count is only supported for list commands"),
        "expected unsupported error, got: {stderr}"
    );
}
