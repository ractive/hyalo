use super::common::{hyalo_no_hints, write_md};

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

    let output = hyalo_no_hints()
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

    let output = hyalo_no_hints()
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

    let output = hyalo_no_hints()
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

    let output = hyalo_no_hints()
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

// ---------------------------------------------------------------------------
// --filter typo → suggest --property (not --file)
// ---------------------------------------------------------------------------

#[test]
fn suggest_property_when_filter_used() {
    let tmp = tempfile::tempdir().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--filter", "status=draft"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--property"),
        "expected '--property' suggestion in stderr; got: {stderr}"
    );
    assert!(
        !stderr.contains("--file"),
        "unexpected '--file' suggestion in stderr; got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Bug 7 — bare-word typos that resemble top-level flags should suggest them
// ---------------------------------------------------------------------------

#[test]
fn suggest_version_for_typo() {
    let output = hyalo_no_hints().arg("versio").output().unwrap();

    assert!(
        !output.status.success(),
        "expected failure for unknown subcommand 'versio'"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--version"),
        "expected '--version' suggestion in stderr; got: {stderr}"
    );
}

#[test]
fn suggest_help_for_typo() {
    let output = hyalo_no_hints().arg("hep").output().unwrap();

    assert!(
        !output.status.success(),
        "expected failure for unknown subcommand 'hep'"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--help"),
        "expected '--help' suggestion in stderr; got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// NEW-UX-1: `append --tag` → friendly hint, not clap's unknown-arg error
// ---------------------------------------------------------------------------

#[test]
fn append_tag_shows_friendly_hint() {
    let tmp = tempfile::tempdir().unwrap();
    setup_file(&tmp);

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["append", "tasks.md", "--tag", "foo"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("`hyalo append` does not accept --tag"),
        "expected friendly hint in stderr; got: {stderr}"
    );
    assert!(
        stderr.contains("hyalo set"),
        "expected `hyalo set` recommendation in stderr; got: {stderr}"
    );
}

#[test]
fn append_tag_hint_only_fires_for_real_append_subcommand() {
    // CodeRabbit review finding: the hint previously matched any argv element
    // equal to "append", so commands like `hyalo find append --tag foo` also
    // got the `hyalo append`-specific message. Gate on the resolved top-level
    // subcommand instead.
    let tmp = tempfile::tempdir().unwrap();
    setup_file(&tmp);

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "append", "--tag", "foo"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("`hyalo append` does not accept --tag"),
        "append-specific hint must not fire when subcommand is `find`; got: {stderr}"
    );
}
