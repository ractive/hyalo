//! Vault index abstraction — decouples commands from their data source.
//!
//! The [`VaultIndex`] trait provides a uniform interface over pre-scanned vault
//! data. Commands program against this trait and don't know whether data came
//! from a live filesystem scan ([`ScannedIndex`]) or a serialized snapshot.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::filter::extract_tags;
use crate::frontmatter;
use crate::link_graph::{FileLinks, LinkGraph, LinkGraphVisitor};
use crate::links::Link;
use crate::scanner::{self, FileVisitor, FrontmatterCollector, ScanAction};
use crate::tasks::TaskExtractor;
use crate::types::{FindTaskInfo, OutlineSection, TaskCount};

// ---------------------------------------------------------------------------
// IndexEntry
// ---------------------------------------------------------------------------

/// Per-file pre-scanned data stored in the index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    /// Vault-relative path (forward slashes).
    pub rel_path: String,
    /// ISO 8601 mtime string.
    pub modified: String,
    /// Raw frontmatter properties.
    pub properties: BTreeMap<String, serde_yaml_ng::Value>,
    /// Extracted tags (from properties).
    pub tags: Vec<String>,
    /// Document outline sections.
    pub sections: Vec<OutlineSection>,
    /// Task checkboxes with section context.
    pub tasks: Vec<FindTaskInfo>,
    /// Outbound links with 1-based line numbers.
    pub links: Vec<(usize, Link)>,
}

// ---------------------------------------------------------------------------
// VaultIndex trait
// ---------------------------------------------------------------------------

/// Abstraction over how vault data is obtained.
/// Commands program against this trait, not a concrete data source.
pub trait VaultIndex {
    /// All entries in the index, in vault-relative path order.
    fn entries(&self) -> &[IndexEntry];

    /// Look up a single file by vault-relative path.
    fn get(&self, rel_path: &str) -> Option<&IndexEntry>;

    /// The pre-built link graph for backlink lookups.
    fn link_graph(&self) -> &LinkGraph;
}

// ---------------------------------------------------------------------------
// ScannedIndex — live filesystem scan
// ---------------------------------------------------------------------------

/// A vault index built by scanning files from disk.
///
/// This extracts the per-file scan logic that was previously inlined in each
/// command (`find`, `summary`, etc.) into a reusable builder behind the
/// [`VaultIndex`] trait. No new functionality — it's a refactor of existing
/// scanning patterns.
pub struct ScannedIndex {
    entries: Vec<IndexEntry>,
    /// Fast path → index lookup built at construction time.
    path_index: HashMap<String, usize>,
    graph: LinkGraph,
}

/// Warning produced during index build (e.g. malformed YAML frontmatter).
pub struct IndexWarning {
    /// Vault-relative path of the file that was skipped.
    pub rel_path: String,
    /// Human-readable error message.
    pub message: String,
}

/// Result of building a [`ScannedIndex`].
pub struct ScannedIndexBuild {
    /// The built index.
    pub index: ScannedIndex,
    /// Files that were skipped (e.g. malformed frontmatter).
    pub warnings: Vec<IndexWarning>,
}

impl ScannedIndex {
    /// Build an index by scanning a list of files from disk.
    ///
    /// `files` is a slice of `(full_path, rel_path)` pairs, as returned by
    /// `collect_files` or `discover_files`. Each file is scanned in a single
    /// pass with multiple visitors.
    ///
    /// `site_prefix` is passed through to the link graph builder for resolving
    /// absolute links.
    pub fn build(
        files: &[(PathBuf, String)],
        site_prefix: Option<&str>,
    ) -> Result<ScannedIndexBuild> {
        let mut entries = Vec::with_capacity(files.len());
        let mut file_links_vec: Vec<FileLinks> = Vec::with_capacity(files.len());
        let mut warnings: Vec<IndexWarning> = Vec::new();

        for (full_path, rel_path) in files {
            match scan_one_file(full_path, rel_path) {
                Ok((entry, file_links)) => {
                    entries.push(entry);
                    file_links_vec.push(file_links);
                }
                Err(e) if frontmatter::is_parse_error(&e) => {
                    warnings.push(IndexWarning {
                        rel_path: rel_path.clone(),
                        message: e.to_string(),
                    });
                }
                Err(e) => return Err(e),
            }
        }

        let graph_build = LinkGraph::from_file_links(file_links_vec, site_prefix);
        // from_file_links never produces warnings (callers handle parse errors
        // before collecting FileLinks), but we could extend this if needed.

        let path_index: HashMap<String, usize> = entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.rel_path.clone(), i))
            .collect();

        Ok(ScannedIndexBuild {
            index: ScannedIndex {
                entries,
                path_index,
                graph: graph_build.graph,
            },
            warnings,
        })
    }
}

