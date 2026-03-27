mod common;

use common::{hyalo, write_md};

// ---------------------------------------------------------------------------
// e2e tests for subcommand-flag suggestion
// ---------------------------------------------------------------------------
//
// These tests verify that when the user passes a subcommand name as a
// `--flag` (e.g. `--toggle` instead of `toggle`), the CLI prints a
// "did you mean" tip to stderr and exits with code 2.

fn setup_file(tmp: &tempfile::TempDir) {
    write_md(
        tmp.path(),
        "tasks.md",
        "---\ntitle: Test\n---\n- [ ] First task\n",
    );
}

// ---------------------------------------------------------------------------
// task subcommand misplacement
// ---------------------------------------------------------------------------

#[test]
fn suggest_task_toggle_as_flag() {
    let tmp = tempfile::tempdir().unwrap();
    setup_file(&tmp);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["task", "--toggle", "--file", "tasks.md", "--line", "4"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("did you mean"),
        "expected 'did you mean' in stderr; got: {stderr}"
    );
    assert!(
        stderr.contains("task toggle"),
        "expected corrected command 'task toggle' in stderr; got: {stderr}"
    );
}

#[test]
fn suggest_properties_rename_as_flag() {
    let tmp = tempfile::tempdir().unwrap();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "--rename", "--from", "old", "--to", "new"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("did you mean"),
        "expected 'did you mean' in stderr; got: {stderr}"
    );
    assert!(
        stderr.contains("properties rename"),
        "expected 'properties rename' in stderr; got: {stderr}"
    );
}

#[test]
fn suggest_tags_summary_as_flag() {
    let tmp = tempfile::tempdir().unwrap();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "--summary"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("did you mean"),
        "expected 'did you mean' in stderr; got: {stderr}"
    );
    assert!(
        stderr.contains("tags summary"),
        "expected 'tags summary' in stderr; got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// No suggestion for valid commands
// ---------------------------------------------------------------------------

#[test]
fn no_suggestion_for_valid_task_toggle() {
    let tmp = tempfile::tempdir().unwrap();
    setup_file(&tmp);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["task", "toggle", "--file", "tasks.md", "--line", "4"])
        .output()
        .unwrap();

    // Should succeed (or fail with exit 1 for a content error, not 2)
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("did you mean"),
        "unexpected suggestion for a valid command; stderr: {stderr}"
    );
    assert_ne!(
        output.status.code(),
        Some(2),
        "exit code 2 indicates a clap error was hit; stderr: {stderr}"
    );
}
