//! `--files-from` input parsing and path resolution.
//!
//! Provides [`load`] (reads raw lines from a file path or stdin) and
//! [`resolve`] (converts raw lines into vault-relative `(PathBuf, String)` pairs
//! while counting skipped entries by category).

use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};

// ---------------------------------------------------------------------------
// Line loading
// ---------------------------------------------------------------------------

/// Read raw path lines from `source`:
/// - `"-"` → read from stdin.
/// - anything else → open as a file path.
///
/// Handles:
/// - Optional UTF-8 BOM on the first byte (stripped silently).
/// - CRLF line endings (stripped).
/// - Empty / whitespace-only lines (skipped silently).
/// - Leading `./` stripped from each path.
/// - Backslashes normalized to forward slashes (Windows-friendly).
///
/// Returns an error if the source is not valid UTF-8.
pub fn load(source: &str) -> Result<Vec<String>> {
    let raw = if source == "-" {
        let mut buf = String::new();
        io::stdin()
            .read_to_string(&mut buf)
            .context("failed to read --files-from stdin")?;
        buf
    } else {
        std::fs::read_to_string(source)
            .with_context(|| format!("failed to read --files-from file: {source}"))?
    };

    // Strip optional UTF-8 BOM (EF BB BF) from the very start.
    let raw = raw.strip_prefix('\u{feff}').unwrap_or(&raw);

    let entries: Vec<String> = raw
        .lines()
        .map(|line| {
            // Normalize backslashes → forward slashes.
            let line = line.replace('\\', "/");
            // Strip leading `./`.
            if let Some(rest) = line.strip_prefix("./") {
                rest.to_owned()
            } else {
                line
            }
        })
        .filter(|line| !line.trim().is_empty())
        .collect();

    Ok(entries)
}

// ---------------------------------------------------------------------------
// Path resolution
// ---------------------------------------------------------------------------

/// Counters for entries that were skipped during resolution.
#[derive(Debug, Default)]
pub struct FilesFromCounters {
    pub files_missing: u64,
    pub files_skipped_non_md: u64,
    pub files_skipped_outside_vault: u64,
}

impl FilesFromCounters {
    /// Returns a hint string when every entry was missing (suggesting the vault
    /// dir prefix may need stripping). Only fires when at least one entry was
    /// provided and every entry ended up in `files_missing`.
    pub fn all_missing_hint(&self, files_resolved: usize, total_inputs: usize) -> Option<String> {
        if files_resolved == 0
            && self.files_missing > 0
            && total_inputs > 0
            && self.files_missing == total_inputs as u64
        {
            Some(
                "hint: all --files-from entries were missing; \
                 if paths include the vault dir prefix (e.g. kb/notes/foo.md with --dir kb), \
                 hyalo strips it automatically — check that the vault dir name matches"
                    .to_owned(),
            )
        } else {
            None
        }
    }
}

/// Result of resolving a `--files-from` list against a vault directory.
pub struct FilesFromResolved {
    /// `(full_path, vault_relative_path)` pairs for every accepted entry.
    pub files: Vec<(PathBuf, String)>,
    pub counters: FilesFromCounters,
}

