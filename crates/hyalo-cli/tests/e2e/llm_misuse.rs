//! e2e tests for the LLM-misuse warning (iter-128).
//!
//! When an LLM-driven shell `cd`s into the configured `dir` or passes an
//! absolute `--file` path, hyalo emits a stderr warning that teaches the LLM
//! to run from the project root with vault-relative paths.

use super::common::{hyalo_no_hints, write_md};
use tempfile::TempDir;

/// Build a project root with a `.hyalo.toml` that pins `dir = "kb"`, plus a
/// pre-populated `kb/iterations/iteration-17.md` file.
fn make_project() -> TempDir {
    let project = TempDir::new().unwrap();
    std::fs::write(project.path().join(".hyalo.toml"), "dir = \"kb\"\n").unwrap();
    write_md(
        &project.path().join("kb"),
        "iterations/iteration-17.md",
        "---\ntitle: Iter 17\ntype: iteration\n---\nBody.\n",
    );
    project
}

#[test]
fn warns_when_cwd_is_inside_configured_dir() {
    let project = make_project();
    let vault = project.path().join("kb");

    let assert = hyalo_no_hints()
        .current_dir(&vault)
        .args(["find", "--limit", "1"])
        .assert()
        .success();

    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("hyalo is configured with dir = \"kb\""),
        "expected misuse warning in stderr, got: {stderr}"
    );
    assert!(
        stderr.contains("Do not cd into \"kb\""),
        "expected corrective text in stderr, got: {stderr}"
    );
}

#[test]
fn no_warning_when_cwd_is_project_root() {
    let project = make_project();

    let assert = hyalo_no_hints()
        .current_dir(project.path())
        .args(["find", "--limit", "1"])
        .assert()
        .success();

    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        !stderr.contains("hyalo is configured with dir"),
        "expected no misuse warning when run from project root, got: {stderr}"
    );
}

#[test]
fn warns_and_succeeds_when_absolute_file_inside_vault() {
    let project = make_project();
    // Canonicalize the project path so the absolute argument exactly matches
    // the canonical vault prefix (macOS tempdirs go through /private).
    let canonical_project = dunce::canonicalize(project.path()).unwrap();
    let abs_file = canonical_project
        .join("kb/iterations/iteration-17.md")
        .to_string_lossy()
        .into_owned();

    let assert = hyalo_no_hints()
        .current_dir(project.path())
        .args(["read", "--file", &abs_file])
        .assert()
        .success();

    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("hyalo is configured with dir = \"kb\""),
        "expected misuse warning when --file is absolute path inside vault, got: {stderr}"
    );

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(
        stdout.contains("Body."),
        "expected the file body to be read despite the warning, got: {stdout}"
    );
}

#[test]
fn absolute_file_outside_vault_still_errors() {
    let project = make_project();
    // Use a tempdir outside the project as a clearly-out-of-vault path.
    let outside = TempDir::new().unwrap();
    write_md(
        outside.path(),
        "stray.md",
        "---\ntitle: Stray\n---\nBody.\n",
    );
    let canonical_outside = dunce::canonicalize(outside.path()).unwrap();
    let abs_file = canonical_outside
        .join("stray.md")
        .to_string_lossy()
        .into_owned();

    let assert = hyalo_no_hints()
        .current_dir(project.path())
        .args(["read", "--file", &abs_file])
        .assert()
        .failure();

    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("outside vault") || stderr.contains("OutsideVault"),
        "expected outside-vault error for absolute path outside vault, got: {stderr}"
    );
    // And the misuse warning must NOT fire — that's reserved for paths that
    // resolve inside the vault.
    assert!(
        !stderr.contains("hyalo is configured with dir"),
        "misuse warning should not fire for out-of-vault absolute path, got: {stderr}"
    );
}

#[test]
fn warning_fires_only_once_for_multiple_file_args() {
    let project = make_project();
    write_md(
        &project.path().join("kb"),
        "iterations/iteration-18.md",
        "---\ntitle: Iter 18\ntype: iteration\n---\nBody 18.\n",
    );

    let canonical_project = dunce::canonicalize(project.path()).unwrap();
    let abs_a = canonical_project
        .join("kb/iterations/iteration-17.md")
        .to_string_lossy()
        .into_owned();
    let abs_b = canonical_project
        .join("kb/iterations/iteration-18.md")
        .to_string_lossy()
        .into_owned();

    // `set` accepts multiple --file flags; both are absolute and inside the vault.
    let assert = hyalo_no_hints()
        .current_dir(project.path())
        .args([
            "set",
            "--file",
            &abs_a,
            "--file",
            &abs_b,
            "--property",
            "status=in-progress",
        ])
        .assert()
        .success();

    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let occurrences = stderr
        .matches("hyalo is configured with dir = \"kb\"")
        .count();
    assert_eq!(
        occurrences, 1,
        "warning should be deduplicated; got {occurrences} occurrences:\n{stderr}"
    );
}

#[test]
fn quiet_flag_suppresses_misuse_warning() {
    let project = make_project();
    let vault = project.path().join("kb");

    let assert = hyalo_no_hints()
        .current_dir(&vault)
        .args(["--quiet", "find", "--limit", "1"])
        .assert()
        .success();

    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        !stderr.contains("hyalo is configured with dir"),
        "--quiet should suppress the misuse warning, got: {stderr}"
    );
}
