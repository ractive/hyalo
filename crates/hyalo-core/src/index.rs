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
use crate::link_graph::{FileLinks, LinkGraph, LinkGraphVisitor};
use crate::links::{Link, SelfAnchor};
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
    pub self_anchors: Vec<SelfAnchor>,
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

    /// The format version of the snapshot backing this index (G4, iter-276),
    /// or `None` for a live disk scan.
    ///
    /// `summary` surfaces it so an agent can tell a snapshot this binary would
    /// refuse from a fresh one before the two disagree on the numbers.
    fn snapshot_format_version(&self) -> Option<u32> {
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

/// Why an invalid-UTF-8 file is reported by `create-index` (iter-265, BUG-14).
///
/// It is still indexed as a note — its frontmatter, tags, links and headings
/// are all readable — but it is excluded from the BM25 corpus so `--index`
/// scores match the disk scan's, which drops the file outright.
pub const INVALID_UTF8_INDEX_MESSAGE: &str = "invalid UTF-8 — indexed as a note but excluded from full-text search, \
     matching the disk scan (`find -e` still matches it lossily)";

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
        // iter-262: `None` means "scan every frontmatter value"; an explicit
        // list is the opt-out that narrows the scan back to named properties.
        let fm_link_props: Option<Vec<String>> =
            options.frontmatter_link_props.map(<[String]>::to_vec);
        let scan = |(full_path, rel_path): &(std::path::PathBuf, String)| {
            scan_one_file(
                full_path,
                rel_path,
                options.scan_body,
                options.bm25_tokenize,
                default_language,
                fm_link_props.as_deref(),
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
                    // A BM25 build that produced no tokens for a file means the
                    // file was not valid UTF-8 (iter-265, BUG-14) — it stays in
                    // the index as a note but out of the search corpus, exactly
                    // as the disk scan has it. Report it so `create-index`
                    // `warnings` accounts for the difference.
                    if options.bm25_tokenize && entry.bm25_tokens.is_none() {
                        warnings.push(IndexWarning {
                            rel_path: entry.rel_path.clone(),
                            message: INVALID_UTF8_INDEX_MESSAGE.to_owned(),
                        });
                    }
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
            // iter-272 Part B (DEC-296): every entry's frontmatter is already
            // parsed, so the declared `aliases:` come for free — the graph
            // resolves an alias-named wikilink to the same file
            // `find --fields links` reports, and `backlinks` / `--orphan` /
            // `--dead-end` / `summary.links` all agree with it.
            let aliases: Vec<(String, Vec<String>)> = if crate::discovery::link_aliases_enabled() {
                entries
                    .iter()
                    .filter_map(|e| {
                        let declared = crate::filter::extract_aliases(&e.properties);
                        (!declared.is_empty()).then(|| (e.rel_path.clone(), declared))
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let graph_build = LinkGraph::from_file_links(file_links_vec, site_prefix, &aliases);
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

/// Snapshot format version stamped into every header written by this binary.
///
/// Bumped whenever a *semantic* change makes an older snapshot answer
/// differently from a disk scan of the same vault — not when a field is merely
/// added with a `serde(default)`. A snapshot whose version is below this is
/// refused and the run falls back to disk (BUG-12, dogfood v0.22.0: MDN's Sep 3
/// index, written before iter-272's `SelfAnchor` links and iter-273's header
/// fields, answered `summary --index` with `links.total 49774` against disk's
/// `51075` and said nothing).
///
/// | version | shipped in | what changed |
/// |---|---|---|
/// | 0 | ≤ iter-275 | no stamp; every pre-276 snapshot reads as 0 |
/// | 1 | iter-276 | iter-272 self-anchor links + iter-273 `scan_excluded` /
/// |   |          | `scan_exclude` / `attachments` header fields |
pub const SNAPSHOT_FORMAT_VERSION: u32 = 1;

/// Metadata header embedded in every snapshot file.
#[derive(Debug, Serialize, Deserialize)]
struct SnapshotHeader {
    /// Snapshot format version — see [`SNAPSHOT_FORMAT_VERSION`].
    ///
    /// Defaulted to `0` on load so a snapshot written before iter-276 still
    /// *decodes*; the version check then refuses it with a named version pair
    /// rather than serving stale answers.
    #[serde(default)]
    format_version: u32,
    /// Canonical vault directory path (informational; not re-validated on load).
    vault_dir: String,
    /// Site prefix used when building the index (informational).
    site_prefix: Option<String>,
    /// Unix timestamp (seconds) when the snapshot was created.
    created_at: u64,
    /// PID of the process that created this snapshot.
    pid: u32,
    /// Vault-relative paths of the vault's **attachments** — every non-`.md`
    /// file carrying an extension (iter-261 / BUG-5, BUG-6).
    ///
    /// The entries list holds only notes, so without this an `--index` run
    /// could not resolve `![[img.png]]` or `[[Books.base]]` and would report
    /// links the same command resolves off-disk as broken. Defaulted on load,
    /// and skipped when empty, so snapshots written by an older hyalo keep
    /// loading and snapshots of vaults with no attachments are byte-identical
    /// to before.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    attachments: Vec<String>,
    /// How many files `[scan] exclude` dropped while this snapshot was built
    /// (iter-273, BUG-18).
    ///
    /// The entries list holds only what survived exclusion, so a load has no
    /// way to recompute this without the vault walk `--index` exists to avoid
    /// — which is why `summary --index` used to report `excluded: 0` against a
    /// disk scan's `excluded: 52`. Defaulted and skipped when zero, so an older
    /// snapshot still loads and an unexcluded vault's bytes are unchanged.
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    scan_excluded: u64,
    /// The `[scan] exclude` patterns that produced [`Self::scan_excluded`].
    ///
    /// Recorded so a load can tell "the same exclusions are still configured,
    /// so the stored count still describes this vault" from "the config has
    /// changed since the index was built", where the count would be a lie.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    scan_exclude: Vec<String>,
}

/// `skip_serializing_if` predicate keeping a zero count out of the wire format.
#[allow(clippy::trivially_copy_pass_by_ref)] // serde requires the by-ref shape
fn is_zero_u64(n: &u64) -> bool {
    *n == 0
}

/// The snapshot wire format — header + entries + graph + optional BM25 index,
/// serialized by `rmp_serde::to_vec_named` as a MessagePack map with string
/// keys in *declaration order*.
///
/// This is the only definition of the envelope: it is borrowed (entries and
/// graph are never cloned to write a snapshot), and the read side is the
/// hand-written [`SnapshotSeed`] visitor below rather than a derived
/// `Deserialize`, so that the trailing `bm25_index` value can be left undecoded
/// (iter-260).
///
/// **Field order is load-bearing.** `bm25_index` MUST stay last — the lazy load
/// path can only skip it if every field it does need has already been visited.
/// Pinned by `bm25_index_is_the_last_envelope_key`.
#[derive(Serialize)]
struct SnapshotDataRef<'a> {
    header: SnapshotHeader,
    entries: &'a [IndexEntry],
    graph: &'a LinkGraph,
    #[serde(skip_serializing_if = "Option::is_none")]
    bm25_index: Option<&'a Bm25InvertedIndex>,
}

// ---------------------------------------------------------------------------
// Lazy BM25 section (iter-260)
// ---------------------------------------------------------------------------
//
// On a MDN-scale vault the `bm25_index` value is ~76 % of the snapshot file and
// ~180 ms of the ~240 ms MessagePack decode, plus ~43 ms of teardown — all of it
// paid by every indexed command, including the majority that never search text
// (`find --property`, `links`, `lint`, `summary`, `properties`, `tags`).
// See `research/snapshot-load-floor-2026-09-01.md`.
//
// `rmp_serde::to_vec_named` writes the envelope as a MessagePack map with string
// keys in declaration order, so `bm25_index` is the *last* key. The load path
// therefore visits the map by hand and stops when it reaches that key, keeping
// the raw bytes of its (unread) value so the section can be decoded later if a
// command actually needs it. Nothing about the on-disk format changes.

/// Which envelope field a top-level map key names.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SnapshotField {
    Header,
    Entries,
    Graph,
    Bm25,
    /// A key this version does not know — its value is skipped.
    Unknown,
}

impl SnapshotField {
    fn classify(name: &str) -> Self {
        match name {
            "header" => Self::Header,
            "entries" => Self::Entries,
            "graph" => Self::Graph,
            "bm25_index" => Self::Bm25,
            _ => Self::Unknown,
        }
    }
}

/// A top-level envelope key, plus the borrowed key text when the MessagePack
/// reader handed us a slice pointing into the source buffer.
///
/// The borrowed slice is what makes the early stop possible: the value of a map
/// entry starts at the byte immediately after its key, so the key's address
/// inside the buffer locates the start of the `bm25_index` value without any
/// separate offset bookkeeping.
struct SnapshotKey<'de> {
    field: SnapshotField,
    borrowed: Option<&'de str>,
}

impl<'de> Deserialize<'de> for SnapshotKey<'de> {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct KeyVisitor;

        impl<'de> serde::de::Visitor<'de> for KeyVisitor {
            type Value = SnapshotKey<'de>;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a snapshot envelope field name")
            }

            fn visit_str<E: serde::de::Error>(
                self,
                v: &str,
            ) -> std::result::Result<Self::Value, E> {
                Ok(SnapshotKey {
                    field: SnapshotField::classify(v),
                    borrowed: None,
                })
            }

            fn visit_borrowed_str<E: serde::de::Error>(
                self,
                v: &'de str,
            ) -> std::result::Result<Self::Value, E> {
                Ok(SnapshotKey {
                    field: SnapshotField::classify(v),
                    borrowed: Some(v),
                })
            }

            fn visit_bytes<E: serde::de::Error>(
                self,
                v: &[u8],
            ) -> std::result::Result<Self::Value, E> {
                let name = std::str::from_utf8(v).unwrap_or("");
                Ok(SnapshotKey {
                    field: SnapshotField::classify(name),
                    borrowed: None,
                })
            }

            fn visit_borrowed_bytes<E: serde::de::Error>(
                self,
                v: &'de [u8],
            ) -> std::result::Result<Self::Value, E> {
                match std::str::from_utf8(v) {
                    Ok(name) => Ok(SnapshotKey {
                        field: SnapshotField::classify(name),
                        borrowed: Some(name),
                    }),
                    Err(_) => Ok(SnapshotKey {
                        field: SnapshotField::Unknown,
                        borrowed: None,
                    }),
                }
            }
        }

        deserializer.deserialize_str(KeyVisitor)
    }
}

