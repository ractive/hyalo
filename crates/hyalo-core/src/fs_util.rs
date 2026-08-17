#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result, bail};
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

/// Maximum number of symlink hops followed when resolving a write destination.
///
/// A chain longer than this is treated as a loop (or as pathological) and the
/// write is refused rather than spun on.
const MAX_SYMLINK_HOPS: usize = 32;

/// Resolve a write destination through any symlink chain, returning the real
/// file that should be replaced.
///
/// Deliberately *not* `fs::canonicalize`: the destination of a write may not
/// exist yet (creating a brand-new note), and `canonicalize` fails outright in
/// that case. This walks `read_link` manually instead, stopping at the first
/// component that is not a symlink — existing or not.
///
/// Relative link targets are resolved against the directory holding the link,
/// matching kernel semantics. Returns `path` unchanged when it is not a
/// symlink, which is the common case and costs one `symlink_metadata` call.
fn resolve_write_target(path: &Path) -> Result<PathBuf> {
    let mut current = path.to_path_buf();
    for _ in 0..MAX_SYMLINK_HOPS {
        let is_symlink = std::fs::symlink_metadata(&current)
            .is_ok_and(|m| m.file_type().is_symlink());
        if !is_symlink {
            return Ok(current);
        }
        let target = std::fs::read_link(&current)
            .with_context(|| format!("failed to read symlink {}", current.display()))?;
        current = if target.is_absolute() {
            target
        } else {
            current.parent().unwrap_or(Path::new(".")).join(target)
        };
    }
    bail!(
        "refusing to write through a symlink chain deeper than {MAX_SYMLINK_HOPS} hops: {}",
        path.display()
    )
}

/// Verify that a symlink-resolved destination is still inside `vault_root`.
///
/// Called only when resolution actually changed the path, so ordinary
/// (non-symlink) writes pay no extra syscalls.
fn ensure_resolved_within(vault_root: &Path, original: &Path, resolved: &Path) -> Result<()> {
    let canonical_root = dunce::canonicalize(vault_root).with_context(|| {
        format!(
            "failed to canonicalize vault dir {} while checking symlink target",
            vault_root.display()
        )
    })?;
    // The resolved file itself may not exist yet (dangling symlink), so
    // canonicalize its parent and re-attach the file name.
    let parent = resolved
        .parent()
        .context("cannot determine parent directory of symlink target")?;
    let canonical_parent = dunce::canonicalize(parent).with_context(|| {
        format!(
            "failed to resolve symlink target directory {}",
            parent.display()
        )
    })?;
    let canonical_target = match resolved.file_name() {
        Some(name) => canonical_parent.join(name),
        None => canonical_parent,
    };
    if !canonical_target.starts_with(&canonical_root) {
        bail!(
            "refusing to write through symlink: {} resolves outside vault boundary: {}",
            original.display(),
            canonical_target.display()
        );
    }
    Ok(())
}

/// Write data to a file atomically.
///
/// Creates a temporary file in the same directory as the destination, writes
/// all data, flushes it to stable storage, then renames it into place. A crash
/// mid-write therefore never leaves a truncated or corrupted file — the
/// original is either fully replaced or left untouched.
///
/// **Durability:** the temp file is `sync_all`ed *before* the rename, and on
/// Unix the destination's parent directory is `sync_all`ed *after* it, so the
/// rename itself survives a power loss. The directory fsync is best-effort —
/// some filesystems reject `fsync` on a directory handle, and failing the whole
/// write there would be worse than the missing guarantee (see DEC-063).
///
/// **Symlinks:** when the destination is a symlink, the link is followed (up to
/// [`MAX_SYMLINK_HOPS`] hops) and the *target* is replaced; the symlink stays a
/// symlink (DEC-062). Following is unconditional here — this entry point has no
/// vault context — so callers must have already validated the path. Every CLI
/// mutation path does, via `discovery::resolve_file`, which canonicalizes and
/// enforces the vault boundary. Callers that hold the vault root should prefer
/// [`atomic_write_within`], which re-checks the boundary against the *resolved*
/// destination.
///
/// When the destination already exists, its permissions are preserved.
/// `NamedTempFile` defaults to mode `0600`, so without this step rewrites
/// would silently tighten file permissions on every mutation.
pub fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    write_impl(None, path, data)
}

/// Like [`atomic_write`], but refuses to follow a symlink whose target escapes
/// `vault_root`.
///
/// Use this from every call site that knows the vault directory. The extra
/// canonicalization runs only when the destination really was a symlink, so
/// the ordinary write path is unaffected.
pub fn atomic_write_within(vault_root: &Path, path: &Path, data: &[u8]) -> Result<()> {
    write_impl(Some(vault_root), path, data)
}

fn write_impl(vault_root: Option<&Path>, path: &Path, data: &[u8]) -> Result<()> {
    let dest = resolve_write_target(path)?;
    if dest != path {
        if let Some(root) = vault_root {
            ensure_resolved_within(root, path, &dest)?;
        }
    }
    let dest = dest.as_path();

    let parent = dest
        .parent()
        .context("cannot determine parent directory for atomic write")?;

    // Capture existing permissions (if any) before the rename so we can restore
    // them on the new file — otherwise `NamedTempFile`'s default `0600` wins.
    let existing_perms = std::fs::metadata(dest).ok().map(|m| m.permissions());

    let mut tmp = NamedTempFile::new_in(parent)
        .with_context(|| format!("failed to create temp file in {}", parent.display()))?;

    tmp.write_all(data)
        .with_context(|| format!("failed to write temp file for {}", dest.display()))?;

    if let Some(perms) = existing_perms.clone() {
        std::fs::set_permissions(tmp.path(), perms).with_context(|| {
            format!(
                "failed to restore permissions on temp file for {}",
                dest.display()
            )
        })?;
    }

    // Flush the data before the rename: without this, a crash right after the
    // rename can leave the new directory entry pointing at unwritten blocks.
    tmp.as_file()
        .sync_all()
        .with_context(|| format!("failed to flush temp file for {}", dest.display()))?;

    tmp.persist(dest)
        .with_context(|| format!("failed to persist temp file to {}", dest.display()))?;

    // On some platforms `persist` can reset the mode; re-apply for safety.
    if let Some(perms) = existing_perms {
        std::fs::set_permissions(dest, perms)
            .with_context(|| format!("failed to restore permissions on {}", dest.display()))?;
    }

    sync_parent_dir(parent);

    Ok(())
}

