#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use crate::discovery;
use crate::links::{Link, extract_links_from_text};
use crate::scanner::{self, FileVisitor, ScanAction, strip_inline_code};

/// A single backlink: a file that links to some target.
#[derive(Debug, Clone)]
pub struct BacklinkEntry {
    /// The file containing the link (relative to vault root).
    pub source: PathBuf,
    /// 1-based line number where the link appears.
    pub line: usize,
    /// The parsed link.
    pub link: Link,
}

/// In-memory reverse index mapping link targets to their sources.
///
/// Keys are normalized target strings (as they appear in `[[target]]` or
/// `[text](target)`, without fragment). Values are all files that link to
/// that target.
pub struct LinkGraph {
    index: HashMap<String, Vec<BacklinkEntry>>,
}

impl LinkGraph {
    /// Build a link graph by scanning all `.md` files under `dir`.
    pub fn build(dir: &Path) -> Result<Self> {
        let files = discovery::discover_files(dir)?;
        let mut index: HashMap<String, Vec<BacklinkEntry>> = HashMap::new();

        for file in &files {
            let rel = file.strip_prefix(dir).unwrap_or(file).to_path_buf();

            let mut visitor = LinkGraphVisitor::new(rel);
            scanner::scan_file_multi(file, &mut [&mut visitor])
                .with_context(|| format!("scanning {}", file.display()))?;

            for (line, mut link) in visitor.links {
                // Normalize relative markdown link targets (those containing path
                // separators) so that `sub/a.md` linking to `../target.md` is
                // stored as `target.md` — matching how callers query by vault-
                // relative path.  Bare wikilink targets (no `/` or `\`) are left
                // unchanged because they are note names, not file system paths.
                if link.target.contains('/') || link.target.contains('\\') {
                    link.target = normalize_target(&visitor.source, &link.target);
                }
                index
                    .entry(link.target.clone())
                    .or_default()
                    .push(BacklinkEntry {
                        source: visitor.source.clone(),
                        line,
                        link,
                    });
            }
        }

        Ok(Self { index })
    }

    /// Look up all files that link to the given target.
    ///
    /// `target` should be the relative path without `.md` extension (matching
    /// how wikilinks are written), or with `.md` for markdown-style links.
    /// Both forms are checked.
    pub fn backlinks(&self, target: &str) -> Vec<&BacklinkEntry> {
        let mut results = Vec::new();

        // Check exact target as given
        if let Some(entries) = self.index.get(target) {
            results.extend(entries);
        }

        // Also check with/without .md extension
        let alt = if let Some(stem) = target.strip_suffix(".md") {
            stem.to_string()
        } else {
            format!("{target}.md")
        };
        if let Some(entries) = self.index.get(&alt) {
            results.extend(entries);
        }

        results
    }
}

/// Visitor that collects links with their line numbers.
/// Skips frontmatter parsing for performance.
struct LinkGraphVisitor {
    source: PathBuf,
    links: Vec<(usize, Link)>,
    scratch: Vec<Link>,
}

impl LinkGraphVisitor {
    fn new(source: PathBuf) -> Self {
        Self {
            source,
            links: Vec::new(),
            scratch: Vec::new(),
        }
    }
}

impl FileVisitor for LinkGraphVisitor {
    fn on_body_line(&mut self, raw: &str, line_num: usize) -> ScanAction {
        let cleaned = strip_inline_code(raw);
        self.scratch.clear();
        extract_links_from_text(&cleaned, &mut self.scratch);
        for link in self.scratch.drain(..) {
            self.links.push((line_num, link));
        }
        ScanAction::Continue
    }

    fn needs_frontmatter(&self) -> bool {
        false
    }
}

/// Resolve a relative markdown link target against the source file's directory,
/// producing a clean vault-relative path.
///
/// Only called for targets that contain `/` or `\`.  Wikilink-style bare note
/// names are left unchanged by the caller.
pub(crate) fn normalize_target(source: &Path, target: &str) -> String {
    let base = source.parent().unwrap_or(Path::new(""));
    let joined = base.join(target);
    normalize_path_components(&joined)
}

/// Remove `.` and resolve `..` components in `path`, returning a forward-slash
/// separated string.  Does not touch the filesystem (`canonicalize` is avoided
/// so that this works for files that may not exist yet, e.g. in tests).
pub(crate) fn normalize_path_components(path: &Path) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {} // skip `.`
            Component::ParentDir => {
                // Pop the previous normal component.  If the stack is empty or
                // its top is already `..`, we would escape the vault root —
                // push `..` literally so the path is preserved as-is.
                if parts.last().is_some_and(|p| *p != "..") {
                    parts.pop();
                } else {
                    parts.push("..");
                }
            }
            Component::Normal(s) => {
                if let Some(s) = s.to_str() {
                    parts.push(s);
                }
            }
            // Prefix / RootDir only appear on absolute paths; skip them so we
            // always produce a relative vault path.
            Component::Prefix(_) | Component::RootDir => {}
        }
    }
    parts.join("/")
}

