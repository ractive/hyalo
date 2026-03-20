use std::collections::HashMap;
use std::path::Path;

/// Maps lowercased filename stems to their relative paths.
/// Used for Obsidian shortest-path resolution: `[[foo]]` → shortest path matching `foo.md`.
#[derive(Debug)]
pub struct FileIndex {
    /// stem (lowercased) → list of relative paths (sorted by length for shortest-path resolution)
    stems: HashMap<String, Vec<String>>,
    /// All relative paths in the vault (lowercased → original)
    paths: HashMap<String, String>,
}

impl FileIndex {
    /// Build a file index from a pre-discovered list of relative paths.
    /// Avoids a redundant directory walk when the caller already has the file list.
    pub fn from_paths(relative_paths: &[String]) -> Self {
        let mut stems: HashMap<String, Vec<String>> = HashMap::new();
        let mut paths: HashMap<String, String> = HashMap::new();

        for rel in relative_paths {
            let lower_rel = rel.to_ascii_lowercase();
            paths.insert(lower_rel, rel.clone());

            // Extract stem (filename without extension)
            if let Some(stem) = Path::new(rel).file_stem().and_then(|s| s.to_str()) {
                let lower_stem = stem.to_ascii_lowercase();
                stems.entry(lower_stem).or_default().push(rel.clone());
            }
        }

        // Sort each stem's paths by length (shortest first) for Obsidian resolution
        for paths_list in stems.values_mut() {
            paths_list.sort_by_key(|p| p.len());
        }

        Self { stems, paths }
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

    fn setup_index() -> FileIndex {
        FileIndex::from_paths(&[
            "note.md".to_string(),
            "sub/note.md".to_string(),
            "sub/other.md".to_string(),
            "deep/nested/note.md".to_string(),
        ])
    }

    #[test]
    fn resolve_simple_stem_shortest_path() {
        let index = setup_index();
        let resolved = index.resolve_target("note").unwrap();
        assert_eq!(resolved, "note.md");
    }

    #[test]
    fn resolve_case_insensitive() {
        let index = setup_index();
        assert!(index.resolve_target("Note").is_some());
        assert!(index.resolve_target("NOTE").is_some());
    }

    #[test]
    fn resolve_with_path() {
        let index = setup_index();
        let resolved = index.resolve_target("sub/other.md").unwrap();
        assert_eq!(resolved, "sub/other.md");
    }

    #[test]
    fn resolve_path_without_extension() {
        let index = setup_index();
        let resolved = index.resolve_target("sub/other").unwrap();
        assert_eq!(resolved, "sub/other.md");
    }

    #[test]
    fn resolve_unique_stem() {
        let index = setup_index();
        let resolved = index.resolve_target("other").unwrap();
        assert_eq!(resolved, "sub/other.md");
    }

    #[test]
    fn resolve_nonexistent_returns_none() {
        let index = setup_index();
        assert!(index.resolve_target("nonexistent").is_none());
    }

    #[test]
    fn resolve_empty_target() {
        let index = setup_index();
        assert!(index.resolve_target("").is_none());
    }

    #[test]
    fn resolve_exact_path_not_found() {
        let index = setup_index();
        assert!(index.resolve_target("foo/bar.md").is_none());
    }

    #[test]
    fn empty_index() {
        let index = FileIndex::from_paths(&[]);
        assert!(index.resolve_target("anything").is_none());
        assert!(index.resolve_target("").is_none());
    }

    #[test]
    fn file_with_no_extension() {
        // README has no extension — stem is "README", no crash expected
        let index = FileIndex::from_paths(&["README".to_string(), "note.md".to_string()]);
        // The extensionless file doesn't interfere with other resolution
        let resolved = index.resolve_target("note").unwrap();
        assert_eq!(resolved, "note.md");
        // README itself is reachable by stem
        assert!(index.resolve_target("README").is_some());
    }
}
