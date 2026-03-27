#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result};
use globset::{GlobBuilder, GlobSetBuilder};
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

use crate::link_graph::strip_site_prefix;

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
    // Reject null bytes before any further processing.  A null byte in the
    // path could bypass the `.md` extension check on some platforms because
    // the OS treats the string as ending at the first `\0`.
    if path_arg.contains('\0') {
        return Err(FileResolveError::NotFound {
            path: path_arg.to_owned(),
        });
    }

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

    // After confirming the file exists, canonicalize to resolve symlinks and
    // verify the real path stays within the vault directory.
    let canonical_dir = canonicalize_vault_dir(dir).map_err(|_| FileResolveError::NotFound {
        path: normalized.clone(),
    })?;
    match ensure_within_vault(&canonical_dir, &full) {
        Ok(true) => {}
        Ok(false) => {
            return Err(FileResolveError::OutsideVault {
                path: normalized.clone(),
            });
        }
        Err(_) => {
            // Canonicalization of the target failed (permission error, symlink loop, etc.).
            // Do not claim "outside vault" — the path simply could not be resolved.
            return Err(FileResolveError::NotFound { path: normalized });
        }
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

/// Canonicalize the vault directory once.
///
/// Callers that invoke `ensure_within_vault` in a loop should call this once
/// upfront and pass the result to every `ensure_within_vault` call, avoiding
/// repeated canonicalization of the same directory.
pub fn canonicalize_vault_dir(dir: &Path) -> Result<PathBuf> {
    dunce::canonicalize(dir)
        .with_context(|| format!("failed to canonicalize vault dir: {}", dir.display()))
}

/// Verify that `full` resolves to a path within `canonical_dir` after following symlinks.
///
/// Accepts an already-canonicalized vault directory to avoid re-canonicalizing
/// on every call (important when called in a per-link loop).
///
/// Returns:
/// - `Ok(true)`  — `full` is within the vault
/// - `Ok(false)` — `full` resolves outside the vault boundary
/// - `Err(_)`    — `full` could not be canonicalized (permission error, symlink loop, etc.)
pub(crate) fn ensure_within_vault(canonical_dir: &Path, full: &Path) -> Result<bool> {
    let canonical_full = dunce::canonicalize(full)
        .with_context(|| format!("failed to canonicalize path: {}", full.display()))?;
    Ok(canonical_full.starts_with(canonical_dir))
}

/// Normalize a path argument: strip leading `./`, normalize separators to forward slashes.
fn normalize_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    normalized
        .strip_prefix("./")
        .unwrap_or(&normalized)
        .to_owned()
}

/// Check if a path argument is a glob or negation pattern.
///
/// Returns `true` for paths containing `*`, `?`, `[`, or a leading `!`
/// (negation glob).
#[must_use]
pub fn is_glob(path: &str) -> bool {
    path.starts_with('!')
        || path.starts_with("\\!")
        || path.contains('*')
        || path.contains('?')
        || path.contains('[')
}

