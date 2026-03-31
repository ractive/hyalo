#![allow(clippy::missing_errors_doc)]
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use crate::discovery;
use crate::frontmatter;
use crate::links::{Link, LinkKind, extract_links_from_text_with_original};
use crate::scanner::{self, FileVisitor, ScanAction};

/// A single backlink: a file that links to some target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacklinkEntry {
    /// The file containing the link (relative to vault root).
    pub source: PathBuf,
    /// 1-based line number where the link appears.
    pub line: usize,
    /// The parsed link.
    pub link: Link,
}

/// Result of building a link graph, including any files that were skipped.
pub struct LinkGraphBuild {
    /// The built link graph.
    pub graph: LinkGraph,
    /// Files that were skipped due to parse errors, with the error message.
    pub warnings: Vec<(PathBuf, String)>,
}

/// In-memory reverse index mapping link targets to their sources.
///
/// Keys are normalized target strings (as they appear in `[[target]]` or
/// `[text](target)`, without fragment). Values are all files that link to
/// that target.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct LinkGraph {
    index: HashMap<String, Vec<BacklinkEntry>>,
}

impl LinkGraph {
    /// Build a link graph by scanning all `.md` files under `dir`.
    ///
    /// `site_prefix` is an optional path prefix stripped from absolute links
    /// (those starting with `/`) before resolution.  It is derived from the
    /// `dir` config value: when `dir = "docs"`, pass `Some("docs")` so that
    /// `/docs/page.md` resolves to `page.md` relative to the vault root.
    /// Pass `None` when `dir` is `"."` (repo root).
    ///
    /// Files with malformed frontmatter are skipped and reported in
    /// `LinkGraphBuild::warnings` so callers can decide how to surface them.
    pub fn build(dir: &Path, site_prefix: Option<&str>) -> Result<LinkGraphBuild> {
        let files = discovery::discover_files(dir)?;
        let pairs: Vec<(PathBuf, PathBuf)> = files
            .into_iter()
            .map(|f| {
                let rel = f.strip_prefix(dir).unwrap_or(&f).to_path_buf();
                (f, rel)
            })
            .collect();
        Self::build_from_files(&pairs, site_prefix)
    }

    /// Build a link graph from a pre-collected list of `(absolute_path, relative_path)` pairs.
    ///
    /// `site_prefix` is an optional path prefix stripped from absolute links
    /// (those starting with `/`) before resolution.  See [`LinkGraph::build`] for details.
    ///
    /// Use this when the caller already has the file list (e.g. from `collect_files`)
    /// to avoid a redundant directory traversal.
    pub fn build_from_files(
        files: &[(PathBuf, PathBuf)],
        site_prefix: Option<&str>,
    ) -> Result<LinkGraphBuild> {
        let mut index: HashMap<String, Vec<BacklinkEntry>> = HashMap::with_capacity(files.len());
        let mut warnings: Vec<(PathBuf, String)> = Vec::new();

        for (full_path, rel) in files {
            let mut visitor = LinkGraphVisitor::new(rel.clone());
            match scanner::scan_file_multi(full_path, &mut [&mut visitor]) {
                Ok(()) => {}
                Err(e) if frontmatter::is_parse_error(&e) => {
                    warnings.push((rel.clone(), e.to_string()));
                    continue;
                }
                Err(e) => return Err(e),
            }
            insert_file_links(&mut index, visitor.into_file_links(), site_prefix);
        }

        Ok(LinkGraphBuild {
            graph: Self { index },
            warnings,
        })
    }

    /// Build a link graph from pre-collected per-file link data.
    ///
    /// Use this when the caller has already scanned files (e.g. `summary` combining
    /// frontmatter + task + link visitors in a single pass) and wants to build the
    /// graph without re-reading files.
    ///
    /// Warnings are always empty because callers are expected to handle parse errors
    /// before collecting `FileLinks`.
    pub(crate) fn from_file_links(
        file_links: Vec<FileLinks>,
        site_prefix: Option<&str>,
    ) -> LinkGraphBuild {
        let mut index: HashMap<String, Vec<BacklinkEntry>> =
            HashMap::with_capacity(file_links.len());
        for fl in file_links {
            insert_file_links(&mut index, fl, site_prefix);
        }
        LinkGraphBuild {
            graph: Self { index },
            warnings: Vec::new(),
        }
    }