/// Compute the relative path from the directory of `from_file` to `to_file`.
/// Both paths must be vault-relative with forward slashes.
///
/// Example: `relative_path_between("sub/a.md", "other/b.md")` → `"../other/b.md"`
/// Example: `relative_path_between("a.md", "sub/b.md")` → `"sub/b.md"`
/// Example: `relative_path_between("sub/a.md", "sub/b.md")` → `"b.md"`
#[allow(dead_code)] // used by the upcoming mv command (link_rewrite)
pub(crate) fn relative_path_between(from_file: &str, to_file: &str) -> String {
    // from_dir components: everything except the last segment (the filename)
    let from_parts: Vec<&str> = from_file.split('/').collect();
    let from_dir: Vec<&str> = if from_parts.len() > 1 {
        from_parts[..from_parts.len() - 1].to_vec()
    } else {
        Vec::new()
    };

    // If the source file is at vault root there are no directory components to
    // traverse — the relative path is just to_file itself.
    if from_dir.is_empty() {
        return to_file.to_string();
    }

    let to_parts: Vec<&str> = to_file.split('/').collect();

    // Find common prefix length between from_dir and the full to_file path.
    let common = from_dir
        .iter()
        .zip(to_parts.iter())
        .take_while(|(a, b)| a == b)
        .count();

    // One `..` per remaining component in from_dir past the common prefix.
    let up_count = from_dir.len() - common;
    let remaining = &to_parts[common..];

    let mut result: Vec<&str> = Vec::with_capacity(up_count + remaining.len());
    result.extend(std::iter::repeat_n("..", up_count));
    result.extend_from_slice(remaining);
    result.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_vault(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, content) in files {
            let path = dir.path().join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, content).unwrap();
        }
        dir
    }

    #[test]
    fn build_graph_simple_wikilinks() {
        let vault = create_vault(&[
            ("a.md", "---\ntitle: A\n---\nSee [[b]]\n"),
            ("b.md", "---\ntitle: B\n---\nSee [[a]] and [[c]]\n"),
            ("c.md", "---\ntitle: C\n---\nNo links here\n"),
        ]);
        let graph = LinkGraph::build(vault.path()).unwrap();

        let bl_b = graph.backlinks("b");
        assert_eq!(bl_b.len(), 1);
        assert_eq!(bl_b[0].source, PathBuf::from("a.md"));
        assert_eq!(bl_b[0].line, 4);

        let bl_a = graph.backlinks("a");
        assert_eq!(bl_a.len(), 1);
        assert_eq!(bl_a[0].source, PathBuf::from("b.md"));

        let bl_c = graph.backlinks("c");
        assert_eq!(bl_c.len(), 1);
        assert_eq!(bl_c[0].source, PathBuf::from("b.md"));

        // No one links to a non-existent target
        assert!(graph.backlinks("nonexistent").is_empty());
    }

    #[test]
    fn build_graph_markdown_links() {
        let vault = create_vault(&[
            ("a.md", "See [note](sub/b.md) for details\n"),
            // `../a.md` from `sub/` resolves to `a.md` at the vault root.
            ("sub/b.md", "Back to [a](../a.md)\n"),
        ]);
        let graph = LinkGraph::build(vault.path()).unwrap();

        // Down-path links are stored normalized (no change needed).
        let bl = graph.backlinks("sub/b.md");
        assert_eq!(bl.len(), 1);
        assert_eq!(bl[0].source, PathBuf::from("a.md"));

        // Cross-directory `../` link must resolve to the vault-relative target.
        let bl = graph.backlinks("a.md");
        assert_eq!(bl.len(), 1);
        assert_eq!(bl[0].source, PathBuf::from("sub/b.md"));
    }

    #[test]
    fn cross_directory_relative_link_normalized() {
        // source at `notes/page.md` links to `../assets/img.md`
        // → should resolve to `assets/img.md`
        let vault = create_vault(&[
            ("assets/img.md", "# Image\n"),
            ("notes/page.md", "See [img](../assets/img.md)\n"),
        ]);
        let graph = LinkGraph::build(vault.path()).unwrap();

        let bl = graph.backlinks("assets/img.md");
        assert_eq!(bl.len(), 1);
        assert_eq!(bl[0].source, PathBuf::from("notes/page.md"));

        // The raw `../assets/img.md` form must NOT appear in the index.
        let raw_bl = graph.backlinks("../assets/img.md");
        assert!(raw_bl.is_empty());
    }

    #[test]
    fn parent_dir_link_from_subdirectory() {
        // source at `sub/a.md` links to `../target.md`
        // → should resolve to `target.md`
        let vault = create_vault(&[
            ("target.md", "# Target\n"),
            ("sub/a.md", "[link](../target.md)\n"),
        ]);
        let graph = LinkGraph::build(vault.path()).unwrap();

        let bl = graph.backlinks("target.md");
        assert_eq!(bl.len(), 1);
        assert_eq!(bl[0].source, PathBuf::from("sub/a.md"));
    }

    #[test]
    fn normalize_path_components_dot_dot() {
        assert_eq!(
            normalize_path_components(Path::new("sub/../target.md")),
            "target.md"
        );
        assert_eq!(
            normalize_path_components(Path::new("a/b/../../c.md")),
            "c.md"
        );
        assert_eq!(
            normalize_path_components(Path::new("notes/../assets/img.md")),
            "assets/img.md"
        );
    }

    #[test]
    fn build_graph_with_alias() {
        let vault = create_vault(&[("a.md", "See [[b|my note B]]\n")]);
        let graph = LinkGraph::build(vault.path()).unwrap();

        let bl = graph.backlinks("b");
        assert_eq!(bl.len(), 1);
        assert_eq!(bl[0].link.label.as_deref(), Some("my note B"));
    }

    #[test]
    fn backlinks_matches_with_and_without_md_extension() {
        let vault = create_vault(&[
            ("a.md", "Link to [[notes]]\n"),
            ("b.md", "Link to [text](notes.md)\n"),
        ]);
        let graph = LinkGraph::build(vault.path()).unwrap();

        // Query with .md finds both the .md link and the bare wikilink
        let bl = graph.backlinks("notes.md");
        assert_eq!(bl.len(), 2);

        // Query without .md also finds both
        let bl = graph.backlinks("notes");
        assert_eq!(bl.len(), 2);
    }

    #[test]
    fn links_inside_code_blocks_ignored() {
        let vault = create_vault(&[("a.md", "---\ntitle: A\n---\n```\n[[b]]\n```\nReal [[c]]\n")]);
        let graph = LinkGraph::build(vault.path()).unwrap();

        assert!(graph.backlinks("b").is_empty());
        assert_eq!(graph.backlinks("c").len(), 1);
    }

    #[test]
    fn links_inside_inline_code_ignored() {
        let vault = create_vault(&[("a.md", "Use `[[b]]` syntax and [[c]]\n")]);
        let graph = LinkGraph::build(vault.path()).unwrap();

        assert!(graph.backlinks("b").is_empty());
        assert_eq!(graph.backlinks("c").len(), 1);
    }

    #[test]
    fn malformed_frontmatter_skipped() {
        // With needs_frontmatter=false, malformed YAML shouldn't cause errors
        let vault = create_vault(&[("a.md", "---\n: bad yaml [[\n---\n[[b]]\n")]);
        let graph = LinkGraph::build(vault.path()).unwrap();
        assert_eq!(graph.backlinks("b").len(), 1);
    }

    #[test]
    fn empty_vault() {
        let vault = create_vault(&[]);
        let graph = LinkGraph::build(vault.path()).unwrap();
        assert!(graph.backlinks("anything").is_empty());
    }

    #[test]
    fn relative_path_same_directory() {
        assert_eq!(relative_path_between("sub/a.md", "sub/b.md"), "b.md");
    }

    #[test]
    fn relative_path_from_root_to_subdir() {
        assert_eq!(relative_path_between("a.md", "sub/b.md"), "sub/b.md");
    }

    #[test]
    fn relative_path_from_subdir_to_root() {
        assert_eq!(relative_path_between("sub/a.md", "b.md"), "../b.md");
    }

    #[test]
    fn relative_path_cross_directory() {
        assert_eq!(
            relative_path_between("sub/a.md", "other/b.md"),
            "../other/b.md"
        );
    }

    #[test]
    fn relative_path_deep_to_shallow() {
        assert_eq!(relative_path_between("a/b/c.md", "d.md"), "../../d.md");
    }

    #[test]
    fn relative_path_shallow_to_deep() {
        assert_eq!(relative_path_between("a.md", "x/y/z.md"), "x/y/z.md");
    }

    #[test]
    fn relative_path_nested_common_prefix() {
        assert_eq!(relative_path_between("a/b/c.md", "a/d/e.md"), "../d/e.md");
    }
}