/// Match discovered files against a glob pattern.
///
/// If `pattern` starts with `!`, it is treated as a negation: all discovered
/// files are returned **except** those matching the remainder of the pattern.
///
/// Positive patterns (no `!` prefix) work as before — only files matching the
/// pattern are returned.
///
/// The glob is matched against paths relative to `dir`.
pub fn match_glob(dir: &Path, files: &[PathBuf], pattern: &str) -> Result<Vec<(PathBuf, String)>> {
    // Normalize `\!` → `!` so that shell-escaped negation globs work.
    // Some shells (and Claude Code's Bash tool) escape `!` to `\!` even
    // inside single quotes.
    let normalized;
    let pattern = if let Some(rest) = pattern.strip_prefix("\\!") {
        normalized = format!("!{rest}");
        normalized.as_str()
    } else {
        pattern
    };

    if let Some(neg_pattern) = pattern.strip_prefix('!') {
        anyhow::ensure!(
            !neg_pattern.is_empty(),
            "negation glob pattern must not be empty (got '!')"
        );
        // Negation glob: return all files that do NOT match the pattern.
        let glob = GlobBuilder::new(neg_pattern)
            .literal_separator(true)
            .build()
            .context("invalid glob negation pattern")?
            .compile_matcher();

        let mut matched = Vec::new();
        for file in files {
            let rel = relative_path(dir, file);
            if !glob.is_match(&rel) {
                matched.push((file.clone(), rel));
            }
        }
        return Ok(matched);
    }

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

/// Match discovered files against multiple glob patterns.
///
/// Patterns prefixed with `!` (or `\!`) are treated as negations.
/// - If there are any positive patterns, a file must match at least one.
/// - If there are no positive patterns, all files start as candidates.
/// - A file is excluded if it matches any negative pattern.
///
/// The glob is matched against paths relative to `dir`.
pub fn match_globs(
    dir: &Path,
    files: &[PathBuf],
    patterns: &[String],
) -> Result<Vec<(PathBuf, String)>> {
    // Normalize `\!` → `!` for each pattern
    let normalized: Vec<String> = patterns
        .iter()
        .map(|p| {
            if let Some(rest) = p.strip_prefix("\\!") {
                format!("!{rest}")
            } else {
                p.clone()
            }
        })
        .collect();

    // Separate into positive and negative patterns
    let mut positive: Vec<&str> = Vec::new();
    let mut negative: Vec<&str> = Vec::new();
    for p in &normalized {
        if let Some(neg) = p.strip_prefix('!') {
            anyhow::ensure!(
                !neg.is_empty(),
                "negation glob pattern must not be empty (got '!')"
            );
            negative.push(neg);
        } else {
            positive.push(p.as_str());
        }
    }

    // Build the positive GlobSet (empty means "match all")
    let positive_set = if positive.is_empty() {
        None
    } else {
        let mut builder = GlobSetBuilder::new();
        for pat in &positive {
            builder.add(
                GlobBuilder::new(pat)
                    .literal_separator(true)
                    .build()
                    .context("invalid glob pattern")?,
            );
        }
        Some(
            builder
                .build()
                .context("failed to build positive globset")?,
        )
    };

    // Build the negative GlobSet
    let negative_set = if negative.is_empty() {
        None
    } else {
        let mut builder = GlobSetBuilder::new();
        for pat in &negative {
            builder.add(
                GlobBuilder::new(pat)
                    .literal_separator(true)
                    .build()
                    .context("invalid glob negation pattern")?,
            );
        }
        Some(
            builder
                .build()
                .context("failed to build negative globset")?,
        )
    };

    let mut matched = Vec::new();
    for file in files {
        let rel = relative_path(dir, file);
        let passes_positive = positive_set.as_ref().is_none_or(|gs| gs.is_match(&rel));
        let passes_negative = negative_set.as_ref().is_none_or(|gs| !gs.is_match(&rel));
        if passes_positive && passes_negative {
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
    OutsideVault { path: String },
}

impl std::fmt::Display for FileResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound { path } => write!(f, "file not found: {path}"),
            Self::MissingExtension { path, hint } => {
                write!(f, "file not found: {path} (did you mean {hint}?)")
            }
            Self::OutsideVault { path } => {
                write!(f, "file resolves outside vault boundary: {path}")
            }
        }
    }
}

impl std::error::Error for FileResolveError {}