impl VaultIndex for ScannedIndex {
    fn entries(&self) -> &[IndexEntry] {
        &self.entries
    }

    fn get(&self, rel_path: &str) -> Option<&IndexEntry> {
        self.path_index.get(rel_path).map(|&i| &self.entries[i])
    }

    fn link_graph(&self) -> &LinkGraph {
        &self.graph
    }
}

// ---------------------------------------------------------------------------
// SnapshotIndex — MessagePack-serialized snapshot
// ---------------------------------------------------------------------------

/// Metadata header embedded in every snapshot file.
#[derive(Debug, Serialize, Deserialize)]
struct SnapshotHeader {
    /// Canonical vault directory path (informational; not re-validated on load).
    vault_dir: String,
    /// Site prefix used when building the index (informational).
    site_prefix: Option<String>,
    /// Unix timestamp (seconds) when the snapshot was created.
    created_at: u64,
    /// PID of the process that created this snapshot.
    pid: u32,
}

/// Internal serialization envelope — header + entries + graph.
#[derive(Serialize, Deserialize)]
struct SnapshotData {
    header: SnapshotHeader,
    entries: Vec<IndexEntry>,
    graph: LinkGraph,
}

/// A vault index loaded from a MessagePack snapshot file.
///
/// Created by [`SnapshotIndex::save`] and loaded by [`SnapshotIndex::load`].
/// Implements [`VaultIndex`] so commands can use it transparently.
pub struct SnapshotIndex {
    entries: Vec<IndexEntry>,
    /// Fast path → index lookup built after deserialization.
    path_index: HashMap<String, usize>,
    graph: LinkGraph,
    header: SnapshotHeader,
}

impl SnapshotIndex {
    /// Load a snapshot from a MessagePack file.
    ///
    /// Returns `Ok(Some(index))` on success.
    /// Returns `Ok(None)` when the file is present but cannot be deserialized
    /// (e.g. after a hyalo upgrade that changed the schema) — callers should
    /// fall back to a disk scan.
    /// Returns `Err` only for hard I/O failures.
    pub fn load(path: &Path) -> Result<Option<Self>> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("failed to read index file: {}", path.display()))?;

        match rmp_serde::from_slice::<SnapshotData>(&bytes) {
            Ok(data) => {
                let path_index: HashMap<String, usize> = data
                    .entries
                    .iter()
                    .enumerate()
                    .map(|(i, e)| (e.rel_path.clone(), i))
                    .collect();
                Ok(Some(Self {
                    entries: data.entries,
                    path_index,
                    graph: data.graph,
                    header: data.header,
                }))
            }
            Err(e) => {
                eprintln!(
                    "warning: index file is incompatible ({}); falling back to disk scan",
                    e
                );
                Ok(None)
            }
        }
    }

    /// Save a snapshot of `index` to a MessagePack file at `path`.
    ///
    /// `vault_dir` and `site_prefix` are stored in the header for informational
    /// purposes (shown by `create-index` on load; not validated on subsequent loads).
    pub fn save(
        index: &dyn VaultIndex,
        path: &Path,
        vault_dir: &str,
        site_prefix: Option<&str>,
    ) -> Result<()> {
        let header = SnapshotHeader {
            vault_dir: vault_dir.to_owned(),
            site_prefix: site_prefix.map(str::to_owned),
            created_at: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            pid: std::process::id(),
        };
        let data = SnapshotData {
            header,
            entries: index.entries().to_vec(),
            graph: index.link_graph().clone(),
        };
        let bytes = rmp_serde::to_vec_named(&data).context("failed to serialize index")?;
        let tmp_path = path.with_extension("hyalo-index.tmp");
        std::fs::write(&tmp_path, &bytes)
            .with_context(|| format!("failed to write temp index: {}", tmp_path.display()))?;
        std::fs::rename(&tmp_path, path)
            .with_context(|| format!("failed to rename index into place: {}", path.display()))?;
        Ok(())
    }

    /// Return header metadata: `(vault_dir, site_prefix, created_at_secs, pid)`.
    pub fn header_info(&self) -> (&str, Option<&str>, u64, u32) {
        (
            &self.header.vault_dir,
            self.header.site_prefix.as_deref(),
            self.header.created_at,
            self.header.pid,
        )
    }
}

impl VaultIndex for SnapshotIndex {
    fn entries(&self) -> &[IndexEntry] {
        &self.entries
    }

