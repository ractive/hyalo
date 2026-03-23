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
pub fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .context("cannot determine parent directory for atomic write")?;

    let mut tmp = NamedTempFile::new_in(parent)
        .with_context(|| format!("failed to create temp file in {}", parent.display()))?;

    tmp.write_all(data)
        .with_context(|| format!("failed to write temp file for {}", path.display()))?;

    tmp.persist(path)
        .with_context(|| format!("failed to persist temp file to {}", path.display()))?;

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
