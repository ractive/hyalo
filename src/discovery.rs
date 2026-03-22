#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result};
use globset::GlobBuilder;
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
/// Returns the full path under `dir` and the normalized relative path (for display).
/// Rejects absolute paths and `..` segments to prevent escaping the base directory.
pub fn resolve_file(dir: &Path, path_arg: &str) -> Result<(PathBuf, String), FileResolveError> {
    let normalized = normalize_path(path_arg);

    // Reject path traversal attempts
    if normalized.starts_with('/')
        || has_parent_traversal(&normalized)
        || Path::new(&normalized).is_absolute()
    {
        return Err(FileResolveError::NotFound { path: normalized });
    }

    if !std::path::Path::new(&normalized)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
    {
        let hint = format!("{normalized}.md");
        return Err(FileResolveError::MissingExtension {
            path: normalized,
            hint,
        });
    }

    let full = dir.join(&normalized);
    if !full.is_file() {
        return Err(FileResolveError::NotFound { path: normalized });
    }

    Ok((full, normalized))
}

/// Return true if the path contains any `..` (parent directory) component.
/// This is the correct way to detect path traversal — checking for the `..`
/// component directly rather than a substring match, which incorrectly rejects
/// legitimate filenames like `etc..md`.
fn has_parent_traversal(path: &str) -> bool {
    Path::new(path)
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
}

/// Normalize a path argument: strip leading `./`, normalize separators to forward slashes.
fn normalize_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    normalized
        .strip_prefix("./")
        .unwrap_or(&normalized)
        .to_owned()
}

/// Check if a path argument contains glob characters.
#[must_use]
pub fn is_glob(path: &str) -> bool {
    path.contains('*') || path.contains('?') || path.contains('[')
}

/// Match discovered files against a glob pattern.
/// The glob is matched against paths relative to `dir`.
pub fn match_glob(dir: &Path, files: &[PathBuf], pattern: &str) -> Result<Vec<(PathBuf, String)>> {
    let glob = GlobBuilder::new(pattern)
        .literal_separator(true)
        .build()
        .context("invalid glob pattern")?
        .compile_matcher();

    let mut matched = Vec::new();
    for file in files {
        let rel = relative_path(dir, file);
        if glob.is_match(&rel) {
            matched.push((file.clone(), rel));
        }
    }
    Ok(matched)
}

/// Get the relative path of a file from a directory, using forward slashes on all platforms.
#[must_use]
pub fn relative_path(dir: &Path, file: &Path) -> String {
    let raw = file.strip_prefix(dir).map_or_else(
        |_| file.to_string_lossy().to_string(),
        |p| p.to_string_lossy().to_string(),
    );
    // Normalize to forward slashes for consistent output and glob matching on Windows.
    raw.replace('\\', "/")
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
            Self::MissingExtension { path, hint } => {
                write!(f, "file not found: {path} (did you mean {hint}?)")
            }
        }
    }
}

impl std::error::Error for FileResolveError {}

