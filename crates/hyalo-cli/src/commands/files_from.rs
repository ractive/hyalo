//! `--files-from` input parsing and path resolution.
//!
//! Provides [`load`] (reads raw lines from a file path or stdin) and
//! [`resolve`] (converts raw lines into vault-relative `(PathBuf, String)` pairs
//! while counting skipped entries by category).

use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};
use indexmap::IndexSet;

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
/// - Leading/trailing whitespace trimmed from each line (NEW-4).
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
            // Trim leading/trailing whitespace (spaces, tabs, etc.).
            let line = line.trim();
            // Normalize backslashes → forward slashes.
            let line = line.replace('\\', "/");
            // Strip leading `./`.
            if let Some(rest) = line.strip_prefix("./") {
                rest.to_owned()
            } else {
                line
            }
        })
        .filter(|line| !line.is_empty())
        .collect();

    Ok(entries)
}

// ---------------------------------------------------------------------------
// Path resolution
// ---------------------------------------------------------------------------

/// Counters for entries that were skipped during resolution.
#[derive(Debug, Default, Clone)]
pub struct FilesFromCounters {
    pub files_missing: u64,
    pub files_skipped_non_md: u64,
    pub files_skipped_outside_vault: u64,
}

impl FilesFromCounters {
    /// Returns a hint string when every entry was missing (suggesting the vault
    /// dir prefix may need stripping). Only fires when at least one entry was
    /// provided and every entry ended up in `files_missing`.
    ///
    /// `vault_dir_display` is the human-readable form of the configured `--dir`
    /// (e.g. `"files/en-us"`) so the hint can quote the actual prefix in use.
    pub fn all_missing_hint(
        &self,
        files_resolved: usize,
        total_inputs: usize,
        vault_dir_display: &str,
    ) -> Option<String> {
        if files_resolved == 0
            && self.files_missing > 0
            && total_inputs > 0
            && self.files_missing == total_inputs as u64
        {
            let example_path = if vault_dir_display == "." {
                "notes/foo.md".to_owned()
            } else {
                format!("{vault_dir_display}/notes/foo.md")
            };
            let dir_clause = if vault_dir_display == "." {
                String::new()
            } else {
                format!(" with --dir {vault_dir_display}")
            };
            Some(format!(
                "all --files-from entries were missing; \
                 if paths include the vault dir prefix (e.g. {example_path}{dir_clause}), \
                 hyalo strips it automatically — check that the vault dir matches"
            ))
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
/// `dir` is the absolute path to the vault directory.
/// `configured_dir` is the configured `dir` value as written in `.hyalo.toml` or
/// passed via `--dir` (e.g. `"files/en-us"` or `"kb"`). It is used to strip the
/// configured vault dir prefix from repo-relative input paths (NEW-2). Pass `"."` when
/// the vault is at the repo root; no prefix stripping occurs in that case.
///
/// Rules applied in order per entry:
/// 1. Absolute paths: rewrite via `strip_absolute_vault_prefix`; outside-vault → counter.
/// 2. Relative paths: treat as vault-relative; reject `..` components → outside-vault counter.
/// 3. Dir-prefix stripping (NEW-2): if the entry starts with `configured_dir/` and the
///    vault-relative literal does NOT exist on disk, strip the prefix and retry.
///    Precedence: vault-relative literal first (A), strip-and-retry only if (A) misses (B).
///    When `configured_dir` is `"."`, no prefix to strip.
/// 4. Non-`.md` extension → `files_skipped_non_md` counter (silent).
/// 5. Path does not exist on disk → `files_missing` counter (silent).
/// 6. Duplicate resolved paths are silently dropped; first-seen order is preserved (NEW-6).
/// 7. Accept: push `(dir.join(rel), rel)` to output.
pub fn resolve(dir: &Path, entries: &[String], configured_dir: &str) -> Result<FilesFromResolved> {
    Ok(resolve_with_membership(
        dir,
        entries,
        configured_dir,
        Path::is_file,
    ))
}

/// Resolve `--files-from` entries against a snapshot index instead of the
/// disk. A path is considered "present" iff its vault-relative form has an
/// entry in `index`. Paths absent from the snapshot count as `files_missing`,
/// **with no disk fallback** — matches iter-139's contract that `--index`
/// makes the snapshot the source of truth.
///
/// Use this when both `--index` and `--files-from` are active. Without
/// `--index`, callers must use [`resolve`].
pub fn resolve_with_index(
    dir: &Path,
    entries: &[String],
    configured_dir: &str,
    index: &hyalo_core::index::SnapshotIndex,
) -> Result<FilesFromResolved> {
    use hyalo_core::index::VaultIndex as _;
    Ok(resolve_with_membership(
        dir,
        entries,
        configured_dir,
        |full| {
            // `membership` is called with the full disk path
            // (`dir.join(&rel)`). Convert back to the vault-relative form for
            // snapshot lookup.
            let Ok(rel_path) = full.strip_prefix(dir) else {
                return false;
            };
            let rel_fwd = rel_path.to_string_lossy().replace('\\', "/");
            index.get(&rel_fwd).is_some()
        },
    ))
}

/// Internal: shared logic for [`resolve`] and [`resolve_with_index`].
/// `membership(full)` returns whether the candidate exists in the resolution
/// universe (disk for `resolve`, snapshot for `resolve_with_index`).
fn resolve_with_membership(
    dir: &Path,
    entries: &[String],
    configured_dir: &str,
    membership: impl Fn(&Path) -> bool,
) -> FilesFromResolved {
    let mut files = Vec::with_capacity(entries.len());
    let mut counters = FilesFromCounters::default();
    // Deduplicate resolved vault-relative paths, preserving first-seen order (NEW-6).
    let mut seen: IndexSet<String> = IndexSet::with_capacity(entries.len());

    // Compute the dir prefix for stripping (NEW-2).
    //
    // Use `configured_dir` when it is a relative path with content (e.g. "files/en-us"
    // or "kb"). If it is absolute (the caller passed an absolute --dir path) or ".",
    // fall back to the single-segment `dir.file_name()` approach (iter-140 BUG-2).
    //
    // This ensures backward compatibility: an absolute --dir still strips the vault
    // basename, while a relative multi-segment --dir strips the full prefix.
    let dir_prefix: Option<String> = {
        let normalized = configured_dir.replace('\\', "/");
        let trimmed = normalized.trim_end_matches('/');
        if trimmed == "." || trimmed.is_empty() || Path::new(trimmed).is_absolute() {
            // Fall back to single-segment (vault directory's last component).
            dir.file_name()
                .and_then(|s| s.to_str())
                .filter(|s| !s.is_empty())
                .map(|s| format!("{s}/"))
        } else {
            // Relative configured dir: use full prefix (single or multi-segment).
            Some(format!("{trimmed}/"))
        }
    };

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

            // Default to the entry as-is; prefix-strip is attempted lazily below
            // only if the vault-relative literal does not exist on disk (NEW-2).
            entry.clone()
        };

        // Filter to `.md` only (case-insensitive).
        if !rel.to_lowercase().ends_with(".md") {
            counters.files_skipped_non_md += 1;
            continue;
        }

        let mut full = dir.join(&rel);

        // Check existence; on miss, lazily try the prefix-stripped form (NEW-2).
        //
        // Precedence (from iter-140 BUG-2, preserved here):
        //   (A) Vault-relative literal exists → use it.
        //   (B) Otherwise, if `entry` starts with the configured dir prefix and the
        //       stripped form exists, use the stripped form.
        //   (C) Neither exists → counted as missing.
        //
        // This handles the ambiguity case: configured_dir = "notes",
        // entry = "notes/notes/foo.md". If the literal exists (A), we use it
        // (single stat). Only on miss do we pay a second stat for (B).
        let mut rel = rel;
        if !membership(&full)
            && let Some(prefix) = &dir_prefix
            && Path::new(entry).is_relative()
            && let Some(stripped) = entry.strip_prefix(prefix.as_str())
        {
            let stripped_full = dir.join(stripped);
            if membership(&stripped_full) {
                stripped.clone_into(&mut rel);
                full = stripped_full;
            }
        }

        if !membership(&full) {
            counters.files_missing += 1;
            continue;
        }

        // Deduplicate: skip if this resolved path was already seen (NEW-6).
        if !seen.insert(rel.clone()) {
            continue;
        }

        files.push((full, rel));
    }

    FilesFromResolved { files, counters }
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

        let r = resolve(tmp.path(), &["note.md".to_owned()], ".").unwrap();
        assert_eq!(r.files.len(), 1);
        assert_eq!(r.files[0].1, "note.md");
        assert_eq!(r.counters.files_missing, 0);
        assert_eq!(r.counters.files_skipped_non_md, 0);
        assert_eq!(r.counters.files_skipped_outside_vault, 0);
    }

