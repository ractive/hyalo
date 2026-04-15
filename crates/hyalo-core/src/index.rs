//! Vault index abstraction — decouples commands from their data source.
//!
//! The [`VaultIndex`] trait provides a uniform interface over pre-scanned vault
//! data. Commands program against this trait and don't know whether data came
//! from a live filesystem scan ([`ScannedIndex`]) or a serialized snapshot.

use anyhow::{Context, Result};
use indexmap::IndexMap;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::bm25::{Bm25InvertedIndex, resolve_language, tokenize};
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
        let results: Vec<Result<(IndexEntry, Option<FileLinks>)>> = files
            .par_iter()
            .map(|(full_path, rel_path)| {
                scan_one_file(
                    full_path,
                    rel_path,
                    options.scan_body,
                    options.bm25_tokenize,
                    default_language,
                    &fm_link_props,
                )
            })
            .collect();

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

    /// Re-scan a single file and replace its index entry.
    ///
    /// Returns the `FileLinks` for the re-scanned file so the caller can
    /// update the link graph separately. Returns `Ok(None)` if the file
    /// is not in the index.
    pub(crate) fn rescan_entry(&mut self, dir: &Path, rel_path: &str) -> Result<Option<FileLinks>> {
        let Some(&idx) = self.path_index.get(rel_path) else {
            return Ok(None);
        };
        let full_path = dir.join(rel_path);
        let fm_props = self.effective_frontmatter_link_props();
        let (entry, file_links) =
            scan_one_file(&full_path, rel_path, true, false, None, &fm_props)?;
        self.entries[idx] = entry;
        Ok(file_links)
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
        let (entry, _file_links) =
            scan_one_file(&full_path, new_rel, true, false, None, &fm_props)?;

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
    fn rebuild_path_index(&mut self) {
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
                Some(Self {
                    entries,
                    path_index,
                    graph: data.graph,
                    header: data.header,
                    bm25_index: data.bm25_index,
                    frontmatter_link_props: None,
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
        let bytes = std::fs::read(path)
            .with_context(|| format!("failed to read index file: {}", path.display()))?;
        Ok(Self::load_inner(&bytes, true))
    }

    /// Load a snapshot silently — identical to [`load`] but suppresses the
    /// incompatibility warning.  Used by `find_stale_indexes` which expects to
    /// silently skip files that cannot be deserialized.
    fn load_silent(path: &Path) -> Result<Option<Self>> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("failed to read index file: {}", path.display()))?;
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
            .map(|d| d.as_secs())
            .unwrap_or(0),
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
    // Use a kernel-assigned temp-file name in the same directory as the
    // target to avoid a predictable path that could be exploited via a
    // pre-created symlink (symlink-substitution attack).  Placing the temp
    // file in the same directory as `path` ensures the subsequent atomic
    // rename stays on the same filesystem.
    let parent = path
        .parent()
        .context("index path has no parent directory")?;
    let mut tmp =
        tempfile::NamedTempFile::new_in(parent).context("failed to create temp file for index")?;
    tmp.write_all(&bytes)
        .context("failed to write temp index")?;
    // On persist failure, dropping `e.file` removes the temp file automatically.
    tmp.persist(path)
        .map_err(|e| e.error)
        .with_context(|| format!("failed to rename index into place: {}", path.display()))?;
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

    let (sections, tasks, links, file_links) = if scan_body {
        let mut section_scanner = SectionScanner::new();
        let mut task_extractor = TaskExtractor::new();
        let mut link_visitor = LinkGraphVisitor::with_frontmatter_props(
            PathBuf::from(rel_path),
            frontmatter_link_props.to_vec(),
        );

        scanner::scan_file_multi(
            full_path,
            &mut [
                &mut fm,
                &mut section_scanner,
                &mut task_extractor,
                &mut link_visitor,
                &mut body_collector,
            ],
        )?;

        let sections = section_scanner.into_sections();
        let tasks = task_extractor.into_tasks();
        let fl = link_visitor.into_file_links();
        let links_clone: Vec<(usize, Link)> = fl
            .links
            .iter()
            .map(|(line, link)| (*line, link.clone()))
            .collect();
        (sections, tasks, links_clone, Some(fl))
    } else {
        scanner::scan_file_multi(full_path, &mut [&mut fm, &mut body_collector])?;
        (Vec::new(), Vec::new(), Vec::new(), None)
    };

    let props = fm.into_props();
    let tags = extract_tags(&props);
    let modified = format_modified(full_path)?;

    // Populate BM25 pre-tokenized data during index creation.
    // The body text was accumulated by `BodyCollector` during the scan pass above —
    // no second file read is needed.
    let (bm25_tokens, bm25_language) = if bm25_tokenize {
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

        (Some(tokens), Some(lang.canonical_name().to_owned()))
    } else {
        (None, None)
    };

    let entry = IndexEntry {
        rel_path: rel_path.to_owned(),
        modified,
        properties: props,
        tags,
        sections,
        tasks,
        links,
        bm25_tokens,
        bm25_language,
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

    fn on_body_line(&mut self, raw: &str, _cleaned: &str, _line_num: usize) -> ScanAction {
        if !self.buf.is_empty() {
            self.buf.push('\n');
        }
        self.buf.push_str(raw);
        ScanAction::Continue
    }

    fn on_code_block_line(&mut self, raw: &str, _line_num: usize) -> ScanAction {
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
}
