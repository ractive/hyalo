//! Vault index abstraction — decouples commands from their data source.
//!
//! The [`VaultIndex`] trait provides a uniform interface over pre-scanned vault
//! data. Commands program against this trait and don't know whether data came
//! from a live filesystem scan ([`ScannedIndex`]) or a serialized snapshot.

use anyhow::{Context, Result};
use indexmap::IndexMap;
#[cfg(not(miri))]
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::bm25::{Bm25InvertedIndex, resolve_language, tokenize};
use crate::case_index::CaseInsensitiveIndex;
use crate::filter::extract_tags;
use crate::frontmatter;
use crate::link_graph::{
    DEFAULT_FRONTMATTER_LINK_PROPERTIES, FileLinks, LinkGraph, LinkGraphVisitor,
};
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
    /// File size in bytes (iteration 252). Defaulted so a snapshot written by
    /// an older hyalo still loads — it reports `0` until the next
    /// `create-index`.
    #[serde(default)]
    pub size: u64,
    /// Line count (see [`crate::scanner::ScanStats`] for the exact
    /// definition). Defaulted for the same backwards-compatibility reason as
    /// [`size`](Self::size).
    #[serde(default)]
    pub lines: usize,
    /// Raw frontmatter properties.
    pub properties: IndexMap<String, serde_json::Value>,
    /// Extracted tags (from properties).
    pub tags: Vec<String>,
    /// Document outline sections.
    pub sections: Vec<OutlineSection>,
    /// Task checkboxes with section context.
    pub tasks: Vec<FindTaskInfo>,
    /// Outbound links with 1-based line numbers.
    pub links: Vec<(usize, Link)>,
    /// Same-file heading anchors (`[b](#frag)`, `[[#frag]]`) with 1-based line
    /// numbers; the fragment text only, without the leading `#`.
    ///
    /// Separate from [`links`](Self::links) because these have no target file:
    /// they name a heading in *this* document. `find --broken-links` validates
    /// them against this entry's own [`sections`](Self::sections)
    /// (iter-211 / BUG-8). Defaulted + skipped when empty so snapshots written
    /// by older hyalo versions keep loading.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub self_anchors: Vec<(usize, String)>,
    /// Pre-tokenized BM25 tokens (body + title, stemmed). Populated by `create-index`
    /// when `scan_body` is `true`. `None` when the index was created before BM25
    /// support or with `scan_body = false`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bm25_tokens: Option<Vec<String>>,
    /// Stemming language used when producing [`bm25_tokens`]. Matches the
    /// `language` frontmatter property of this document (or `"english"` as the
    /// default). `None` when [`bm25_tokens`] is `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bm25_language: Option<String>,
    /// [`crate::bm25::TOKENIZER_VERSION`] that produced [`bm25_tokens`]. `None`
    /// when [`bm25_tokens`] is `None`, or when the snapshot predates this field
    /// (serde default) — either case reads as "not the current tokenizer",
    /// which is exactly the fallback condition readers want. Lets `find` detect
    /// a snapshot tokenized before a `tokenize()` algorithm change (e.g. DEC-094's
    /// CJK-bigram fix) and re-tokenize from disk instead of silently serving
    /// stale tokens that can never match a since-fixed query shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bm25_tokenizer_version: Option<u32>,
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

    /// Return the persisted BM25 inverted index, if available.
    ///
    /// Returns `Some` only for [`SnapshotIndex`] instances that were saved with
    /// `bm25_tokenize = true`. Returns `None` for live [`ScannedIndex`] instances
    /// and for snapshots built without BM25 tokenization.
    fn bm25_index(&self) -> Option<&Bm25InvertedIndex> {
        None
    }
}

// ---------------------------------------------------------------------------
// ScanOptions — controls what ScannedIndex::build scans
// ---------------------------------------------------------------------------