    /// Return the set of all normalized link targets that have at least one
    /// inbound link.
    pub fn all_targets(&self) -> HashSet<String> {
        self.index.keys().cloned().collect()
    }

    /// Return the set of all files that contain at least one outbound link.
    /// Paths are vault-relative with forward slashes (e.g. `"notes/a.md"`).
    pub fn all_sources(&self) -> HashSet<String> {
        self.index
            .values()
            .flatten()
            .map(|entry| {
                // Normalize to forward slashes for consistent cross-platform comparison
                // with vault-relative paths from discovery.
                entry.source.to_string_lossy().replace('\\', "/")
            })
            .collect()
    }

    /// Look up all files that link to the given target.
    ///
    /// `target` should be the relative path without `.md` extension (matching
    /// how wikilinks are written), or with `.md` for markdown-style links.
    /// Both forms are checked.
    ///
    /// Self-links are **not** filtered here — the raw index is returned.
    /// Callers that present results to users (e.g. the `backlinks` command and
    /// `find --fields backlinks`) should filter with [`is_self_link`].  The
    /// `mv` command must *not* filter so that a file's own self-references are
    /// also rewritten when it is renamed.
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

    /// Update the link graph after a file move from `old_rel` to `new_rel`.
    ///
    /// Renames target keys and source paths that reference the old vault-relative
    /// path to use the new path. Both `.md` and stem (without `.md`) variants
    /// are handled.
    pub fn rename_path(&mut self, old_rel: &str, new_rel: &str) {
        let old_stem = old_rel.strip_suffix(".md").unwrap_or(old_rel);
        let new_stem = new_rel.strip_suffix(".md").unwrap_or(new_rel);

        // Rename target keys: both stem and full-path (.md) forms.
        let key_pairs: [(&str, &str); 2] = [(old_stem, new_stem), (old_rel, new_rel)];
        for (old_key, new_key) in key_pairs {
            // Only rename if the key actually changed (avoids a spurious
            // remove+insert when old_rel already lacks an .md suffix).
            if old_key != new_key
                && let Some(entries) = self.index.remove(old_key)
            {
                self.index.insert(new_key.to_owned(), entries);
            }
        }

        // Rename source paths: any BacklinkEntry whose source matches old_rel.
        // Compare in forward-slash form so the check works cross-platform
        // regardless of how the PathBuf was originally constructed.
        let new_path = PathBuf::from(new_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        for entries in self.index.values_mut() {
            for entry in entries.iter_mut() {
                let src_fwd = entry.source.to_string_lossy().replace('\\', "/");
                if src_fwd == old_rel {
                    entry.source.clone_from(&new_path);
                }
            }
        }
    }
}

/// Returns `true` when `entry` is a self-link — i.e. the source file and the
/// lookup target refer to the same vault-relative path.
///
/// Both sides are normalised to forward-slash form and compared with and
/// without the `.md` extension so all link styles are covered.
///
/// Use this to filter [`LinkGraph::backlinks`] results at display boundaries
/// (CLI commands) where self-links should be hidden from the user.
pub fn is_self_link(entry: &BacklinkEntry, target: &str) -> bool {
    let alt = if let Some(stem) = target.strip_suffix(".md") {
        stem.to_string()
    } else {
        format!("{target}.md")
    };
    let source = entry.source.to_string_lossy().replace('\\', "/");
    source == target || source == alt
}

/// Per-file link data produced by scanning a single file.
pub struct FileLinks {
    /// Relative path of the source file (vault-relative).
    pub source: PathBuf,
    /// Links extracted from the file body, with 1-based line numbers.
    pub links: Vec<(usize, Link)>,
}

/// Normalize and insert one file's links into the shared index.
fn insert_file_links(
    index: &mut HashMap<String, Vec<BacklinkEntry>>,
    file_links: FileLinks,
    site_prefix: Option<&str>,
) {
    for (line, mut link) in file_links.links {
        // Normalize markdown link targets that contain path separators
        // so that, for example, `sub/a.md` linking to `../target.md`
        // is stored as `target.md`, matching how callers query by
        // vault-relative path.
        //
        // Wikilinks are vault-relative by definition — `[[backlog/item]]`
        // written in any file always refers to `backlog/item.md` at the
        // vault root, never a path relative to the source file.  They
        // must NOT be passed through `normalize_target`.
        if link.kind == LinkKind::Markdown
            && (link.target.contains('/') || link.target.contains('\\'))
        {
            if link.target.starts_with('/') {
                link.target = strip_site_prefix(&link.target, site_prefix);
            } else {
                link.target = normalize_target(&file_links.source, &link.target);
            }
        }
        index
            .entry(link.target.clone())
            .or_default()
            .push(BacklinkEntry {
                source: file_links.source.clone(),
                line,
                link,
            });
    }
}

/// Visitor that collects links with their line numbers.
/// Skips frontmatter parsing for performance.
pub(crate) struct LinkGraphVisitor {
    source: PathBuf,
    links: Vec<(usize, Link)>,
    scratch: Vec<Link>,
}

impl LinkGraphVisitor {
    /// Create a new visitor for the given source file (vault-relative path).
    pub fn new(source: PathBuf) -> Self {
        Self {
            source,
            links: Vec::new(),
            scratch: Vec::new(),
        }
    }

