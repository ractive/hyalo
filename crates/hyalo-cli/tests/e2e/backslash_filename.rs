//! BUG-11 (dogfood-v0250, iter-314, DEC-363): a literal backslash in a
//! filename is legal on macOS and Linux — POSIX treats it as an ordinary
//! filename byte, never a path separator. `notes/back\slash.md` used to be
//! silently corrupted into the nonexistent `notes/back/slash.md` by a blind
//! `path.replace('\\', "/")` at every command that touches a real OS path,
//! so display showed the wrong name, lookups failed, and batch mutations
//! crashed with an internal `io::Error` (exit 2).
//!
//! This is the consolidated regression test the PR #378 review asked for,
//! covering every command dogfood-v0250's own repro named in one place:
//! `find` (default listing), `--glob`, `hyalo backlinks`, `set --glob`, and
//! `mv --glob`. Each individual fix also has its own narrower pin elsewhere
//! (`files_from.rs`, `links.rs`, `mv.rs`) for the call sites this file does
//! not reach on its own (`--files-from`, `links fix`, an `mv --to`
//! destination).
//!
//! Gated to non-Windows, where `\` is a literal filename byte rather than
//! the real path separator.
#![cfg(unix)]

use super::common::{hyalo_no_hints, write_md};
use tempfile::TempDir;

/// Builds a vault with a backslash-named file that both has an outbound
/// link (for `--glob` coverage) and is linked *to* by another file (for
/// `backlinks` coverage).
fn build_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "notes/back\\slash.md",
        "---\ntitle: Back\n---\n[[other]]\n",
    );
    write_md(
        tmp.path(),
        "other.md",
        "---\ntitle: Other\n---\n[[notes/back\\slash]]\n",
    );
    tmp
}

#[test]
fn find_reports_the_real_filename() {
    let tmp = build_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let files: Vec<&str> = envelope["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["file"].as_str().unwrap())
        .collect();
    assert!(
        files.contains(&"notes/back\\slash.md"),
        "the real name must be reported, never `notes/back/slash.md`: {files:?}"
    );
}

#[test]
fn glob_matches_the_real_filename() {
    let tmp = build_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--glob", "notes/*.md", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        envelope["total"].as_u64(),
        Some(1),
        "the glob must match the backslash-named file: {envelope}"
    );
    assert_eq!(
        envelope["results"][0]["file"].as_str(),
        Some("notes/back\\slash.md")
    );
}

#[test]
fn backlinks_reports_the_real_source_filename() {
    let tmp = build_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "backlinks",
            "--file",
            "notes/back\\slash.md",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let sources: Vec<&str> = envelope["results"]["backlinks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["source"].as_str().unwrap())
        .collect();
    assert_eq!(
        sources,
        vec!["other.md"],
        "backlinks must resolve `other.md`'s link to the backslash-named file: {envelope}"
    );
}

#[test]
fn set_glob_writes_the_real_filename() {
    let tmp = build_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "set",
            "--glob",
            "notes/*.md",
            "--property",
            "status=done",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "set --glob must not crash on a backslash-named file (exit 2 is the \
         BUG-11 symptom): stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        envelope["results"]["modified"]
            .as_array()
            .unwrap()
            .as_slice(),
        &[serde_json::json!("notes/back\\slash.md")],
        "{envelope}"
    );
    let text = std::fs::read_to_string(tmp.path().join("notes").join("back\\slash.md")).unwrap();
    assert!(text.contains("status: done"), "{text:?}");
}

#[test]
fn mv_glob_moves_the_real_filename() {
    let tmp = build_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "mv",
            "--glob",
            "notes/*.md",
            "--to",
            "archive/",
            "--dry-run",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "mv --glob must not crash on a backslash-named file (exit 2 is the \
         BUG-11 symptom): stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let moves = envelope["results"]["moves"].as_array().unwrap();
    assert_eq!(moves.len(), 1, "{envelope}");
    assert_eq!(
        moves[0]["from"].as_str(),
        Some("notes/back\\slash.md"),
        "{envelope}"
    );
    assert_eq!(
        moves[0]["to"].as_str(),
        Some("archive/back\\slash.md"),
        "the destination must keep the real filename too: {envelope}"
    );
}