/// Resolve a link target to a file path relative to the vault root.
/// Tries the target as-is, then with `.md` appended.
/// Returns the relative path if the file exists, or None.
#[must_use]
pub fn resolve_target(dir: &Path, target: &str) -> Option<String> {
    if target.is_empty() {
        return None;
    }

    // Reject path traversal attempts
    let target = target.replace('\\', "/");
    if target.starts_with('/') || has_parent_traversal(&target) || Path::new(&target).is_absolute()
    {
        return None;
    }

    if dir.join(&target).is_file() {
        return Some(target.clone());
    }

    if !std::path::Path::new(&target)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
    {
        let with_ext = format!("{target}.md");
        if dir.join(&with_ext).is_file() {
            return Some(with_ext);
        }
    }

    None
}

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
    fn glob_star_does_not_cross_slash() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["a.md", "b.md", "sub/c.md", "sub/deep/d.md"]);
        let files = discover_files(tmp.path()).unwrap();

        let star = match_glob(tmp.path(), &files, "*.md").unwrap();
        // *.md should NOT match sub/c.md or sub/deep/d.md
        assert_eq!(star.len(), 2);

        let double_star = match_glob(tmp.path(), &files, "**/*.md").unwrap();
        assert_eq!(double_star.len(), 4);
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
    fn resolve_file_strips_leading_dot_backslash() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "").unwrap();

        let (_, rel) = resolve_file(tmp.path(), r".\note.md").unwrap();
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
            other @ FileResolveError::NotFound { .. } => {
                panic!("expected MissingExtension, got {other:?}")
            }
        }
    }

    #[test]
    fn resolve_file_rejects_path_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "").unwrap();

        // Absolute path
        assert!(matches!(
            resolve_file(tmp.path(), "/etc/passwd.md"),
            Err(FileResolveError::NotFound { .. })
        ));

        // Parent directory traversal
        assert!(matches!(
            resolve_file(tmp.path(), "../secret.md"),
            Err(FileResolveError::NotFound { .. })
        ));

        // Embedded traversal
        assert!(matches!(
            resolve_file(tmp.path(), "sub/../../../etc/passwd.md"),
            Err(FileResolveError::NotFound { .. })
        ));
    }

    #[test]
    fn is_glob_detects_patterns() {
        assert!(is_glob("*.md"));
        assert!(is_glob("notes/**/*.md"));
        assert!(is_glob("note[123].md"));
        assert!(!is_glob("notes/file.md"));
    }

    fn make_files(dir: &Path, paths: &[&str]) {
        for path in paths {
            let full = dir.join(path);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(full, "").unwrap();
        }
    }

    #[test]
    fn resolve_target_stem_appends_md() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["note.md"]);
        assert_eq!(
            resolve_target(tmp.path(), "note"),
            Some("note.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_explicit_md_extension() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["note.md"]);
        assert_eq!(
            resolve_target(tmp.path(), "note.md"),
            Some("note.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_subpath_stem() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["sub/other.md"]);
        assert_eq!(
            resolve_target(tmp.path(), "sub/other"),
            Some("sub/other.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_subpath_with_extension() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["sub/other.md"]);
        assert_eq!(
            resolve_target(tmp.path(), "sub/other.md"),
            Some("sub/other.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_bare_stem_does_not_match_subdirectory() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["sub/other.md"]);
        assert_eq!(resolve_target(tmp.path(), "other"), None);
    }

    #[test]
    fn resolve_target_nonexistent_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(resolve_target(tmp.path(), "nonexistent"), None);
    }

    #[test]
    fn resolve_target_empty_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(resolve_target(tmp.path(), ""), None);
    }

    #[test]
    fn resolve_target_rejects_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["note.md"]);
        assert_eq!(resolve_target(tmp.path(), "../note"), None);
        assert_eq!(resolve_target(tmp.path(), "sub/../../note"), None);
        assert_eq!(resolve_target(tmp.path(), "/etc/passwd"), None);
    }

    #[test]
    fn resolve_target_non_md_file_exact_match() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["image.png"]);
        assert_eq!(
            resolve_target(tmp.path(), "image.png"),
            Some("image.png".to_owned())
        );
    }

    // --- path traversal: dotdot in filename should not be rejected ---

    #[test]
    fn resolve_file_accepts_dotdot_in_filename() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("notes")).unwrap();
        fs::write(tmp.path().join("notes/etc..md"), "# dotdot").unwrap();

        let (path, rel) = resolve_file(tmp.path(), "notes/etc..md").unwrap();
        assert!(path.is_file());
        assert_eq!(rel, "notes/etc..md");
    }

    #[test]
    fn resolve_file_rejects_parent_traversal_segments() {
        let tmp = tempfile::tempdir().unwrap();

        assert!(matches!(
            resolve_file(tmp.path(), "../secret.md"),
            Err(FileResolveError::NotFound { .. })
        ));

        assert!(matches!(
            resolve_file(tmp.path(), "sub/../../etc/passwd.md"),
            Err(FileResolveError::NotFound { .. })
        ));
    }

    #[test]
    fn resolve_target_accepts_dotdot_in_filename() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["etc..md"]);

        assert_eq!(
            resolve_target(tmp.path(), "etc..md"),
            Some("etc..md".to_owned())
        );
    }

    #[test]
    fn resolve_target_rejects_parent_traversal_segment() {
        let tmp = tempfile::tempdir().unwrap();

        assert_eq!(resolve_target(tmp.path(), "../secret.md"), None);
    }
}
