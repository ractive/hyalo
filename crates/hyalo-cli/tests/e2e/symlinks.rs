//! Write-path behaviour when the target of a mutation is a symlink
//! (iteration 191).
//!
//! Before iter-191 every mutating command replaced the symlink with a regular
//! file holding the new content: the aliasing relationship disappeared and the
//! real target silently kept the stale content. `fs_util::atomic_write` now
//! follows the link and replaces the *target* (DEC-062).
//!
//! Unix only: creating a symlink on Windows requires either developer mode or
//! elevation, so these tests cannot run there.
#![cfg(unix)]

use super::common::{hyalo_no_hints, write_md};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Build a vault holding `alias.md` — a symlink to a real note parked in a
/// hidden directory inside the same vault.
///
/// The target lives under `.store/` on purpose. The discovery walker skips
/// hidden directories, so the vault contains exactly *one* discoverable
/// markdown file. Without that, both the symlink and its target would be
/// walked as two separate files backed by one inode, and any whole-vault
/// command would write the same inode twice — a fixture artefact, not the
/// behaviour under test.
fn vault_with_symlink(content: &str) -> (TempDir, std::path::PathBuf) {
    let tmp = TempDir::new().unwrap();
    fs::create_dir(tmp.path().join(".store")).unwrap();
    let target = tmp.path().join(".store").join("real.md");
    fs::write(&target, content).unwrap();
    std::os::unix::fs::symlink(&target, tmp.path().join("alias.md")).unwrap();
    (tmp, target)
}

/// Assert `path` is still a symlink — i.e. the write went *through* it rather
/// than replacing it with a regular file.
fn assert_still_symlink(path: &Path) {
    let meta = fs::symlink_metadata(path)
        .unwrap_or_else(|e| panic!("{} should exist: {e}", path.display()));
    assert!(
        meta.file_type().is_symlink(),
        "{} must still be a symlink after the write",
        path.display()
    );
}

// ---------------------------------------------------------------------------
// task toggle
// ---------------------------------------------------------------------------

#[test]
fn task_toggle_follows_intra_vault_symlink() {
    let (tmp, target) = vault_with_symlink("---\ntitle: Test\n---\n# Tasks\n- [ ] First task\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["task", "toggle", "--file", "alias.md", "--line", "5"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "toggle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        fs::read_to_string(&target).unwrap().contains("- [x] First"),
        "the symlink target must carry the toggled task"
    );
    assert_still_symlink(&tmp.path().join("alias.md"));
}

// ---------------------------------------------------------------------------
// lint --fix
// ---------------------------------------------------------------------------

#[test]
fn lint_fix_through_symlink_is_idempotent() {
    // Trailing whitespace on the body line is a fixable MD009 violation.
    let (tmp, target) =
        vault_with_symlink("---\ntitle: Test\ntype: note\n---\n\n# Heading\n\nBody   \n");
    write_md(
        tmp.path(),
        ".hyalo.toml",
        "dir = \".\"\n[schema.types.note]\nrequired = [\"title\"]\n",
    );

    let first = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["lint", "--fix", "--format", "json"])
        .output()
        .unwrap();
    let first_out = String::from_utf8_lossy(&first.stdout).to_string();

    assert!(
        !fs::read_to_string(&target).unwrap().contains("Body   \n"),
        "the fix must land on the symlink target, not on a replacement file: {first_out}"
    );
    assert_still_symlink(&tmp.path().join("alias.md"));

    // Second run: nothing left to report.
    let second = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["lint", "--format", "json"])
        .output()
        .unwrap();
    let val: serde_json::Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_eq!(
        val["results"]["total"],
        0,
        "second lint must be clean, got: {}",
        String::from_utf8_lossy(&second.stdout)
    );
}

// ---------------------------------------------------------------------------
// Boundary: following a symlink must not become an escape hatch
// ---------------------------------------------------------------------------

