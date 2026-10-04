//! `hyalo new --dry-run` must not hint `lint --file <path>` for a file it
//! never wrote.
//!
//! `new`'s hint used to unconditionally suggest validating the scaffolded
//! file with `lint --file <path>`, even under `--dry-run`, where the file
//! was only printed to stdout and never created — so the hinted command
//! would just report the path doesn't exist.

use super::common::{hyalo, write_md};

fn setup_with_note_type() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    write_md(
        tmp.path(),
        "placeholder.md",
        "---\ntitle: Placeholder\n---\n",
    );
    std::fs::write(
        tmp.path().join(".hyalo.toml"),
        "dir = \".\"\n\n[schema.types.note]\nrequired = [\"title\"]\n",
    )
    .unwrap();
    tmp
}

#[test]
fn dry_run_drops_the_lint_file_hint() {
    let tmp = setup_with_note_type();

    let output = hyalo()
        .arg("--dir")
        .arg(tmp.path())
        .args([
            "new",
            "--type",
            "note",
            "--file",
            "fresh.md",
            "--dry-run",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "new --dry-run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !tmp.path().join("fresh.md").exists(),
        "--dry-run must not create the file"
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json output");
    let hints = json["hints"].as_array().cloned().unwrap_or_default();
    assert!(
        !hints.iter().any(|h| h["cmd"]
            .as_str()
            .is_some_and(|c| c.contains("lint") && c.contains("fresh.md"))),
        "a dry run must not hint validating a file that was never written: {hints:?}"
    );
}

/// Without `--dry-run`, the file is actually written, so the `lint --file`
/// hint is still expected.
#[test]
fn real_run_keeps_the_lint_file_hint() {
    let tmp = setup_with_note_type();

    let output = hyalo()
        .arg("--dir")
        .arg(tmp.path())
        .args([
            "new", "--type", "note", "--file", "fresh.md", "--format", "json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "new failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(tmp.path().join("fresh.md").exists());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json output");
    let hints = json["hints"].as_array().cloned().unwrap_or_default();
    assert!(
        hints.iter().any(|h| h["cmd"]
            .as_str()
            .is_some_and(|c| c.contains("lint") && c.contains("fresh.md"))),
        "expected the lint --file hint after an actual write: {hints:?}"
    );
}
