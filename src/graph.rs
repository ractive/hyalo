use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::discovery;
use crate::links::Link;

/// Maps lowercased filename stems to their relative paths.
/// Used for Obsidian shortest-path resolution: `[[foo]]` → shortest path matching `foo.md`.
#[derive(Debug)]
pub struct FileIndex {
    /// stem (lowercased) → list of relative paths (sorted by length for shortest-path resolution)
    stems: HashMap<String, Vec<String>>,
    /// All relative paths in the vault (lowercased → original)
    paths: HashMap<String, String>,
}

/// A link with resolution information.
#[derive(Debug, Clone)]
pub struct ResolvedLink {
    pub source: String,
    pub link: Link,
    pub resolved_path: Option<String>,
}

impl FileIndex {
    /// Build a file index from all `.md` files in the vault directory.
    pub fn build(dir: &Path) -> Result<Self> {
        let files = discovery::discover_files(dir)?;
        let mut stems: HashMap<String, Vec<String>> = HashMap::new();
        let mut paths: HashMap<String, String> = HashMap::new();

        for file in &files {
            let rel = discovery::relative_path(dir, file);
            let lower_rel = rel.to_ascii_lowercase();
            paths.insert(lower_rel, rel.clone());

            // Extract stem (filename without extension)
            if let Some(stem) = Path::new(&rel).file_stem().and_then(|s| s.to_str()) {
                let lower_stem = stem.to_ascii_lowercase();
                stems.entry(lower_stem).or_default().push(rel);
            }
        }

        // Sort each stem's paths by length (shortest first) for Obsidian resolution
        for paths_list in stems.values_mut() {
            paths_list.sort_by_key(|p| p.len());
        }

        Ok(Self { stems, paths })
    }

    /// Resolve a link target to a file path in the vault.
    /// Uses Obsidian shortest-path resolution for simple names.
    pub fn resolve_target(&self, target: &str) -> Option<&str> {
        if target.is_empty() {
            return None;
        }

        let lower = target.to_ascii_lowercase();

        // If target contains a path separator or .md extension, try exact match
        if target.contains('/') || lower.ends_with(".md") {
            // Try exact path match (with .md appended if needed)
            let with_ext = if lower.ends_with(".md") {
                lower.clone()
            } else {
                format!("{lower}.md")
            };
            if let Some(original) = self.paths.get(&with_ext) {
                return Some(original);
            }
            return None;
        }

        // Simple name — use stem lookup (shortest path wins)
        if let Some(candidates) = self.stems.get(&lower) {
            candidates.first().map(|s| s.as_str())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_vault() -> (tempfile::TempDir, FileIndex) {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "# Note").unwrap();
        fs::create_dir_all(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("sub/note.md"), "# Sub Note").unwrap();
        fs::write(tmp.path().join("sub/other.md"), "# Other").unwrap();
        fs::create_dir_all(tmp.path().join("deep/nested")).unwrap();
        fs::write(tmp.path().join("deep/nested/note.md"), "# Deep").unwrap();
        let index = FileIndex::build(tmp.path()).unwrap();
        (tmp, index)
    }

    #[test]
    fn resolve_simple_stem_shortest_path() {
        let (_tmp, index) = setup_vault();
        // "note" should resolve to "note.md" (shortest path)
        let resolved = index.resolve_target("note").unwrap();
        assert_eq!(resolved, "note.md");
    }

    #[test]
    fn resolve_case_insensitive() {
        let (_tmp, index) = setup_vault();
        assert!(index.resolve_target("Note").is_some());
        assert!(index.resolve_target("NOTE").is_some());
    }

    #[test]
    fn resolve_with_path() {
        let (_tmp, index) = setup_vault();
        let resolved = index.resolve_target("sub/other.md").unwrap();
        assert_eq!(resolved, "sub/other.md");
    }

    #[test]
    fn resolve_path_without_extension() {
        let (_tmp, index) = setup_vault();
        let resolved = index.resolve_target("sub/other").unwrap();
        assert_eq!(resolved, "sub/other.md");
    }

    #[test]
    fn resolve_unique_stem() {
        let (_tmp, index) = setup_vault();
        let resolved = index.resolve_target("other").unwrap();
        assert_eq!(resolved, "sub/other.md");
    }

    #[test]
    fn resolve_nonexistent_returns_none() {
        let (_tmp, index) = setup_vault();
        assert!(index.resolve_target("nonexistent").is_none());
    }

    #[test]
    fn resolve_empty_target() {
        let (_tmp, index) = setup_vault();
        assert!(index.resolve_target("").is_none());
    }

    #[test]
    fn resolve_exact_path_not_found() {
        let (_tmp, index) = setup_vault();
        assert!(index.resolve_target("foo/bar.md").is_none());
    }
}