/// The `bm25_index` value as the envelope visitor left it.
enum DecodedBm25<'de> {
    /// The envelope has no `bm25_index` key.
    Absent,
    /// Raw, still-undecoded MessagePack bytes of the value.
    Deferred(&'de [u8]),
    /// Decoded in full — the fallback taken when the early stop is not safe
    /// (key not borrowed from the buffer, or `bm25_index` not last).
    Eager(Box<Bm25InvertedIndex>),
}

/// Result of visiting the top-level envelope map.
struct DecodedSnapshot<'de> {
    header: SnapshotHeader,
    entries: Vec<IndexEntry>,
    graph: LinkGraph,
    bm25: DecodedBm25<'de>,
}

/// Seed carrying the whole source buffer so the visitor can turn a borrowed key
/// slice into "everything after this key".
struct SnapshotSeed<'de> {
    whole: &'de [u8],
}

/// Bytes of `whole` that follow `key`, when `key` really is a slice of `whole`.
///
/// Returns `None` when the reader did not borrow (so the pointer says nothing
/// about a position in `whole`) — the caller then falls back to a full decode.
fn bytes_after_key<'de>(whole: &'de [u8], key: &str) -> Option<&'de [u8]> {
    let base = whole.as_ptr().addr();
    let start = key.as_ptr().addr();
    let end = start.checked_add(key.len())?;
    let limit = base.checked_add(whole.len())?;
    if start < base || end > limit {
        return None;
    }
    Some(&whole[end - base..])
}

impl<'de> serde::de::DeserializeSeed<'de> for SnapshotSeed<'de> {
    type Value = DecodedSnapshot<'de>;

    fn deserialize<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(self)
    }
}