    /// Consume the visitor and return the collected per-file link data.
    pub fn into_file_links(self) -> FileLinks {
        FileLinks {
            source: self.source,
            links: self.links,
        }
    }
}

impl FileVisitor for LinkGraphVisitor {
    fn on_body_line(&mut self, raw: &str, cleaned: &str, line_num: usize) -> ScanAction {
        // Use `cleaned` (inline code and comments stripped) so that [[links]]
        // inside backtick spans are not indexed as real links.
        // Pass `raw` as original so backtick-wrapped link labels are preserved.
        self.scratch.clear();
        extract_links_from_text_with_original(cleaned, raw, &mut self.scratch);
        for link in self.scratch.drain(..) {
            self.links.push((line_num, link));
        }
        ScanAction::Continue
    }

    fn needs_frontmatter(&self) -> bool {
        false
    }
}

/// Strip the leading `/` and optional site prefix from an absolute link target.
///
/// For example, with `site_prefix = Some("docs")`:
///   `/docs/page.md`  → `page.md`
///   `/docs/sub/a.md` → `sub/a.md`
///   `/other/b.md`    → `other/b.md`  (prefix doesn't match, just strip `/`)
///
/// With `site_prefix = None`:
///   `/page.md` → `page.md`
pub(crate) fn strip_site_prefix(target: &str, site_prefix: Option<&str>) -> String {
    let without_slash = target.strip_prefix('/').unwrap_or(target);
    if let Some(prefix) = site_prefix {
        // Try stripping "prefix/" from the front
        let with_slash = format!("{prefix}/");
        if let Some(rest) = without_slash.strip_prefix(&with_slash) {
            return rest.to_owned();
        }
    }
    without_slash.to_owned()
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
            // CurDir (`.`), Prefix, and RootDir are all skipped: `.` is a no-op,
            // Prefix / RootDir only appear on absolute paths; skip them so we
            // always produce a relative vault path.
            Component::CurDir | Component::Prefix(_) | Component::RootDir => {}
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
    use std::fmt::Write as _;
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
        let build = LinkGraph::build(vault.path(), None).unwrap();
        let graph = build.graph;

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
        let build = LinkGraph::build(vault.path(), None).unwrap();
        let graph = build.graph;

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
        let build = LinkGraph::build(vault.path(), None).unwrap();
        let graph = build.graph;

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
        let build = LinkGraph::build(vault.path(), None).unwrap();
        let graph = build.graph;

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
        let build = LinkGraph::build(vault.path(), None).unwrap();
        let graph = build.graph;

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
        let build = LinkGraph::build(vault.path(), None).unwrap();
        let graph = build.graph;

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
        let build = LinkGraph::build(vault.path(), None).unwrap();
        let graph = build.graph;

        assert!(graph.backlinks("b").is_empty());
        assert_eq!(graph.backlinks("c").len(), 1);
    }