#[test]
fn symlink_escaping_vault_is_refused() {
    let outside = TempDir::new().unwrap();
    let secret = outside.path().join("secret.md");
    fs::write(&secret, "---\ntitle: Secret\n---\n- [ ] Untouched\n").unwrap();

    let tmp = TempDir::new().unwrap();
    std::os::unix::fs::symlink(&secret, tmp.path().join("escape.md")).unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["task", "toggle", "--file", "escape.md", "--line", "4"])
        .output()
        .unwrap();

    assert!(!output.status.success(), "escaping symlink must be refused");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("file resolves outside vault boundary"),
        "expected the vault-boundary error, got: {combined}"
    );
    assert!(
        fs::read_to_string(&secret)
            .unwrap()
            .contains("- [ ] Untouched"),
        "the out-of-vault file must be byte-for-byte untouched"
    );
}

#[test]
fn set_symlink_escaping_vault_is_refused() {
    let outside = TempDir::new().unwrap();
    let secret = outside.path().join("secret.md");
    fs::write(&secret, "---\ntitle: Secret\n---\nBody\n").unwrap();

    let tmp = TempDir::new().unwrap();
    std::os::unix::fs::symlink(&secret, tmp.path().join("escape.md")).unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["set", "--file", "escape.md", "--property", "status=done"])
        .output()
        .unwrap();

    assert!(!output.status.success(), "escaping symlink must be refused");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("file resolves outside vault boundary"),
        "expected the vault-boundary error, got: {combined}"
    );
    assert!(
        !fs::read_to_string(&secret).unwrap().contains("status:"),
        "the out-of-vault file must be byte-for-byte untouched"
    );
}

#[test]
fn append_symlink_escaping_vault_is_refused() {
    let outside = TempDir::new().unwrap();
    let secret = outside.path().join("secret.md");
    fs::write(&secret, "---\ntitle: Secret\ntags:\n  - one\n---\nBody\n").unwrap();

    let tmp = TempDir::new().unwrap();
    std::os::unix::fs::symlink(&secret, tmp.path().join("escape.md")).unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["append", "--file", "escape.md", "--property", "tags=two"])
        .output()
        .unwrap();

    assert!(!output.status.success(), "escaping symlink must be refused");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("file resolves outside vault boundary"),
        "expected the vault-boundary error, got: {combined}"
    );
    assert!(
        !fs::read_to_string(&secret).unwrap().contains("two"),
        "the out-of-vault file must be byte-for-byte untouched"
    );
}

// ---------------------------------------------------------------------------
// One test per mutating command — no shared fixture shortcuts
// ---------------------------------------------------------------------------

#[test]
fn set_through_symlink_updates_target() {
    let (tmp, target) = vault_with_symlink("---\ntitle: Test\n---\n# Body\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["set", "--file", "alias.md", "--property", "status=done"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "set failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        fs::read_to_string(&target)
            .unwrap()
            .contains("status: done"),
        "set must write the symlink target"
    );
    assert_still_symlink(&tmp.path().join("alias.md"));
}

#[test]
fn append_through_symlink_updates_target() {
    let (tmp, target) = vault_with_symlink("---\ntitle: Test\ntags:\n  - one\n---\n# Body\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["append", "--file", "alias.md", "--property", "tags=two"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "append failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(&target).unwrap();
    assert!(content.contains("one"), "existing tag must survive");
    assert!(
        content.contains("two"),
        "append must write the symlink target, got: {content}"
    );
    assert_still_symlink(&tmp.path().join("alias.md"));
}

#[test]
fn mv_link_rewrite_through_symlink_updates_target() {
    // `mv` writes through `atomic_write` when it rewrites *inbound* links, so
    // the symlinked file here is the one holding the link, not the one moved.
    let (tmp, target) = vault_with_symlink("---\ntitle: Hub\n---\nSee [B](b.md) for details.\n");
    write_md(tmp.path(), "b.md", "---\ntitle: B\n---\n# B\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "archive/b.md"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "mv failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(&target).unwrap();
    assert!(
        content.contains("[B](archive/b.md)"),
        "the rewritten link must land on the symlink target, got: {content}"
    );
    assert_still_symlink(&tmp.path().join("alias.md"));
}