impl<'de> serde::de::Visitor<'de> for SnapshotSeed<'de> {
    type Value = DecodedSnapshot<'de>;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a hyalo index snapshot envelope")
    }

    fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        use serde::de::Error as _;

        let mut header: Option<SnapshotHeader> = None;
        let mut entries: Option<Vec<IndexEntry>> = None;
        let mut graph: Option<LinkGraph> = None;
        let mut bm25 = DecodedBm25::Absent;

        while let Some(key) = map.next_key::<SnapshotKey<'de>>()? {
            match key.field {
                SnapshotField::Header => header = Some(map.next_value()?),
                SnapshotField::Entries => entries = Some(map.next_value()?),
                SnapshotField::Graph => graph = Some(map.next_value()?),
                SnapshotField::Bm25 => {
                    // Only skip the value when every other field is already in
                    // hand: the tail we keep runs to the end of the buffer, so
                    // stopping before a field we still need would lose it. In a
                    // snapshot written by any hyalo version this is always true
                    // — `bm25_index` is emitted last (pinned by
                    // `bm25_index_is_the_last_envelope_key`).
                    let tail = if header.is_some() && entries.is_some() && graph.is_some() {
                        key.borrowed.and_then(|k| bytes_after_key(self.whole, k))
                    } else {
                        None
                    };
                    match tail {
                        Some(tail) => {
                            bm25 = DecodedBm25::Deferred(tail);
                            // The whole point: return without reading the value.
                            break;
                        }
                        None => bm25 = DecodedBm25::Eager(map.next_value()?),
                    }
                }
                SnapshotField::Unknown => {
                    map.next_value::<serde::de::IgnoredAny>()?;
                }
            }
        }

        Ok(DecodedSnapshot {
            header: header.ok_or_else(|| A::Error::missing_field("header"))?,
            entries: entries.ok_or_else(|| A::Error::missing_field("entries"))?,
            graph: graph.ok_or_else(|| A::Error::missing_field("graph"))?,
            bm25,
        })
    }
}

/// Maximum total postings accepted in a snapshot's BM25 index (SEC-3).
const MAX_BM25_POSTINGS: usize = 50_000_000;

/// SEC-3 + MED-1 validation of a decoded BM25 index.
///
/// Returns `false` — and warns when `warn` — for an index that would let a
/// crafted snapshot blow up memory (`total_postings`) or panic inside `score()`
/// with an out-of-bounds `doc_id` (`validate_doc_ids`). A refused section is
/// treated as absent, which routes text queries to the live-scan fallback
/// exactly as a snapshot without a BM25 index does.
fn validate_bm25(bm25: &Bm25InvertedIndex, warn: bool) -> bool {
    let posting_count = bm25.total_postings();
    if posting_count > MAX_BM25_POSTINGS {
        if warn {
            eprintln!(
                "warning: index file contains too many BM25 postings ({posting_count}); ignoring the BM25 section"
            );
        }
        return false;
    }
    if !bm25.validate_doc_ids() {
        if warn {
            eprintln!(
                "warning: index file contains out-of-bounds BM25 doc_id; ignoring the BM25 section"
            );
        }
        return false;
    }
    true
}

/// A BM25 section as the envelope visitor left it, with the borrow of the
/// source buffer already resolved to an offset so the buffer can be moved.
enum PendingBm25 {
    Absent,
    Eager(Box<Bm25InvertedIndex>),
    /// Offset into the snapshot buffer where the (still undecoded) MessagePack
    /// value of the `bm25_index` key starts.
    Deferred(usize),
}

/// The BM25 inverted index of a loaded snapshot, decoded on demand.
///
/// DEC-265: the deferred variant keeps the snapshot *bytes* rather than
/// re-reading the file on first use. Re-reading would need the index path
/// threaded through `SnapshotIndex` and would re-open a file that may have been
/// replaced since the load (a mutating command in another process rewrites it
/// atomically), so the decoded section could disagree with the entries it was
/// loaded beside. Holding the buffer costs the file's size in RSS until the
/// first text query — less than the decoded structure it stands in for — and
/// the buffer is dropped as soon as the decode consumes it, so a text query
/// ends up with the same steady-state footprint as before this change.
enum Bm25Section {
    /// The snapshot carries no BM25 index, or its section was refused by
    /// [`validate_bm25`].
    Absent,
    /// Decoded and validated.
    Loaded(Box<Bm25InvertedIndex>),
    /// Not decoded yet.
    Deferred {
        /// The whole snapshot buffer; `raw[offset..]` is the MessagePack value
        /// of the envelope's `bm25_index` key. Taken — and thereby freed — by
        /// the first [`Bm25Section::get`].
        raw: std::sync::Mutex<Option<Vec<u8>>>,
        offset: usize,
        /// Whether a refusal should print a warning (mirrors `load_inner`'s
        /// `warn`, so `load_silent` stays silent).
        warn: bool,
        decoded: std::sync::OnceLock<Option<Box<Bm25InvertedIndex>>>,
    },
}

impl Bm25Section {
    /// The decoded index, decoding (and validating) it on first call.
    ///
    /// Returns `None` for a snapshot without a BM25 section and for one whose
    /// section is unreadable or fails [`validate_bm25`]; callers treat all three
    /// the same way, by falling back to a live scan.
    fn get(&self) -> Option<&Bm25InvertedIndex> {
        match self {
            Self::Absent => None,
            Self::Loaded(bm25) => Some(bm25),
            Self::Deferred {
                raw,
                offset,
                warn,
                decoded,
            } => decoded
                .get_or_init(|| {
                    // `take()` hands the buffer to this closure, which drops it
                    // on return — the raw bytes do not outlive the decode.
                    let buf = match raw.lock() {
                        Ok(mut guard) => guard.take(),
                        Err(poisoned) => poisoned.into_inner().take(),
                    }?;
                    let bytes = buf.get(*offset..)?;
                    match rmp_serde::from_slice::<Bm25InvertedIndex>(bytes) {
                        Ok(bm25) if validate_bm25(&bm25, *warn) => Some(Box::new(bm25)),
                        Ok(_) => None,
                        Err(e) => {
                            if *warn {
                                eprintln!(
                                    "warning: index file has an unreadable BM25 section ({e}); ignoring it"
                                );
                            }
                            None
                        }
                    }
                })
                .as_deref(),
        }
    }

    /// Whether the snapshot carries a BM25 section, *without* decoding it.
    ///
    /// Used on hot paths that only need the yes/no answer (deciding whether an
    /// incrementally re-scanned entry must be re-tokenized).
    fn is_present(&self) -> bool {
        !matches!(self, Self::Absent)
    }

