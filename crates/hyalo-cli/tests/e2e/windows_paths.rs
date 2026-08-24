//! T-4 (iter-224): CLI-level (e2e) coverage for M-2 — Windows drive-relative
//! paths (`C:foo`) and NTFS Alternate Data Stream markers (`a.md:stream`) in
//! a `--file` argument (`reviews/adversarial-review-2026-08-23.md`,
//! `crates/hyalo-core/src/discovery.rs::has_unsafe_windows_colon`).
//!
//! Before this file, M-2 had only `hyalo-core` unit coverage
//! (`discovery.rs`'s `resolve_file_rejects_windows_drive_relative_path` /
//! `resolve_file_rejects_ntfs_alternate_data_stream_path`, and the mirrored
//! snapshot-loader checks in `index.rs`) — nothing exercised the refusal
//! through the actual CLI, where the error has to reach the user with the
//! right exit code and message rather than just the right `Result` variant.
//!
//! Windows-only: `has_unsafe_windows_colon` is a deliberate no-op on every
//! other platform (a colon is a legal filename character there), so these
//! shapes are only rejections on Windows. CI runs `windows-latest`, so this
//! executes for real; see `discovery.rs` for the platform-gated unit tests
//! this file complements rather than duplicates.
#![cfg(windows)]

use super::common::hyalo_no_hints;
use tempfile::TempDir;

fn run(vault: &std::path::Path, args: &[&str]) -> (i32, String) {
    let out = hyalo_no_hints()
        .args(["--dir", vault.to_str().unwrap()])
        .args(args)
        .output()
        .unwrap();
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code().unwrap_or(-1), combined)
}

fn assert_rejected(code: i32, combined: &str, context: &str) {
    assert_eq!(
        code, 1,
        "{context}: expected exit 1, got {code}: {combined}"
    );
    assert!(
        combined.contains("resolves outside vault boundary"),
        "{context}: expected the vault-boundary wording, got: {combined}"
    );
}

// --- drive-relative (`C:foo.md`) --------------------------------------

#[test]
fn read_rejects_drive_relative_path() {
    let tmp = TempDir::new().unwrap();
    let (code, out) = run(tmp.path(), &["read", "C:foo.md"]);
    assert_rejected(code, &out, "read C:foo.md");
}

#[test]
fn set_rejects_drive_relative_path() {
    let tmp = TempDir::new().unwrap();
    let (code, out) = run(
        tmp.path(),
        &["set", "--file", "C:foo.md", "--property", "status=done"],
    );
    assert_rejected(code, &out, "set --file C:foo.md");
}

// --- NTFS Alternate Data Stream (`a.md:stream`) -------------------------

#[test]
fn read_rejects_ntfs_alternate_data_stream() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("a.md"), "---\ntitle: A\n---\nbody\n").unwrap();
    let (code, out) = run(tmp.path(), &["read", "a.md:stream"]);
    assert_rejected(code, &out, "read a.md:stream");
}

#[test]
fn set_rejects_ntfs_alternate_data_stream() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("a.md"), "---\ntitle: A\n---\nbody\n").unwrap();
    let (code, out) = run(
        tmp.path(),
        &["set", "--file", "a.md:stream", "--property", "status=done"],
    );
    assert_rejected(code, &out, "set --file a.md:stream");

    // The refusal must be total: the real file is untouched, and nothing was
    // written to an actual alternate data stream on it either.
    let content = std::fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(
        !content.contains("status: done"),
        "a.md must be untouched by a rejected write: {content}"
    );
}

// --- ordinary colon-free paths still work (no false positives) ---------

#[test]
fn read_accepts_ordinary_nested_path() {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join("sub")).unwrap();
    std::fs::write(
        tmp.path().join("sub/note.md"),
        "---\ntitle: Note\n---\nbody\n",
    )
    .unwrap();
    let (code, out) = run(tmp.path(), &["read", "sub/note.md"]);
    assert_eq!(code, 0, "sub/note.md should resolve fine: {out}");
}