    #[test]
    fn links_inside_inline_code_ignored() {
        let vault = create_vault(&[("a.md", "Use `[[b]]` syntax and [[c]]\n")]);
        let build = LinkGraph::build(vault.path(), None).unwrap();
        let graph = build.graph;

        assert!(graph.backlinks("b").is_empty());
        assert_eq!(graph.backlinks("c").len(), 1);
    }

    #[test]
    fn malformed_yaml_ignored_when_frontmatter_not_needed() {
        // With needs_frontmatter=false, malformed YAML is never parsed — file is still indexed
        let vault = create_vault(&[("a.md", "---\n: bad yaml [[\n---\n[[b]]\n")]);
        let build = LinkGraph::build(vault.path(), None).unwrap();
        let graph = build.graph;
        assert_eq!(graph.backlinks("b").len(), 1);
    }

    #[test]
    fn unclosed_frontmatter_skipped() {
        // Unclosed frontmatter triggers an error even with needs_frontmatter=false.
        // The link graph builder should warn-and-skip, not fatally error.
        let vault = create_vault(&[
            ("good.md", "---\ntitle: Good\n---\n[[target]]\n"),
            (
                "bad.md",
                "---\nunclosed frontmatter without closing delimiter\n",
            ),
            ("also_good.md", "---\ntitle: Also Good\n---\n[[target]]\n"),
        ]);
        let build = LinkGraph::build(vault.path(), None).unwrap();

        // Warning should mention the bad file
        assert_eq!(build.warnings.len(), 1);
        assert!(build.warnings[0].0.to_str().unwrap().contains("bad.md"));

        // Both good files should contribute their links
        let bl = build.graph.backlinks("target");
        assert_eq!(bl.len(), 2, "both good files should link to target");
    }

    #[test]
    fn frontmatter_too_large_skipped() {
        // A file with >200 frontmatter content lines (no closing ---) should be skipped.
        // MAX_FRONTMATTER_LINES is 200; 201 content lines triggers the error.
        let mut huge_fm = String::from("---\n");
        for i in 0..201 {
            let _ = writeln!(huge_fm, "key{i}: value");
        }
        // No closing ---, so scanner bails with "frontmatter too large"
        huge_fm.push_str("[[target]]\n");

        let vault = create_vault(&[("good.md", "[[target]]\n"), ("huge.md", &huge_fm)]);
        let build = LinkGraph::build(vault.path(), None).unwrap();
        let graph = build.graph;

        let bl = graph.backlinks("target");
        assert_eq!(bl.len(), 1, "good file should still be indexed");
    }