/// Resolve raw path strings into vault-relative `(PathBuf, String)` pairs.
///
/// Rules applied in order per entry:
/// 1. Absolute paths: rewrite via `strip_absolute_vault_prefix`; outside-vault → counter.
/// 2. Relative paths: treat as vault-relative; reject `..` components → outside-vault counter.
/// 3. Non-`.md` extension → `files_skipped_non_md` counter (silent).
/// 4. Path does not exist on disk → `files_missing` counter (silent).
/// 5. Accept: push `(dir.join(rel), rel)` to output.
pub fn resolve(dir: &Path, entries: &[String]) -> Result<FilesFromResolved> {
    let mut files = Vec::with_capacity(entries.len());
    let mut counters = FilesFromCounters::default();

    for entry in entries {
        let rel: String = if Path::new(entry).is_absolute() {
            // Absolute path: try to strip vault prefix.
            if let Some(r) = hyalo_core::discovery::strip_absolute_vault_prefix(dir, entry) {
                r
            } else {
                counters.files_skipped_outside_vault += 1;
                continue;
            }
        } else {
            // Relative path: reject `..` traversal.
            let p = Path::new(entry);
            if p.components().any(|c| matches!(c, Component::ParentDir)) {
                counters.files_skipped_outside_vault += 1;
                continue;
            }

            // Strip vault-dir prefix when git outputs repo-relative paths.
            // E.g. if dir = /repo/kb, entry = "kb/notes/foo.md" → "notes/foo.md".
            // Strategy: if entry starts with "<vault_name>/", try vault-relative first
            // (A = entry as-is); if A doesn't exist on disk, use stripped form (B).
            // When both or neither exist, prefer B (the stripped form is the intent).
            let vault_name = dir.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if vault_name.is_empty() {
                entry.clone()
            } else {
                let prefix = format!("{vault_name}/");
                if let Some(stripped) = entry.strip_prefix(prefix.as_str()) {
                    let a_exists = dir.join(entry.as_str()).is_file();
                    if a_exists {
                        entry.clone()
                    } else {
                        stripped.to_owned()
                    }
                } else {
                    entry.clone()
                }
            }
        };

        // Filter to `.md` only (case-insensitive).
        if !rel.to_lowercase().ends_with(".md") {
            counters.files_skipped_non_md += 1;
            continue;
        }

        let full = dir.join(&rel);

        // Check existence.
        if !full.is_file() {
            counters.files_missing += 1;
            continue;
        }

        files.push((full, rel));
    }

    Ok(FilesFromResolved { files, counters })
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    // -----------------------------------------------------------------------
    // load() — line parsing
    // -----------------------------------------------------------------------

    #[test]
    fn load_empty_string_returns_empty() {
        // We test via a temp file because `load` requires a real path.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let result = load(tmp.path().to_str().unwrap()).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn load_whitespace_only_lines_are_skipped() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "   ").unwrap();
        writeln!(tmp, "\t").unwrap();
        tmp.write_all(b"\n").unwrap();
        writeln!(tmp, "a.md").unwrap();
        let result = load(tmp.path().to_str().unwrap()).unwrap();
        assert_eq!(result, vec!["a.md"]);
    }

    #[test]
    fn load_crlf_endings_stripped() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(b"a.md\r\nb.md\r\n").unwrap();
        let result = load(tmp.path().to_str().unwrap()).unwrap();
        assert_eq!(result, vec!["a.md", "b.md"]);
    }

    #[test]
    fn load_utf8_bom_stripped() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        // UTF-8 BOM bytes + content
        tmp.write_all(b"\xef\xbb\xbfa.md\n").unwrap();
        let result = load(tmp.path().to_str().unwrap()).unwrap();
        assert_eq!(result, vec!["a.md"]);
    }

    #[test]
    fn load_strips_leading_dot_slash() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "./foo/bar.md").unwrap();
        writeln!(tmp, "baz.md").unwrap();
        let result = load(tmp.path().to_str().unwrap()).unwrap();
        assert_eq!(result, vec!["foo/bar.md", "baz.md"]);
    }

    #[test]
    fn load_normalizes_backslashes() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, r"sub\nested.md").unwrap();
        let result = load(tmp.path().to_str().unwrap()).unwrap();
        assert_eq!(result, vec!["sub/nested.md"]);
    }

    #[test]
    fn load_no_comment_stripping_hash_lines_kept() {
        // Lines starting with `#` are NOT treated as comments — they are literal paths.
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "# this is NOT a comment").unwrap();
        writeln!(tmp, "real.md").unwrap();
        let result = load(tmp.path().to_str().unwrap()).unwrap();
        assert_eq!(result, vec!["# this is NOT a comment", "real.md"]);
    }

    // -----------------------------------------------------------------------
    // resolve() — path resolution
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_relative_md_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("note.md");
        std::fs::write(&path, "").unwrap();

        let r = resolve(tmp.path(), &["note.md".to_owned()]).unwrap();
        assert_eq!(r.files.len(), 1);
        assert_eq!(r.files[0].1, "note.md");
        assert_eq!(r.counters.files_missing, 0);
        assert_eq!(r.counters.files_skipped_non_md, 0);
        assert_eq!(r.counters.files_skipped_outside_vault, 0);
    }

    #[test]
    fn resolve_missing_file_counts() {
        let tmp = tempfile::tempdir().unwrap();
        let r = resolve(tmp.path(), &["nonexistent.md".to_owned()]).unwrap();
        assert!(r.files.is_empty());
        assert_eq!(r.counters.files_missing, 1);
    }

    #[test]
    fn resolve_non_md_counts() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("readme.txt");
        std::fs::write(path, "").unwrap();

        let r = resolve(tmp.path(), &["readme.txt".to_owned()]).unwrap();
        assert!(r.files.is_empty());
        assert_eq!(r.counters.files_skipped_non_md, 1);
    }

    #[test]
    fn resolve_parent_traversal_counts_as_outside_vault() {
        let tmp = tempfile::tempdir().unwrap();
        let r = resolve(tmp.path(), &["../outside.md".to_owned()]).unwrap();
        assert!(r.files.is_empty());
        assert_eq!(r.counters.files_skipped_outside_vault, 1);
    }

    #[test]
    fn resolve_absolute_in_vault() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("note.md");
        std::fs::write(&path, "").unwrap();
        let abs = path.to_str().unwrap().to_owned();

        let r = resolve(tmp.path(), &[abs]).unwrap();
        assert_eq!(r.files.len(), 1);
        assert_eq!(r.files[0].1, "note.md");
    }

    #[test]
    fn resolve_absolute_outside_vault_counts() {
        let tmp = tempfile::tempdir().unwrap();
        // /tmp itself is outside the tmp subdir vault.
        let outside = std::env::temp_dir().to_str().unwrap().to_owned() + "/some_file.md";
        // Whether it exists or not — it's outside the vault.
        let r = resolve(tmp.path(), &[outside]).unwrap();
        // Either outside-vault OR missing; absolute outside-vault is always skipped first.
        assert!(r.counters.files_skipped_outside_vault > 0 || r.counters.files_missing > 0);
        assert!(r.files.is_empty());
    }

    #[test]
    fn resolve_mixed_entries() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.md"), "").unwrap();

        let entries = vec![
            "a.md".to_owned(),          // valid
            "missing.md".to_owned(),    // missing
            "config.toml".to_owned(),   // non-md
            "../outside.md".to_owned(), // outside vault
        ];
        let r = resolve(tmp.path(), &entries).unwrap();
        assert_eq!(r.files.len(), 1);
        assert_eq!(r.counters.files_missing, 1);
        assert_eq!(r.counters.files_skipped_non_md, 1);
        assert_eq!(r.counters.files_skipped_outside_vault, 1);
    }
}
