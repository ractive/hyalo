/// E2E regression test for F2 (codebase review 2026-10-03): batch `mv`
/// panicked on a filename containing a 4-byte UTF-8 character (an emoji)
/// when the moved file's own body held a self-link, because
/// `hyalo_core::discovery::directory_for_index_file` sliced the candidate
/// path at a byte offset computed from a fixed ASCII suffix length without
/// first checking that the offset landed on a char boundary.
use super::common::{hyalo_no_hints, write_md};
use std::fs;
use tempfile::TempDir;

#[test]
fn batch_mv_does_not_panic_on_emoji_filename_with_self_link() {
    let tmp = TempDir::new().unwrap();
    // `dir = "."` (the review's exact repro vault shape).
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write_md(
        tmp.path(),
        "notes/Emoji 🚀 Note.md",
        "# test\n\nSee also [[Emoji 🚀 Note]] and [link](Emoji%20%F0%9F%9A%80%20Note.md).\n",
    );
    let dir = tmp.path().to_str().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", dir])
        .args(["mv", "--glob", "notes/*.md", "--to", "archive/", "--apply"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "batch mv on an emoji filename must not panic; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let results = &json["results"];
    assert_eq!(results["totals"]["moves"], 1);
    assert!(
        tmp.path().join("archive/Emoji 🚀 Note.md").exists(),
        "moved file must exist at its new, still-emoji-bearing path"
    );
    assert!(
        !tmp.path().join("notes/Emoji 🚀 Note.md").exists(),
        "source must be gone after apply"
    );

    // The moved file's own self-link must have been rewritten, not dropped.
    let moved = fs::read_to_string(tmp.path().join("archive/Emoji 🚀 Note.md")).unwrap();
    assert!(
        moved.contains("[[Emoji 🚀 Note]]"),
        "self wikilink should still resolve after the move: {moved}"
    );
}

#[test]
fn single_file_mv_already_worked_on_emoji_filename_with_self_link() {
    // Companion to the batch test above: single-file `mv` of the same
    // fixture never panicked (per the review's repro notes), so this pins
    // that it still doesn't regress alongside the batch-mode fix.
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write_md(
        tmp.path(),
        "notes/Emoji 🚀 Note.md",
        "# test\n\nSee also [[Emoji 🚀 Note]].\n",
    );
    let dir = tmp.path().to_str().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", dir])
        .args(["mv", "notes/Emoji 🚀 Note.md", "archive/Emoji 🚀 Note.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(tmp.path().join("archive/Emoji 🚀 Note.md").exists());
}