    #[test]
    fn empty_vault() {
        let vault = create_vault(&[]);
        let build = LinkGraph::build(vault.path(), None).unwrap();
        let graph = build.graph;
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

    #[test]
    fn wikilink_with_path_separator_found_by_backlinks() {
        // This is the core bug: [[backlog/item]] from any directory should
        // be findable via backlinks("backlog/item.md") or backlinks("backlog/item").
        let vault = create_vault(&[
            ("backlog/item.md", "---\ntitle: Item\n---\nContent\n"),
            (
                "iterations/iter-1.md",
                "---\ntitle: Iter 1\n---\nSee [[backlog/item]]\n",
            ),
        ]);
        let build = LinkGraph::build(vault.path(), None).unwrap();
        let graph = build.graph;

        let bl = graph.backlinks("backlog/item");
        assert_eq!(bl.len(), 1, "backlinks('backlog/item') should find 1 link");
        assert_eq!(bl[0].source, PathBuf::from("iterations/iter-1.md"));

        let bl = graph.backlinks("backlog/item.md");
        assert_eq!(
            bl.len(),
            1,
            "backlinks('backlog/item.md') should find 1 link"
        );
    }

    #[test]
    fn wikilink_with_path_and_md_extension() {
        // [[backlog/item.md]] — explicit extension in wikilink
        let vault = create_vault(&[
            ("backlog/item.md", "Content\n"),
            ("a.md", "See [[backlog/item.md]]\n"),
        ]);
        let build = LinkGraph::build(vault.path(), None).unwrap();
        let graph = build.graph;

        let bl = graph.backlinks("backlog/item.md");
        assert_eq!(bl.len(), 1);

        let bl = graph.backlinks("backlog/item");
        assert_eq!(bl.len(), 1);
    }

    #[test]
    fn wikilink_from_subdirectory_with_path() {
        // A file in sub/ links to [[other/target]] — the target is vault-relative,
        // NOT relative to sub/.
        let vault = create_vault(&[
            ("other/target.md", "Content\n"),
            ("sub/source.md", "See [[other/target]]\n"),
        ]);
        let build = LinkGraph::build(vault.path(), None).unwrap();
        let graph = build.graph;

        // Must find the backlink via vault-relative path
        let bl = graph.backlinks("other/target");
        assert_eq!(bl.len(), 1, "should find wikilink from sub/source.md");
        assert_eq!(bl[0].source, PathBuf::from("sub/source.md"));

        // Must NOT store it under the incorrectly-normalized key "sub/other/target"
        let bl_wrong = graph.backlinks("sub/other/target");
        assert!(
            bl_wrong.is_empty(),
            "should NOT find under sub/other/target"
        );
    }

    #[test]
    fn markdown_link_with_path_still_normalized() {
        // Relative markdown links from subdirectories must STILL be normalized.
        // This ensures we didn't break existing behavior.
        let vault = create_vault(&[
            ("target.md", "Content\n"),
            ("sub/source.md", "See [link](../target.md)\n"),
        ]);
        let build = LinkGraph::build(vault.path(), None).unwrap();
        let graph = build.graph;

        let bl = graph.backlinks("target.md");
        assert_eq!(
            bl.len(),
            1,
            "relative markdown link should still be normalized"
        );
        assert_eq!(bl[0].source, PathBuf::from("sub/source.md"));
    }

    #[test]
    fn absolute_link_resolved_with_site_prefix() {
        // With site_prefix = Some("docs"), `/docs/target.md` resolves to
        // `target.md` — matching how the vault is rooted at `docs/`.
        let vault = create_vault(&[
            ("source.md", "[link](/docs/target.md)\n"),
            ("target.md", "# Target\n"),
        ]);
        let build = LinkGraph::build(vault.path(), Some("docs")).unwrap();
        let graph = build.graph;

        let bl = graph.backlinks("target.md");
        assert_eq!(bl.len(), 1, "absolute link should resolve to target.md");
        assert_eq!(bl[0].source, PathBuf::from("source.md"));
    }

    #[test]
    fn absolute_link_without_prefix() {
        // With site_prefix = None, `/page.md` resolves to `page.md` by
        // stripping only the leading `/`.
        let vault = create_vault(&[("source.md", "[link](/page.md)\n"), ("page.md", "# Page\n")]);
        let build = LinkGraph::build(vault.path(), None).unwrap();
        let graph = build.graph;

        let bl = graph.backlinks("page.md");
        assert_eq!(
            bl.len(),
            1,
            "absolute link should resolve to page.md with no prefix"
        );
        assert_eq!(bl[0].source, PathBuf::from("source.md"));

        // The raw `/page.md` form must NOT appear in the index.
        let raw_bl = graph.backlinks("/page.md");
        assert!(raw_bl.is_empty(), "raw absolute path must not be indexed");
    }

    #[test]
    fn self_links_present_in_raw_backlinks() {
        // backlinks() returns the raw index — self-links are included.
        // Filtering is delegated to is_self_link() at the CLI layer.
        let vault = create_vault(&[
            ("a.md", "Self-reference: [[a]] and also [[b]]\n"),
            ("b.md", "Link to [[a]]\n"),
        ]);
        let build = LinkGraph::build(vault.path(), None).unwrap();
        let graph = build.graph;

        // Raw backlinks for "a" include the self-link from a.md
        let bl_a = graph.backlinks("a");
        assert_eq!(bl_a.len(), 2, "raw results must include the self-link");
        let sources: Vec<_> = bl_a.iter().map(|e| e.source.to_str().unwrap()).collect();
        assert!(
            sources.contains(&"a.md"),
            "self-link must be in raw results"
        );
        assert!(sources.contains(&"b.md"));

        // After filtering with is_self_link, only b.md remains
        let filtered: Vec<_> = bl_a.into_iter().filter(|e| !is_self_link(e, "a")).collect();
        assert_eq!(filtered.len(), 1, "filtered result excludes the self-link");
        assert_eq!(filtered[0].source, PathBuf::from("b.md"));

        // b.md has no external links to itself — raw count is still 1
        let bl_b = graph.backlinks("b");
        assert_eq!(bl_b.len(), 1, "a.md links to b, so 1 raw backlink expected");
        assert_eq!(bl_b[0].source, PathBuf::from("a.md"));
    }

    #[test]
    fn self_links_present_with_md_extension() {
        // a.md links to itself via [link](a.md) — markdown-style self-link.
        // backlinks() returns it; is_self_link() detects it.
        let vault = create_vault(&[
            ("a.md", "Self: [me](a.md)\n"),
            ("b.md", "Also: [a](a.md)\n"),
        ]);
        let build = LinkGraph::build(vault.path(), None).unwrap();
        let graph = build.graph;

        let bl = graph.backlinks("a.md");
        assert_eq!(
            bl.len(),
            2,
            "raw results include both self and external link"
        );

        let filtered: Vec<_> = bl
            .into_iter()
            .filter(|e| !is_self_link(e, "a.md"))
            .collect();
        assert_eq!(filtered.len(), 1, "only b.md after filtering");
        assert_eq!(filtered[0].source, PathBuf::from("b.md"));
    }

    #[test]
    fn self_link_only_raw_has_one_entry() {
        // a.md only links to itself — raw backlinks has one entry (the self-link).
        // After filtering with is_self_link the result is empty.
        let vault = create_vault(&[("a.md", "See [[a]] for details.\n")]);
        let build = LinkGraph::build(vault.path(), None).unwrap();
        let graph = build.graph;

        let raw_a = graph.backlinks("a");
        assert_eq!(raw_a.len(), 1, "raw result contains the self-link");
        assert!(is_self_link(raw_a[0], "a"));

        let raw_a_md = graph.backlinks("a.md");
        assert_eq!(raw_a_md.len(), 1, "same via .md form");
        assert!(is_self_link(raw_a_md[0], "a.md"));

        // After filtering, empty
        assert_eq!(
            graph
                .backlinks("a")
                .into_iter()
                .filter(|e| !is_self_link(e, "a"))
                .count(),
            0,
            "filtered backlinks must be empty for a self-link-only file"
        );
    }

    #[test]
    fn from_file_links_parity_with_build() {
        // Build via LinkGraph::build and via LinkGraph::from_file_links for the
        // same vault, then assert backlinks() results are identical for every target.
        let vault = create_vault(&[
            ("a.md", "---\ntitle: A\n---\nSee [[b]] and [c](c.md)\n"),
            ("b.md", "---\ntitle: B\n---\nSee [[a]]\n"),
            ("c.md", "---\ntitle: C\n---\nNo links here\n"),
        ]);

        // Path 1: build via the standard scanner
        let build1 = LinkGraph::build(vault.path(), None).unwrap();
        let graph1 = build1.graph;

        // Path 2: scan with LinkGraphVisitor, collect FileLinks, then from_file_links
        let files = crate::discovery::discover_files(vault.path()).unwrap();
        let file_links: Vec<FileLinks> = files
            .iter()
            .map(|full_path| {
                let rel = full_path
                    .strip_prefix(vault.path())
                    .unwrap_or(full_path)
                    .to_path_buf();
                let mut visitor = LinkGraphVisitor::new(rel);
                crate::scanner::scan_file_multi(full_path, &mut [&mut visitor]).unwrap();
                visitor.into_file_links()
            })
            .collect();
        let build2 = LinkGraph::from_file_links(file_links, None);
        let graph2 = build2.graph;

        // Verify warnings from from_file_links are always empty
        assert!(build2.warnings.is_empty());

        // Both graphs must produce identical backlinks for all targets
        for target in &["a", "b", "c.md", "c"] {
            let mut bl1: Vec<&str> = graph1
                .backlinks(target)
                .iter()
                .map(|e| e.source.to_str().unwrap())
                .collect();
            let mut bl2: Vec<&str> = graph2
                .backlinks(target)
                .iter()
                .map(|e| e.source.to_str().unwrap())
                .collect();
            bl1.sort_unstable();
            bl2.sort_unstable();
            assert_eq!(
                bl1, bl2,
                "backlinks mismatch for target '{target}': build={bl1:?} vs from_file_links={bl2:?}"
            );
        }
    }

    #[test]
    fn mixed_wiki_and_markdown_links_with_paths() {
        // Same file has both a wikilink and markdown link to different targets.
        let vault = create_vault(&[
            ("docs/a.md", "Content A\n"),
            ("notes/b.md", "Content B\n"),
            (
                "sub/source.md",
                "Wiki: [[docs/a]] and md: [link](../notes/b.md)\n",
            ),
        ]);
        let build = LinkGraph::build(vault.path(), None).unwrap();
        let graph = build.graph;

        let bl_a = graph.backlinks("docs/a");
        assert_eq!(bl_a.len(), 1, "wikilink to docs/a should be found");

        let bl_b = graph.backlinks("notes/b.md");
        assert_eq!(bl_b.len(), 1, "markdown link to notes/b.md should be found");
    }

    #[test]
    fn rename_path_updates_keys_and_sources() {
        // Build a graph where:
        //   - "source.md" links to "notes/old" (wikilink → stem key)
        //   - "other.md"  links to "notes/old.md" (markdown → full key)
        //   - "notes/old.md" links to "unrelated" (its source should be renamed)
        let vault = create_vault(&[
            ("source.md", "See [[notes/old]]\n"),
            ("other.md", "[link](notes/old.md)\n"),
            ("notes/old.md", "See [[unrelated]]\n"),
            ("unrelated.md", "Content\n"),
        ]);
        let build = LinkGraph::build(vault.path(), None).unwrap();
        let mut graph = build.graph;

        graph.rename_path("notes/old.md", "notes/new.md");

        // 1. Old keys must be gone — no backlinks for old stem or old full path.
        assert!(
            graph.backlinks("notes/old").is_empty(),
            "old stem key must be gone"
        );
        assert!(
            graph.backlinks("notes/old.md").is_empty(),
            "old full key must be gone"
        );

        // 2. `backlinks("notes/new")` finds both the stem key ("notes/new")
        //    and the full key ("notes/new.md"), so it returns 2 entries total:
        //    the wikilink from source.md and the markdown link from other.md.
        let bl = graph.backlinks("notes/new");
        assert_eq!(bl.len(), 2, "must find both wikilink and markdown link entries");
        let sources: Vec<String> = bl
            .iter()
            .map(|b| b.source.to_string_lossy().replace('\\', "/"))
            .collect();
        assert!(sources.contains(&"source.md".to_owned()), "wikilink source; got: {sources:?}");
        assert!(sources.contains(&"other.md".to_owned()), "markdown link source; got: {sources:?}");

        // 3. The source entry under "unrelated" must now point to "notes/new.md".
        let bl_unrelated = graph.backlinks("unrelated");
        assert_eq!(
            bl_unrelated.len(),
            1,
            "unrelated must still have 1 backlink"
        );
        let src_fwd = bl_unrelated[0].source.to_string_lossy().replace('\\', "/");
        assert_eq!(
            src_fwd, "notes/new.md",
            "source path must be updated to new path"
        );
    }
}