/// Controls which parts of each file are scanned during index building.
///
/// When `scan_body` is `false`, only YAML frontmatter is read — sections, tasks,
/// and links fields in [`IndexEntry`] will be empty `Vec`s. The [`LinkGraph`]
/// will be empty.  This is an optimization for commands that only need
/// frontmatter data (e.g. `properties summary`, `tags summary`,
/// `find --property status=planned` without body fields).
#[derive(Debug, Clone)]
pub struct ScanOptions<'a> {
    /// When false, only frontmatter is read.
    pub scan_body: bool,
    /// When true, pre-tokenize file content for BM25 search and store tokens
    /// in each [`IndexEntry`]. This requires an extra file read per document
    /// and is intended only for `create-index` (the write path), not for live
    /// scanning at query time.
    pub bm25_tokenize: bool,
    /// Default stemming language from `[search] language` in `.hyalo.toml`.
    /// Used as the fallback language when a document has no `language` frontmatter
    /// property. `None` falls back to English.
    pub default_language: Option<&'a str>,
    /// Frontmatter property names scanned for `[[wikilink]]` values during link
    /// graph construction. `None` uses [`DEFAULT_FRONTMATTER_LINK_PROPERTIES`].
    pub frontmatter_link_props: Option<&'a [String]>,
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
        options: &ScanOptions<'_>,
    ) -> Result<ScannedIndexBuild> {
        let mut entries = Vec::with_capacity(files.len());
        let mut file_links_vec: Vec<FileLinks> = Vec::with_capacity(files.len());
        let mut warnings: Vec<IndexWarning> = Vec::new();

        let default_language = options.default_language;
        let fm_link_props: Vec<String> = options.frontmatter_link_props.map_or_else(
            || {
                DEFAULT_FRONTMATTER_LINK_PROPERTIES
                    .iter()
                    .map(|s| (*s).to_owned())
                    .collect()
            },
            <[String]>::to_vec,
        );
        let scan = |(full_path, rel_path): &(std::path::PathBuf, String)| {
            scan_one_file(
                full_path,
                rel_path,
                options.scan_body,
                options.bm25_tokenize,
                default_language,
                &fm_link_props,
            )
        };
        #[cfg(not(miri))]
        let results: Vec<Result<(IndexEntry, Option<FileLinks>)>> =
            files.par_iter().map(scan).collect();
        #[cfg(miri)]
        let results: Vec<Result<(IndexEntry, Option<FileLinks>)>> =
            files.iter().map(scan).collect();

        for (i, result) in results.into_iter().enumerate() {
            match result {
                Ok((entry, file_links)) => {
                    entries.push(entry);
                    if let Some(fl) = file_links {
                        file_links_vec.push(fl);
                    }
                }
                Err(e) if frontmatter::is_parse_error(&e) => {
                    warnings.push(IndexWarning {
                        rel_path: files[i].1.clone(),
                        message: e.to_string(),
                    });
                }
                Err(e) => return Err(e),
            }
        }

        // Sort entries by vault-relative path so VaultIndex::entries() guarantees
        // a stable, deterministic order (as documented on the trait).
        entries.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

        let graph = if options.scan_body {
            let graph_build = LinkGraph::from_file_links(file_links_vec, site_prefix);
            graph_build.graph
        } else {
            LinkGraph::default()
        };

        // Build path_index AFTER sorting so indices remain valid.
        let path_index: HashMap<String, usize> = entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.rel_path.clone(), i))
            .collect();

        Ok(ScannedIndexBuild {
            index: ScannedIndex {
                entries,
                path_index,
                graph,
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

/// Internal serialization envelope — header + entries + graph + optional BM25 index.
#[derive(Serialize, Deserialize)]
struct SnapshotData {
    header: SnapshotHeader,
    entries: Vec<IndexEntry>,
    graph: LinkGraph,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bm25_index: Option<Bm25InvertedIndex>,
}

/// Borrowed variant used only for serialization — avoids cloning all entries.
#[derive(Serialize)]
struct SnapshotDataRef<'a> {
    header: SnapshotHeader,
    entries: &'a [IndexEntry],
    graph: &'a LinkGraph,
    #[serde(skip_serializing_if = "Option::is_none")]
    bm25_index: Option<&'a Bm25InvertedIndex>,
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
    /// Persisted BM25 inverted index (if the snapshot was built with `bm25_tokenize = true`).
    bm25_index: Option<Bm25InvertedIndex>,
    /// Frontmatter property names scanned for `[[wikilink]]` values when
    /// `rescan_entry` / `rename_entry` re-scan a file after a mutation. Not
    /// persisted in the snapshot — callers must set it for each session via
    /// [`SnapshotIndex::set_frontmatter_link_props`]; `None` falls back to
    /// [`DEFAULT_FRONTMATTER_LINK_PROPERTIES`].
    frontmatter_link_props: Option<Vec<String>>,
    /// Lazily built case-insensitive path lookup used by incremental link
    /// refreshes ([`refresh_links`](Self::refresh_links)). Rebuilding it per
    /// refreshed file would be O(entries × refreshed files) in bulk loops
    /// (`lint --fix --index`, `links fix --apply`), so it is cached here and
    /// invalidated whenever the set of indexed paths changes
    /// ([`rebuild_path_index`](Self::rebuild_path_index)). Not persisted.
    case_index_cache: Option<CaseInsensitiveIndex>,
}

impl SnapshotIndex {
    // ------------------------------------------------------------------
    // Mutation helpers — update entries in-place after a mutation command
    // ------------------------------------------------------------------

    /// Remove an entry by vault-relative path (for `mv` old path).
    pub fn remove_entry(&mut self, rel_path: &str) {
        if let Some(&idx) = self.path_index.get(rel_path) {
            self.entries.remove(idx);
            self.rebuild_path_index();
        }
    }

    /// Insert a new entry (for `mv` new path). Maintains sorted order.
    pub fn insert_entry(&mut self, entry: IndexEntry) {
        let pos = self
            .entries
            .binary_search_by(|e| e.rel_path.cmp(&entry.rel_path))
            .unwrap_or_else(|i| i);
        self.entries.insert(pos, entry);
        self.rebuild_path_index();
    }

    /// Get a mutable reference to an entry by path.
    pub fn get_mut(&mut self, rel_path: &str) -> Option<&mut IndexEntry> {
        self.path_index
            .get(rel_path)
            .copied()
            .map(|i| &mut self.entries[i])
    }

    /// Get a mutable reference to the link graph for in-place updates.
    pub fn graph_mut(&mut self) -> &mut LinkGraph {
        &mut self.graph
    }

    /// Set the frontmatter-property list used by `rescan_entry` / `rename_entry`
    /// when they re-scan a file after a mutation. Callers typically set this
    /// once after loading the snapshot from the active `.hyalo.toml` config so
    /// incremental re-scans produce the same link set as the initial build.
    ///
    /// Pass `None` to fall back to [`DEFAULT_FRONTMATTER_LINK_PROPERTIES`].
    pub fn set_frontmatter_link_props(&mut self, props: Option<Vec<String>>) {
        self.frontmatter_link_props = props;
    }

    /// Resolved frontmatter property list — either the session-configured list
    /// or the built-in defaults.
    fn effective_frontmatter_link_props(&self) -> Vec<String> {
        self.frontmatter_link_props.clone().unwrap_or_else(|| {
            DEFAULT_FRONTMATTER_LINK_PROPERTIES
                .iter()
                .map(|s| (*s).to_owned())
                .collect()
        })
    }

    /// BM25 scan arguments for an incremental re-scan of `rel_path` (BUG-4,
    /// iter-244).
    ///
    /// When the snapshot carries a persisted BM25 inverted index, a re-scanned
    /// entry must come back with fresh `bm25_tokens` — otherwise a mutation
    /// wave leaves the inverted index rebuilt from stale tokens and
    /// `find --index` scores drift from a disk scan. The previous entry's
    /// `bm25_language` is passed as the scan's default language so an
    /// unchanged language config keeps producing identical tokens (frontmatter
    /// `language` still wins inside [`scan_one_file`]).
    fn bm25_scan_args(&self, rel_path: &str) -> (bool, Option<String>) {
        (
            self.bm25_index.is_some(),
            self.path_index
                .get(rel_path)
                .and_then(|&i| self.entries[i].bm25_language.clone()),
        )
    }

    /// Rebuild the persisted BM25 inverted index from the current entries
    /// (BUG-4, iter-244). No-op when this snapshot has no BM25 index.
    ///
    /// Entries mutated since load carry fresh `bm25_tokens` (incremental
    /// re-scans tokenize when a BM25 index is present). Entries the mutation
    /// wave never touched had their tokens stripped at snapshot-write time —
    /// their tokens are reconstructed from the *old* inverted index's
    /// postings, which is exactly what a no-change rebuild would produce.
    ///
    /// Call once after a mutation wave, before [`Self::save_to`], so corpus
    /// statistics (N, per-term df, avgdl, doc lengths) match a fresh
    /// `create-index` build and `find --index` scores stay byte-identical to
    /// a disk scan without an intervening rebuild.
    pub fn rebuild_bm25_index(&mut self) {
        let Some(old) = self.bm25_index.as_ref() else {
            return;
        };
        let reconstructed = old.reconstruct_all_tokens();
        let docs: Vec<crate::bm25::PreTokenizedInput> = self
            .entries
            .iter()
            .filter_map(|e| {
                let tokens = e
                    .bm25_tokens
                    .clone()
                    .or_else(|| reconstructed.get(e.rel_path.as_str()).cloned())?;
                Some(crate::bm25::PreTokenizedInput {
                    rel_path: e.rel_path.clone(),
                    tokens,
                })
            })
            .collect();
        self.bm25_index = Some(crate::bm25::Bm25InvertedIndex::build_from_tokens(docs));
    }

    /// Re-scan a single file and replace its index entry.
    ///
    /// Returns the `FileLinks` for the re-scanned file so the caller can
    /// update the link graph separately. Returns `Ok(None)` if the file
    /// is not in the index.
    pub(crate) fn rescan_entry(&mut self, dir: &Path, rel_path: &str) -> Result<Option<FileLinks>> {
        self.rescan_entry_at(&dir.join(rel_path), rel_path)
    }

    /// Shared implementation behind [`rescan_entry`] and [`refresh_links`]:
    /// scan `full_path` from disk and replace the index entry for `rel_path`
    /// wholesale. Returns the file's `FileLinks` for graph updates, or `None`
    /// if `rel_path` is not in the index.
    fn rescan_entry_at(&mut self, full_path: &Path, rel_path: &str) -> Result<Option<FileLinks>> {
        let Some(&idx) = self.path_index.get(rel_path) else {
            return Ok(None);
        };
        let fm_props = self.effective_frontmatter_link_props();
        let (bm25_tokenize, default_language) = self.bm25_scan_args(rel_path);
        let (entry, file_links) = scan_one_file(
            full_path,
            rel_path,
            true,
            bm25_tokenize,
            default_language.as_deref(),
            &fm_props,
        )?;
        self.entries[idx] = entry;
        Ok(file_links)
    }

    /// Build a case-insensitive lookup index from every path currently known
    /// to this snapshot. Used to resolve bare-basename wikilinks (e.g.
    /// `[[note]]` matching a unique `sub/note.md`) the same way a full
    /// [`LinkGraph::build`] does, without persisting the case index in the
    /// snapshot itself.
    fn build_case_index(&self) -> CaseInsensitiveIndex {
        let mut case_index = CaseInsensitiveIndex::new();
        case_index.set_case_insensitive_paths(true);
        for entry in &self.entries {
            case_index.insert(&entry.rel_path);
        }
        case_index
    }

    /// Re-scan the body/frontmatter of `rel_path` (already written to disk at
    /// `full_path`) and refresh the parts of its index entry and the
    /// persisted [`LinkGraph`] that are derived from wikilinks: the entry's
    /// `sections`, `tasks`, and `links` fields, plus the graph's outbound
    /// edges for this file.
    ///
    /// `size` and `lines` are refreshed too (both are derived from the bytes
    /// on disk, which a body rewrite changes).
    ///
    /// `properties`, `tags`, and `modified` are left untouched — callers that
    /// already know the new values in memory (e.g. `set`/`append`/`remove`)
    /// should patch those directly, since they don't require a disk read.
    /// `bm25_tokens`/`bm25_language` are refreshed from the re-scan when the
    /// snapshot carries a BM25 inverted index (BUG-4, iter-244); without one
    /// they stay untouched, as only `create-index --bm25` populates them.
    ///
    /// This closes the gap where a frontmatter link property (`related`,
    /// `depends-on`, ...) mutated via `set`/`append`/`remove`/`lint --fix`
    /// with `--index` left the persisted link graph stale — `backlinks` and
    /// `find --fields backlinks`/`links` would return pre-mutation results
    /// until a full `create-index` rebuild.
    ///
    /// Returns `Ok(true)` if `rel_path` was found and refreshed, `Ok(false)`
    /// if it is not in the index (a no-op).
    pub fn refresh_links(&mut self, full_path: &Path, rel_path: &str) -> Result<bool> {
        let Some(&idx) = self.path_index.get(rel_path) else {
            return Ok(false);
        };
        let fm_props = self.effective_frontmatter_link_props();
        let (bm25_tokenize, default_language) = self.bm25_scan_args(rel_path);
        let (scanned, file_links) = scan_one_file(
            full_path,
            rel_path,
            true,
            bm25_tokenize,
            default_language.as_deref(),
            &fm_props,
        )?;

        let entry = &mut self.entries[idx];
        entry.sections = scanned.sections;
        entry.tasks = scanned.tasks;
        entry.links = scanned.links;
        // iter-252: `size`/`lines` are body-derived, so every write path that
        // routes through `refresh_links` (set/append/remove, task toggles)
        // keeps them in step with the bytes now on disk.
        entry.size = scanned.size;
        entry.lines = scanned.lines;
        // BUG-4 (iter-244): keep BM25 tokens current alongside the body so a
        // post-mutation rebuild of the inverted index scores this file as a
        // fresh disk scan would. Only touched when the snapshot is a BM25
        // snapshot (`scan_one_file` returns `None` otherwise).
        entry.bm25_tokens = scanned.bm25_tokens;
        entry.bm25_language = scanned.bm25_language;
        entry.bm25_tokenizer_version = scanned.bm25_tokenizer_version;

        self.graph.remove_source(rel_path);
        if let Some(fl) = file_links {
            self.insert_graph_links(fl);
        }

        Ok(true)
    }

    /// Insert a file's outbound links into the graph, using (and lazily
    /// building) the cached case-insensitive index. The cache is taken out
    /// and put back so `self.graph` and `self.header` can be borrowed
    /// alongside it.
    fn insert_graph_links(&mut self, fl: FileLinks) {
        let case_index = self
            .case_index_cache
            .take()
            .unwrap_or_else(|| self.build_case_index());
        self.graph
            .insert_links(fl, self.header.site_prefix.as_deref(), &case_index);
        self.case_index_cache = Some(case_index);
    }

    /// Re-scan `rel_path` once and refresh both its full index entry (like
    /// [`Self::refresh_entry`]) and the persisted link graph (like
    /// [`Self::refresh_links`]) — one disk read instead of two for callers
    /// that need both, e.g. `links fix --apply --index` patching every
    /// rewritten file.
    ///
    /// Returns `Ok(true)` if `rel_path` was found and refreshed, `Ok(false)`
    /// if it is not in the index (a no-op).
    pub fn refresh_entry_and_links(&mut self, dir: &Path, rel_path: &str) -> Result<bool> {
        self.refresh_entry_and_links_at(&dir.join(rel_path), rel_path)
    }

    /// [`Self::refresh_entry_and_links`] for callers that already hold the
    /// file's full path.
    ///
    /// `dir.join(rel_path)` is not always the path a command actually read:
    /// a positional file argument may reach the command through a symlink or
    /// a canonicalised prefix, and re-deriving it from `dir` would scan a
    /// different file (or none). Mutation commands carry both halves, so they
    /// pass the one they read (iter-255, BUG-2).
    pub fn refresh_entry_and_links_at(&mut self, full_path: &Path, rel_path: &str) -> Result<bool> {
        if !self.path_index.contains_key(rel_path) {
            return Ok(false);
        }
        let file_links = self.rescan_entry_at(full_path, rel_path)?;
        self.graph.remove_source(rel_path);
        if let Some(fl) = file_links {
            self.insert_graph_links(fl);
        }
        Ok(true)
    }

    /// Re-scan a single file from disk and replace its index entry in-place.
    ///
    /// This updates the entry's properties, tags, sections, tasks, links, and
    /// modified timestamp. The link graph is **not** touched — callers that
    /// need graph updates should use [`LinkGraph::rename_path`] separately.
    ///
    /// Returns `true` if the entry was found and refreshed, `false` if
    /// `rel_path` is not in the index.
    pub fn refresh_entry(&mut self, dir: &Path, rel_path: &str) -> Result<bool> {
        match self.rescan_entry(dir, rel_path)? {
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }

    /// Scan a file at `full_path` and insert (or replace) its entry under
    /// `rel_path`, maintaining sorted order. Returns the file's `FileLinks`
    /// for callers that want to update the link graph themselves.
    fn insert_or_replace_entry_impl(
        &mut self,
        full_path: &Path,
        rel_path: &str,
    ) -> Result<Option<FileLinks>> {
        let fm_props = self.effective_frontmatter_link_props();
        let (bm25_tokenize, default_language) = self.bm25_scan_args(rel_path);
        let (entry, file_links) = scan_one_file(
            full_path,
            rel_path,
            true,
            bm25_tokenize,
            default_language.as_deref(),
            &fm_props,
        )?;
        if let Some(&idx) = self.path_index.get(rel_path) {
            self.entries[idx] = entry;
        } else {
            let pos = self
                .entries
                .binary_search_by(|e| e.rel_path.cmp(&entry.rel_path))
                .unwrap_or_else(|i| i);
            self.entries.insert(pos, entry);
            self.rebuild_path_index();
        }
        Ok(file_links)
    }

    /// Scan a file at `full_path` and insert (or replace) its entry under
    /// `rel_path`, maintaining sorted order.
    ///
    /// Use this after creating a brand-new file (e.g. from `hyalo new`) so the
    /// snapshot index sees the file without a full rebuild. When `rel_path` is
    /// already present, the existing entry is replaced in-place (idempotent).
    ///
    /// The link graph is **not** touched — outbound links from the new file
    /// will only appear in backlink queries after the next full `create-index`
    /// rebuild, or immediately when using
    /// [`Self::insert_or_replace_entry_with_links`]. BM25 tokens are
    /// re-scanned when the snapshot carries a BM25 inverted index (BUG-4,
    /// iter-244), so a `new` file's body enters the rebuilt corpus statistics.
    /// This matches the behaviour of [`SnapshotIndex::refresh_entry`] and the
    /// other in-place mutation helpers (set/append/lint --fix). Callers that
    /// need the link graph kept current immediately should use
    /// [`Self::insert_or_replace_entry_with_links`] instead.
    pub fn insert_or_replace_entry(&mut self, full_path: &Path, rel_path: &str) -> Result<()> {
        self.insert_or_replace_entry_impl(full_path, rel_path)?;
        Ok(())
    }

    /// Like [`Self::insert_or_replace_entry`], but also registers the file's
    /// outbound links in the persisted [`crate::link_graph::LinkGraph`] —
    /// one disk scan, entry and graph both current.
    ///
    /// Use this to *upsert* an entry for a file the index has never seen
    /// (e.g. a file created outside `hyalo new`, or present before the
    /// index existed) from a mutating command that must keep backlink
    /// queries accurate without a full `create-index` rebuild.
    ///
    /// Idempotent: replaces any existing entry/edges for `rel_path` first.
    pub fn insert_or_replace_entry_with_links(
        &mut self,
        full_path: &Path,
        rel_path: &str,
    ) -> Result<()> {
        let file_links = self.insert_or_replace_entry_impl(full_path, rel_path)?;
        self.graph.remove_source(rel_path);
        if let Some(fl) = file_links {
            self.insert_graph_links(fl);
        }
        Ok(())
    }

    /// Rename an entry: remove the old entry, scan the file at its new path,
    /// and insert the result — rebuilding the path index only once.
    ///
    /// This is the preferred move/rename counterpart of [`refresh_entry`].
    /// Unlike calling [`remove_entry`] followed by [`insert_entry`] (two
    /// path-index rebuilds), this method defers the rebuild until both the
    /// removal and insertion are complete.
    ///
    /// The link graph is **not** touched — callers must update it separately
    /// via [`LinkGraph::rename_path`].
    ///
    /// Returns `Ok(true)` if `old_rel` was found and replaced, `Ok(false)` if
    /// `old_rel` was not in the index (in which case nothing is changed).
    pub fn rename_entry(&mut self, dir: &Path, old_rel: &str, new_rel: &str) -> Result<bool> {
        let Some(&old_idx) = self.path_index.get(old_rel) else {
            return Ok(false);
        };

        // Scan first — if this fails, the index is left untouched.
        let full_path = dir.join(new_rel);
        let fm_props = self.effective_frontmatter_link_props();
        let (bm25_tokenize, default_language) = self.bm25_scan_args(old_rel);
        let (entry, _file_links) = scan_one_file(
            &full_path,
            new_rel,
            true,
            bm25_tokenize,
            default_language.as_deref(),
            &fm_props,
        )?;

        // Remove without triggering a path-index rebuild.
        self.entries.remove(old_idx);

        // Insert in sorted order.
        let pos = self
            .entries
            .binary_search_by(|e| e.rel_path.cmp(&entry.rel_path))
            .unwrap_or_else(|i| i);
        self.entries.insert(pos, entry);

        // Single rebuild covering both the removal and the insertion.
        self.rebuild_path_index();
        Ok(true)
    }

    /// Rebuild the path → index lookup after insertions/removals.
    ///
    /// Also drops the cached case-insensitive index: every code path that
    /// changes the set of indexed paths funnels through here, so this is the
    /// single invalidation point for [`Self::case_index_cache`].
    fn rebuild_path_index(&mut self) {
        self.case_index_cache = None;
        self.path_index = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.rel_path.clone(), i))
            .collect();
    }

    /// Re-serialize and atomically save the (possibly mutated) snapshot.
    ///
    /// Reuses the original header's `vault_dir` and `site_prefix`.
    pub fn save_to(&self, path: &Path) -> Result<()> {
        write_snapshot(
            self,
            path,
            &self.header.vault_dir,
            self.header.site_prefix.as_deref(),
            self.bm25_index.as_ref(),
        )
    }

    // ------------------------------------------------------------------
    // Deserialization
    // ------------------------------------------------------------------

    /// Deserialize snapshot bytes into a `SnapshotIndex`, optionally printing a
    /// warning when the schema is incompatible.
    ///
    /// Returns `Ok(Some(index))` on success, `Ok(None)` on schema mismatch.
    fn load_inner(bytes: &[u8], warn: bool) -> Option<Self> {
        match rmp_serde::from_slice::<SnapshotData>(bytes) {
            Ok(data) => {
                // Limits used by the SEC-2 and SEC-3 defense-in-depth checks below.
                // All consts are hoisted to the top of the arm so they appear before
                // any statements (clippy::items_after_statements).
                const MAX_ENTRIES: usize = 5_000_000;
                const MAX_GRAPH_EDGES: usize = 50_000_000;
                const MAX_BM25_POSTINGS: usize = 50_000_000;

                // SEC-2 (defense-in-depth): reject snapshots with an implausible
                // number of entries — a crafted MessagePack header claiming millions
                // of entries can trigger large allocations even with file-size caps.
                if data.entries.len() > MAX_ENTRIES {
                    if warn {
                        eprintln!(
                            "warning: index file contains {} entries (limit {}); falling back to disk scan",
                            data.entries.len(),
                            MAX_ENTRIES
                        );
                    }
                    return None;
                }

                // SEC-1: Validate every rel_path before trusting snapshot data.
                // Reject the entire snapshot if any path is unsafe — a crafted
                // snapshot with path-traversal entries could escape the vault.
                for entry in &data.entries {
                    let rel_path = &entry.rel_path;
                    if rel_path.contains('\0') {
                        if warn {
                            eprintln!(
                                "warning: index file contains unsafe path '{rel_path}'; falling back to disk scan"
                            );
                        }
                        return None;
                    }
                    if std::path::Path::new(rel_path.as_str()).is_absolute() {
                        if warn {
                            eprintln!(
                                "warning: index file contains unsafe path '{rel_path}'; falling back to disk scan"
                            );
                        }
                        return None;
                    }
                    if std::path::Path::new(rel_path.as_str())
                        .components()
                        .any(|c| {
                            matches!(
                                c,
                                std::path::Component::ParentDir
                                    | std::path::Component::RootDir
                                    | std::path::Component::Prefix(_)
                            )
                        })
                    {
                        if warn {
                            eprintln!(
                                "warning: index file contains unsafe path '{rel_path}'; falling back to disk scan"
                            );
                        }
                        return None;
                    }
                    // M-2 (adversarial-review-2026-08-23.md): the `Prefix(_)`
                    // arm above already catches a Windows drive-relative
                    // path like `C:foo`, but an NTFS Alternate Data Stream
                    // marker (`a.md:stream`) has no `Prefix` component at
                    // all — the colon sits inside an ordinary `Normal`
                    // component. Reject it explicitly (Windows-only check;
                    // a no-op elsewhere).
                    if crate::discovery::has_unsafe_windows_colon(rel_path) {
                        if warn {
                            eprintln!(
                                "warning: index file contains unsafe path '{rel_path}'; falling back to disk scan"
                            );
                        }
                        return None;
                    }
                }

                // SEC-3 (defense-in-depth): reject snapshots whose link graph or
                // BM25 index would expand to an implausibly large in-memory
                // structure.  A crafted snapshot could claim a plausible number
                // of top-level keys while hiding millions of per-key entries,
                // causing allocations far exceeding the file size cap.
                let edge_count = data.graph.total_edges();
                if edge_count > MAX_GRAPH_EDGES {
                    if warn {
                        eprintln!(
                            "warning: index file contains too many graph edges ({edge_count}); falling back to disk scan"
                        );
                    }
                    return None;
                }

                if let Some(ref bm25) = data.bm25_index {
                    let posting_count = bm25.total_postings();
                    if posting_count > MAX_BM25_POSTINGS {
                        if warn {
                            eprintln!(
                                "warning: index file contains too many BM25 postings ({posting_count}); falling back to disk scan"
                            );
                        }
                        return None;
                    }
                }

                // MED-1: Validate BM25 doc_id bounds before the index is used.
                // A crafted snapshot can embed posting list entries whose doc_id
                // values exceed doc_paths / doc_lengths, causing an out-of-bounds
                // panic inside score(). Reject the entire snapshot if invalid.
                if let Some(ref bm25) = data.bm25_index
                    && !bm25.validate_doc_ids()
                {
                    if warn {
                        eprintln!(
                            "warning: index file contains out-of-bounds BM25 doc_id; falling back to disk scan"
                        );
                    }
                    return None;
                }

                // Entries are stored in sorted order (ScannedIndex::build sorts
                // before saving).  Re-sort here to guarantee the invariant even
                // if an older snapshot was created without sorting.
                let mut entries = data.entries;
                entries.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

                let path_index: HashMap<String, usize> = entries
                    .iter()
                    .enumerate()
                    .map(|(i, e)| (e.rel_path.clone(), i))
                    .collect();
                // The graph's lowercased companion map is `#[serde(skip)]`, so a
                // freshly-deserialized graph has an empty one — rebuild it from
                // the restored index keys so `backlinks_ci` works off snapshots.
                let mut graph = data.graph;
                graph.rebuild_lower_index();
                Some(Self {
                    entries,
                    path_index,
                    graph,
                    header: data.header,
                    bm25_index: data.bm25_index,
                    frontmatter_link_props: None,
                    case_index_cache: None,
                })
            }
            Err(e) => {
                if warn {
                    eprintln!(
                        "warning: index file is incompatible ({e}); falling back to disk scan"
                    );
                }
                None
            }
        }
    }

    /// Load a snapshot from a MessagePack file.
    ///
    /// Returns `Ok(Some(index))` on success.
    /// Returns `Ok(None)` when the file is present but cannot be deserialized
    /// (e.g. after a hyalo upgrade that changed the schema) — callers should
    /// fall back to a disk scan. A warning is printed to stderr in this case.
    /// Returns `Err` only for hard I/O failures.
    pub fn load(path: &Path) -> Result<Option<Self>> {
        let Some(bytes) = read_index_bytes(path, true)? else {
            return Ok(None);
        };
        Ok(Self::load_inner(&bytes, true))
    }

    /// Load a snapshot silently — identical to [`load`] but suppresses the
    /// incompatibility warning.  Used by `find_stale_indexes` which expects to
    /// silently skip files that cannot be deserialized.
    fn load_silent(path: &Path) -> Result<Option<Self>> {
        let Some(bytes) = read_index_bytes(path, false)? else {
            return Ok(None);
        };
        Ok(Self::load_inner(&bytes, false))
    }

    /// Check whether this snapshot's header matches the expected vault settings.
    ///
    /// Returns `true` when both `vault_dir` and `site_prefix` match the stored
    /// header values.  Callers can use this to detect stale snapshots that were
    /// built for a different vault or with a different site prefix.
    pub fn validate(&self, vault_dir: &str, site_prefix: Option<&str>) -> bool {
        self.header.vault_dir == vault_dir && self.header.site_prefix.as_deref() == site_prefix
    }

    /// Save a snapshot of `index` to a MessagePack file at `path`.
    ///
    /// `vault_dir` and `site_prefix` are stored in the header for informational
    /// purposes (shown by `create-index` on load; not validated on subsequent loads).
    ///
    /// `bm25_index` is an optional pre-built BM25 inverted index to persist alongside
    /// the entries. When `Some`, subsequent loads will expose it via [`VaultIndex::bm25_index`].
    pub fn save(
        index: &dyn VaultIndex,
        path: &Path,
        vault_dir: &str,
        site_prefix: Option<&str>,
        bm25_index: Option<&Bm25InvertedIndex>,
    ) -> Result<()> {
        write_snapshot(index, path, vault_dir, site_prefix, bm25_index)
    }

    /// Return the persisted BM25 inverted index, if present.
    pub fn bm25_index(&self) -> Option<&Bm25InvertedIndex> {
        self.bm25_index.as_ref()
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

/// Maximum index file size accepted by [`read_index_bytes`].
///
/// Files larger than this are almost certainly corrupt or crafted to trigger an
/// OOM condition during deserialization.  512 MiB is a generous upper bound for
/// even the largest real-world knowledgebases.
const MAX_INDEX_FILE_SIZE: u64 = 512 * 1024 * 1024;

/// Read the raw bytes of an index file, enforcing the size limit.
///
/// Returns `Ok(Some(bytes))` when the file is within [`MAX_INDEX_FILE_SIZE`].
/// Returns `Ok(None)` when the file exceeds the limit (a warning is printed
/// when `warn` is `true`).
/// Returns `Err` for hard I/O failures.
fn read_index_bytes(path: &Path, warn: bool) -> Result<Option<Vec<u8>>> {
    use std::io::Read as _;

    let file = std::fs::File::open(path)
        .with_context(|| format!("failed to open index file: {}", path.display()))?;
    let meta = file
        .metadata()
        .with_context(|| format!("failed to stat index file: {}", path.display()))?;
    if meta.len() > MAX_INDEX_FILE_SIZE {
        if warn {
            eprintln!(
                "warning: index file is too large ({} bytes, limit {}); falling back to disk scan",
                meta.len(),
                MAX_INDEX_FILE_SIZE
            );
        }
        return Ok(None);
    }
    // Size was already checked against MAX_INDEX_FILE_SIZE (512 MiB) which
    // fits in usize on all supported targets (32-bit and above).
    #[allow(clippy::cast_possible_truncation)]
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    file.take(MAX_INDEX_FILE_SIZE + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("failed to read index file: {}", path.display()))?;
    Ok(Some(bytes))
}

/// Shared serialization logic for saving a snapshot index to disk.
///
/// Writes to a temporary file first, then atomically renames into place.
fn write_snapshot(
    index: &dyn VaultIndex,
    path: &Path,
    vault_dir: &str,
    site_prefix: Option<&str>,
    bm25_index: Option<&Bm25InvertedIndex>,
) -> Result<()> {
    let header = SnapshotHeader {
        vault_dir: vault_dir.to_owned(),
        site_prefix: site_prefix.map(str::to_owned),
        created_at: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
        pid: std::process::id(),
    };
    // When a BM25 inverted index is present, strip per-entry `bm25_tokens` to
    // avoid duplicating the same data (the inverted index already encodes it).
    // This roughly halves the snapshot size on large vaults.
    let stripped_entries: Vec<IndexEntry>;
    let entries: &[IndexEntry] = if bm25_index.is_some() {
        stripped_entries = index
            .entries()
            .iter()
            .map(|e| {
                let mut e = e.clone();
                e.bm25_tokens = None;
                e.bm25_language = None;
                e
            })
            .collect();
        &stripped_entries
    } else {
        index.entries()
    };

    let data = SnapshotDataRef {
        header,
        entries,
        graph: index.link_graph(),
        bm25_index,
    };
    let bytes = rmp_serde::to_vec_named(&data).context("failed to serialize index")?;
    // Route through the shared write policy (DEC-062: when `path` is a
    // symlink, follow it and replace the *target*, leaving the symlink in
    // place) instead of a hand-rolled `NamedTempFile` + `persist` pair, so
    // index writes give the same answer as every other atomic write in
    // hyalo for the same input (L-1, adversarial-review-2026-08-23.md). This
    // also picks up the kernel-assigned temp-file name in the same directory
    // as the target (the same symlink-substitution defense the old code
    // commented on), permission preservation, and parent-dir fsync for free.
    //
    // MUST be `atomic_write_within`, not the unguarded `atomic_write`: the
    // first cut of this fix used `atomic_write`, which follows a symlink
    // chain with NO boundary check — `atomic_write`'s own doc comment says
    // as much ("this entry point has no vault context — so callers must
    // have already validated the path"). `path` here is never validated
    // that way (it's an index destination, not a `resolve_file`-checked
    // vault file), so a symlinked index (`.hyalo-index -> ../../secret.txt`)
    // let every mutating command that patches the index — `save_to`,
    // reached from `mutation.rs`/`tasks.rs`/`properties.rs`/`tags.rs` —
    // silently clobber a file *outside* the vault with MessagePack bytes.
    // `vault_dir` is the trustworthy boundary to check against here: it is
    // always a canonicalized path (`create_index.rs`'s
    // `std::fs::canonicalize(dir)...`), carried unchanged through
    // `SnapshotHeader` on every subsequent `save_to` of a loaded snapshot.
    // `atomic_write_within` only re-canonicalizes when `path` actually is a
    // symlink (the common non-symlink case pays no extra cost and is
    // unaffected), and an index destination that is legitimately outside
    // the vault — accepted upfront via `create-index --allow-outside-vault`
    // — still writes there directly as long as it is not itself a symlink;
    // a symlink chain redirecting even an allowed-outside destination
    // somewhere else again is refused with a clear error rather than
    // silently followed (verified: adversarial review Finding 1 repro no
    // longer touches the outside target's content).
    crate::fs_util::atomic_write_within(Path::new(vault_dir), path, &bytes)
        .context("failed to write index")?;
    Ok(())
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

    fn bm25_index(&self) -> Option<&Bm25InvertedIndex> {
        self.bm25_index.as_ref()
    }
}

/// Check whether a PID corresponds to a running process.
///
/// On Unix this uses `kill(pid, 0)` (signal 0 is a no-op that only tests
/// existence). On all other platforms we conservatively assume the PID is
/// alive so that we never falsely claim a running process is stale.
fn is_pid_alive(pid: u32) -> bool {
    // pid 0 means "my own process group" for kill() on Unix, not a specific
    // process.  A crafted snapshot with pid=0 would always pass the liveness
    // check, preventing stale-index cleanup.  Guard before platform-specific
    // blocks so it applies on all targets.
    if pid == 0 {
        return false;
    }

    #[cfg(unix)]
    {
        // A tampered snapshot could carry a PID that exceeds `i32::MAX`.  On
        // platforms where `pid_t` is `i32` the cast would wrap, potentially
        // targeting a real process and blocking stale-index cleanup.  Treat
        // any out-of-range PID as "not alive" so the stale index is removed.
        if pid > i32::MAX as u32 {
            return false;
        }

        // SAFETY: kill(pid, 0) sends signal 0, which is a pure existence check —
        // no signal is actually delivered. The only side effect is updating errno.
        // The guard above ensures pid <= i32::MAX, so cast_signed() is lossless.
        let res = unsafe { libc::kill(pid.cast_signed(), 0) };
        if res == 0 {
            // Process exists and we have permission to signal it.
            true
        } else {
            // ESRCH means "no such process" — definitively dead.
            // EPERM means "process exists but we lack permission" — still alive.
            // Any other errno is treated as alive (conservative default).
            let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
            errno != libc::ESRCH
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        true
    }
}

/// Tolerance, in seconds, applied to the snapshot staleness probe.
///
/// `SnapshotHeader::created_at` is truncated to whole seconds while directory
/// mtimes are not, so a directory touched in the same second the snapshot was
/// written can read as up to one second newer. One second of slack keeps the
/// probe from crying stale about the index's own creation.
pub const STALENESS_TOLERANCE_SECS: u64 = 1;

/// Depth to which [`newest_dir_mtime`] descends below the vault root.
///
/// `0` means "`dir` itself only" (the pre-iter-249 behaviour); `1` adds
/// `dir`'s immediate subdirectories (the old `newest_shallow_dir_mtime`
/// probe, which stopped there). iter-249 UX-1 raised this to `3` so the
/// probe also sees a directory created two levels below an existing one —
/// e.g. `iterations/done/` in this vault, or `web/css/zz-dogfood/` in MDN —
/// without paying for a full recursive walk.
///
/// A true unbounded walk was measured (iter-249) against MDN's `en-us` tree
/// (14,375 files across ~14,376 directories — one folder per page, so
/// directory count tracks file count almost 1:1 there) and added roughly
/// 65% to an already-indexed `find --limit 1 --index` query — far past the
/// ~15% budget for a probe that exists purely to avoid a full scan. Capping
/// at depth 3 keeps the walk to the directories that exist above the bulk of
/// real content (MDN's own pages mostly live at relative depth 4+, i.e.
/// *below* this cap) while adding no measurable overhead.
const STALENESS_PROBE_MAX_DEPTH: u32 = 3;

/// Cheap staleness probe: the newest mtime among `dir` and its subdirectories
/// down to [`STALENESS_PROBE_MAX_DEPTH`] levels, as whole seconds since the
/// Unix epoch.
///
/// A directory's mtime moves when an entry is created, renamed or removed
/// inside it, so this catches notes added or deleted within the probed
/// depth — the common "the vault was edited behind the index's back" case —
/// at the cost of one `read_dir` plus one `stat` per directory visited
/// (files are never stat'd, and dot-directories such as `.git`/
/// `.hyalo-index` plus symlinked directories are skipped and not
/// descended into), never a full walk (which is exactly what the snapshot
/// exists to avoid).
///
/// It deliberately does NOT catch every drift: an in-place edit of an
/// existing note (that doesn't touch a directory itself) leaves every
/// directory mtime untouched, and a file added or removed *below*
/// [`STALENESS_PROBE_MAX_DEPTH`] directories from `dir` is invisible —
/// still true for the majority of files in a deeply-nested vault like MDN
/// or GitHub Docs (see the constant's doc for why an unbounded walk isn't
/// affordable there). Callers must treat a `None`/older result as "no
/// evidence of staleness", never as "the index is current".
///
/// Renamed from `newest_shallow_dir_mtime` (iter-249, UX-1 dogfood finding):
/// the old top-two-levels-only probe (depth 1) missed changes in
/// `iterations/done/` and nearly everything in MDN/GitHub Docs, where notes
/// live two or more directories deep.
pub fn newest_dir_mtime(dir: &Path) -> Option<u64> {
    fn mtime_secs(path: &Path) -> Option<u64> {
        std::fs::metadata(path)
            .ok()?
            .modified()
            .ok()?
            .duration_since(SystemTime::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs())
    }

    fn is_hidden(entry: &std::fs::DirEntry) -> bool {
        entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with('.'))
    }

    let mut newest = mtime_secs(dir);
    // (path, depth-below-root) — root itself is depth 0 and is not
    // re-visited (already stat'd above); its immediate children are depth 1.
    let mut stack = vec![(dir.to_path_buf(), 0u32)];
    while let Some((current, depth)) = stack.pop() {
        let Ok(read_dir) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in read_dir.flatten() {
            if is_hidden(&entry) {
                continue;
            }
            // `file_type()` comes from the directory entry on every platform
            // we support, so this costs no extra syscall in the common case.
            // Symlinked directories are skipped rather than followed, to
            // avoid cycles and to match `discover_files`'s vault-boundary
            // treatment of symlinks.
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() || !file_type.is_dir() {
                continue;
            }
            let path = entry.path();
            let child_depth = depth + 1;
            if let Some(m) = mtime_secs(&path) {
                newest = Some(newest.map_or(m, |n: u64| n.max(m)));
            }
            if child_depth < STALENESS_PROBE_MAX_DEPTH {
                stack.push((path, child_depth));
            }
        }
    }
    newest
}

/// Parse an ISO 8601 UTC timestamp as written by [`format_iso8601`]
/// (`YYYY-MM-DDTHH:MM:SSZ`) back to whole Unix seconds.
///
/// Returns `None` for anything that does not match that exact shape —
/// callers treat an unparseable stored mtime as "unknown", never as "stale"
/// or "current".
fn parse_iso8601_secs(s: &str) -> Option<u64> {
    let bytes = s.as_bytes();
    if bytes.len() != 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'Z'
    {
        return None;
    }
    let num = |range: std::ops::Range<usize>| -> Option<u64> {
        let mut v: u64 = 0;
        for &b in &bytes[range] {
            if !b.is_ascii_digit() {
                return None;
            }
            v = v * 10 + u64::from(b - b'0');
        }
        Some(v)
    };
    let year = num(0..4)?;
    let month = num(5..7)?;
    let day = num(8..10)?;
    let hh = num(11..13)?;
    let mm = num(14..16)?;
    let ss = num(17..19)?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hh > 23 || mm > 59 || ss > 59 {
        return None;
    }
    // Reject impossible day-of-month values (PR #277 review N-1): a bare
    // 1..=31 check would silently accept `2026-02-31` and fold it into
    // March 3, desynchronizing the stored mtime from the real one.
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month: [u64; 12] = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if day > days_in_month[usize::try_from(month - 1).ok()?] {
        return None;
    }
    // Days since 1970-01-01 from civil date (Howard Hinnant's algorithm),
    // the same math `format_iso8601` uses in reverse.
    let y = i64::try_from(year).ok()? - i64::from(month <= 2);
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400).cast_unsigned();
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = (era * 146_097 + doe.cast_signed() - 719_468).cast_unsigned();
    Some(days * 86_400 + hh * 3_600 + mm * 60 + ss)
}

/// Entries of a snapshot whose file on disk is newer than the indexed mtime.
///
/// BUG-2 (dogfood v0.20.0) detection half: an in-place edit of an indexed
/// file (by hand, by Obsidian, by anything but hyalo's own journaled write
/// paths) leaves every directory mtime untouched, so the
/// [`newest_dir_mtime`] probe cannot see it — yet the stored
/// `links` field and link-graph edges describe the *old* body, and
/// `links fix --apply --index` would report `broken: 0` for a link that was
/// added seconds ago. This per-entry mtime comparison catches exactly that:
/// one `stat` per indexed file, no content read.
///
/// A file whose stored mtime cannot be parsed, or whose file no longer
/// exists, is not reported — the former is "unknown", the latter is the
/// caller's deletion handling, not staleness.
pub fn files_modified_since_snapshot(index: &SnapshotIndex, dir: &Path) -> Vec<String> {
    let mut stale = Vec::new();
    for entry in index.entries() {
        let rel = entry.rel_path.as_str();
        let Some(indexed) = parse_iso8601_secs(&entry.modified) else {
            continue;
        };
        let Ok(meta) = std::fs::metadata(dir.join(rel)) else {
            continue;
        };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        let Ok(disk) = modified.duration_since(SystemTime::UNIX_EPOCH) else {
            continue;
        };
        if disk.as_secs() > indexed.saturating_add(STALENESS_TOLERANCE_SECS) {
            stale.push(rel.to_owned());
        }
    }
    stale
}

/// Files present under `dir` (per [`crate::discovery::discover_files`]) but
/// absent from `index` — files created outside hyalo after the last
/// `create-index`.
///
/// BUG-1 (iter-243): a mutating command under `--index` can write such a
/// file, and read paths that resolve links through the snapshot would never
/// see it. Discovery is the same directory walk the disk scan uses, so the
/// result is exactly what a scan would have found.
#[must_use]
pub fn files_missing_from_snapshot(index: &SnapshotIndex, dir: &Path) -> Vec<String> {
    let Ok(files) = crate::discovery::discover_files(dir) else {
        return Vec::new();
    };
    files
        .into_iter()
        .filter_map(|f| {
            let rel = f
                .strip_prefix(dir)
                .unwrap_or(&f)
                .to_string_lossy()
                .replace('\\', "/");
            if index.get(&rel).is_some() {
                None
            } else {
                Some(rel)
            }
        })
        .collect()
}

#[cfg(test)]
mod iso_tests {
    use super::*;

    #[test]
    fn iso8601_round_trips() {
        // 1_709_164_800 = 2024-02-29T00:00:00Z — the leap-day case (PR #277
        // review N-1), plus the day right after it.
        for secs in [
            0_u64,
            1,
            86_400,
            1_700_000_000,
            1_709_164_800,
            1_709_251_200,
            1_759_276_800,
        ] {
            assert_eq!(parse_iso8601_secs(&format_iso8601(secs)), Some(secs));
        }
    }

    #[test]
    fn iso8601_rejects_malformed() {
        for bad in [
            "",
            "2026-08-27",
            "2026-08-27T10:00:00",
            "not-a-date",
            "2026-13-01T00:00:00Z",
            "2026-08-27T25:00:00Z",
            // Day beyond the month's length (PR #277 review N-1).
            "2026-02-31T00:00:00Z",
            "2023-02-29T00:00:00Z",
            "2026-04-31T00:00:00Z",
        ] {
            assert_eq!(parse_iso8601_secs(bad), None, "{bad} should not parse");
        }
    }

    #[test]
    fn modified_since_snapshot_detects_edited_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.md");
        std::fs::write(&file, "---\ntitle: a\n---\n\nbody\n").unwrap();
        let files = vec![(file.clone(), "a.md".to_owned())];
        let build = crate::index::ScannedIndex::build(
            &files,
            None,
            &crate::index::ScanOptions {
                scan_body: true,
                bm25_tokenize: false,
                default_language: None,
                frontmatter_link_props: None,
            },
        )
        .unwrap();
        let snap_path = dir.path().join(".hyalo-index");
        let vault = dir.path().to_string_lossy().to_string();
        SnapshotIndex::save(&build.index, &snap_path, &vault, None, None).unwrap();
        let index = SnapshotIndex::load(&snap_path).unwrap().unwrap();
        assert!(files_modified_since_snapshot(&index, dir.path()).is_empty());
        // Same-second touch: within tolerance, not stale.
        std::fs::write(&file, "---\ntitle: a\n---\n\nbody v2\n").unwrap();
        std::thread::sleep(std::time::Duration::from_secs(2));
        std::fs::write(&file, "---\ntitle: a\n---\n\nbody v3\n").unwrap();
        let stale = files_modified_since_snapshot(&index, dir.path());
        assert_eq!(stale, vec!["a.md".to_owned()]);
    }

    /// UX-1 (iter-249 dogfood): the pre-fix `newest_shallow_dir_mtime` only
    /// stat'd `dir` and its immediate children, so a directory created two
    /// or three levels down (e.g. `iterations/done/`, MDN's
    /// `web/css/zz/`) never moved the probe's result. `newest_dir_mtime`
    /// must see a directory created at [`STALENESS_PROBE_MAX_DEPTH`] itself.
    #[test]
    fn newest_dir_mtime_detects_depth_3_directory() {
        let dir = tempfile::tempdir().unwrap();
        // The whole chain `a/b/c` (c at relative depth 3, exactly
        // `STALENESS_PROBE_MAX_DEPTH`) exists before the baseline is read,
        // so no ancestor's mtime will move afterwards.
        std::fs::create_dir_all(dir.path().join("a/b/c")).unwrap();
        let baseline = newest_dir_mtime(dir.path());
        assert!(baseline.is_some());

        std::thread::sleep(std::time::Duration::from_secs(2));

        // Adding a file directly inside `c` bumps only `c`'s own mtime —
        // a probe that stops at depth 1 or 2 cannot see this; the old
        // root+children probe passed the previous version of this test.
        std::fs::write(dir.path().join("a/b/c/new.md"), "x").unwrap();
        let after = newest_dir_mtime(dir.path());
        assert!(
            after > baseline,
            "expected a newer mtime after creating a depth-3 directory: baseline={baseline:?}, after={after:?}"
        );
    }

    /// Honest documentation of the probe's bound: a new directory whose
    /// *parent* sits below [`STALENESS_PROBE_MAX_DEPTH`] is invisible,
    /// because the probe never descends far enough to stat that parent.
    /// This is the behaviour `newest_dir_mtime`'s doc comment and
    /// `create-index --help` describe — pin it so a future depth change is
    /// a deliberate, documented decision rather than a silent drift.
    #[test]
    fn newest_dir_mtime_does_not_see_past_max_depth() {
        let dir = tempfile::tempdir().unwrap();
        // a(1)/b(2)/c(3)/d(4): `d`'s parent `c` is already at the cap, so a
        // new directory created *inside* `d` never gets stat'd.
        std::fs::create_dir_all(dir.path().join("a/b/c/d")).unwrap();
        let baseline = newest_dir_mtime(dir.path());
        assert!(baseline.is_some());

        std::thread::sleep(std::time::Duration::from_secs(2));

        std::fs::create_dir_all(dir.path().join("a/b/c/d/e")).unwrap();
        let after = newest_dir_mtime(dir.path());
        assert_eq!(
            after, baseline,
            "a directory created below STALENESS_PROBE_MAX_DEPTH must not move the probe"
        );
    }
}

/// Scan `dir` for `.hyalo-index` files whose creator PID is no longer running.
///
/// Returns a list of `(path, vault_dir, created_at)` tuples for stale files.
/// Files that cannot be loaded (incompatible schema, I/O error) are silently
/// skipped — they are already unreachable by the normal load path.
pub fn find_stale_indexes(dir: &Path) -> Result<Vec<(PathBuf, String, u64)>> {
    let mut stale = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return Ok(stale);
    };
    for entry in read_dir {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(".hyalo-index") {
            continue;
        }
        if let Ok(Some(idx)) = SnapshotIndex::load_silent(&path) {
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

/// Scan a single file and return its `IndexEntry` plus optionally `FileLinks`
/// for the link graph.
///
/// When `scan_body` is `false`, only frontmatter is read — sections, tasks, and
/// links are empty, and no `FileLinks` are produced.
pub(crate) fn scan_one_file(
    full_path: &Path,
    rel_path: &str,
    scan_body: bool,
    bm25_tokenize: bool,
    default_language: Option<&str>,
    frontmatter_link_props: &[String],
) -> Result<(IndexEntry, Option<FileLinks>)> {
    let mut fm = FrontmatterCollector::new(scan_body);
    let mut body_collector = BodyCollector::new(bm25_tokenize);

    let stats;
    let (sections, tasks, links, file_links) = if scan_body {
        let mut section_scanner = SectionScanner::new();
        let mut task_extractor = TaskExtractor::new();
        let mut link_visitor = LinkGraphVisitor::with_frontmatter_props(
            PathBuf::from(rel_path),
            frontmatter_link_props.to_vec(),
        );

        stats = scanner::scan_file_multi_stats(
            full_path,
            &mut [
                &mut fm,
                &mut section_scanner,
                &mut task_extractor,
                &mut link_visitor,
                &mut body_collector,
            ],
            true,
        )?;

        let sections = section_scanner.into_sections();
        let tasks = task_extractor.into_tasks();
        let fl = link_visitor.into_file_links();
        let links_clone: Vec<(usize, Link)> = fl
            .links
            .iter()
            .map(|(line, link)| (*line, link.clone()))
            .collect();
        let self_anchors = fl.self_anchors.clone();
        (sections, tasks, (links_clone, self_anchors), Some(fl))
    } else {
        stats =
            scanner::scan_file_multi_stats(full_path, &mut [&mut fm, &mut body_collector], true)?;
        (Vec::new(), Vec::new(), (Vec::new(), Vec::new()), None)
    };
    let (links, self_anchors) = links;

    let props = fm.into_props();
    let tags = extract_tags(&props);
    let modified = format_modified(full_path)?;

    // Populate BM25 pre-tokenized data during index creation.
    // The body text was accumulated by `BodyCollector` during the scan pass above —
    // no second file read is needed.
    let (bm25_tokens, bm25_language, bm25_tokenizer_version) = if bm25_tokenize {
        let body = body_collector.into_body();

        // Resolve title: frontmatter property > first H1 heading.
        let title: &str = props
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| {
                sections
                    .iter()
                    .find(|s| s.level == 1)
                    .and_then(|s| s.heading.as_deref())
                    .unwrap_or("")
            });

        // Resolve stemming language: frontmatter > config default > English.
        let fm_lang = props.get("language").and_then(|v| v.as_str());
        let lang = resolve_language(fm_lang, None, default_language);

        let combined = format!("{title} {body}");
        let stemmer = rust_stemmers::Stemmer::create(lang.to_algorithm());
        let tokens = tokenize(&combined, &stemmer);

        (
            Some(tokens),
            Some(lang.canonical_name().to_owned()),
            Some(crate::bm25::TOKENIZER_VERSION),
        )
    } else {
        (None, None, None)
    };

    let entry = IndexEntry {
        rel_path: rel_path.to_owned(),
        modified,
        size: stats.size,
        lines: stats.lines,
        properties: props,
        tags,
        sections,
        tasks,
        links,
        self_anchors,
        bm25_tokens,
        bm25_language,
        bm25_tokenizer_version,
    };

    Ok((entry, file_links))
}

/// Format a file's last-modified time as ISO 8601 UTC.
pub fn format_modified(path: &Path) -> Result<String> {
    let meta = std::fs::metadata(path)
        .with_context(|| format!("failed to read metadata for {}", path.display()))?;
    let mtime = meta
        .modified()
        .with_context(|| format!("mtime not available for {}", path.display()))?;
    Ok(format_mtime(mtime, path))
}

/// Format an already-`stat`ed mtime exactly as [`format_modified`] would.
///
/// Callers that hold a fresh [`SystemTime`] (mutation commands read one per
/// file for their concurrent-write guard) use this to compare against an
/// index entry's stored `modified` without paying a second `stat`
/// (iter-255, BUG-2). `path` is used only for the pre-1970 warning.
#[must_use]
pub fn format_mtime(mtime: SystemTime, path: &Path) -> String {
    let secs = mtime.duration_since(SystemTime::UNIX_EPOCH).map_or_else(
        |_| {
            crate::warn::warn(format!(
                "mtime for {} is before 1970-01-01; using epoch as fallback",
                path.display()
            ));
            0
        },
        |d| d.as_secs(),
    );
    format_iso8601(secs)
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

    let z = days.cast_signed() + 719_468_i64;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097).cast_unsigned();
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe.cast_signed() + era * 400;
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

// ---------------------------------------------------------------------------
// BodyCollector visitor
// ---------------------------------------------------------------------------

/// Visitor that accumulates raw body lines into a single `String`.
///
/// Used during BM25 tokenization to capture body text in the same scan pass
/// as frontmatter/section/link extraction, avoiding a second file read.
///
/// When `active` is `false` (constructed via `BodyCollector::new(false)`),
/// the visitor is a no-op and produces an empty string.
struct BodyCollector {
    active: bool,
    buf: String,
}

impl BodyCollector {
    fn new(active: bool) -> Self {
        Self {
            active,
            buf: String::new(),
        }
    }

    /// Consume the collector and return the accumulated body text.
    fn into_body(self) -> String {
        self.buf
    }
}

impl FileVisitor for BodyCollector {
    fn needs_body(&self) -> bool {
        self.active
    }

    /// BUG-4 (iter-243): collect **every** body-region line raw — including
    /// code-fence delimiters (` ```rust `) and `%%` comment-fence lines — so
    /// the accumulated body is byte-for-byte the lines
    /// [`crate::frontmatter::body_only`] contains, and `create-index`
    /// tokenization is indistinguishable from the disk-scan corpus builder
    /// in `find` (which tokenizes `body_only`). The previous pair of
    /// `on_body_line`/`on_code_block_line` callbacks silently dropped the
    /// fence delimiters and comment lines, drifting avgdl/df between the
    /// `--index` and disk paths (dogfood v0.20.0 BUG-4).
    fn on_raw_body_line(&mut self, raw: &str, _line_num: usize) -> ScanAction {
        if !self.buf.is_empty() {
            self.buf.push('\n');
        }
        self.buf.push_str(raw);
        ScanAction::Continue
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

    /// BUG-4 (iter-243): `create-index`'s BM25 tokenization and the disk-scan
    /// corpus builder in `find` (which tokenizes `frontmatter::body_only`)
    /// must produce identical token streams — the raw body collected during
    /// the scan pass must contain exactly the lines `body_only` does,
    /// including code-fence delimiter lines and `%%` comment lines.
    #[test]
    fn bm25_tokens_match_body_only_tokenization() {
        let tmp = tempfile::tempdir().unwrap();
        let rel = "tricky.md";
        let content = md!(r"
---
title: Tricky
---
# Tricky

Text with a fenced block below.

```rust
fn main() {}
```

%% hidden comment %%

<!-- html comment -->

Plain prose ends here.
");
        let full = tmp.path().join(rel);
        fs::write(&full, content).unwrap();

        let (entry, _) = scan_one_file(&full, rel, true, true, None, &[]).unwrap();
        let indexed = entry.bm25_tokens.clone().unwrap();

        let title = entry
            .properties
            .get("title")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .unwrap_or_default();
        let body = crate::frontmatter::body_only(content).to_owned();
        let disk = crate::bm25::tokenize_document(crate::bm25::DocumentInput {
            rel_path: rel.to_owned(),
            title,
            body,
            language: crate::bm25::resolve_language(None, None, None),
        })
        .tokens;

        assert_eq!(
            indexed, disk,
            "create-index tokens and body_only tokenization must agree"
        );
    }

    /// BUG-1 (iter-243): files present on disk but absent from the snapshot
    /// must be reported so callers can upsert them.
    #[test]
    fn files_missing_from_snapshot_reports_unindexed_files() {
        let (tmp, files) = setup_vault();
        let build = ScannedIndex::build(
            &files,
            None,
            &ScanOptions {
                scan_body: true,
                bm25_tokenize: false,
                default_language: None,
                frontmatter_link_props: None,
            },
        )
        .unwrap();
        let snap_path = tmp.path().join(".snap");
        SnapshotIndex::save(&build.index, &snap_path, "/vault", None, None).unwrap();
        let snap = SnapshotIndex::load(&snap_path).unwrap().unwrap();

        assert!(files_missing_from_snapshot(&snap, tmp.path()).is_empty());

        fs::write(tmp.path().join("c.md"), "---\ntitle: C\n---\n\n# C\n").unwrap();
        let missing = files_missing_from_snapshot(&snap, tmp.path());
        assert_eq!(missing, vec!["c.md".to_owned()]);
    }

    #[test]
    fn scanned_index_builds_entries() {
        let (_tmp, files) = setup_vault();
        let build = ScannedIndex::build(
            &files,
            None,
            &ScanOptions {
                scan_body: true,
                bm25_tokenize: false,
                default_language: None,
                frontmatter_link_props: None,
            },
        )
        .unwrap();
        assert!(build.warnings.is_empty());
        assert_eq!(build.index.entries().len(), 2);
    }

    #[test]
    fn scanned_index_bm25_tokenize_sets_tokenizer_version_and_cjk_bigrams() {
        // F-2 / DEC-094: `bm25_tokenizer_version` must be stamped alongside
        // `bm25_tokens`, and CJK content must tokenize to bigrams rather than
        // one unmatchable whole-run token.
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("cjk.md"),
            md!(r"
---
title: CJK
---
日本語のテキストです
"),
        )
        .unwrap();
        let files = vec![(tmp.path().join("cjk.md"), "cjk.md".to_owned())];

        let build = ScannedIndex::build(
            &files,
            None,
            &ScanOptions {
                scan_body: true,
                bm25_tokenize: true,
                default_language: None,
                frontmatter_link_props: None,
            },
        )
        .unwrap();
        let entry = build.index.get("cjk.md").expect("entry should exist");
        assert_eq!(
            entry.bm25_tokenizer_version,
            Some(crate::bm25::TOKENIZER_VERSION)
        );
        let tokens = entry
            .bm25_tokens
            .as_ref()
            .expect("bm25_tokens should be populated");
        assert!(
            tokens.contains(&"日本".to_owned()),
            "expected a CJK bigram token, got {tokens:?}"
        );
    }

    #[test]
    fn scanned_index_get_by_path() {
        let (_tmp, files) = setup_vault();
        let build = ScannedIndex::build(
            &files,
            None,
            &ScanOptions {
                scan_body: true,
                bm25_tokenize: false,
                default_language: None,
                frontmatter_link_props: None,
            },
        )
        .unwrap();
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
        let build = ScannedIndex::build(
            &files,
            None,
            &ScanOptions {
                scan_body: true,
                bm25_tokenize: false,
                default_language: None,
                frontmatter_link_props: None,
            },
        )
        .unwrap();
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
        let build = ScannedIndex::build(
            &files,
            None,
            &ScanOptions {
                scan_body: true,
                bm25_tokenize: false,
                default_language: None,
                frontmatter_link_props: None,
            },
        )
        .unwrap();
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
        let build = ScannedIndex::build(
            &files,
            None,
            &ScanOptions {
                scan_body: true,
                bm25_tokenize: false,
                default_language: None,
                frontmatter_link_props: None,
            },
        )
        .unwrap();
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
        let build = ScannedIndex::build(
            &files,
            None,
            &ScanOptions {
                scan_body: true,
                bm25_tokenize: false,
                default_language: None,
                frontmatter_link_props: None,
            },
        )
        .unwrap();
        assert_eq!(build.index.entries().len(), 1);
        assert_eq!(build.warnings.len(), 1);
        assert_eq!(build.warnings[0].rel_path, "bad.md");
    }

    #[test]
    fn scanned_index_modified_is_iso8601() {
        let (_tmp, files) = setup_vault();
        let build = ScannedIndex::build(
            &files,
            None,
            &ScanOptions {
                scan_body: true,
                bm25_tokenize: false,
                default_language: None,
                frontmatter_link_props: None,
            },
        )
        .unwrap();
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
        let build = ScannedIndex::build(
            &files,
            None,
            &ScanOptions {
                scan_body: true,
                bm25_tokenize: false,
                default_language: None,
                frontmatter_link_props: None,
            },
        )
        .unwrap();
        let index = &build.index;

        let snap_dir = tempfile::tempdir().unwrap();
        let snap_path = snap_dir.path().join(".hyalo-index");

        SnapshotIndex::save(index, &snap_path, "/tmp/vault", None, None).unwrap();
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

    #[test]
    fn scanned_index_skip_body() {
        let (_tmp, files) = setup_vault();
        let build = ScannedIndex::build(
            &files,
            None,
            &ScanOptions {
                scan_body: false,
                bm25_tokenize: false,
                default_language: None,
                frontmatter_link_props: None,
            },
        )
        .unwrap();
        assert!(build.warnings.is_empty());
        let idx = &build.index;

        // Frontmatter is still populated
        let a = idx.get("a.md").unwrap();
        assert_eq!(a.tags, vec!["rust", "cli"]);
        assert_eq!(a.properties.get("status").unwrap(), "draft");

        // Body fields are empty
        assert!(a.sections.is_empty());
        assert!(a.tasks.is_empty());
        assert!(a.links.is_empty());

        // Link graph is empty
        assert!(idx.link_graph().backlinks("a").is_empty());
        assert!(idx.link_graph().backlinks("b").is_empty());
    }

    // -------------------------------------------------------------------------
    // insert_or_replace_entry — incremental insert for `hyalo new`
    // -------------------------------------------------------------------------

    /// Build a snapshot from a vault directory, persist it, and reload it.
    ///
    /// Mirrors the round-trip a real CLI command performs and gives us a
    /// `SnapshotIndex` (not the in-memory `ScannedIndex`) so we can exercise
    /// the `insert_or_replace_entry` helper.
    fn build_and_reload_snapshot(
        dir: &Path,
    ) -> (tempfile::TempDir, std::path::PathBuf, SnapshotIndex) {
        let files = crate::discovery::discover_files(dir).unwrap();
        let pairs: Vec<(PathBuf, String)> = files
            .into_iter()
            .map(|f| {
                let rel = f
                    .strip_prefix(dir)
                    .unwrap_or(&f)
                    .to_string_lossy()
                    .replace('\\', "/");
                (f, rel)
            })
            .collect();
        let build = ScannedIndex::build(
            &pairs,
            None,
            &ScanOptions {
                scan_body: true,
                bm25_tokenize: false,
                default_language: None,
                frontmatter_link_props: None,
            },
        )
        .unwrap();
        let snap_dir = tempfile::tempdir().unwrap();
        let snap_path = snap_dir.path().join(".hyalo-index");
        SnapshotIndex::save(&build.index, &snap_path, "/tmp/vault", None, None).unwrap();
        let loaded = SnapshotIndex::load(&snap_path).unwrap().unwrap();
        (snap_dir, snap_path, loaded)
    }

    #[test]
    fn insert_or_replace_inserts_new_entry() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("a.md"),
            "---\ntitle: A\n---\nHello [[b]].\n",
        )
        .unwrap();
        let (_snap_dir, _snap_path, mut snap) = build_and_reload_snapshot(tmp.path());
        assert_eq!(snap.entries().len(), 1);

        // Add a brand-new file on disk and insert it incrementally.
        fs::write(
            tmp.path().join("c.md"),
            "---\ntitle: C\ntags: [fresh]\n---\nBody of C.\n",
        )
        .unwrap();
        snap.insert_or_replace_entry(&tmp.path().join("c.md"), "c.md")
            .unwrap();

        assert_eq!(snap.entries().len(), 2);
        let c = snap.get("c.md").expect("c.md should be indexed");
        assert_eq!(c.rel_path, "c.md");
        assert_eq!(
            c.properties.get("title").and_then(|v| v.as_str()),
            Some("C")
        );
        assert_eq!(c.tags, vec!["fresh".to_owned()]);
    }

    #[test]
    fn insert_or_replace_is_idempotent_and_refreshes() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a.md"), "---\ntitle: A\n---\nBody.\n").unwrap();
        let (_snap_dir, _snap_path, mut snap) = build_and_reload_snapshot(tmp.path());
        assert_eq!(snap.entries().len(), 1);

        // Mutate file on disk, then call insert_or_replace — entry should
        // refresh with the new content (idempotent: still exactly one entry).
        fs::write(
            tmp.path().join("a.md"),
            "---\ntitle: A updated\ntags: [rewritten]\n---\nBody v2.\n",
        )
        .unwrap();
        snap.insert_or_replace_entry(&tmp.path().join("a.md"), "a.md")
            .unwrap();

        assert_eq!(
            snap.entries().len(),
            1,
            "should still hold exactly one entry"
        );
        let a = snap.get("a.md").expect("a.md should still be indexed");
        assert_eq!(
            a.properties.get("title").and_then(|v| v.as_str()),
            Some("A updated")
        );
        assert_eq!(a.tags, vec!["rewritten".to_owned()]);
    }

    // -------------------------------------------------------------------------
    // Security tests
    // -------------------------------------------------------------------------

    fn make_snapshot_bytes(rel_path: &str) -> Vec<u8> {
        let data = SnapshotData {
            header: SnapshotHeader {
                vault_dir: "/tmp/vault".to_owned(),
                site_prefix: None,
                created_at: 0,
                pid: std::process::id(),
            },
            entries: vec![IndexEntry {
                rel_path: rel_path.to_owned(),
                modified: "2024-01-01T00:00:00Z".to_owned(),
                size: 0,
                lines: 0,
                properties: IndexMap::default(),
                tags: vec![],
                sections: vec![],
                tasks: vec![],
                links: vec![],
                self_anchors: Vec::new(),
                bm25_tokens: None,
                bm25_language: None,
                bm25_tokenizer_version: None,
            }],
            graph: LinkGraph::default(),
            bm25_index: None,
        };
        rmp_serde::to_vec_named(&data).unwrap()
    }

    #[test]
    fn load_inner_rejects_parent_traversal() {
        let bytes = make_snapshot_bytes("../../escape.md");
        assert!(
            SnapshotIndex::load_inner(&bytes, false).is_none(),
            "snapshot with '..' path components must be rejected"
        );
    }

    #[test]
    fn load_inner_rejects_absolute_path() {
        // Unix-style absolute path (on Windows this has a RootDir component
        // but is_absolute() returns false, so the component check must catch it)
        let bytes = make_snapshot_bytes("/etc/passwd");
        assert!(
            SnapshotIndex::load_inner(&bytes, false).is_none(),
            "snapshot with absolute rel_path must be rejected"
        );

        // Windows-style absolute path (only testable on Windows where the
        // Prefix component is recognized by std::path)
        #[cfg(windows)]
        {
            let bytes = make_snapshot_bytes("C:\\Windows\\System32\\config\\sam");
            assert!(
                SnapshotIndex::load_inner(&bytes, false).is_none(),
                "snapshot with Windows absolute rel_path must be rejected"
            );
        }
    }

    // M-2 (adversarial-review-2026-08-23.md): Windows drive-relative and
    // NTFS-ADS rel_paths must be rejected. Gated to Windows because a colon
    // is an ordinary filename character elsewhere, and `Path::new("C:foo")`
    // only parses a `Prefix` component under `#[cfg(windows)]` — on Unix
    // this input is meaningless to test.
    #[test]
    #[cfg(windows)]
    fn load_inner_rejects_windows_drive_relative_path() {
        // No `\` after the colon: drive-*relative*, not absolute — distinct
        // from the already-covered `C:\...` case in
        // `load_inner_rejects_absolute_path`.
        let bytes = make_snapshot_bytes("C:notes.md");
        assert!(
            SnapshotIndex::load_inner(&bytes, false).is_none(),
            "snapshot with a drive-relative rel_path must be rejected"
        );
    }

    #[test]
    #[cfg(windows)]
    fn load_inner_rejects_ntfs_alternate_data_stream_path() {
        // Lexically inside the vault (no Prefix/RootDir/ParentDir
        // component) but resolves to an ADS on `notes.md`, not the file.
        let bytes = make_snapshot_bytes("notes.md:hidden-stream");
        assert!(
            SnapshotIndex::load_inner(&bytes, false).is_none(),
            "snapshot with an NTFS-ADS rel_path must be rejected"
        );
    }

    #[test]
    fn load_inner_rejects_null_byte() {
        let bytes = make_snapshot_bytes("foo\0bar.md");
        assert!(
            SnapshotIndex::load_inner(&bytes, false).is_none(),
            "snapshot with null-byte path must be rejected"
        );
    }

    #[test]
    fn load_inner_rejects_bm25_out_of_bounds_doc_id() {
        use crate::bm25::{Bm25InvertedIndex, Posting};
        use std::collections::HashMap;

        // Build a BM25 index where the posting list references doc_id 999
        // but doc_paths only has 1 entry — this should trigger MED-1 rejection.
        let mut postings: HashMap<String, Vec<Posting>> = HashMap::new();
        postings.insert(
            "rust".to_owned(),
            vec![Posting {
                doc_id: 999, // out-of-bounds: only 1 doc exists
                term_freq: 1,
                positions: vec![0],
            }],
        );
        let bad_bm25 = Bm25InvertedIndex::new_for_test(
            postings,
            vec![5],                   // doc_lengths: 1 entry
            vec!["doc.md".to_owned()], // doc_paths: 1 entry
            5.0,
        );

        let data = SnapshotData {
            header: SnapshotHeader {
                vault_dir: "/tmp/vault".to_owned(),
                site_prefix: None,
                created_at: 0,
                pid: std::process::id(),
            },
            entries: vec![IndexEntry {
                rel_path: "doc.md".to_owned(),
                modified: "2024-01-01T00:00:00Z".to_owned(),
                size: 0,
                lines: 0,
                properties: IndexMap::default(),
                tags: vec![],
                sections: vec![],
                tasks: vec![],
                links: vec![],
                self_anchors: Vec::new(),
                bm25_tokens: None,
                bm25_language: None,
                bm25_tokenizer_version: None,
            }],
            graph: LinkGraph::default(),
            bm25_index: Some(bad_bm25),
        };
        let bytes = rmp_serde::to_vec_named(&data).unwrap();

        assert!(
            SnapshotIndex::load_inner(&bytes, false).is_none(),
            "snapshot with out-of-bounds BM25 doc_id must be rejected (MED-1)"
        );
    }

    #[test]
    fn load_inner_rejects_bm25_mismatched_doc_lengths() {
        use crate::bm25::{Bm25InvertedIndex, Posting};
        use std::collections::HashMap;

        // doc_lengths.len() != doc_paths.len() — structurally invalid
        let mut postings: HashMap<String, Vec<Posting>> = HashMap::new();
        postings.insert(
            "rust".to_owned(),
            vec![Posting {
                doc_id: 0,
                term_freq: 1,
                positions: vec![0],
            }],
        );
        let bad_bm25 = Bm25InvertedIndex::new_for_test(
            postings,
            vec![5, 10],               // doc_lengths: 2 entries
            vec!["doc.md".to_owned()], // doc_paths: 1 entry — mismatch
            7.5,
        );

        let data = SnapshotData {
            header: SnapshotHeader {
                vault_dir: "/tmp/vault".to_owned(),
                site_prefix: None,
                created_at: 0,
                pid: std::process::id(),
            },
            entries: vec![IndexEntry {
                rel_path: "doc.md".to_owned(),
                modified: "2024-01-01T00:00:00Z".to_owned(),
                size: 0,
                lines: 0,
                properties: IndexMap::default(),
                tags: vec![],
                sections: vec![],
                tasks: vec![],
                links: vec![],
                self_anchors: Vec::new(),
                bm25_tokens: None,
                bm25_language: None,
                bm25_tokenizer_version: None,
            }],
            graph: LinkGraph::default(),
            bm25_index: Some(bad_bm25),
        };
        let bytes = rmp_serde::to_vec_named(&data).unwrap();

        assert!(
            SnapshotIndex::load_inner(&bytes, false).is_none(),
            "snapshot with mismatched BM25 doc_lengths/doc_paths must be rejected (MED-1)"
        );
    }

    #[test]
    fn is_pid_alive_zero_returns_false() {
        assert!(
            !is_pid_alive(0),
            "pid 0 must not be treated as an alive process"
        );
    }

    #[test]
    fn load_rejects_oversized_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("big.hyalo-index");
        let f = std::fs::File::create(&path).unwrap();
        f.set_len(MAX_INDEX_FILE_SIZE + 1).unwrap();
        let result = SnapshotIndex::load(&path).unwrap();
        assert!(
            result.is_none(),
            "oversized index file must return Ok(None)"
        );
    }
}
