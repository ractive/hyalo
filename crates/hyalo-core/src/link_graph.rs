#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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

            for (line, link) in visitor.links {
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
        let alt = if target.ends_with(".md") {
            target.strip_suffix(".md").unwrap().to_string()
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
            ("sub/b.md", "Back to [a](../a.md)\n"),
        ]);
        let graph = LinkGraph::build(vault.path()).unwrap();

        // Markdown links store target as-is
        let bl = graph.backlinks("sub/b.md");
        assert_eq!(bl.len(), 1);
        assert_eq!(bl[0].source, PathBuf::from("a.md"));

        let bl = graph.backlinks("../a.md");
        assert_eq!(bl.len(), 1);
        assert_eq!(bl[0].source, PathBuf::from("sub/b.md"));
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
}