    #[test]
    fn resolve_missing_file_counts() {
        let tmp = tempfile::tempdir().unwrap();
        let r = resolve(tmp.path(), &["nonexistent.md".to_owned()], ".").unwrap();
        assert!(r.files.is_empty());
        assert_eq!(r.counters.files_missing, 1);
    }

    #[test]
    fn resolve_non_md_counts() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("readme.txt");
        std::fs::write(path, "").unwrap();

        let r = resolve(tmp.path(), &["readme.txt".to_owned()], ".").unwrap();
        assert!(r.files.is_empty());
        assert_eq!(r.counters.files_skipped_non_md, 1);
    }

    #[test]
    fn resolve_parent_traversal_counts_as_outside_vault() {
        let tmp = tempfile::tempdir().unwrap();
        let r = resolve(tmp.path(), &["../outside.md".to_owned()], ".").unwrap();
        assert!(r.files.is_empty());
        assert_eq!(r.counters.files_skipped_outside_vault, 1);
    }

    #[test]
    fn resolve_absolute_in_vault() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("note.md");
        std::fs::write(&path, "").unwrap();
        let abs = path.to_str().unwrap().to_owned();

        let r = resolve(tmp.path(), &[abs], ".").unwrap();
        assert_eq!(r.files.len(), 1);
        assert_eq!(r.files[0].1, "note.md");
    }

    #[test]
    fn resolve_absolute_outside_vault_counts() {
        let tmp = tempfile::tempdir().unwrap();
        // /tmp itself is outside the tmp subdir vault.
        let outside = std::env::temp_dir().to_str().unwrap().to_owned() + "/some_file.md";
        // Whether it exists or not — it's outside the vault.
        let r = resolve(tmp.path(), &[outside], ".").unwrap();
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
        let r = resolve(tmp.path(), &entries, ".").unwrap();
        assert_eq!(r.files.len(), 1);
        assert_eq!(r.counters.files_missing, 1);
        assert_eq!(r.counters.files_skipped_non_md, 1);
        assert_eq!(r.counters.files_skipped_outside_vault, 1);
    }

    // -----------------------------------------------------------------------
    // NEW-4 — whitespace trimming in load()
    // -----------------------------------------------------------------------

    #[test]
    fn load_trims_leading_spaces() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "  note.md").unwrap();
        let result = load(tmp.path().to_str().unwrap()).unwrap();
        assert_eq!(result, vec!["note.md"]);
    }

    #[test]
    fn load_trims_trailing_spaces() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "note.md   ").unwrap();
        let result = load(tmp.path().to_str().unwrap()).unwrap();
        assert_eq!(result, vec!["note.md"]);
    }

    #[test]
    fn load_trims_tabs() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "\tnote.md\t").unwrap();
        let result = load(tmp.path().to_str().unwrap()).unwrap();
        assert_eq!(result, vec!["note.md"]);
    }

    #[test]
    fn load_trims_mixed_whitespace() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(b"  a.md  \n\t b.md \t\n").unwrap();
        let result = load(tmp.path().to_str().unwrap()).unwrap();
        assert_eq!(result, vec!["a.md", "b.md"]);
    }

    // -----------------------------------------------------------------------
    // NEW-2 — multi-segment --dir prefix stripping
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_strips_multi_segment_dir_prefix() {
        // Simulate: configured_dir = "files/en-us", git outputs "files/en-us/x.md".
        // The vault dir is an absolute path; configured_dir is the relative string.
        let vault = tempfile::tempdir().unwrap();
        std::fs::write(vault.path().join("x.md"), "").unwrap();

        // Entry as git would output it (repo-relative): "files/en-us/x.md".
        // configured_dir = "files/en-us" so prefix = "files/en-us/".
        // Vault-relative literal "files/en-us/x.md" does NOT exist in vault.
        // Stripped form "x.md" DOES exist → should resolve.
        let r = resolve(
            vault.path(),
            &["files/en-us/x.md".to_owned()],
            "files/en-us",
        )
        .unwrap();
        assert_eq!(r.files.len(), 1);
        assert_eq!(r.files[0].1, "x.md");
        assert_eq!(r.counters.files_missing, 0);
    }

    #[test]
    fn resolve_strips_single_segment_dir_prefix_unchanged() {
        // Regression: single-segment vault (kb) still works after NEW-2.
        let vault = tempfile::tempdir().unwrap();
        std::fs::write(vault.path().join("note.md"), "").unwrap();

        // Git outputs "kb/note.md"; configured_dir = "kb"; stripped = "note.md".
        let r = resolve(vault.path(), &["kb/note.md".to_owned()], "kb").unwrap();
        assert_eq!(r.files.len(), 1);
        assert_eq!(r.files[0].1, "note.md");
    }

    #[test]
    fn resolve_vault_relative_literal_takes_precedence_over_strip() {
        // Ambiguity: configured_dir = "notes", entry = "notes/notes/foo.md".
        // The vault contains "notes/notes/foo.md" (vault-relative literal exists).
        // Even though the prefix "notes/" matches, (A) vault-relative literal wins.
        let vault = tempfile::tempdir().unwrap();
        // Create vault/notes/notes/foo.md (two nested levels).
        let nested = vault.path().join("notes").join("notes");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("foo.md"), "").unwrap();

        let r = resolve(vault.path(), &["notes/notes/foo.md".to_owned()], "notes").unwrap();
        // Vault-relative literal "notes/notes/foo.md" exists → kept as-is.
        assert_eq!(r.files.len(), 1);
        assert_eq!(r.files[0].1, "notes/notes/foo.md");
    }

    #[test]
    fn resolve_no_prefix_strip_when_configured_dir_is_dot() {
        // When configured_dir = ".", no stripping occurs.
        let vault = tempfile::tempdir().unwrap();
        std::fs::write(vault.path().join("note.md"), "").unwrap();

        // Entry "kb/note.md" with configured_dir "." — no stripping, counts as missing.
        let r = resolve(vault.path(), &["kb/note.md".to_owned()], ".").unwrap();
        assert!(r.files.is_empty());
        assert_eq!(r.counters.files_missing, 1);
    }

    // -----------------------------------------------------------------------
    // NEW-6 — deduplicate resolved paths, first-seen order preserved
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_deduplicates_same_path() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.md"), "").unwrap();
        std::fs::write(tmp.path().join("b.md"), "").unwrap();

        let entries = vec![
            "a.md".to_owned(),
            "b.md".to_owned(),
            "a.md".to_owned(), // duplicate
            "a.md".to_owned(), // duplicate again
        ];
        let r = resolve(tmp.path(), &entries, ".").unwrap();
        // Only 2 unique files, a.md first then b.md.
        assert_eq!(r.files.len(), 2);
        assert_eq!(r.files[0].1, "a.md");
        assert_eq!(r.files[1].1, "b.md");
        // Counters do not inflate for deduped entries.
        assert_eq!(r.counters.files_missing, 0);
    }

    #[test]
    fn resolve_dedup_preserves_first_seen_order() {
        let tmp = tempfile::tempdir().unwrap();
        for name in ["c.md", "a.md", "b.md"] {
            std::fs::write(tmp.path().join(name), "").unwrap();
        }

        let entries = vec![
            "c.md".to_owned(),
            "a.md".to_owned(),
            "b.md".to_owned(),
            "c.md".to_owned(), // dup
            "a.md".to_owned(), // dup
        ];
        let r = resolve(tmp.path(), &entries, ".").unwrap();
        assert_eq!(r.files.len(), 3);
        assert_eq!(r.files[0].1, "c.md");
        assert_eq!(r.files[1].1, "a.md");
        assert_eq!(r.files[2].1, "b.md");
    }
}