    /// Whether the section is still undecoded — the state the whole iteration
    /// exists to reach. Tests assert it so a regression that quietly re-enables
    /// the eager path (a field reorder, a reader that stops borrowing) shows up
    /// as a failure rather than as 180 ms nobody notices.
    #[cfg(test)]
    fn is_deferred(&self) -> bool {
        matches!(self, Self::Deferred { decoded, .. } if decoded.get().is_none())
    }
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
    /// Persisted BM25 inverted index (if the snapshot was built with
    /// `bm25_tokenize = true`), decoded lazily on first use (iter-260).
    bm25: Bm25Section,
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
    fn effective_frontmatter_link_props(&self) -> Option<Vec<String>> {
        self.frontmatter_link_props.clone()
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
            self.bm25.is_present(),
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
        // Forces a deferred section: a rebuild needs the old postings to
        // reconstruct tokens for entries the mutation wave never touched.
        let Some(old) = self.bm25.get() else {
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
        // Replacing the section with the rebuilt index also drops the retained
        // snapshot bytes the deferred variant was holding.
        self.bm25 = Bm25Section::Loaded(Box::new(
            crate::bm25::Bm25InvertedIndex::build_from_tokens(docs),
        ));
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
            fm_props.as_deref(),
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
            fm_props.as_deref(),
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
            fm_props.as_deref(),
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
            fm_props.as_deref(),
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
    ///
    /// Forces a deferred BM25 section (iter-260) before re-serializing: the
    /// snapshot on disk must keep the section it came with, or every mutating
    /// command (`set`, `remove`, `append`, `task toggle`, `mv`,
    /// `lint --fix --index`) would silently delete the search index it was
    /// only ever meant to patch.
    pub fn save_to(&self, path: &Path) -> Result<()> {
        write_snapshot(
            self,
            path,
            &self.header.vault_dir,
            self.header.site_prefix.as_deref(),
            self.bm25.get(),
            &self.header.attachments,
        )
    }

    /// The vault's attachment paths as recorded when the snapshot was built
    /// (iter-261). Empty for a snapshot written before attachments were
    /// indexed, or for a vault that has none.
    #[must_use]
    pub fn attachments(&self) -> &[String] {
        &self.header.attachments
    }

    // ------------------------------------------------------------------
    // Deserialization
    // ------------------------------------------------------------------

    /// Deserialize snapshot bytes into a `SnapshotIndex`, optionally printing a
    /// warning when the schema is incompatible.
    ///
    /// Returns `Ok(Some(index))` on success, `Ok(None)` on schema mismatch.
    fn load_inner(bytes: Vec<u8>, warn: bool) -> Option<Self> {
        use serde::de::DeserializeSeed as _;

        // Limits used by the SEC-2 and SEC-3 defense-in-depth checks below.
        const MAX_ENTRIES: usize = 5_000_000;
        const MAX_GRAPH_EDGES: usize = 50_000_000;

        // Visit the envelope by hand so the `bm25_index` value can be left
        // undecoded (iter-260). The borrow of `bytes` ends with this block: the
        // deferred tail is converted to an offset so `bytes` can be moved into
        // the returned index.
        let (header, entries, graph, pending_bm25) = {
            let mut de = rmp_serde::Deserializer::from_read_ref(bytes.as_slice());
            let seed = SnapshotSeed {
                whole: bytes.as_slice(),
            };
            match seed.deserialize(&mut de) {
                Ok(decoded) => {
                    let bm25 = match decoded.bm25 {
                        DecodedBm25::Absent => PendingBm25::Absent,
                        DecodedBm25::Eager(bm25) => PendingBm25::Eager(bm25),
                        DecodedBm25::Deferred(tail) => {
                            PendingBm25::Deferred(bytes.len() - tail.len())
                        }
                    };
                    (decoded.header, decoded.entries, decoded.graph, bm25)
                }
                Err(e) => {
                    if warn {
                        eprintln!(
                            "warning: index file is incompatible ({e}); falling back to disk scan"
                        );
                    }
                    return None;
                }
            }
        };

        // SEC-2 (defense-in-depth): reject snapshots with an implausible
        // number of entries — a crafted MessagePack header claiming millions
        // of entries can trigger large allocations even with file-size caps.
        if entries.len() > MAX_ENTRIES {
            if warn {
                eprintln!(
                    "warning: index file contains {} entries (limit {}); falling back to disk scan",
                    entries.len(),
                    MAX_ENTRIES
                );
            }
            return None;
        }

        // SEC-1: Validate every rel_path before trusting snapshot data.
        // Reject the entire snapshot if any path is unsafe — a crafted
        // snapshot with path-traversal entries could escape the vault.
        for entry in &entries {
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

        // SEC-3 (defense-in-depth): reject snapshots whose link graph would
        // expand to an implausibly large in-memory structure.  A crafted
        // snapshot could claim a plausible number of top-level keys while
        // hiding millions of per-key entries, causing allocations far
        // exceeding the file size cap.
        let edge_count = graph.total_edges();
        if edge_count > MAX_GRAPH_EDGES {
            if warn {
                eprintln!(
                    "warning: index file contains too many graph edges ({edge_count}); falling back to disk scan"
                );
            }
            return None;
        }

        // SEC-3 / MED-1 for the BM25 section move to `validate_bm25`, which runs
        // at decode time — immediately here for an eagerly decoded section, on
        // first use for a deferred one (iter-260). A refused section is dropped
        // rather than rejecting the whole snapshot: by the time a deferred
        // section is decoded the caller is mid-query and the "fall back to a
        // disk scan" escape hatch no longer composes, whereas "this snapshot has
        // no BM25 index" is a state every caller already handles by live
        // scanning. The crafted postings still never reach `score()`.
        let bm25 = match pending_bm25 {
            PendingBm25::Absent => Bm25Section::Absent,
            PendingBm25::Eager(bm25) => {
                if validate_bm25(&bm25, warn) {
                    Bm25Section::Loaded(bm25)
                } else {
                    Bm25Section::Absent
                }
            }
            PendingBm25::Deferred(offset) => Bm25Section::Deferred {
                raw: std::sync::Mutex::new(Some(bytes)),
                offset,
                warn,
                decoded: std::sync::OnceLock::new(),
            },
        };

        // Entries are stored in sorted order (ScannedIndex::build sorts
        // before saving).  Re-sort here to guarantee the invariant even
        // if an older snapshot was created without sorting.
        let mut entries = entries;
        entries.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

        // `[scan] exclude` (iter-265) is applied on load, not only on the disk
        // walk, so `--index` and off-disk runs agree on which files exist. A
        // snapshot built before the exclusion was configured — or by a version
        // that predates it — therefore needs no rebuild to be correct. The
        // excluded sources are dropped from the link graph too, so a note that
        // only an excluded template links to is still an orphan.
        let mut graph = graph;
        if crate::discovery::scan_exclude().is_some() {
            let mut removed = 0usize;
            entries.retain(|e| {
                if crate::discovery::is_scan_excluded(&e.rel_path) {
                    graph.remove_source(&e.rel_path);
                    removed += 1;
                    false
                } else {
                    true
                }
            });
            if removed > 0 {
                crate::discovery::note_scan_excluded(removed);
            }
        }
        // BUG-18 (iter-273): the retain above only accounts for files the
        // *snapshot still carries*. When the index was built with the same
        // exclusions already in force, those files never entered it, so the
        // build-time figure recorded in the header is the only witness left —
        // without it `summary --index` reported `excluded: 0` for a vault the
        // disk scan reports 52 excluded files for. Trusted only while the
        // configured patterns still match the ones that produced it.
        if !header.scan_exclude.is_empty()
            && header.scan_exclude == crate::discovery::scan_exclude_patterns()
        {
            crate::discovery::note_scan_excluded(
                usize::try_from(header.scan_excluded).unwrap_or(usize::MAX),
            );
        }

        let path_index: HashMap<String, usize> = entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.rel_path.clone(), i))
            .collect();
        // The graph's lowercased companion map is `#[serde(skip)]`, so a
        // freshly-deserialized graph has an empty one — rebuild it from
        // the restored index keys so `backlinks_ci` works off snapshots.
        graph.rebuild_lower_index();
        Some(Self {
            entries,
            path_index,
            graph,
            header,
            bm25,
            frontmatter_link_props: None,
            case_index_cache: None,
        })
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
        Ok(Self::load_inner(bytes, true))
    }

    /// Load a snapshot silently — identical to [`load`] but suppresses the
    /// incompatibility warning.  Used by `find_stale_indexes` which expects to
    /// silently skip files that cannot be deserialized.
    fn load_silent(path: &Path) -> Result<Option<Self>> {
        let Some(bytes) = read_index_bytes(path, false)? else {
            return Ok(None);
        };
        Ok(Self::load_inner(bytes, false))
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
        write_snapshot(index, path, vault_dir, site_prefix, bm25_index, &[])
    }

    /// Like [`save`](Self::save) but also records the vault's attachment paths
    /// (iter-261) so `--index` runs resolve `![[img.png]]` and `[[Books.base]]`
    /// exactly as an off-disk run does.
    pub fn save_with_attachments(
        index: &dyn VaultIndex,
        path: &Path,
        vault_dir: &str,
        site_prefix: Option<&str>,
        bm25_index: Option<&Bm25InvertedIndex>,
        attachments: &[String],
    ) -> Result<()> {
        write_snapshot(index, path, vault_dir, site_prefix, bm25_index, attachments)
    }

    /// Return the persisted BM25 inverted index, if present.
    pub fn bm25_index(&self) -> Option<&Bm25InvertedIndex> {
        self.bm25.get()
    }

    /// Whether the BM25 section is still undecoded (test-only perf invariant).
    #[cfg(test)]
    fn bm25_is_deferred(&self) -> bool {
        self.bm25.is_deferred()
    }

    /// The snapshot's format version — `0` for anything written before
    /// iter-276, which stamped none.
    pub fn format_version(&self) -> u32 {
        self.header.format_version
    }

    /// Whether this snapshot was written by a binary whose format this one can
    /// still answer from.
    ///
    /// A *newer* snapshot is accepted: forward fields decode as unknown keys
    /// and are skipped, and refusing one would break a mixed-version team for
    /// no gain. An *older* one is refused (BUG-12) — the fields it lacks are
    /// exactly the ones that make its answers differ from disk.
    pub fn format_is_current(&self) -> bool {
        self.header.format_version >= SNAPSHOT_FORMAT_VERSION
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
    attachments: &[String],
) -> Result<()> {
    let header = SnapshotHeader {
        format_version: SNAPSHOT_FORMAT_VERSION,
        vault_dir: vault_dir.to_owned(),
        site_prefix: site_prefix.map(str::to_owned),
        created_at: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
        pid: std::process::id(),
        attachments: attachments.to_vec(),
        // iter-273 (BUG-18): read straight from the process-global counters
        // rather than threaded through every caller — `create-index` has just
        // finished the walk that set them, and a `save_to` of a snapshot
        // loaded in this same process re-seeded them from the header it read,
        // so both write paths record the figure that describes these bytes.
        scan_excluded: crate::discovery::scan_excluded_count() as u64,
        scan_exclude: crate::discovery::scan_exclude_patterns().to_vec(),
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
        self.bm25.get()
    }

    fn snapshot_format_version(&self) -> Option<u32> {
        Some(self.header.format_version)
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
///
/// The cost is the probe's blind spot, which is therefore **up to ~2 s**, not
/// one: the truncation loses up to a second and this tolerance adds another
/// (BUG-30, iter-276 — DEC-302's original wording said "the same whole
/// second"). `--index --help` says the same.
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
    index
        .entries()
        .iter()
        .filter(|e| entry_is_stale_on_disk(e, dir))
        .map(|e| e.rel_path.clone())
        .collect()
}

/// The first indexed file whose bytes on disk are newer than the snapshot
/// remembers, or `None` when every entry is current.
///
/// INDEX-1 (iter-273, BUG-12) — the DEC-280 amendment. [`newest_dir_mtime`] is
/// blind to an in-place overwrite: rewriting `n2.md` moves that file's mtime
/// but leaves its directory's untouched, so `find --index` answered from a
/// snapshot that no longer described the vault, with no warning and exit 0.
/// This is the same per-entry comparison [`files_modified_since_snapshot`]
/// already made for `links fix`, short-circuited at the first hit: the warning
/// only needs one witness, and stopping there makes the common "something
/// changed" case cost a handful of `stat`s rather than one per indexed file.
///
/// The clean-vault case does pay one `stat` per entry, which is why callers
/// run this **after** the cheaper directory probe and only when a run did not
/// already refresh every file it named (DEC-280's "refresh what you are about
/// to return" half).
pub fn first_file_modified_since_snapshot(index: &SnapshotIndex, dir: &Path) -> Option<String> {
    index
        .entries()
        .iter()
        .find(|e| entry_is_stale_on_disk(e, dir))
        .map(|e| e.rel_path.clone())
}

/// Whether one index entry's file on disk is newer than the entry records.
///
/// A file whose stored mtime cannot be parsed, or which no longer exists, is
/// not stale — the former is "unknown", the latter is the caller's deletion
/// handling. Both keep the historic answer of the loop this was extracted from.
fn entry_is_stale_on_disk(entry: &IndexEntry, dir: &Path) -> bool {
    let Some(indexed) = parse_iso8601_secs(&entry.modified) else {
        return false;
    };
    let Ok(meta) = std::fs::metadata(dir.join(&entry.rel_path)) else {
        return false;
    };
    let Ok(modified) = meta.modified() else {
        return false;
    };
    let Ok(disk) = modified.duration_since(SystemTime::UNIX_EPOCH) else {
        return false;
    };
    disk.as_secs() > indexed.saturating_add(STALENESS_TOLERANCE_SECS)
}

/// Bring one named file's index entry up to date with disk, if it drifted.
///
/// UX-7 (iter-265, DEC-280): the stale-index policy is "refresh what you are
/// about to touch or return, and warn only when you cannot". This is the
/// per-file half — one `stat`, and a re-scan only when the recorded mtime is
/// behind the file's. Used by `--index` reads that named their targets, so
/// `find --index --file just-appended.md` reports the file as it is now rather
/// than as the snapshot remembers it.
///
/// Returns `true` when the entry is now known-current: either it was already
/// current, or the re-scan succeeded. Returns `false` when the caller must fall
/// back to warning — the file is not in the index at all (so there is nothing
/// to refresh in place), it could not be stat'ed, or the re-scan failed.
pub fn refresh_if_changed_on_disk(index: &mut SnapshotIndex, dir: &Path, rel: &str) -> bool {
    let Some(entry) = index.get(rel) else {
        return false;
    };
    let Some(indexed) = parse_iso8601_secs(&entry.modified) else {
        return false;
    };
    let full = dir.join(rel);
    let Ok(meta) = std::fs::metadata(&full) else {
        return false;
    };
    let Ok(modified) = meta.modified() else {
        return false;
    };
    let Ok(disk) = modified.duration_since(SystemTime::UNIX_EPOCH) else {
        return false;
    };
    // Size is compared too: a same-second edit that changes the length is
    // invisible to an mtime-only check, and appending is exactly that case.
    if disk.as_secs() <= indexed.saturating_add(STALENESS_TOLERANCE_SECS)
        && meta.len() == entry.size
    {
        return true;
    }
    index
        .refresh_entry_and_links_at(&full, rel)
        .unwrap_or(false)
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
    frontmatter_link_props: Option<&[String]>,
) -> Result<(IndexEntry, Option<FileLinks>)> {
    let mut fm = FrontmatterCollector::new(scan_body);
    let mut body_collector = BodyCollector::new(bm25_tokenize);

    let stats;
    let (sections, tasks, links, file_links) = if scan_body {
        let mut section_scanner = SectionScanner::new();
        let mut task_extractor = TaskExtractor::new();
        let mut link_visitor = LinkGraphVisitor::with_frontmatter_props(
            PathBuf::from(rel_path),
            frontmatter_link_props.map(<[String]>::to_vec),
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
    // BUG-14 (iter-265): a file whose bytes are not valid UTF-8 is dropped
    // entirely by the disk full-text path (`find <term>` reads it with
    // `read_to_string`), so an index that tokenized its lossy U+FFFD form
    // counted an extra document and a wrong average length — every BM25 score
    // in the vault came out different under `--index` than off disk. Keep it
    // out of the corpus here and let `ScannedIndex::build` report it.
    let (bm25_tokens, bm25_language, bm25_tokenizer_version) = if bm25_tokenize && stats.valid_utf8
    {
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

/// Read one file's heading outline, and nothing else.
///
/// NAMED-3 (iter-273, BUG-9): the broken-anchor verdict needs the *target*
/// file's headings, which the index supplies for free during a vault sweep but
/// not when `--file` / `--glob` narrowed the scan to the source file alone.
/// One targeted read per distinct anchor target is the cheap way to make the
/// four spellings of the same question return the same answer — far cheaper
/// than promoting every per-file link query into a whole-vault scan.
pub fn scan_file_sections(full_path: &Path) -> Result<Vec<OutlineSection>> {
    let mut scanner = SectionScanner::new();
    crate::scanner::scan_file_multi(full_path, &mut [&mut scanner])?;
    Ok(scanner.into_sections())
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
            // iter-261: a section's link list is a vault-link inventory; an
            // external URI (now parsed rather than dropped) does not belong.
            if link.external {
                continue;
            }
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

        let (entry, _) = scan_one_file(&full, rel, true, true, None, None).unwrap();
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

    fn test_entry(rel_path: &str) -> IndexEntry {
        IndexEntry {
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
        }
    }

    /// Serialize a snapshot envelope exactly as `write_snapshot` does, so
    /// crafted-input tests exercise the real wire format (field order included).
    fn snapshot_bytes(entries: &[IndexEntry], bm25: Option<&Bm25InvertedIndex>) -> Vec<u8> {
        let graph = LinkGraph::default();
        let data = SnapshotDataRef {
            header: SnapshotHeader {
                vault_dir: "/tmp/vault".to_owned(),
                site_prefix: None,
                created_at: 0,
                pid: std::process::id(),
                attachments: Vec::new(),
                scan_excluded: 0,
                scan_exclude: Vec::new(),
            },
            entries,
            graph: &graph,
            bm25_index: bm25,
        };
        rmp_serde::to_vec_named(&data).expect("snapshot envelope serializes")
    }

    fn make_snapshot_bytes(rel_path: &str) -> Vec<u8> {
        snapshot_bytes(&[test_entry(rel_path)], None)
    }

    #[test]
    fn load_inner_rejects_parent_traversal() {
        let bytes = make_snapshot_bytes("../../escape.md");
        assert!(
            SnapshotIndex::load_inner(bytes, false).is_none(),
            "snapshot with '..' path components must be rejected"
        );
    }

    #[test]
    fn load_inner_rejects_absolute_path() {
        // Unix-style absolute path (on Windows this has a RootDir component
        // but is_absolute() returns false, so the component check must catch it)
        let bytes = make_snapshot_bytes("/etc/passwd");
        assert!(
            SnapshotIndex::load_inner(bytes, false).is_none(),
            "snapshot with absolute rel_path must be rejected"
        );

        // Windows-style absolute path (only testable on Windows where the
        // Prefix component is recognized by std::path)
        #[cfg(windows)]
        {
            let bytes = make_snapshot_bytes("C:\\Windows\\System32\\config\\sam");
            assert!(
                SnapshotIndex::load_inner(bytes, false).is_none(),
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
            SnapshotIndex::load_inner(bytes, false).is_none(),
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
            SnapshotIndex::load_inner(bytes, false).is_none(),
            "snapshot with an NTFS-ADS rel_path must be rejected"
        );
    }

    #[test]
    fn load_inner_rejects_null_byte() {
        let bytes = make_snapshot_bytes("foo\0bar.md");
        assert!(
            SnapshotIndex::load_inner(bytes, false).is_none(),
            "snapshot with null-byte path must be rejected"
        );
    }

    /// SEC-3 / MED-1 under lazy decoding (iter-260).
    ///
    /// A crafted BM25 section is still refused — it just cannot reject the whole
    /// snapshot any more, because the decode happens mid-query. The observable
    /// contract is that the poisoned postings never reach `score()`:
    /// `bm25_index()` reports the section as absent, which is exactly the state
    /// callers already handle by live scanning.
    fn assert_bm25_section_refused(bm25: &Bm25InvertedIndex, what: &str) {
        let bytes = snapshot_bytes(&[test_entry("doc.md")], Some(bm25));
        let index = SnapshotIndex::load_inner(bytes, false)
            .unwrap_or_else(|| panic!("{what}: the snapshot itself is well-formed and must load"));
        assert!(
            index.bm25_index().is_none(),
            "{what}: the BM25 section must be refused, never handed to score()"
        );
        // Second call goes through the cached `OnceLock` — still refused.
        assert!(
            index.bm25_index().is_none(),
            "{what}: a refused BM25 section must stay refused"
        );
    }

    #[test]
    fn load_inner_rejects_bm25_out_of_bounds_doc_id() {
        use crate::bm25::Posting;
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

        assert_bm25_section_refused(&bad_bm25, "out-of-bounds BM25 doc_id (MED-1)");
    }

    #[test]
    fn load_inner_rejects_bm25_mismatched_doc_lengths() {
        use crate::bm25::Posting;
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

        assert_bm25_section_refused(&bad_bm25, "mismatched BM25 doc_lengths/doc_paths (MED-1)");
    }

    // -------------------------------------------------------------------------
    // Lazy BM25 section (iter-260)
    // -------------------------------------------------------------------------

    fn sample_bm25() -> Bm25InvertedIndex {
        use crate::bm25::Posting;
        use std::collections::HashMap;

        let mut postings: HashMap<String, Vec<Posting>> = HashMap::new();
        postings.insert(
            "rust".to_owned(),
            vec![Posting {
                doc_id: 0,
                term_freq: 2,
                positions: vec![0, 4],
            }],
        );
        postings.insert(
            "index".to_owned(),
            vec![Posting {
                doc_id: 0,
                term_freq: 1,
                positions: vec![2],
            }],
        );
        Bm25InvertedIndex::new_for_test(postings, vec![5], vec!["doc.md".to_owned()], 5.0)
    }

    /// The whole lazy-load scheme rests on `bm25_index` being the LAST key
    /// `rmp_serde::to_vec_named` emits for the envelope: the visitor stops there
    /// and keeps everything after it as the section's bytes, so a field declared
    /// after `bm25_index` would be silently dropped on load.
    ///
    /// A future field reorder must fail here, loudly, rather than quietly
    /// costing ~180 ms per indexed command again (or losing data).
    #[test]
    fn bm25_index_is_the_last_envelope_key() {
        // Walks the top-level map and collects its keys in wire order.
        struct EnvelopeKeys(Vec<String>);

        impl<'de> Deserialize<'de> for EnvelopeKeys {
            fn deserialize<D>(d: D) -> std::result::Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct V;

                impl<'de> serde::de::Visitor<'de> for V {
                    type Value = EnvelopeKeys;

                    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        f.write_str("the snapshot envelope map")
                    }

                    fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
                    where
                        A: serde::de::MapAccess<'de>,
                    {
                        let mut keys = Vec::new();
                        while let Some(k) = map.next_key::<String>()? {
                            keys.push(k);
                            map.next_value::<serde::de::IgnoredAny>()?;
                        }
                        Ok(EnvelopeKeys(keys))
                    }
                }

                d.deserialize_map(V)
            }
        }

        let bytes = snapshot_bytes(&[test_entry("doc.md")], Some(&sample_bm25()));
        let keys = rmp_serde::from_slice::<EnvelopeKeys>(&bytes)
            .expect("envelope is a MessagePack map")
            .0;

        assert_eq!(
            keys,
            vec!["header", "entries", "graph", "bm25_index"],
            "snapshot envelope field order changed; `bm25_index` must stay last \
             or SnapshotSeed's early stop drops the fields after it"
        );
    }

    /// A snapshot without a BM25 section must load, and stay BM25-less.
    #[test]
    fn load_inner_without_bm25_section_reports_absent() {
        let bytes = snapshot_bytes(&[test_entry("doc.md")], None);
        let index = SnapshotIndex::load_inner(bytes, false).expect("snapshot loads");
        assert!(index.bm25_index().is_none());
    }

    /// The deferred section must decode to exactly what was written — and must
    /// genuinely still be deferred until something asks for it.
    #[test]
    fn deferred_bm25_section_decodes_to_the_written_index() {
        let original = sample_bm25();
        let bytes = snapshot_bytes(&[test_entry("doc.md")], Some(&original));
        let index = SnapshotIndex::load_inner(bytes, false).expect("snapshot loads");

        assert!(
            index.bm25_is_deferred(),
            "loading must leave the BM25 section undecoded — that is the whole point"
        );
        // Asking whether a section exists must not force the decode.
        assert!(index.bm25.is_present());
        assert!(index.bm25_is_deferred());

        let loaded = index.bm25_index().expect("BM25 section decodes on demand");
        assert_eq!(loaded.total_postings(), original.total_postings());
        assert_eq!(loaded.doc_count(), original.doc_count());
        // Repeated access is cached, not re-decoded, and stays equivalent.
        let again = index.bm25_index().expect("cached BM25 section");
        assert_eq!(again.total_postings(), original.total_postings());
    }

    /// Forward-safety for the early stop: if a future envelope ever emits
    /// `bm25_index` before another field, the visitor must fall back to decoding
    /// the value in place instead of dropping the fields behind it. Slower, but
    /// never wrong.
    #[test]
    fn envelope_with_bm25_not_last_falls_back_to_eager_decode() {
        #[derive(Serialize)]
        struct Reordered<'a> {
            header: SnapshotHeader,
            bm25_index: Option<&'a Bm25InvertedIndex>,
            entries: &'a [IndexEntry],
            graph: &'a LinkGraph,
        }

        let original = sample_bm25();
        let entries = vec![test_entry("doc.md")];
        let graph = LinkGraph::default();
        let bytes = rmp_serde::to_vec_named(&Reordered {
            header: SnapshotHeader {
                vault_dir: "/tmp/vault".to_owned(),
                site_prefix: None,
                created_at: 0,
                pid: std::process::id(),
                attachments: Vec::new(),
                scan_excluded: 0,
                scan_exclude: Vec::new(),
            },
            bm25_index: Some(&original),
            entries: &entries,
            graph: &graph,
        })
        .expect("reordered envelope serializes");

        let index = SnapshotIndex::load_inner(bytes, false)
            .expect("a reordered envelope must still load, not be dropped");
        assert!(
            !index.bm25_is_deferred(),
            "with bm25_index not last the visitor must decode it eagerly"
        );
        assert_eq!(index.entries().len(), 1, "no field may be lost");
        assert_eq!(
            index
                .bm25_index()
                .expect("eagerly decoded section")
                .total_postings(),
            original.total_postings()
        );
    }

    /// The save hazard the lazy load introduces: `save_to` re-serializes the
    /// BM25 section, so a snapshot that was never asked for its section must
    /// still write it back rather than silently dropping it. Every mutating
    /// command (`set`, `remove`, `append`, `task toggle`, `mv`,
    /// `lint --fix --index`) goes through this path.
    #[test]
    fn save_to_preserves_an_untouched_deferred_bm25_section() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("first.hyalo-index");
        let original = sample_bm25();
        std::fs::write(
            &path,
            snapshot_bytes(&[test_entry("doc.md")], Some(&original)),
        )
        .expect("write snapshot");

        // Load, never touch the BM25 section, save somewhere else.
        let index = SnapshotIndex::load(&path)
            .expect("load succeeds")
            .expect("snapshot is compatible");
        let out = dir.path().join("second.hyalo-index");
        index.save_to(&out).expect("re-save succeeds");

        let reloaded = SnapshotIndex::load(&out)
            .expect("reload succeeds")
            .expect("snapshot is compatible");
        let bm25 = reloaded
            .bm25_index()
            .expect("re-saved snapshot must keep its BM25 section");
        assert_eq!(bm25.total_postings(), original.total_postings());
        assert_eq!(bm25.doc_count(), original.doc_count());
    }

    /// `bytes_after_key` is the one piece of pointer arithmetic the early stop
    /// rests on: it turns "the reader borrowed this key out of the buffer" into
    /// "the value starts here". Getting it wrong slices the buffer at a bogus
    /// offset, so the in-bounds answer and every out-of-bounds refusal are
    /// pinned directly rather than only through the decode path.
    #[test]
    fn bytes_after_key_locates_the_value_or_refuses() {
        let whole = b"\x00\x01header\x02\x03".as_slice();

        // A key that really is a slice of `whole`: the tail starts right after
        // it, and the returned slice is the same allocation, not a copy.
        let key = std::str::from_utf8(&whole[2..8]).expect("ascii");
        assert_eq!(key, "header");
        let tail = bytes_after_key(whole, key).expect("an interior key resolves");
        assert_eq!(tail, b"\x02\x03");
        assert!(std::ptr::eq(tail.as_ptr(), whole[8..].as_ptr()));

        // A key ending exactly at the end of the buffer yields an empty tail
        // rather than being refused for straddling the boundary.
        let last = std::str::from_utf8(&whole[8..]).expect("control bytes are valid utf-8");
        assert_eq!(
            bytes_after_key(whole, last).expect("a trailing key resolves"),
            b"",
            "a key ending at the buffer's end has an empty tail, not no tail"
        );

        // A key the reader did NOT borrow out of `whole` — the case that forces
        // the eager fallback — must be refused, not turned into a wild offset.
        // Taken from the middle of a longer independent buffer, so the refusal
        // is decided by the bounds check and not by a length coincidence.
        let elsewhere = String::from("xxxheaderxxx");
        let owned_key = &elsewhere[3..9];
        assert_eq!(owned_key, "header");
        assert!(
            bytes_after_key(whole, owned_key).is_none(),
            "a key outside the buffer must not resolve to an offset"
        );

        // A key that starts inside the buffer but runs past its end is refused
        // too — the check covers the key's whole extent, not just its start.
        let short = &whole[..7];
        assert!(
            bytes_after_key(short, key).is_none(),
            "a key overrunning the buffer end must be refused"
        );
    }

    /// A deferred section whose bytes do not decode must be refused the same way
    /// a section failing `validate_bm25` is: reported absent, so the caller live
    /// scans. The snapshot around it is still perfectly good and must survive —
    /// this is the mid-query failure that made "reject the whole snapshot"
    /// untenable in the first place.
    #[test]
    fn deferred_bm25_section_with_corrupt_bytes_is_refused() {
        let mut bytes = snapshot_bytes(&[test_entry("doc.md")], Some(&sample_bm25()));
        // Corrupt the tail — the `bm25_index` value is the last thing in the
        // envelope, so trailing garbage lands inside it and nowhere else.
        let len = bytes.len();
        for b in &mut bytes[len - 16..] {
            *b = 0xC1; // never-valid MessagePack byte
        }

        let index = SnapshotIndex::load_inner(bytes, false)
            .expect("header/entries/graph decode fine; only the BM25 tail is junk");
        assert_eq!(
            index.entries().len(),
            1,
            "a corrupt BM25 section must not cost the entries"
        );
        assert!(
            index.bm25_index().is_none(),
            "an undecodable BM25 section must be refused, not panic or be handed out"
        );
        assert!(
            index.bm25_index().is_none(),
            "the refusal must be cached, not retried on every query"
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
