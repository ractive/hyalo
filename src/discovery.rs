use anyhow::{Context, Result};
use globset::{Glob, GlobMatcher};
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

/// Collect all `.md` files under the given directory, respecting `.gitignore` and skipping hidden dirs.
pub fn discover_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    let walker = WalkBuilder::new(dir)
        .hidden(true) // skip hidden files/dirs
        .git_ignore(true)
        .build();

    for entry in walker {
        let entry = entry.context("error walking directory")?;
        let path = entry.path();
        if path.is_file()
            && let Some(ext) = path.extension()
            && ext == "md"
        {
            files.push(path.to_path_buf());
        }
    }

    files.sort();
    Ok(files)
}

/// Resolve a path argument relative to `--dir`. Verifies it exists and is `.md`.
/// Returns the canonical path and the normalized relative path (for display).
pub fn resolve_file(dir: &Path, path_arg: &str) -> Result<(PathBuf, String), FileResolveError> {
    let normalized = normalize_path(path_arg);

    if !normalized.ends_with(".md") {
        // Check if adding .md would work
        let with_ext = format!("{normalized}.md");
        let full = dir.join(&with_ext);
        if full.is_file() {
            return Err(FileResolveError::MissingExtension {
                path: normalized,
                hint: with_ext,
            });
        }
        return Err(FileResolveError::MissingExtension {
            path: normalized.clone(),
            hint: format!("{normalized}.md"),
        });
    }

    let full = dir.join(&normalized);
    if !full.is_file() {
        // Check without .md hint
        return Err(FileResolveError::NotFound { path: normalized });
    }

    Ok((full, normalized))
}

/// Normalize a path argument: strip leading `./`, normalize separators.
fn normalize_path(path: &str) -> String {
    let p = path.strip_prefix("./").unwrap_or(path);
    p.to_owned()
}

/// Check if a path argument contains glob characters.
pub fn is_glob(path: &str) -> bool {
    path.contains('*') || path.contains('?') || path.contains('[')
}

/// Match discovered files against a glob pattern.
/// The glob is matched against paths relative to `dir`.
pub fn match_glob(dir: &Path, files: &[PathBuf], pattern: &str) -> Result<Vec<(PathBuf, String)>> {
    let glob = Glob::new(pattern)
        .context("invalid glob pattern")?
        .compile_matcher();

    let mut matched = Vec::new();
    for file in files {
        let rel = relative_path(dir, file);
        if glob_matches(&glob, &rel) {
            matched.push((file.clone(), rel));
        }
    }
    Ok(matched)
}

/// Get the relative path of a file from a directory.
fn relative_path(dir: &Path, file: &Path) -> String {
    file.strip_prefix(dir)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| file.to_string_lossy().to_string())
}

/// Check if a relative path matches a glob pattern.
fn glob_matches(glob: &GlobMatcher, rel_path: &str) -> bool {
    glob.is_match(rel_path)
}

/// Errors specific to file resolution.
#[derive(Debug)]
pub enum FileResolveError {
    NotFound { path: String },
    MissingExtension { path: String, hint: String },
}

impl std::fmt::Display for FileResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound { path } => write!(f, "file not found: {path}"),
            Self::MissingExtension { path, .. } => write!(f, "file not found: {path}"),
        }
    }
}

impl std::error::Error for FileResolveError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn discover_finds_md_files() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "# Note").unwrap();
        fs::write(tmp.path().join("readme.txt"), "text").unwrap();
        fs::create_dir_all(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("sub/deep.md"), "# Deep").unwrap();

        let files = discover_files(tmp.path()).unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|f| f.extension().unwrap() == "md"));
    }

    #[test]
    fn discover_skips_hidden_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("visible.md"), "# Visible").unwrap();
        fs::create_dir_all(tmp.path().join(".hidden")).unwrap();
        fs::write(tmp.path().join(".hidden/secret.md"), "# Secret").unwrap();

        let files = discover_files(tmp.path()).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("visible.md"));
    }

    #[test]
    fn glob_matching() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a.md"), "").unwrap();
        fs::create_dir_all(tmp.path().join("notes")).unwrap();
        fs::write(tmp.path().join("notes/b.md"), "").unwrap();
        fs::write(tmp.path().join("notes/c.md"), "").unwrap();

        let files = discover_files(tmp.path()).unwrap();

        let matched = match_glob(tmp.path(), &files, "notes/*.md").unwrap();
        assert_eq!(matched.len(), 2);

        let matched_all = match_glob(tmp.path(), &files, "**/*.md").unwrap();
        assert_eq!(matched_all.len(), 3);
    }

    #[test]
    fn resolve_file_success() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "").unwrap();

        let (path, rel) = resolve_file(tmp.path(), "note.md").unwrap();
        assert!(path.is_file());
        assert_eq!(rel, "note.md");
    }

    #[test]
    fn resolve_file_strips_leading_dot_slash() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "").unwrap();

        let (_, rel) = resolve_file(tmp.path(), "./note.md").unwrap();
        assert_eq!(rel, "note.md");
    }

    #[test]
    fn resolve_file_missing_extension_hint() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "").unwrap();

        let err = resolve_file(tmp.path(), "note").unwrap_err();
        match err {
            FileResolveError::MissingExtension { path, hint } => {
                assert_eq!(path, "note");
                assert_eq!(hint, "note.md");
            }
            other => panic!("expected MissingExtension, got {other:?}"),
        }
    }

    #[test]
    fn is_glob_detects_patterns() {
        assert!(is_glob("*.md"));
        assert!(is_glob("notes/**/*.md"));
        assert!(is_glob("note[123].md"));
        assert!(!is_glob("notes/file.md"));
    }
}
