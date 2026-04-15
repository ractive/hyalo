#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result};
use std::io::Write;
use std::path::Path;
use tempfile::NamedTempFile;

/// Write data to a file atomically.
///
/// Creates a temporary file in the same directory as `path`, writes all data,
/// then renames it into place. This ensures that a crash mid-write never leaves
/// a truncated or corrupted file — the original is either fully replaced or
/// left untouched.
///
/// When `path` already exists, the original file's permissions are preserved.
/// `NamedTempFile` defaults to mode `0600`, so without this step rewrites
/// would silently tighten file permissions on every mutation.
pub(crate) fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .context("cannot determine parent directory for atomic write")?;

    // Capture existing permissions (if any) before the rename so we can restore
    // them on the new file — otherwise `NamedTempFile`'s default `0600` wins.
    let existing_perms = std::fs::metadata(path).ok().map(|m| m.permissions());

    let mut tmp = NamedTempFile::new_in(parent)
        .with_context(|| format!("failed to create temp file in {}", parent.display()))?;

    tmp.write_all(data)
        .with_context(|| format!("failed to write temp file for {}", path.display()))?;

    if let Some(perms) = existing_perms.clone() {
        std::fs::set_permissions(tmp.path(), perms).with_context(|| {
            format!(
                "failed to restore permissions on temp file for {}",
                path.display()
            )
        })?;
    }

    tmp.persist(path)
        .with_context(|| format!("failed to persist temp file to {}", path.display()))?;

    // On some platforms `persist` can reset the mode; re-apply for safety.
    if let Some(perms) = existing_perms {
        std::fs::set_permissions(path, perms)
            .with_context(|| format!("failed to restore permissions on {}", path.display()))?;
    }

    Ok(())
}

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