/// Resolve a link target to a file path relative to the vault root.
/// Tries the target as-is, then with `.md` appended.
/// Returns the relative path if the file exists within the vault, or None.
///
/// `canonical_dir` must be a pre-canonicalized vault path (see `canonicalize_vault_dir`).
/// Callers iterating over many links should canonicalize once and reuse the result.
#[must_use]
pub fn resolve_target(
    canonical_dir: &Path,
    target: &str,
    site_prefix: Option<&str>,
) -> Option<String> {
    if target.is_empty() {
        return None;
    }

    // Normalize backslashes to forward slashes
    let mut target = target.replace('\\', "/");

    // Strip fragment (#...) and query string (?...) before resolution.
    // These are URL components that don't correspond to filesystem paths.
    if let Some(pos) = target.find('#') {
        target.truncate(pos);
    }
    if let Some(pos) = target.find('?') {
        target.truncate(pos);
    }
    // Strip trailing slash (e.g. "docs/page/" → "docs/page")
    while target.ends_with('/') && target.len() > 1 {
        target.pop();
    }
    if target.is_empty() {
        return None;
    }

    // Normalize absolute paths using site_prefix (same logic as LinkGraph).
    // `/docs/page.md` with site_prefix "docs" becomes `page.md`.
    let target = if target.starts_with('/') {
        let stripped = strip_site_prefix(&target, site_prefix);
        // Reject traversal even after prefix stripping (e.g. `/docs/../../etc/passwd`)
        if has_parent_traversal(&stripped) {
            return None;
        }
        stripped
    } else {
        if has_parent_traversal(&target) || Path::new(&target).is_absolute() {
            return None;
        }
        target
    };

    let full = canonical_dir.join(&target);
    if full.is_file() {
        // Ok(true) = within vault; Ok(false) or Err = reject
        if ensure_within_vault(canonical_dir, &full).unwrap_or(false) {
            return Some(target.clone());
        }
        return None;
    }

    if !std::path::Path::new(&target)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
    {
        let with_ext = format!("{target}.md");
        let full = canonical_dir.join(&with_ext);
        if full.is_file() {
            if ensure_within_vault(canonical_dir, &full).unwrap_or(false) {
                return Some(with_ext);
            }
            return None;
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
            other => {
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

    #[test]
    fn is_glob_detects_negation_prefix() {
        assert!(is_glob("!notes/draft.md"));
        assert!(is_glob("!**/index.md"));
    }

    #[test]
    fn is_glob_detects_escaped_negation_prefix() {
        assert!(is_glob("\\!notes/draft.md"));
        assert!(is_glob("\\!**/index.md"));
    }

    #[test]
    fn glob_negation_escaped_backslash_bang() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(
            tmp.path(),
            &["a.md", "b.md", "notes/draft.md", "notes/final.md"],
        );
        let files = discover_files(tmp.path()).unwrap();

        // `\!` should be treated identically to `!` (shell escaping workaround)
        let matched = match_glob(tmp.path(), &files, "\\!notes/draft.md").unwrap();
        let rels: Vec<_> = matched.iter().map(|(_, r)| r.as_str()).collect();
        assert!(
            !rels.contains(&"notes/draft.md"),
            "draft.md should be excluded via escaped negation"
        );
        assert!(rels.contains(&"notes/final.md"));
        assert!(rels.contains(&"a.md"));
        assert_eq!(matched.len(), 3);
    }

    #[test]
    fn glob_negation_excludes_matching_files() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(
            tmp.path(),
            &["a.md", "b.md", "notes/draft.md", "notes/final.md"],
        );
        let files = discover_files(tmp.path()).unwrap();

        // Exclude a specific file
        let matched = match_glob(tmp.path(), &files, "!notes/draft.md").unwrap();
        let rels: Vec<_> = matched.iter().map(|(_, r)| r.as_str()).collect();
        assert!(
            !rels.contains(&"notes/draft.md"),
            "draft.md should be excluded"
        );
        assert!(rels.contains(&"notes/final.md"));
        assert!(rels.contains(&"a.md"));
        assert_eq!(matched.len(), 3);
    }

    #[test]
    fn glob_negation_with_wildcard() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(
            tmp.path(),
            &["a.md", "draft-b.md", "draft-c.md", "final.md"],
        );
        let files = discover_files(tmp.path()).unwrap();

        let matched = match_glob(tmp.path(), &files, "!draft-*").unwrap();
        let rels: Vec<_> = matched.iter().map(|(_, r)| r.as_str()).collect();
        assert!(!rels.iter().any(|r| r.starts_with("draft-")));
        assert!(rels.contains(&"a.md"));
        assert!(rels.contains(&"final.md"));
        assert_eq!(matched.len(), 2);
    }

    #[test]
    fn glob_negation_double_star_excludes_recursively() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["index.md", "notes/index.md", "notes/real.md"]);
        let files = discover_files(tmp.path()).unwrap();

        let matched = match_glob(tmp.path(), &files, "!**/index.md").unwrap();
        let rels: Vec<_> = matched.iter().map(|(_, r)| r.as_str()).collect();
        assert!(!rels.iter().any(|r| r.ends_with("index.md")));
        assert!(rels.contains(&"notes/real.md"));
        assert_eq!(matched.len(), 1);
    }

    #[test]
    fn match_globs_multiple_positive_patterns_union() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(
            tmp.path(),
            &["root.md", "sub1/a.md", "sub1/b.md", "sub2/c.md"],
        );
        let files = discover_files(tmp.path()).unwrap();

        let patterns: Vec<String> = vec!["sub1/**".to_owned(), "sub2/**".to_owned()];
        let matched = match_globs(tmp.path(), &files, &patterns).unwrap();
        let rels: Vec<_> = matched.iter().map(|(_, r)| r.as_str()).collect();
        assert_eq!(matched.len(), 3);
        assert!(rels.contains(&"sub1/a.md"));
        assert!(rels.contains(&"sub1/b.md"));
        assert!(rels.contains(&"sub2/c.md"));
        assert!(!rels.contains(&"root.md"));
    }

    #[test]
    fn match_globs_positive_and_negative() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["sub/keep.md", "sub/draft.md", "root.md"]);
        let files = discover_files(tmp.path()).unwrap();

        let patterns: Vec<String> = vec!["sub/**".to_owned(), "!sub/draft.md".to_owned()];
        let matched = match_globs(tmp.path(), &files, &patterns).unwrap();
        let rels: Vec<_> = matched.iter().map(|(_, r)| r.as_str()).collect();
        assert_eq!(matched.len(), 1);
        assert!(rels.contains(&"sub/keep.md"));
        assert!(!rels.contains(&"sub/draft.md"));
    }

    #[test]
    fn match_globs_no_positive_means_all_files() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["a.md", "b.md", "draft.md"]);
        let files = discover_files(tmp.path()).unwrap();

        // Only a negation pattern — should return all files except matching ones
        let patterns: Vec<String> = vec!["!draft.md".to_owned()];
        let matched = match_globs(tmp.path(), &files, &patterns).unwrap();
        let rels: Vec<_> = matched.iter().map(|(_, r)| r.as_str()).collect();
        assert_eq!(matched.len(), 2);
        assert!(rels.contains(&"a.md"));
        assert!(rels.contains(&"b.md"));
        assert!(!rels.contains(&"draft.md"));
    }

    #[test]
    fn match_globs_single_pattern_same_as_match_glob() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["a.md", "notes/b.md", "notes/c.md"]);
        let files = discover_files(tmp.path()).unwrap();

        let single: Vec<String> = vec!["notes/*.md".to_owned()];
        let matched = match_globs(tmp.path(), &files, &single).unwrap();
        assert_eq!(matched.len(), 2);
    }

    #[test]
    fn match_globs_empty_negation_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["a.md"]);
        let files = discover_files(tmp.path()).unwrap();
        let patterns: Vec<String> = vec!["!".to_owned()];
        assert!(match_globs(tmp.path(), &files, &patterns).is_err());
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
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "note", None),
            Some("note.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_explicit_md_extension() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["note.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "note.md", None),
            Some("note.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_subpath_stem() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["sub/other.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "sub/other", None),
            Some("sub/other.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_subpath_with_extension() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["sub/other.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "sub/other.md", None),
            Some("sub/other.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_bare_stem_does_not_match_subdirectory() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["sub/other.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(resolve_target(&canonical, "other", None), None);
    }

    #[test]
    fn resolve_target_nonexistent_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(resolve_target(&canonical, "nonexistent", None), None);
    }

    #[test]
    fn resolve_target_empty_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(resolve_target(&canonical, "", None), None);
    }

    #[test]
    fn resolve_target_rejects_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["note.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(resolve_target(&canonical, "../note", None), None);
        assert_eq!(resolve_target(&canonical, "sub/../../note", None), None);
        // /etc/passwd normalizes to "etc/passwd" which doesn't exist in the vault
        assert_eq!(resolve_target(&canonical, "/etc/passwd", None), None);
    }

    #[test]
    fn resolve_target_non_md_file_exact_match() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["image.png"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "image.png", None),
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
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();

        assert_eq!(
            resolve_target(&canonical, "etc..md", None),
            Some("etc..md".to_owned())
        );
    }

    #[test]
    fn resolve_target_rejects_parent_traversal_segment() {
        let tmp = tempfile::tempdir().unwrap();
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();

        assert_eq!(resolve_target(&canonical, "../secret.md", None), None);
    }

    // --- symlink escape tests ---

    #[cfg(unix)]
    #[test]
    fn resolve_file_rejects_symlink_escape() {
        let vault = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.md"), "# Secret").unwrap();

        // Create a symlink inside vault that points outside
        std::os::unix::fs::symlink(outside.path(), vault.path().join("linked")).unwrap();

        let err = resolve_file(vault.path(), "linked/secret.md").unwrap_err();
        assert!(
            matches!(err, FileResolveError::OutsideVault { .. }),
            "expected OutsideVault, got {err:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn resolve_target_rejects_symlink_escape() {
        let vault = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.md"), "# Secret").unwrap();

        std::os::unix::fs::symlink(outside.path(), vault.path().join("linked")).unwrap();

        let canonical = canonicalize_vault_dir(vault.path()).unwrap();
        assert_eq!(resolve_target(&canonical, "linked/secret", None), None);
        assert_eq!(resolve_target(&canonical, "linked/secret.md", None), None);
    }

    // --- site_prefix resolution ---

    #[test]
    fn resolve_target_absolute_with_site_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["page.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "/docs/page.md", Some("docs")),
            Some("page.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_absolute_no_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["page.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "/page.md", None),
            Some("page.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_absolute_nonmatching_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["other/b.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        // site_prefix "docs" doesn't match "/other/b.md", so strip just the "/"
        assert_eq!(
            resolve_target(&canonical, "/other/b.md", Some("docs")),
            Some("other/b.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_absolute_stem_with_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["page.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "/docs/page", Some("docs")),
            Some("page.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_strips_trailing_slash() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["page.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "page.md/", None),
            Some("page.md".to_owned())
        );
        assert_eq!(
            resolve_target(&canonical, "page/", None),
            Some("page.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_strips_query_string() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["page.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "page?foo=bar", None),
            Some("page.md".to_owned())
        );
        assert_eq!(
            resolve_target(&canonical, "page.md?dv=winzip", None),
            Some("page.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_strips_fragment() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["page.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "page#section", None),
            Some("page.md".to_owned())
        );
        assert_eq!(
            resolve_target(&canonical, "page.md#heading", None),
            Some("page.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_strips_query_and_fragment_combined() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["page.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "page?foo=bar#section", None),
            Some("page.md".to_owned())
        );
        // Trailing slash + query + fragment
        assert_eq!(
            resolve_target(&canonical, "page/?q=1#top", None),
            Some("page.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_fragment_only_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["page.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        // "#section" → empty target after stripping → None
        assert_eq!(resolve_target(&canonical, "#section", None), None);
    }

    #[cfg(unix)]
    #[test]
    fn resolve_file_allows_symlink_within_vault() {
        let vault = tempfile::tempdir().unwrap();
        let subdir = vault.path().join("notes");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("real.md"), "# Real").unwrap();

        // Symlink within the vault is fine
        std::os::unix::fs::symlink(&subdir, vault.path().join("alias")).unwrap();

        let (path, rel) = resolve_file(vault.path(), "alias/real.md").unwrap();
        assert!(path.is_file());
        assert_eq!(rel, "alias/real.md");
    }

    #[test]
    fn resolve_file_rejects_null_byte_in_path() {
        let vault = tempfile::tempdir().unwrap();
        // A null byte must be rejected before the `.md` check so that it
        // cannot be used to bypass the extension validation on any platform.
        let err = resolve_file(vault.path(), "notes/file\0.md").unwrap_err();
        assert!(matches!(err, FileResolveError::NotFound { .. }));
    }

    #[test]
    fn resolve_file_rejects_null_byte_only_path() {
        let vault = tempfile::tempdir().unwrap();
        let err = resolve_file(vault.path(), "\0").unwrap_err();
        assert!(matches!(err, FileResolveError::NotFound { .. }));
    }
}