    fn get(&self, rel_path: &str) -> Option<&IndexEntry> {
        self.path_index.get(rel_path).map(|&i| &self.entries[i])
    }

    fn link_graph(&self) -> &LinkGraph {
        &self.graph
    }
}

/// Check whether a PID corresponds to a running process.
///
/// On Unix this uses `kill(pid, 0)` (signal 0 is a no-op that only tests
/// existence). On all other platforms we conservatively assume the PID is
/// alive so that we never falsely claim a running process is stale.
fn is_pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // SAFETY: kill(pid, 0) is a pure existence check — no signal is sent.
        // Returns 0 if the process exists (and we have permission), -1 otherwise.
        unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        true
    }
}

/// Scan `dir` for `.hyalo-index` files whose creator PID is no longer running.
///
/// Returns a list of `(path, vault_dir, created_at)` tuples for stale files.
/// Files that cannot be loaded (incompatible schema, I/O error) are silently
/// skipped — they are already unreachable by the normal load path.
pub fn find_stale_indexes(dir: &Path) -> Result<Vec<(PathBuf, String, u64)>> {
    let mut stale = Vec::new();
    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return Ok(stale),
    };
    for entry in read_dir {
        let entry = entry?;
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if !name.ends_with(".hyalo-index") {
            continue;
        }
        if let Ok(Some(idx)) = SnapshotIndex::load(&path) {
            let (vault_dir, _, created_at, pid) = idx.header_info();
            if !is_pid_alive(pid) {
                stale.push((path, vault_dir.to_owned(), created_at));
            }
        }
    }
    Ok(stale)
}

// ---------------------------------------------------------------------------
// Per-file scan — single pass with multiple visitors
// ---------------------------------------------------------------------------

/// Scan a single file and return its `IndexEntry` plus `FileLinks` for the
/// link graph. Mirrors the multi-visitor pattern used by the `find` command.
fn scan_one_file(full_path: &Path, rel_path: &str) -> Result<(IndexEntry, FileLinks)> {
    let mut fm = FrontmatterCollector::new(true);
    let mut section_scanner = SectionScanner::new();
    let mut task_extractor = TaskExtractor::new();
    let mut link_visitor = LinkGraphVisitor::new(PathBuf::from(rel_path));

    scanner::scan_file_multi(
        full_path,
        &mut [
            &mut fm,
            &mut section_scanner,
            &mut task_extractor,
            &mut link_visitor,
        ],
    )?;

    let props = fm.into_props();
    let tags = extract_tags(&props);
    let sections = section_scanner.into_sections();
    let tasks = task_extractor.into_tasks();
    let file_links = link_visitor.into_file_links();

    // Clone link data before it's moved into the FileLinks (we need both
    // the raw links for IndexEntry and the FileLinks for the graph builder).
    let links_clone: Vec<(usize, Link)> = file_links
        .links
        .iter()
        .map(|(line, link)| (*line, link.clone()))
        .collect();

    let modified = format_modified(full_path)?;

    let entry = IndexEntry {
        rel_path: rel_path.to_owned(),
        modified,
        properties: props,
        tags,
        sections,
        tasks,
        links: links_clone,
    };

    Ok((entry, file_links))
}

/// Format a file's last-modified time as ISO 8601 UTC.
fn format_modified(path: &Path) -> Result<String> {
    let meta = std::fs::metadata(path)
        .with_context(|| format!("failed to read metadata for {}", path.display()))?;
    let mtime = meta
        .modified()
        .with_context(|| format!("mtime not available for {}", path.display()))?;
    let secs = mtime
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Ok(format_iso8601(secs))
}