/// Best-effort fsync of the directory holding a just-renamed file, so the
/// rename itself is durable. No-op on non-Unix targets, where directory
/// handles cannot be opened this way.
#[cfg(unix)]
fn sync_parent_dir(parent: &Path) {
    if let Ok(dir) = std::fs::File::open(parent) {
        drop(dir.sync_all());
    }
}

#[cfg(not(unix))]
fn sync_parent_dir(_parent: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("output.txt");
        atomic_write(&target, b"hello world").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "hello world");
    }

    #[test]
    fn atomic_write_overwrites_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("output.txt");
        std::fs::write(&target, "old content").unwrap();
        atomic_write(&target, b"new content").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new content");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_preserves_existing_mode_0644() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("mode-0644.txt");
        std::fs::write(&target, "old").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644)).unwrap();
        atomic_write(&target, b"new content").unwrap();
        let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644, "mode should be preserved across rewrite");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_preserves_existing_mode_0600() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("mode-0600.txt");
        std::fs::write(&target, "old").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        atomic_write(&target, b"new content").unwrap();
        let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "tight mode should be preserved across rewrite");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_new_file_uses_platform_default() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("brand-new.txt");
        atomic_write(&target, b"data").unwrap();
        // For a brand-new file we don't enforce a specific mode — just make sure the
        // file was created. Platform umask governs the exact bits.
        let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert!(mode != 0, "mode should be non-zero: {mode:o}");
    }

    /// Durability is not observable from a unit test — a real power-loss
    /// window cannot be reproduced in-process. Assert it structurally instead:
    /// the `sync_all` call must appear before `persist` in the source, and the
    /// parent-directory fsync after it (iter-191).
    #[test]
    fn atomic_write_syncs_before_persist() {
        let src = include_str!("fs_util.rs");
        let sync = src
            .find("tmp.as_file()")
            .expect("atomic_write must sync the temp file");
        let persist = src
            .find("tmp.persist(dest)")
            .expect("atomic_write must persist the temp file");
        let dir_sync = src
            .find("sync_parent_dir(parent);")
            .expect("atomic_write must fsync the parent directory");
        assert!(
            sync < persist,
            "sync_all must happen before persist, otherwise the rename can \
             expose unwritten blocks"
        );
        assert!(
            persist < dir_sync,
            "the parent directory fsync must follow the rename it is making durable"
        );
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_follows_symlink_and_keeps_it_a_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("real.md");
        std::fs::write(&target, "old").unwrap();
        let link = tmp.path().join("alias.md");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        atomic_write(&link, b"new").unwrap();

        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
        assert!(
            std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink(),
            "the symlink must survive the write"
        );
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_follows_relative_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("notes")).unwrap();
        let target = tmp.path().join("notes").join("real.md");
        std::fs::write(&target, "old").unwrap();
        let link = tmp.path().join("alias.md");
        std::os::unix::fs::symlink("notes/real.md", &link).unwrap();

        atomic_write(&link, b"new").unwrap();

        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_within_refuses_symlink_escaping_root() {
        let outside = tempfile::tempdir().unwrap();
        let escapee = outside.path().join("secret.md");
        std::fs::write(&escapee, "untouched").unwrap();

        let vault = tempfile::tempdir().unwrap();
        let link = vault.path().join("alias.md");
        std::os::unix::fs::symlink(&escapee, &link).unwrap();

        let err = atomic_write_within(vault.path(), &link, b"pwned").unwrap_err();
        assert!(
            err.to_string().contains("outside vault boundary"),
            "got: {err}"
        );
        assert_eq!(
            std::fs::read_to_string(&escapee).unwrap(),
            "untouched",
            "the out-of-vault file must not be modified"
        );
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_within_allows_intra_vault_symlink() {
        let vault = tempfile::tempdir().unwrap();
        let target = vault.path().join("real.md");
        std::fs::write(&target, "old").unwrap();
        let link = vault.path().join("alias.md");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        atomic_write_within(vault.path(), &link, b"new").unwrap();

        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_refuses_symlink_loop() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.md");
        let b = tmp.path().join("b.md");
        std::os::unix::fs::symlink(&b, &a).unwrap();
        std::os::unix::fs::symlink(&a, &b).unwrap();

        let err = atomic_write(&a, b"data").unwrap_err();
        assert!(err.to_string().contains("symlink chain"), "got: {err}");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_creates_target_of_dangling_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("not-yet.md");
        let link = tmp.path().join("alias.md");
        std::os::unix::fs::symlink(&missing, &link).unwrap();

        atomic_write(&link, b"created").unwrap();

        assert_eq!(std::fs::read_to_string(&missing).unwrap(), "created");
    }

    #[test]
    fn atomic_write_fails_if_parent_missing() {
        let tmp = tempfile::tempdir().unwrap();
        // The "missing" subdirectory does not exist, so the temp file cannot be created.
        let target = tmp.path().join("missing").join("file.txt");
        let err = atomic_write(&target, b"data").unwrap_err();
        assert!(
            err.to_string().contains("failed to create temp file"),
            "got: {err}"
        );
    }
}