/// Format Unix timestamp as ISO 8601 UTC (`YYYY-MM-DDTHH:MM:SSZ`).
pub fn format_iso8601(secs: u64) -> String {
    const SECS_PER_MIN: u64 = 60;
    const SECS_PER_HOUR: u64 = 3600;
    const SECS_PER_DAY: u64 = 86400;

    let days = secs / SECS_PER_DAY;
    let rem = secs % SECS_PER_DAY;
    let hh = rem / SECS_PER_HOUR;
    let mm = (rem % SECS_PER_HOUR) / SECS_PER_MIN;
    let ss = rem % SECS_PER_MIN;

    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

// ---------------------------------------------------------------------------
// SectionScanner — inline visitor (mirrors hyalo-cli's SectionScanner)
// ---------------------------------------------------------------------------

// We need a section scanner here in hyalo-core for the index builder.
// This is equivalent to the one in hyalo-cli/src/commands/section_scanner.rs
// but lives in core so it can be used without depending on the CLI crate.

use crate::heading::parse_atx_heading;
use crate::links;

/// State accumulated for the current section being built.
struct SectionBuilder {
    level: u8,
    heading: Option<String>,
    line: usize,
    links: Vec<String>,
    task_total: usize,
    task_done: usize,
    code_blocks: Vec<String>,
}

impl SectionBuilder {
    fn new(level: u8, heading: Option<String>, line: usize) -> Self {
        Self {
            level,
            heading,
            line,
            links: Vec::new(),
            task_total: 0,
            task_done: 0,
            code_blocks: Vec::new(),
        }
    }

    fn finish(self) -> OutlineSection {
        let tasks = if self.task_total > 0 {
            Some(TaskCount {
                total: self.task_total,
                done: self.task_done,
            })
        } else {
            None
        };
        OutlineSection {
            level: self.level,
            heading: self.heading,
            line: self.line,
            links: self.links,
            tasks,
            code_blocks: self.code_blocks,
        }
    }
}

/// Visitor that builds outline sections from body events.
struct SectionScanner {
    current: SectionBuilder,
    sections: Vec<OutlineSection>,
}

impl SectionScanner {
    fn new() -> Self {
        Self {
            current: SectionBuilder::new(0, None, 1),
            sections: Vec::new(),
        }
    }

    fn into_sections(mut self) -> Vec<OutlineSection> {
        let last = std::mem::replace(&mut self.current, SectionBuilder::new(0, None, 0));
        let finished = last.finish();
        let should_emit = finished.level > 0
            || !finished.links.is_empty()
            || finished.tasks.is_some()
            || !finished.code_blocks.is_empty();
        if should_emit {
            self.sections.push(finished);
        }
        self.sections
    }
}

impl FileVisitor for SectionScanner {
    fn on_body_line(&mut self, raw: &str, cleaned: &str, line_num: usize) -> ScanAction {
        if let Some((level, heading_text)) = parse_atx_heading(raw) {
            let finished = std::mem::replace(
                &mut self.current,
                SectionBuilder::new(level, Some(heading_text.to_owned()), line_num),
            );
            let should_emit = finished.level > 0
                || !finished.links.is_empty()
                || finished.task_total > 0
                || !finished.code_blocks.is_empty();
            if should_emit {
                self.sections.push(finished.finish());
            }
            return ScanAction::Continue;
        }

        let mut line_links: Vec<links::Link> = Vec::new();
        links::extract_links_from_text(cleaned, &mut line_links);
        for link in line_links {
            self.current.links.push(format_link_string(&link));
        }

        if let Some((_status, done)) = crate::tasks::detect_task_checkbox(raw) {
            self.current.task_total += 1;
            if done {
                self.current.task_done += 1;
            }
        }

        ScanAction::Continue
    }

    fn on_code_fence_open(&mut self, _raw: &str, language: &str, _line_num: usize) -> ScanAction {
        if !language.is_empty() {
            self.current.code_blocks.push(language.to_owned());
        }
        ScanAction::Continue
    }
}

/// Format a `Link` into a human-readable string for storage in the outline.
fn format_link_string(link: &links::Link) -> String {
    match link.kind {
        links::LinkKind::Wikilink => match &link.label {
            Some(label) if !label.is_empty() => format!("[[{}|{}]]", link.target, label),
            _ => format!("[[{}]]", link.target),
        },
        links::LinkKind::Markdown => match &link.label {
            Some(label) if !label.is_empty() => format!("[{}]({})", label, link.target),
            _ => format!("[]({})", link.target),
        },
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    macro_rules! md {
        ($s:expr) => {
            $s.strip_prefix('\n').unwrap_or($s)
        };
    }

    fn setup_vault() -> (tempfile::TempDir, Vec<(PathBuf, String)>) {
        let tmp = tempfile::tempdir().unwrap();

        fs::write(
            tmp.path().join("a.md"),
            md!(r"
---
title: Alpha
status: draft
tags:
  - rust
  - cli
---
# Introduction

See [[b]] for context.

## Tasks

- [ ] Write tests
- [x] Write code
"),
        )
        .unwrap();

        fs::write(
            tmp.path().join("b.md"),
            md!(r"
---
title: Beta
status: done
tags:
  - rust
---
# Content

See [[a]] for details.
"),
        )
        .unwrap();

        let files = vec![
            (tmp.path().join("a.md"), "a.md".to_owned()),
            (tmp.path().join("b.md"), "b.md".to_owned()),
        ];
        (tmp, files)
    }

    #[test]
    fn scanned_index_builds_entries() {
        let (_tmp, files) = setup_vault();
        let build = ScannedIndex::build(&files, None).unwrap();
        assert!(build.warnings.is_empty());
        assert_eq!(build.index.entries().len(), 2);
    }

    #[test]
    fn scanned_index_get_by_path() {
        let (_tmp, files) = setup_vault();
        let build = ScannedIndex::build(&files, None).unwrap();
        let idx = &build.index;

        let a = idx.get("a.md").unwrap();
        assert_eq!(a.tags, vec!["rust", "cli"]);
        assert_eq!(a.properties.get("status").unwrap(), "draft");

        let b = idx.get("b.md").unwrap();
        assert_eq!(b.tags, vec!["rust"]);

        assert!(idx.get("c.md").is_none());
    }

    #[test]
    fn scanned_index_sections_and_tasks() {
        let (_tmp, files) = setup_vault();
        let build = ScannedIndex::build(&files, None).unwrap();
        let a = build.index.get("a.md").unwrap();

        // a.md has 2 sections: Introduction and Tasks
        assert_eq!(a.sections.len(), 2);
        assert_eq!(a.sections[0].heading.as_deref(), Some("Introduction"));
        assert_eq!(a.sections[1].heading.as_deref(), Some("Tasks"));

        // a.md has 2 tasks
        assert_eq!(a.tasks.len(), 2);
        assert!(!a.tasks[0].done);
        assert!(a.tasks[1].done);
    }

    #[test]
    fn scanned_index_link_graph() {
        let (_tmp, files) = setup_vault();
        let build = ScannedIndex::build(&files, None).unwrap();
        let graph = build.index.link_graph();

        // a.md links to b, b.md links to a
        let a_backlinks = graph.backlinks("a");
        assert!(!a_backlinks.is_empty());
        let b_backlinks = graph.backlinks("b");
        assert!(!b_backlinks.is_empty());
    }

    #[test]
    fn scanned_index_outbound_links() {
        let (_tmp, files) = setup_vault();
        let build = ScannedIndex::build(&files, None).unwrap();
        let a = build.index.get("a.md").unwrap();

        // a.md has one outbound link: [[b]]
        assert_eq!(a.links.len(), 1);
        assert_eq!(a.links[0].1.target, "b");
    }

    #[test]
    fn scanned_index_skips_broken_frontmatter() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("good.md"),
            md!(r"
---
title: Good
---
Content.
"),
        )
        .unwrap();
        fs::write(
            tmp.path().join("bad.md"),
            "---\n: invalid yaml [[[{\n---\nContent.\n",
        )
        .unwrap();

        let files = vec![
            (tmp.path().join("good.md"), "good.md".to_owned()),
            (tmp.path().join("bad.md"), "bad.md".to_owned()),
        ];
        let build = ScannedIndex::build(&files, None).unwrap();
        assert_eq!(build.index.entries().len(), 1);
        assert_eq!(build.warnings.len(), 1);
        assert_eq!(build.warnings[0].rel_path, "bad.md");
    }

    #[test]
    fn scanned_index_modified_is_iso8601() {
        let (_tmp, files) = setup_vault();
        let build = ScannedIndex::build(&files, None).unwrap();
        let a = build.index.get("a.md").unwrap();
        assert!(
            a.modified.contains('T') && a.modified.ends_with('Z'),
            "unexpected timestamp: {}",
            a.modified
        );
    }

    #[test]
    fn snapshot_roundtrip() {
        let (_tmp, files) = setup_vault();
        let build = ScannedIndex::build(&files, None).unwrap();
        let index = &build.index;

        let snap_dir = tempfile::tempdir().unwrap();
        let snap_path = snap_dir.path().join(".hyalo-index");

        SnapshotIndex::save(index, &snap_path, "/tmp/vault", None).unwrap();
        let loaded = SnapshotIndex::load(&snap_path)
            .unwrap()
            .expect("snapshot should deserialize");

        assert_eq!(loaded.entries().len(), index.entries().len());
        let a = loaded.get("a.md").unwrap();
        assert_eq!(a.tags, vec!["rust", "cli"]);
        assert_eq!(a.properties.get("status").unwrap(), "draft");
        assert_eq!(a.sections.len(), 2);
        assert_eq!(a.tasks.len(), 2);
        assert_eq!(a.links.len(), 1);
        assert_eq!(a.links[0].1.target, "b");

        // Link graph survives roundtrip
        let bl = loaded.link_graph().backlinks("a");
        assert!(!bl.is_empty());
    }
}
