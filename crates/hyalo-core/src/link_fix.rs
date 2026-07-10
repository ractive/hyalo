//! Broken link detection and auto-repair with fuzzy matching.
//!
//! # Overview
//!
//! 1. [`detect_broken_links`] / [`detect_broken_links_from_index`] — scan a
//!    vault for links that cannot be resolved to an existing file and return a
//!    [`BrokenLinkReport`].
//!
//! 2. [`plan_fixes`] — for each broken link, find the best candidate file using
//!    a priority-ordered strategy (case-insensitive → extension mismatch →
//!    shortest-path → fuzzy Jaro-Winkler) and produce a [`FixReport`].
//!
//! 3. [`apply_fixes`] — convert [`FixPlan`]s to [`RewritePlan`]s and write
//!    the corrected link text back to disk.

#![allow(clippy::missing_errors_doc)]

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::case_index::CaseInsensitiveIndex;
use crate::discovery::canonicalize_vault_dir;
use crate::discovery::resolve_target;
use crate::index::VaultIndex;
use crate::link_graph::{FileLinks, normalize_target};
use crate::link_rewrite::{
    Replacement, RewritePlan, apply_replacements, execute_plans, find_frontmatter_wikilinks,
};
use crate::links::{LinkKind, extract_link_spans_with_original, parse_wikilink};
use crate::scanner::{
    FenceTracker, MAX_FILE_SIZE, is_comment_fence, strip_inline_code, strip_inline_comments,
};
// ---------------------------------------------------------------------------
// Report types
// ---------------------------------------------------------------------------

/// A single broken link with source file, line number, and raw target.
#[derive(Debug, Clone, Serialize)]
pub struct BrokenLinkInfo {
    pub source: String,
    pub line: usize,
    pub target: String,
}

/// Summary of broken link detection across the vault.
#[derive(Debug, Clone, Serialize)]
pub struct BrokenLinkReport {
    pub total_links: usize,
    pub broken: Vec<BrokenLinkInfo>,
    /// Links that resolve via case-insensitive fallback but whose written casing
    /// differs from the canonical on-disk path.  These are NOT broken — the
    /// file exists — but they carry the wrong casing and can be auto-fixed.
    ///
    /// All entries use [`FixStrategy::LinkCaseMismatch`]. Two scenarios are
    /// covered:
    /// - Path-form link whose casing differs from the on-disk file — the
    ///   `new_target` is the canonical path. Only detected when the
    ///   [`CaseInsensitiveIndex`] has case-insensitive path lookups enabled.
    /// - Short-form bare wikilink whose stem casing differs from the on-disk
    ///   filename — the `new_target` is the corrected short-form stem (never
    ///   a full path). Detected via the stem index, which is always active
    ///   regardless of case-insensitive-path mode.
    pub case_mismatches: Vec<FixPlan>,
    /// Short-form wikilinks (no `/`) whose stem matches ≥2 files in the vault.
    /// These are left untouched by `--apply` because the correct target is
    /// ambiguous and auto-picking would be wrong.
    pub ambiguous: Vec<BrokenLinkInfo>,
}

/// A single actionable fix: rewrite `old_target` → `new_target` in `source`.
#[derive(Debug, Clone, Serialize)]
pub struct FixPlan {
    /// Vault-relative path of the file containing the broken link.
    pub source: String,
    /// 1-based line number where the broken link appears.
    pub line: usize,
    /// The original (broken) link target as written in the source file.
    pub old_target: String,
    /// The corrected link target.
    pub new_target: String,
    /// How the match was found.
    pub strategy: FixStrategy,
    /// Similarity confidence in `[0.0, 1.0]`.
    pub confidence: f64,
}

/// How a candidate file was matched to a broken link target.
#[derive(Debug, Clone, Serialize)]
pub enum FixStrategy {
    /// The target matched an existing file path case-insensitively.
    CaseInsensitive,
    /// The target was written with or without `.md` and the other form matched.
    ExtensionMismatch,
    /// The bare stem matched exactly one file anywhere in the vault.
    ShortestPath,
    /// Jaro-Winkler similarity above the configured threshold.
    FuzzyMatch,
    /// The target resolves to an existing file but with different casing.
    ///
    /// Rule code: `link-case-mismatch`. The `new_target` in the [`FixPlan`]
    /// holds either the canonical on-disk path (for path-form links and
    /// markdown links) or the canonical short-form stem (for bare wikilinks
    /// whose stem lookup succeeded with a case-only difference).
    LinkCaseMismatch,
    /// Reserved for future use; no current code path emits this. Previously
    /// emitted for short-form wikilink stem casing mismatches, but those are
    /// now reported as [`LinkCaseMismatch`] for consistency with path-form
    /// case mismatches — they represent the same user intent (fix the
    /// casing).
    #[doc(hidden)]
    ShortFormStemMismatch,
}

/// Result of planning fixes for a set of broken links.
#[derive(Debug, Clone, Serialize)]
pub struct FixReport {
    /// Broken links for which a candidate fix was found.
    pub fixes: Vec<FixPlan>,
    /// Broken links for which no suitable candidate could be found.
    pub unfixable: Vec<BrokenLinkInfo>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Classify a single link's resolution against the filesystem and an optional
/// case-insensitive index.
///
/// Returns:
/// - `Resolved(None)` — link resolves exactly and its on-disk casing matches
///   the canonical form (or no index was supplied).
/// - `Resolved(Some(canonical))` — link resolves exactly but the on-disk
///   casing differs from the canonical form (case-insensitive filesystem
///   papered over a mismatch); caller should record as a case-mismatch.
/// - `CaseMismatch(canonical)` — exact resolution failed but the case index
///   found a unique canonical path; caller should record as a case-mismatch.
/// - `ShortFormValid` — a short-form wikilink whose stem resolves to exactly
///   one file in the vault with matching casing; nothing to fix.
/// - `ShortFormStemMismatch(correct_stem)` — a short-form wikilink whose stem
///   resolves to exactly one file, but the written casing of the stem differs
///   from the on-disk filename stem; `new_target` is the corrected stem
///   (never a path — never expanded).
/// - `ShortFormAmbiguous` — a short-form wikilink whose stem matches ≥2 files.
/// - `Broken` — nothing resolves.
#[derive(PartialEq)]
enum LinkResolution {
    Resolved(Option<String>),
    CaseMismatch(String),
    ShortFormValid,
    ShortFormStemMismatch(String),
    ShortFormAmbiguous,
    Broken,
}

/// Precomputed case-insensitive stem → candidate paths map used to resolve
/// short-form wikilinks when no [`CaseInsensitiveIndex`] is available.
/// Built once per `detect_broken_links*` call so each lookup is O(1).
struct StemIndex {
    map: HashMap<String, Vec<String>>,
}

impl StemIndex {
    fn build(vault_files: &[String]) -> Self {
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        for path in vault_files {
            let fname = path.rsplit('/').next().unwrap_or(path.as_str());
            let stem = fname.strip_suffix(".md").unwrap_or(fname);
            map.entry(stem.to_ascii_lowercase())
                .or_default()
                .push(path.clone());
        }
        Self { map }
    }

    fn lookup(&self, stem: &str) -> Vec<&str> {
        self.map
            .get(&stem.to_ascii_lowercase())
            .map(|v| v.iter().map(String::as_str).collect())
            .unwrap_or_default()
    }
}

/// Classify a short-form wikilink target (no `/`) against the vault's stem
/// index.  Returns a `LinkResolution` that covers valid, stem-case-mismatch,
/// ambiguous, and broken cases without ever producing a full path.
///
/// When `expand_short_form` is `true`, the caller has opted into path
/// expansion — skip the short-form special handling and let the caller fall
/// through to regular path-based classification.
fn classify_short_form_wikilink(
    target: &str,
    stem_index: &StemIndex,
    case_index: Option<&CaseInsensitiveIndex>,
    expand_short_form: bool,
) -> Option<LinkResolution> {
    if expand_short_form {
        return None; // caller should use regular path-based classification
    }

    // Only apply to bare stems (no directory separator). Wikilinks with an
    // explicit `.md` extension (e.g. `[[Note.md]]`) are path-like targets;
    // let the caller handle them via regular path-based classification rather
    // than mismatching them as stem lookups against `"Note.md"`.
    if target.contains('/') || target.contains('\\') {
        return None;
    }
    // Skip wikilinks with an explicit `.md` extension (case-insensitive),
    // which are path-like targets and should go through path-based handling.
    if std::path::Path::new(target)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
    {
        return None;
    }

    // Look up the stem case-insensitively. Prefer the case_index when
    // available (O(1) hash lookup); otherwise use the precomputed
    // per-invocation stem index built from `vault_files`.
    let matches: Vec<&str> = if let Some(idx) = case_index {
        idx.lookup_stem_all(target)
            .iter()
            .map(String::as_str)
            .collect()
    } else {
        stem_index.lookup(target)
    };

    match matches.len() {
        0 => Some(LinkResolution::Broken),
        1 => {
            // Exactly one match — the link is valid. Check if the stem casing differs.
            let canonical_path = matches[0];
            let canonical_fname = canonical_path.rsplit('/').next().unwrap_or(canonical_path);
            let canonical_stem = canonical_fname
                .strip_suffix(".md")
                .unwrap_or(canonical_fname);

            if target == canonical_stem {
                Some(LinkResolution::ShortFormValid)
            } else {
                // Stem casing differs — propose the canonical stem (not a full path).
                Some(LinkResolution::ShortFormStemMismatch(
                    canonical_stem.to_string(),
                ))
            }
        }
        _ => Some(LinkResolution::ShortFormAmbiguous),
    }
}

fn classify_link(
    canonical_dir: &Path,
    resolved_target: &str,
    site_prefix: Option<&str>,
    case_index: Option<&CaseInsensitiveIndex>,
) -> LinkResolution {
    let exact = resolve_target(canonical_dir, resolved_target, site_prefix, None);

    if let Some(exact_str) = exact {
        // Link resolves exactly. If we have a case index, also check whether the
        // resolved path has incorrect casing compared to the canonical on-disk
        // path. On case-insensitive filesystems, `exact` may contain the
        // user-written casing rather than the canonical casing.
        if let Some(idx) = case_index
            && let Some(canonical_path) =
                resolve_target(canonical_dir, resolved_target, site_prefix, Some(idx))
        {
            let canonical_fwd = canonical_path.replace('\\', "/");
            let exact_fwd = exact_str.replace('\\', "/");
            if exact_fwd != canonical_fwd {
                return LinkResolution::Resolved(Some(canonical_fwd));
            }
        }
        return LinkResolution::Resolved(None);
    }

    // Exact resolution failed. If we have a case index, try the
    // case-insensitive fallback. `resolve_target` already handles the `.md`
    // extension fallback internally, so any successful indexed resolution
    // here means the link is a case-mismatch (possibly combined with a
    // stem/full extension style difference).
    if let Some(idx) = case_index
        && let Some(canonical_path) =
            resolve_target(canonical_dir, resolved_target, site_prefix, Some(idx))
    {
        return LinkResolution::CaseMismatch(canonical_path.replace('\\', "/"));
    }

    LinkResolution::Broken
}

/// Resolve a link's target to a vault-relative path and classify it.
///
/// Centralizes the bare-basename-fallback logic shared by
/// [`detect_broken_links`] and [`detect_broken_links_from_index`].  The
/// returned `LinkResolution` is the verdict for the *resolved* path so the
/// caller doesn't need to invoke [`classify_link`] a second time (which would
/// double the stat syscalls in the bare-basename path).
///
/// `vault_files` is the flat list of vault-relative paths (used for short-form
/// stem resolution when `case_index` is `None`).
///
/// `expand_short_form` — when `true`, skip Obsidian short-form handling and
/// fall through to regular path-based classification (old behavior, opt-in via
/// `--expand-short-form`).
fn resolve_and_classify_link(
    canonical: &Path,
    source_rel: &str,
    link: &crate::links::Link,
    site_prefix: Option<&str>,
    case_index: Option<&CaseInsensitiveIndex>,
    stem_index: &StemIndex,
    expand_short_form: bool,
) -> (String, LinkResolution) {
    match link.kind {
        LinkKind::Wikilink => {
            // For short-form wikilinks (no `/`), apply Obsidian stem resolution first.
            // This prevents `resolve_target`'s internal stem lookup (inside classify_link)
            // from misidentifying a valid short-form link as a CaseMismatch.
            //
            // Strategy (when !expand_short_form):
            // 1. Try strict path-only check (no case_index) to catch vault-root exact files.
            // 2. If path-only resolves → check for case mismatch via the full classify_link.
            // 3. If path-only fails → use stem classification to determine the correct verdict.
            //
            // When expand_short_form=true: bypass stem classification entirely and use the
            // regular classify_link path, which may expand short-form via stem resolution.
            if !link.target.contains('/') && !link.target.contains('\\') {
                if expand_short_form {
                    // `--expand-short-form` opted into old path-expansion behavior.
                    // Check path-only (no index) so that the internal stem lookup in
                    // `resolve_target` cannot silently turn `[[Corina]]` into
                    // `CaseMismatch("sub/Corina.md")` — we want it to be `Broken`
                    // when `Corina.md` doesn't exist at the vault root, so that
                    // `plan_fixes` can then suggest the full path `[[sub/Corina]]`.
                    let res = classify_link(canonical, &link.target, site_prefix, None);
                    return (link.target.clone(), res);
                }
                // Strategy (when !expand_short_form):
                // 1. Try strict path-only check (no case_index) to catch vault-root exact files.
                // 2. If path-only resolves → check for case mismatch via the full classify_link.
                // 3. If path-only fails → use stem classification to determine the correct verdict.
                let path_only = classify_link(canonical, &link.target, site_prefix, None);
                if let LinkResolution::Resolved(_) = path_only {
                    // File exists at the vault root (exact path). Re-run with full
                    // case_index to detect root-file casing mismatches (e.g. [[corina]]
                    // for vault-root Corina.md) and keep the short form.
                    let full_res = classify_link(canonical, &link.target, site_prefix, case_index);
                    return (link.target.clone(), full_res);
                }
                // Path-only failed → use stem classification.
                if let Some(stem_res) = classify_short_form_wikilink(
                    &link.target,
                    stem_index,
                    case_index,
                    false, // expand_short_form already checked above
                ) {
                    return (link.target.clone(), stem_res);
                }
            }
            // Path-form link or classify_short_form_wikilink returned None (shouldn't
            // happen; it always returns Some when called with expand_short_form=false).
            // Fall through to the regular path-based classification.
            let res = classify_link(canonical, &link.target, site_prefix, case_index);
            (link.target.clone(), res)
        }
        LinkKind::Markdown => {
            if link.target.starts_with('/') {
                let res = classify_link(canonical, &link.target, site_prefix, case_index);
                (link.target.clone(), res)
            } else if link.target.contains('/') || link.target.contains('\\') {
                let target = normalize_target(Path::new(source_rel), &link.target);
                let res = classify_link(canonical, &target, site_prefix, case_index);
                (target, res)
            } else {
                // Bare basename: try source-relative first, fall back to
                // vault-relative on Broken so globally-unique stems still resolve.
                let src_rel = normalize_target(Path::new(source_rel), &link.target);
                let src_resolution = classify_link(canonical, &src_rel, site_prefix, case_index);
                if src_resolution == LinkResolution::Broken {
                    let res = classify_link(canonical, &link.target, site_prefix, case_index);
                    (link.target.clone(), res)
                } else {
                    (src_rel, src_resolution)
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Broken link detection
// ---------------------------------------------------------------------------

/// Detect broken links from pre-collected file links data.
///
/// `file_links` is a slice of [`FileLinks`] (from `link_graph.rs`).
/// Uses [`resolve_target`] to check if each link target exists.
///
/// When `case_index` is provided, links that resolve only via the
/// case-insensitive fallback are surfaced as [`FixStrategy::LinkCaseMismatch`]
/// entries in [`BrokenLinkReport::case_mismatches`] rather than as broken.
///
/// When `expand_short_form` is `true`, short-form wikilinks (no `/`) are NOT
/// given special Obsidian stem resolution — they fall through to path-based
/// classification, which may expand them to full paths.  Default is `false`
/// (Obsidian-compatible short-form handling).
#[allow(dead_code)] // Used in tests only; CLI uses detect_broken_links_from_index
pub(crate) fn detect_broken_links(
    dir: &Path,
    file_links: &[FileLinks],
    site_prefix: Option<&str>,
    case_index: Option<&CaseInsensitiveIndex>,
    expand_short_form: bool,
) -> BrokenLinkReport {
    let Ok(canonical) = canonicalize_vault_dir(dir) else {
        return BrokenLinkReport {
            total_links: 0,
            broken: Vec::new(),
            case_mismatches: Vec::new(),
            ambiguous: Vec::new(),
        };
    };

    // Build a precomputed stem index for short-form stem resolution when no
    // case_index is provided. Built once per call so each lookup is O(1)
    // instead of a full linear scan of the vault per short-form link.
    let vault_files: Vec<String> = file_links
        .iter()
        .map(|fl| fl.source.to_string_lossy().replace('\\', "/"))
        .collect();
    let stem_index = StemIndex::build(&vault_files);

    let mut total_links = 0usize;
    let mut broken: Vec<BrokenLinkInfo> = Vec::new();
    let mut case_mismatches: Vec<FixPlan> = Vec::new();
    let mut ambiguous: Vec<BrokenLinkInfo> = Vec::new();

    for fl in file_links {
        let source_str = fl.source.to_string_lossy().replace('\\', "/");

        for (line, link) in &fl.links {
            total_links += 1;

            let (_resolved_target, resolution) = resolve_and_classify_link(
                &canonical,
                &source_str,
                link,
                site_prefix,
                case_index,
                &stem_index,
                expand_short_form,
            );

            match resolution {
                LinkResolution::Resolved(None) | LinkResolution::ShortFormValid => {}
                LinkResolution::Resolved(Some(canonical_str))
                | LinkResolution::CaseMismatch(canonical_str) => {
                    case_mismatches.push(FixPlan {
                        source: source_str.clone(),
                        line: *line,
                        old_target: link.target.clone(),
                        new_target: canonical_str,
                        strategy: FixStrategy::LinkCaseMismatch,
                        confidence: 1.0,
                    });
                }
                LinkResolution::ShortFormStemMismatch(correct_stem) => {
                    case_mismatches.push(FixPlan {
                        source: source_str.clone(),
                        line: *line,
                        old_target: link.target.clone(),
                        new_target: correct_stem,
                        strategy: FixStrategy::LinkCaseMismatch,
                        confidence: 1.0,
                    });
                }
                LinkResolution::ShortFormAmbiguous => {
                    ambiguous.push(BrokenLinkInfo {
                        source: source_str.clone(),
                        line: *line,
                        target: link.target.clone(),
                    });
                }
                LinkResolution::Broken => {
                    broken.push(BrokenLinkInfo {
                        source: source_str.clone(),
                        line: *line,
                        target: link.target.clone(),
                    });
                }
            }
        }
    }

    broken.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.line.cmp(&b.line)));
    case_mismatches.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.line.cmp(&b.line)));
    ambiguous.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.line.cmp(&b.line)));

    BrokenLinkReport {
        total_links,
        broken,
        case_mismatches,
        ambiguous,
    }
}

/// Detect broken links from index entries.
///
/// Each [`IndexEntry`](crate::index::IndexEntry) has
/// `links: Vec<(usize, Link)>` and `rel_path: String`.
///
/// When `case_index` is provided, links that resolve only via the
/// case-insensitive fallback are surfaced as [`FixStrategy::LinkCaseMismatch`]
/// entries in [`BrokenLinkReport::case_mismatches`] rather than as broken.
///
/// When `expand_short_form` is `true`, short-form wikilinks (no `/`) are NOT
/// given special Obsidian stem resolution — they fall through to path-based
/// classification, which may expand them to full paths.  Default is `false`
/// (Obsidian-compatible short-form handling).
pub fn detect_broken_links_from_index(
    dir: &Path,
    index: &dyn VaultIndex,
    site_prefix: Option<&str>,
    case_index: Option<&CaseInsensitiveIndex>,
    expand_short_form: bool,
) -> BrokenLinkReport {
    let Ok(canonical) = canonicalize_vault_dir(dir) else {
        return BrokenLinkReport {
            total_links: 0,
            broken: Vec::new(),
            case_mismatches: Vec::new(),
            ambiguous: Vec::new(),
        };
    };

    // Build a precomputed stem index for short-form stem resolution when no
    // case_index is provided. Built once per call so each lookup is O(1)
    // instead of a full linear scan of the vault per short-form link.
    let vault_files: Vec<String> = index.entries().iter().map(|e| e.rel_path.clone()).collect();
    let stem_index = StemIndex::build(&vault_files);

    let mut total_links = 0usize;
    let mut broken: Vec<BrokenLinkInfo> = Vec::new();
    let mut case_mismatches: Vec<FixPlan> = Vec::new();
    let mut ambiguous: Vec<BrokenLinkInfo> = Vec::new();

    for entry in index.entries() {
        for (line, link) in &entry.links {
            total_links += 1;

            let (_resolved_target, resolution) = resolve_and_classify_link(
                &canonical,
                &entry.rel_path,
                link,
                site_prefix,
                case_index,
                &stem_index,
                expand_short_form,
            );

            match resolution {
                LinkResolution::Resolved(None) | LinkResolution::ShortFormValid => {}
                LinkResolution::Resolved(Some(canonical_str))
                | LinkResolution::CaseMismatch(canonical_str) => {
                    case_mismatches.push(FixPlan {
                        source: entry.rel_path.clone(),
                        line: *line,
                        old_target: link.target.clone(),
                        new_target: canonical_str,
                        strategy: FixStrategy::LinkCaseMismatch,
                        confidence: 1.0,
                    });
                }
                LinkResolution::ShortFormStemMismatch(correct_stem) => {
                    case_mismatches.push(FixPlan {
                        source: entry.rel_path.clone(),
                        line: *line,
                        old_target: link.target.clone(),
                        new_target: correct_stem,
                        strategy: FixStrategy::LinkCaseMismatch,
                        confidence: 1.0,
                    });
                }
                LinkResolution::ShortFormAmbiguous => {
                    ambiguous.push(BrokenLinkInfo {
                        source: entry.rel_path.clone(),
                        line: *line,
                        target: link.target.clone(),
                    });
                }
                LinkResolution::Broken => {
                    broken.push(BrokenLinkInfo {
                        source: entry.rel_path.clone(),
                        line: *line,
                        target: link.target.clone(),
                    });
                }
            }
        }
    }

    broken.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.line.cmp(&b.line)));
    case_mismatches.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.line.cmp(&b.line)));
    ambiguous.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.line.cmp(&b.line)));

    BrokenLinkReport {
        total_links,
        broken,
        case_mismatches,
        ambiguous,
    }
}

// ---------------------------------------------------------------------------
// Fix planning — candidate matching
// ---------------------------------------------------------------------------

/// Pre-indexed file list for efficient broken link matching.
///
/// Encapsulates the four-strategy matching pipeline:
/// 1. Case-insensitive exact match
/// 2. Extension mismatch (`.md` present/absent)
/// 3. Shortest-path (unique stem match anywhere in vault)
/// 4. Jaro-Winkler fuzzy match
///
/// Build once, then call [`find_match`] for each broken link target.
pub struct LinkMatcher {
    /// All vault-relative file paths (canonical form).
    files: Vec<String>,
    /// Lowercased path → original index into `files`.
    lower_to_idx: HashMap<String, usize>,
    /// Exact-case path → index into `files` (used for O(1) strategy-2 lookup).
    exact_to_idx: HashMap<String, usize>,
    /// Lowercased stem (filename without .md and path) → list of indices.
    /// Used for shortest-path: unique means unambiguous.
    stem_to_indices: HashMap<String, Vec<usize>>,
    /// Minimum Jaro-Winkler score for fuzzy matching.
    threshold: f64,
}

/// Result of a single match attempt.
pub(crate) struct MatchResult {
    /// Vault-relative path of the matched file.
    pub matched_file: String,
    pub strategy: FixStrategy,
    pub confidence: f64,
}

impl LinkMatcher {
    /// Build a matcher from a list of vault-relative file paths.
    pub fn new(files: Vec<String>, threshold: f64) -> Self {
        let mut lower_to_idx = HashMap::with_capacity(files.len());
        let mut exact_to_idx = HashMap::with_capacity(files.len());
        let mut stem_to_indices: HashMap<String, Vec<usize>> = HashMap::new();

        for (i, f) in files.iter().enumerate() {
            // Index by exact path, plus the extension-toggled form.
            exact_to_idx.entry(f.clone()).or_insert(i);
            let alt = if f.to_ascii_lowercase().ends_with(".md") {
                f.strip_suffix(".md")
                    .or_else(|| f.strip_suffix(".MD"))
                    .map(std::string::ToString::to_string)
            } else {
                Some(format!("{f}.md"))
            };
            if let Some(a) = alt {
                exact_to_idx.entry(a).or_insert(i);
            }

            // Index by lowercased full path (with and without .md).
            let lower = f.to_ascii_lowercase();
            lower_to_idx.entry(lower.clone()).or_insert(i);
            if let Some(stem) = lower.strip_suffix(".md") {
                lower_to_idx.entry(stem.to_string()).or_insert(i);
            }

            // Index by lowercased filename stem for shortest-path.
            let fname = f.rsplit('/').next().unwrap_or(f.as_str());
            let fstem = fname.strip_suffix(".md").unwrap_or(fname);
            stem_to_indices
                .entry(fstem.to_ascii_lowercase())
                .or_default()
                .push(i);
        }

        Self {
            files,
            lower_to_idx,
            exact_to_idx,
            stem_to_indices,
            threshold,
        }
    }

    /// Build a matcher from an index (avoids rescanning the directory).
    pub fn from_index(index: &dyn VaultIndex, threshold: f64) -> Self {
        let files: Vec<String> = index.entries().iter().map(|e| e.rel_path.clone()).collect();
        Self::new(files, threshold)
    }

    /// Returns `true` if `candidate` (vault-relative) refers to the same file
    /// as `source`, ignoring `.md` suffix and ASCII case.
    fn is_self_link(source: &str, candidate: &str) -> bool {
        fn strip_md(s: &str) -> &str {
            if s.len() >= 3 && s[s.len() - 3..].eq_ignore_ascii_case(".md") {
                &s[..s.len() - 3]
            } else {
                s
            }
        }
        strip_md(source).eq_ignore_ascii_case(strip_md(candidate))
    }

    /// Try to find a matching file for a broken link target.
    ///
    /// `source` is the vault-relative path of the file that contains the
    /// broken link.  Candidates that resolve back to `source` are skipped so
    /// the matcher never proposes a self-referential fix.
    ///
    /// Returns `None` if no match is found above the configured threshold.
    pub(crate) fn find_match(&self, raw_target: &str, source: &str) -> Option<MatchResult> {
        // Minimum score difference to avoid ambiguous fuzzy matches.
        const TIE_DELTA: f64 = 0.01;

        let target_filename = raw_target.rsplit('/').next().unwrap_or(raw_target);
        let target_stem = target_filename
            .strip_suffix(".md")
            .unwrap_or(target_filename);

        // --- Strategy 1: Case-insensitive exact match ---
        // `target_lower` is also used for the exact-case alt computation below.
        let target_lower = raw_target.to_ascii_lowercase();

        // Precompute the exact-case alt form so strategy 1 doesn't steal strategy 2 hits.
        // Check the .md suffix on the lowercased form to avoid a case-sensitive comparison.
        let exact_alt = if std::path::Path::new(&target_lower)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        {
            // Strip the original suffix (preserving the non-suffix casing).
            raw_target[..raw_target.len() - 3].to_string()
        } else {
            format!("{raw_target}.md")
        };

        if let Some(&idx) = self.lower_to_idx.get(&target_lower) {
            let candidate = &self.files[idx];
            // Only report as case-insensitive if it's not an exact-case extension mismatch
            // and not the source file itself.
            if *candidate != exact_alt && !Self::is_self_link(source, candidate) {
                return Some(MatchResult {
                    matched_file: candidate.clone(),
                    strategy: FixStrategy::CaseInsensitive,
                    confidence: 1.0,
                });
            }
        }

        // --- Strategy 2: Extension mismatch (exact case, only extension differs) ---
        // Use the pre-built exact_to_idx for O(1) lookup instead of a linear scan.
        if let Some(&idx) = self.exact_to_idx.get(&exact_alt)
            && !Self::is_self_link(source, &self.files[idx])
        {
            return Some(MatchResult {
                matched_file: self.files[idx].clone(),
                strategy: FixStrategy::ExtensionMismatch,
                confidence: 1.0,
            });
        }

        // --- Strategy 3: Shortest-path (unique stem match) ---
        let target_stem_lower = target_stem.to_ascii_lowercase();
        if let Some(indices) = self.stem_to_indices.get(&target_stem_lower)
            && indices.len() == 1
            && !Self::is_self_link(source, &self.files[indices[0]])
        {
            return Some(MatchResult {
                matched_file: self.files[indices[0]].clone(),
                strategy: FixStrategy::ShortestPath,
                confidence: 0.95,
            });
        }

        // --- Strategy 4: Fuzzy match (Jaro-Winkler on filename stem) ---
        // Track the top-two scores to detect ties: if two candidates score within
        // TIE_DELTA of each other the match is ambiguous and we return None rather
        // than silently picking the first.
        let mut best_score = self.threshold;
        let mut second_score = 0.0_f64;
        let mut best_idx: Option<usize> = None;

        for (i, candidate) in self.files.iter().enumerate() {
            if Self::is_self_link(source, candidate) {
                continue;
            }
            let fname = candidate.rsplit('/').next().unwrap_or(candidate.as_str());
            let fstem = fname.strip_suffix(".md").unwrap_or(fname);
            let score = strsim::jaro_winkler(target_stem, fstem);
            if score > best_score {
                second_score = best_score;
                best_score = score;
                best_idx = Some(i);
            } else if score > second_score {
                second_score = score;
            }
        }

        // If the runner-up is within TIE_DELTA of the winner the match is
        // ambiguous — decline rather than guessing.
        if best_score - second_score <= TIE_DELTA {
            return None;
        }

        best_idx.map(|idx| MatchResult {
            matched_file: self.files[idx].clone(),
            strategy: FixStrategy::FuzzyMatch,
            confidence: best_score,
        })
    }
}

/// Plan fixes for broken links.
///
/// For each broken link, attempts to find the best matching file using
/// the [`LinkMatcher`] priority-ordered strategy.
///
/// `threshold` is the minimum Jaro-Winkler score (0.0–1.0) for fuzzy matching.
pub fn plan_fixes(broken: &[BrokenLinkInfo], matcher: &LinkMatcher) -> FixReport {
    let mut fixes = Vec::new();
    let mut unfixable = Vec::new();

    for info in broken {
        if let Some(result) = matcher.find_match(&info.target, &info.source) {
            fixes.push(FixPlan {
                source: info.source.clone(),
                line: info.line,
                old_target: info.target.clone(),
                new_target: result.matched_file,
                strategy: result.strategy,
                confidence: result.confidence,
            });
        } else {
            unfixable.push(info.clone());
        }
    }

    FixReport { fixes, unfixable }
}

// ---------------------------------------------------------------------------
// Fix application
// ---------------------------------------------------------------------------

/// Convert fix plans to [`RewritePlan`]s and apply them to disk.
///
/// Groups fixes by source file, reads each file once, builds [`Replacement`]s
/// for every fix in that file (both body links and frontmatter link-property
/// wikilinks), applies them via [`apply_replacements`], and writes back via
/// [`execute_plans`].
///
/// Returns `(plans, unapplied)` where `plans` are the [`RewritePlan`]s
/// actually written to disk, and `unapplied` lists the input [`FixPlan`]s
/// that produced no [`Replacement`] (e.g. because the on-disk text no longer
/// matches what detection saw). Callers should treat `unapplied` fixes as
/// NOT applied when reporting results — do not assume every input fix landed.
pub fn apply_fixes(
    dir: &Path,
    fixes: &[FixPlan],
    site_prefix: Option<&str>,
) -> Result<(Vec<RewritePlan>, Vec<FixPlan>)> {
    // Group fixes by source file.
    let mut by_source: HashMap<&str, Vec<&FixPlan>> = HashMap::new();
    for fix in fixes {
        by_source.entry(fix.source.as_str()).or_default().push(fix);
    }

    let mut plans: Vec<RewritePlan> = Vec::new();
    let mut unapplied: Vec<FixPlan> = Vec::new();

    for (source_rel, file_fixes) in &by_source {
        let abs_path = dir.join(source_rel.replace('\\', "/"));
        let meta = std::fs::metadata(&abs_path)
            .with_context(|| format!("failed to stat {}", abs_path.display()))?;
        let file_size = meta.len();
        if file_size > MAX_FILE_SIZE {
            eprintln!(
                "warning: skipping {} ({} MiB exceeds {} MiB limit)",
                abs_path.display(),
                file_size / (1024 * 1024),
                MAX_FILE_SIZE / (1024 * 1024)
            );
            unapplied.extend(file_fixes.iter().map(|f| (*f).clone()));
            continue;
        }
        let file_mtime = meta
            .modified()
            .with_context(|| format!("failed to read mtime for {}", abs_path.display()))
            .map(|t| (t, file_size))
            .ok();
        let content = std::fs::read_to_string(&abs_path)
            .with_context(|| format!("reading {}", abs_path.display()))?;

        let (replacements, satisfied) =
            build_replacements_for_file(&content, source_rel, file_fixes, site_prefix);

        for (idx, fix) in file_fixes.iter().enumerate() {
            if !satisfied.contains(&idx) {
                unapplied.push((*fix).clone());
            }
        }

        if !replacements.is_empty() {
            let rewritten_content = apply_replacements(&content, &replacements);
            plans.push(RewritePlan {
                path: abs_path,
                rel_path: source_rel.to_string(),
                replacements,
                rewritten_content,
                mtime: file_mtime,
            });
        }
    }

    execute_plans(dir, &plans)?;

    Ok((plans, unapplied))
}

/// Walk `content` line by line and build [`Replacement`]s for all link fixes
/// that apply to this file — both `[[wikilink]]`s inside YAML frontmatter
/// link properties and links in the document body (code fences and Obsidian
/// comment fences are skipped for the latter).
///
/// Returns `(replacements, satisfied)` where `satisfied` holds the indices
/// (into `fixes`) of plans that were matched to an on-disk occurrence and
/// rewritten. Tracking is per-occurrence: each on-disk match consumes the
/// first not-yet-satisfied plan with that target, so duplicate plans for the
/// same `(line, old_target)` — a legitimate case when the same broken target
/// appears twice — are only satisfied by distinct occurrences. Callers use
/// the unsatisfied remainder to detect fixes whose on-disk text no longer
/// matches what detection saw (stale plan) so they are never misreported as
/// applied.
fn build_replacements_for_file(
    content: &str,
    source_rel: &str,
    fixes: &[&FixPlan],
    _site_prefix: Option<&str>,
) -> (Vec<Replacement>, std::collections::HashSet<usize>) {
    // Index fixes by line number for O(1) lookup during the scan, carrying
    // each plan's index into `fixes` for per-occurrence satisfaction
    // tracking.
    let mut fixes_by_line: HashMap<usize, Vec<(usize, &FixPlan)>> = HashMap::new();
    for (idx, fix) in fixes.iter().enumerate() {
        fixes_by_line.entry(fix.line).or_default().push((idx, fix));
    }

    let mut replacements = Vec::new();
    let mut satisfied: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut fence = FenceTracker::new();
    let mut in_comment_fence = false;
    let mut in_frontmatter = false;
    let mut frontmatter_done = false;
    let mut line_num = 0usize;

    // Frontmatter-derived FixPlans always carry `line: 1` (see
    // `LinkGraphVisitor::extract_frontmatter_wikilinks`, which has no
    // meaningful per-line info once YAML is parsed into a `Value`). Look
    // them up once and match by `old_target` against every `[[...]]`
    // occurrence anywhere in the frontmatter block, regardless of which
    // physical line it sits on.
    let frontmatter_fixes: &[(usize, &FixPlan)] = fixes_by_line.get(&1).map_or(&[], Vec::as_slice);

    for line in content.split('\n') {
        line_num += 1;

        // --- Frontmatter ---
        if !frontmatter_done {
            // BOM-aware: detection (scanner) enters frontmatter via the same
            // predicate, so the rewrite path must too or BOM-prefixed files
            // silently keep their broken frontmatter links.
            if line_num == 1 && crate::frontmatter::is_opening_delimiter(line) {
                in_frontmatter = true;
                continue;
            }
            if in_frontmatter {
                if line.trim() == "---" {
                    in_frontmatter = false;
                    frontmatter_done = true;
                    continue;
                }
                if !frontmatter_fixes.is_empty() {
                    for occ in find_frontmatter_wikilinks(line) {
                        let Some(link) = parse_wikilink(occ.target) else {
                            continue;
                        };
                        // Prefer a not-yet-satisfied plan so duplicate plans
                        // for the same target are consumed one occurrence
                        // each; fall back to an already-satisfied one so
                        // extra on-disk occurrences still get rewritten.
                        let matching = || {
                            frontmatter_fixes
                                .iter()
                                .filter(|(_, f)| f.old_target == link.target)
                        };
                        let Some(&(fix_idx, fix)) = matching()
                            .find(|(idx, _)| !satisfied.contains(idx))
                            .or_else(|| matching().next())
                        else {
                            continue;
                        };

                        // Preserve alias (`path|Label`) and written form
                        // (path-form vs bare stem), mirroring `mv`'s
                        // frontmatter rewriter.
                        let alias_suffix = occ.target.find('|').map_or("", |i| &occ.target[i..]);
                        let new_stem = fix
                            .new_target
                            .strip_suffix(".md")
                            .unwrap_or(&fix.new_target);
                        let new_wikilink_target =
                            if link.target.contains('/') || link.target.contains('\\') {
                                new_stem.to_string()
                            } else {
                                new_stem.rsplit('/').next().unwrap_or(new_stem).to_string()
                            };

                        let old_text = line[occ.full_start..occ.full_end].to_string();
                        let new_text = format!("[[{new_wikilink_target}{alias_suffix}]]");
                        if old_text != new_text {
                            replacements.push(Replacement {
                                line: line_num,
                                byte_offset: occ.full_start,
                                old_text,
                                new_text,
                            });
                        }
                        satisfied.insert(fix_idx);
                    }
                }
                continue;
            }
            frontmatter_done = true;
        }

        // --- Comment fence (Obsidian %% blocks) ---
        if is_comment_fence(line) {
            in_comment_fence = !in_comment_fence;
            continue;
        }
        if in_comment_fence {
            continue;
        }

        // --- Fenced code block ---
        if fence.process_line(line) {
            continue;
        }

        // If there are no fixes on this line, skip expensive span extraction.
        let Some(line_fixes) = fixes_by_line.get(&line_num) else {
            continue;
        };

        // Extract link spans (skipping inline code and comments).
        let stripped_code = strip_inline_code(line);
        let cleaned = strip_inline_comments(stripped_code.as_ref());
        let spans = extract_link_spans_with_original(&cleaned, line);

        for span in &spans {
            // Normalize the span's target the same way detection does, so we
            // can match it against each fix's old_target.
            let normalized_span_target = match span.kind {
                LinkKind::Wikilink => span.link.target.clone(),
                LinkKind::Markdown => {
                    if span.link.target.starts_with('/') {
                        span.link.target.clone()
                    } else if span.link.target.contains('/') || span.link.target.contains('\\') {
                        normalize_target(Path::new(source_rel), &span.link.target)
                    } else {
                        span.link.target.clone()
                    }
                }
            };

            // Find the fix for this particular span, preferring a
            // not-yet-satisfied plan (duplicate plans for the same target are
            // consumed one occurrence each) and falling back to an
            // already-satisfied one so extra occurrences still get rewritten.
            let matching = || {
                line_fixes.iter().filter(|(_, f)| {
                    f.old_target == normalized_span_target || f.old_target == span.link.target
                })
            };
            let Some(&(fix_idx, fix)) = matching()
                .find(|(idx, _)| !satisfied.contains(idx))
                .or_else(|| matching().next())
            else {
                continue;
            };

            // Compute new target text based on link kind.
            let new_target_text = match span.kind {
                LinkKind::Wikilink => {
                    // Use stem (without .md) for wikilinks.
                    fix.new_target
                        .strip_suffix(".md")
                        .unwrap_or(&fix.new_target)
                        .to_string()
                }
                LinkKind::Markdown => {
                    // Preserve the original `.md` presence/absence in the link.
                    // If the original target had no `.md` suffix, strip it from
                    // the new target too so the style is unchanged.
                    let orig_had_md = fix.old_target.to_ascii_lowercase().ends_with(".md");
                    if orig_had_md {
                        fix.new_target.clone()
                    } else {
                        fix.new_target
                            .strip_suffix(".md")
                            .unwrap_or(&fix.new_target)
                            .to_string()
                    }
                }
            };

            // Build old_text / new_text from the ORIGINAL line bytes.
            let old_text = line[span.full_start..span.full_end].to_string();
            let new_text = format!(
                "{}{}{}",
                &line[span.full_start..span.target_start],
                new_target_text,
                &line[span.target_end..span.full_end],
            );

            if old_text != new_text {
                replacements.push(Replacement {
                    line: line_num,
                    byte_offset: span.full_start,
                    old_text,
                    new_text,
                });
            }
            satisfied.insert(fix_idx);
        }
    }

    (replacements, satisfied)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    // --- Fuzzy matching helpers ---

    fn make_files(names: &[&str]) -> Vec<String> {
        names.iter().map(std::string::ToString::to_string).collect()
    }

    fn broken(source: &str, line: usize, target: &str) -> BrokenLinkInfo {
        BrokenLinkInfo {
            source: source.to_string(),
            line,
            target: target.to_string(),
        }
    }

    fn vault_with_files(files: &[(&str, &str)]) -> TempDir {
        let dir = TempDir::new().unwrap();
        for (rel, content) in files {
            let path = dir
                .path()
                .join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, content).unwrap();
        }
        dir
    }

    // --- LinkMatcher unit tests ---

    #[test]
    fn matcher_case_insensitive() {
        let matcher = LinkMatcher::new(make_files(&["Auth.md"]), 0.8);
        let result = matcher.find_match("auth", "__test__").unwrap();
        assert_eq!(result.matched_file, "Auth.md");
        assert!(matches!(result.strategy, FixStrategy::CaseInsensitive));
        assert!((result.confidence - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn matcher_extension_mismatch_add_md() {
        let matcher = LinkMatcher::new(make_files(&["notes/foo.md"]), 0.8);
        let result = matcher.find_match("notes/foo", "__test__").unwrap();
        assert_eq!(result.matched_file, "notes/foo.md");
        assert!(matches!(result.strategy, FixStrategy::ExtensionMismatch));
    }

    #[test]
    fn matcher_extension_mismatch_strip_md() {
        let matcher = LinkMatcher::new(make_files(&["foo"]), 0.8);
        let result = matcher.find_match("foo.md", "__test__").unwrap();
        assert_eq!(result.matched_file, "foo");
        assert!(matches!(result.strategy, FixStrategy::ExtensionMismatch));
    }

    #[test]
    fn matcher_shortest_path_unique_stem() {
        let matcher = LinkMatcher::new(make_files(&["sub/deep/bar.md"]), 0.8);
        let result = matcher.find_match("bar", "__test__").unwrap();
        assert_eq!(result.matched_file, "sub/deep/bar.md");
        assert!(matches!(result.strategy, FixStrategy::ShortestPath));
        assert!((result.confidence - 0.95).abs() < f64::EPSILON);
    }

    #[test]
    fn matcher_shortest_path_ambiguous_skipped() {
        let matcher = LinkMatcher::new(make_files(&["a/bar.md", "b/bar.md"]), 0.99);
        let result = matcher.find_match("bar", "__test__");
        // Both stem-match so shortest-path doesn't fire; fuzzy threshold is
        // very high (0.99) but "bar" vs "bar" scores 1.0, so fuzzy wins.
        if let Some(r) = result {
            assert!(!matches!(r.strategy, FixStrategy::ShortestPath));
        }
    }

    #[test]
    fn matcher_fuzzy_match() {
        let matcher = LinkMatcher::new(make_files(&["authentication.md"]), 0.7);
        // "authentcation" is a typo of "authentication"
        let result = matcher.find_match("authentcation", "__test__").unwrap();
        assert_eq!(result.matched_file, "authentication.md");
        assert!(matches!(result.strategy, FixStrategy::FuzzyMatch));
        assert!(result.confidence >= 0.7);
    }

    #[test]
    fn matcher_no_match() {
        let matcher = LinkMatcher::new(make_files(&["completely-unrelated.md"]), 0.95);
        assert!(matcher.find_match("xyz-abc-notexist", "__test__").is_none());
    }

    // --- Self-link guard ---

    #[test]
    fn matcher_rejects_self_link_fuzzy() {
        // When the only fuzzy candidate is the source file itself, return None.
        let matcher = LinkMatcher::new(make_files(&["sort-by-property-value.md"]), 0.7);
        assert!(
            matcher
                .find_match("sort-reverse", "sort-by-property-value.md")
                .is_none(),
            "should not match source file via fuzzy"
        );
    }

    #[test]
    fn matcher_rejects_self_link_picks_next_best() {
        // When the best fuzzy candidate is the source, the runner-up should win.
        let matcher = LinkMatcher::new(
            make_files(&["sort-by-property-value.md", "sort-reverse.md"]),
            0.7,
        );
        let result = matcher
            .find_match("sort-reverse", "sort-by-property-value.md")
            .unwrap();
        assert_eq!(result.matched_file, "sort-reverse.md");
    }

    #[test]
    fn matcher_rejects_self_link_case_insensitive() {
        // The only case-insensitive match is the source file — should return None.
        let matcher = LinkMatcher::new(make_files(&["Auth.md"]), 0.8);
        assert!(matcher.find_match("auth", "Auth.md").is_none());
    }

    #[test]
    fn matcher_rejects_self_link_extension_mismatch() {
        // Source without .md suffix; only candidate is the .md form — should be blocked.
        let matcher = LinkMatcher::new(make_files(&["notes/foo.md"]), 0.8);
        assert!(matcher.find_match("notes/foo.md", "notes/foo").is_none());
    }

    #[test]
    fn matcher_rejects_self_link_shortest_path() {
        // Unique stem match that resolves to the source file — should return None.
        let matcher = LinkMatcher::new(make_files(&["sub/bar.md"]), 0.8);
        assert!(matcher.find_match("bar", "sub/bar.md").is_none());
    }

    #[test]
    fn matcher_self_link_among_ambiguous_stems_picks_other() {
        // Two files share a stem; source is one of them — matcher should pick the other.
        let matcher = LinkMatcher::new(make_files(&["a/bar.md", "b/bar.md"]), 0.8);
        let result = matcher.find_match("bar", "a/bar.md").unwrap();
        assert_eq!(result.matched_file, "b/bar.md");
    }

    #[test]
    fn plan_fixes_self_link_is_unfixable() {
        let matcher = LinkMatcher::new(make_files(&["sort-by-property-value.md"]), 0.7);
        let broken_links = vec![broken("sort-by-property-value.md", 10, "sort-reverse")];
        let report = plan_fixes(&broken_links, &matcher);
        assert!(report.fixes.is_empty(), "self-link should not be a fix");
        assert_eq!(report.unfixable.len(), 1);
    }

    // --- plan_fixes integration ---

    #[test]
    fn plan_fixes_produces_fix_and_unfixable() {
        let matcher = LinkMatcher::new(make_files(&["Auth.md"]), 0.95);
        let broken_links = vec![
            broken("index.md", 1, "auth"),
            broken("index.md", 5, "totally-nonexistent"),
        ];
        let report = plan_fixes(&broken_links, &matcher);
        assert_eq!(report.fixes.len(), 1);
        assert_eq!(report.fixes[0].new_target, "Auth.md");
        assert_eq!(report.unfixable.len(), 1);
    }

    // --- detect_broken_links: basic ---

    #[test]
    fn detect_broken_links_finds_missing() {
        use crate::link_graph::FileLinks;
        use crate::links::{Link, LinkKind};

        let tmp = vault_with_files(&[("index.md", "[[existing]]"), ("existing.md", "")]);

        let file_links = vec![FileLinks {
            source: PathBuf::from("index.md"),
            links: vec![
                (
                    1,
                    Link {
                        target: "existing".to_string(),
                        label: None,
                        kind: LinkKind::Wikilink,
                    },
                ),
                (
                    2,
                    Link {
                        target: "missing".to_string(),
                        label: None,
                        kind: LinkKind::Wikilink,
                    },
                ),
            ],
        }];

        let report = detect_broken_links(tmp.path(), &file_links, None, None, false);

        assert_eq!(report.total_links, 2);
        assert_eq!(report.broken.len(), 1);
        assert_eq!(report.broken[0].target, "missing");
    }

    // --- detect_broken_links: sorted output ---

    #[test]
    fn detect_broken_links_sorted() {
        use crate::link_graph::FileLinks;
        use crate::links::{Link, LinkKind};

        let tmp = vault_with_files(&[("a.md", ""), ("b.md", "")]);

        let file_links = vec![
            FileLinks {
                source: PathBuf::from("b.md"),
                links: vec![(
                    3,
                    Link {
                        target: "gone".to_string(),
                        label: None,
                        kind: LinkKind::Wikilink,
                    },
                )],
            },
            FileLinks {
                source: PathBuf::from("a.md"),
                links: vec![
                    (
                        5,
                        Link {
                            target: "also-gone".to_string(),
                            label: None,
                            kind: LinkKind::Wikilink,
                        },
                    ),
                    (
                        1,
                        Link {
                            target: "nope".to_string(),
                            label: None,
                            kind: LinkKind::Wikilink,
                        },
                    ),
                ],
            },
        ];

        let report = detect_broken_links(tmp.path(), &file_links, None, None, false);

        assert_eq!(report.broken.len(), 3);
        // Sorted by (source, line)
        assert_eq!(report.broken[0].source, "a.md");
        assert_eq!(report.broken[0].line, 1);
        assert_eq!(report.broken[1].source, "a.md");
        assert_eq!(report.broken[1].line, 5);
        assert_eq!(report.broken[2].source, "b.md");
        assert_eq!(report.broken[2].line, 3);
    }

    // --- apply_fixes: wikilink rewrite ---

    #[test]
    fn apply_fixes_rewrites_wikilink() {
        let tmp = vault_with_files(&[
            ("index.md", "See [[wrongname]] for details.\n"),
            ("correct-name.md", ""),
        ]);

        let fixes = vec![FixPlan {
            source: "index.md".to_string(),
            line: 1,
            old_target: "wrongname".to_string(),
            new_target: "correct-name.md".to_string(),
            strategy: FixStrategy::FuzzyMatch,
            confidence: 0.9,
        }];

        let (plans, unapplied) = apply_fixes(tmp.path(), &fixes, None).unwrap();

        assert_eq!(plans.len(), 1);
        assert!(
            unapplied.is_empty(),
            "expected no unapplied fixes: {unapplied:?}"
        );
        let written = fs::read_to_string(tmp.path().join("index.md")).unwrap();
        assert!(
            written.contains("[[correct-name]]"),
            "expected wikilink stem, got: {written}"
        );
    }

    // --- apply_fixes: markdown link rewrite ---

    #[test]
    fn apply_fixes_rewrites_markdown_link() {
        let tmp = vault_with_files(&[
            ("index.md", "See [text](wrong.md) for details.\n"),
            ("correct.md", ""),
        ]);

        let fixes = vec![FixPlan {
            source: "index.md".to_string(),
            line: 1,
            old_target: "wrong.md".to_string(),
            new_target: "correct.md".to_string(),
            strategy: FixStrategy::CaseInsensitive,
            confidence: 1.0,
        }];

        let (plans, unapplied) = apply_fixes(tmp.path(), &fixes, None).unwrap();

        assert_eq!(plans.len(), 1);
        assert!(
            unapplied.is_empty(),
            "expected no unapplied fixes: {unapplied:?}"
        );
        let written = fs::read_to_string(tmp.path().join("index.md")).unwrap();
        assert!(
            written.contains("[text](correct.md)"),
            "expected rewritten link, got: {written}"
        );
    }

    // --- apply_fixes: frontmatter wikilink rewrite (H-bug: frontmatter fixes
    // were silently no-op'd — see iteration-160 fix) ---

    #[test]
    fn apply_fixes_rewrites_frontmatter_only_wikilink() {
        let tmp = vault_with_files(&[
            (
                "a.md",
                "---\ntitle: A\nrelated: [\"[[wrong/real-target]]\"]\n---\nBody.\n",
            ),
            ("sub/real-target.md", "Content\n"),
        ]);

        let fixes = vec![FixPlan {
            source: "a.md".to_string(),
            line: 1,
            old_target: "wrong/real-target".to_string(),
            new_target: "sub/real-target.md".to_string(),
            strategy: FixStrategy::ShortestPath,
            confidence: 0.95,
        }];

        let (plans, unapplied) = apply_fixes(tmp.path(), &fixes, None).unwrap();

        assert_eq!(plans.len(), 1, "frontmatter fix must produce a RewritePlan");
        assert!(
            unapplied.is_empty(),
            "expected no unapplied fixes: {unapplied:?}"
        );
        let written = fs::read_to_string(tmp.path().join("a.md")).unwrap();
        assert!(
            written.contains("[[sub/real-target]]"),
            "frontmatter wikilink was not rewritten, got: {written}"
        );
        assert!(!written.contains("wrong/real-target"), "got: {written}");
    }

    #[test]
    fn apply_fixes_rewrites_body_only_wikilink_line_one() {
        // Regression guard: when the fix is on physical line 1 but there is
        // NO frontmatter block, the body-link scan must still run — the
        // frontmatter-lookup-by-line-1 shortcut must not swallow body fixes.
        let tmp = vault_with_files(&[
            ("a.md", "See [[wrong/real-target]] here.\n"),
            ("sub/real-target.md", "Content\n"),
        ]);

        let fixes = vec![FixPlan {
            source: "a.md".to_string(),
            line: 1,
            old_target: "wrong/real-target".to_string(),
            new_target: "sub/real-target.md".to_string(),
            strategy: FixStrategy::ShortestPath,
            confidence: 0.95,
        }];

        let (plans, unapplied) = apply_fixes(tmp.path(), &fixes, None).unwrap();

        assert_eq!(plans.len(), 1);
        assert!(
            unapplied.is_empty(),
            "expected no unapplied fixes: {unapplied:?}"
        );
        let written = fs::read_to_string(tmp.path().join("a.md")).unwrap();
        assert!(written.contains("[[sub/real-target]]"), "got: {written}");
    }

    #[test]
    fn apply_fixes_rewrites_frontmatter_and_body_both_occurrences() {
        // The exact bug report repro: same broken target in both frontmatter
        // `related:` and the body. Both must be rewritten and both must be
        // reported (no dedup collapsing the two).
        let tmp = vault_with_files(&[
            (
                "a.md",
                "---\ntitle: A\nrelated: [\"[[wrong/real-target]]\"]\n---\nBody also links [[wrong/real-target]].\n",
            ),
            ("sub/real-target.md", "Content\n"),
        ]);

        let fixes = vec![
            FixPlan {
                source: "a.md".to_string(),
                line: 1,
                old_target: "wrong/real-target".to_string(),
                new_target: "sub/real-target.md".to_string(),
                strategy: FixStrategy::ShortestPath,
                confidence: 0.95,
            },
            FixPlan {
                source: "a.md".to_string(),
                line: 5,
                old_target: "wrong/real-target".to_string(),
                new_target: "sub/real-target.md".to_string(),
                strategy: FixStrategy::ShortestPath,
                confidence: 0.95,
            },
        ];

        let (plans, unapplied) = apply_fixes(tmp.path(), &fixes, None).unwrap();

        assert_eq!(plans.len(), 1);
        assert!(
            unapplied.is_empty(),
            "expected no unapplied fixes: {unapplied:?}"
        );
        assert_eq!(
            plans[0].replacements.len(),
            2,
            "both frontmatter and body occurrences must be rewritten: {:?}",
            plans[0].replacements
        );
        let written = fs::read_to_string(tmp.path().join("a.md")).unwrap();
        assert!(!written.contains("wrong/real-target"), "got: {written}");
        assert_eq!(
            written.matches("[[sub/real-target]]").count(),
            2,
            "got: {written}"
        );
    }

    #[test]
    fn apply_fixes_frontmatter_block_list_form() {
        // YAML block-list form (not inline flow-sequence):
        //   related:
        //     - "[[wrong/target]]"
        let tmp = vault_with_files(&[
            (
                "a.md",
                "---\ntitle: A\nrelated:\n  - \"[[wrong/target]]\"\n---\nBody.\n",
            ),
            ("target.md", "Content\n"),
        ]);

        let fixes = vec![FixPlan {
            source: "a.md".to_string(),
            line: 1,
            old_target: "wrong/target".to_string(),
            new_target: "target.md".to_string(),
            strategy: FixStrategy::ShortestPath,
            confidence: 0.95,
        }];

        let (plans, unapplied) = apply_fixes(tmp.path(), &fixes, None).unwrap();

        assert_eq!(plans.len(), 1);
        assert!(
            unapplied.is_empty(),
            "expected no unapplied fixes: {unapplied:?}"
        );
        let written = fs::read_to_string(tmp.path().join("a.md")).unwrap();
        assert!(written.contains("[[target]]"), "got: {written}");
    }

    #[test]
    fn apply_fixes_frontmatter_wikilink_alias_preserved() {
        let tmp = vault_with_files(&[
            (
                "a.md",
                "---\ntitle: A\nrelated: [\"[[wrong/target|My Label]]\"]\n---\nBody.\n",
            ),
            ("target.md", "Content\n"),
        ]);

        let fixes = vec![FixPlan {
            source: "a.md".to_string(),
            line: 1,
            old_target: "wrong/target".to_string(),
            new_target: "target.md".to_string(),
            strategy: FixStrategy::ShortestPath,
            confidence: 0.95,
        }];

        let (plans, unapplied) = apply_fixes(tmp.path(), &fixes, None).unwrap();

        assert_eq!(plans.len(), 1);
        assert!(
            unapplied.is_empty(),
            "expected no unapplied fixes: {unapplied:?}"
        );
        let written = fs::read_to_string(tmp.path().join("a.md")).unwrap();
        assert!(
            written.contains("[[target|My Label]]"),
            "alias must be preserved, got: {written}"
        );
    }

    #[test]
    fn apply_fixes_reports_unapplied_when_target_not_found() {
        // A FixPlan whose old_target text is not actually present on disk
        // (e.g. stale plan from a concurrently-edited file) must be reported
        // as unapplied rather than silently counted as applied.
        let tmp = vault_with_files(&[
            ("a.md", "No matching link here.\n"),
            ("target.md", "Content\n"),
        ]);

        let fixes = vec![FixPlan {
            source: "a.md".to_string(),
            line: 1,
            old_target: "stale/target".to_string(),
            new_target: "target.md".to_string(),
            strategy: FixStrategy::ShortestPath,
            confidence: 0.95,
        }];

        let (plans, unapplied) = apply_fixes(tmp.path(), &fixes, None).unwrap();

        assert!(plans.is_empty(), "no replacement should have been produced");
        assert_eq!(unapplied.len(), 1);
        assert_eq!(unapplied[0].old_target, "stale/target");
    }

    #[test]
    fn apply_fixes_duplicate_plans_single_occurrence_reports_one_unapplied() {
        // Two FixPlans with identical (line, old_target) — e.g. detection saw
        // two occurrences but a concurrent edit removed one — must consume
        // distinct on-disk occurrences. With only one occurrence on disk,
        // exactly one plan is satisfied and the other is unapplied; keying
        // satisfaction on (line, old_target) instead of plan identity would
        // silently absorb the second plan.
        let tmp = vault_with_files(&[
            (
                "a.md",
                "---\ntitle: a\nrelated: [\"[[wrong/target]]\"]\n---\nBody.\n",
            ),
            ("sub/target.md", "Content\n"),
        ]);

        let plan = FixPlan {
            source: "a.md".to_string(),
            line: 1,
            old_target: "wrong/target".to_string(),
            new_target: "sub/target.md".to_string(),
            strategy: FixStrategy::ShortestPath,
            confidence: 0.95,
        };
        let fixes = vec![plan.clone(), plan];

        let (plans, unapplied) = apply_fixes(tmp.path(), &fixes, None).unwrap();

        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].replacements.len(), 1);
        assert_eq!(
            unapplied.len(),
            1,
            "second duplicate plan had no occurrence to consume and must be unapplied"
        );
        let written = fs::read_to_string(tmp.path().join("a.md")).unwrap();
        assert!(written.contains("[[sub/target]]"));
    }

    #[test]
    fn apply_fixes_rewrites_frontmatter_wikilink_in_bom_file() {
        // A UTF-8 BOM before the opening `---` must not disable the
        // frontmatter rewrite path — the scanner (detection side) is
        // BOM-aware, so the write path has to be too.
        let tmp = vault_with_files(&[
            (
                "a.md",
                "\u{feff}---\ntitle: a\nrelated: [\"[[wrong/target]]\"]\n---\nBody.\n",
            ),
            ("sub/target.md", "Content\n"),
        ]);

        let fixes = vec![FixPlan {
            source: "a.md".to_string(),
            line: 1,
            old_target: "wrong/target".to_string(),
            new_target: "sub/target.md".to_string(),
            strategy: FixStrategy::ShortestPath,
            confidence: 0.95,
        }];

        let (plans, unapplied) = apply_fixes(tmp.path(), &fixes, None).unwrap();

        assert_eq!(plans.len(), 1);
        assert!(unapplied.is_empty(), "unexpected unapplied: {unapplied:?}");
        let written = fs::read_to_string(tmp.path().join("a.md")).unwrap();
        assert!(
            written.starts_with('\u{feff}'),
            "BOM must be preserved through the rewrite"
        );
        assert!(written.contains("[[sub/target]]"));
    }

    #[test]
    fn apply_fixes_duplicate_plans_two_occurrences_both_satisfied() {
        // Two plans, two on-disk occurrences of the same broken target in the
        // frontmatter block: each occurrence consumes one plan, both are
        // rewritten, nothing is unapplied.
        let tmp = vault_with_files(&[
            (
                "a.md",
                "---\ntitle: a\nrelated: [\"[[wrong/target]]\", \"[[wrong/target]]\"]\n---\nBody.\n",
            ),
            ("sub/target.md", "Content\n"),
        ]);

        let plan = FixPlan {
            source: "a.md".to_string(),
            line: 1,
            old_target: "wrong/target".to_string(),
            new_target: "sub/target.md".to_string(),
            strategy: FixStrategy::ShortestPath,
            confidence: 0.95,
        };
        let fixes = vec![plan.clone(), plan];

        let (plans, unapplied) = apply_fixes(tmp.path(), &fixes, None).unwrap();

        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].replacements.len(), 2);
        assert!(
            unapplied.is_empty(),
            "both plans consumed by distinct occurrences: {unapplied:?}"
        );
        let written = fs::read_to_string(tmp.path().join("a.md")).unwrap();
        assert_eq!(written.matches("[[sub/target]]").count(), 2);
    }

    // ---------------------------------------------------------------------------
    // Case-mismatch detection tests
    // ---------------------------------------------------------------------------

    #[test]
    fn detect_broken_links_emits_case_mismatch_with_index() {
        use crate::case_index::CaseInsensitiveIndex;
        use crate::link_graph::FileLinks;
        use crate::links::{Link, LinkKind};

        // On-disk: `web/foo.md` (lowercase). Link written as `Web/Foo` (PascalCase).
        let tmp = vault_with_files(&[("web/foo.md", ""), ("source.md", "[[Web/Foo]]")]);

        // Build a case index containing the real path.
        let mut idx = CaseInsensitiveIndex::new();
        idx.insert("web/foo.md");

        let file_links = vec![FileLinks {
            source: PathBuf::from("source.md"),
            links: vec![(
                1,
                Link {
                    target: "Web/Foo".to_string(),
                    label: None,
                    kind: LinkKind::Wikilink,
                },
            )],
        }];

        // Without index: case_mismatches is always empty regardless of FS type.
        // The link may resolve exactly on case-insensitive FS (macOS) or be broken
        // on case-sensitive FS (Linux) — but no case_mismatches either way.
        let report_no_idx = detect_broken_links(tmp.path(), &file_links, None, None, false);
        assert_eq!(report_no_idx.total_links, 1);
        assert!(
            report_no_idx.case_mismatches.is_empty(),
            "case_mismatches must always be empty when no index is provided"
        );

        // With index: total_links is still 1 and accounting is consistent.
        // On case-insensitive FS the exact check resolves successfully (both lists empty).
        // On case-sensitive FS the link is reported as a case_mismatch (not broken).
        let report_with_idx = detect_broken_links(tmp.path(), &file_links, None, Some(&idx), false);
        assert_eq!(report_with_idx.total_links, 1);
        let total_classified = report_with_idx.broken.len() + report_with_idx.case_mismatches.len();
        assert!(
            total_classified <= 1,
            "each link must appear at most once across broken + case_mismatches"
        );
    }

    #[test]
    fn detect_broken_links_case_mismatch_has_correct_strategy() {
        use crate::case_index::CaseInsensitiveIndex;
        use crate::link_graph::FileLinks;
        use crate::links::{Link, LinkKind};

        // Build a case-sensitive vault setup by checking the actual FS behavior.
        let tmp = vault_with_files(&[("web/foo.md", ""), ("source.md", "")]);

        let mut idx = CaseInsensitiveIndex::new();
        idx.insert("web/foo.md");

        let file_links = vec![FileLinks {
            source: PathBuf::from("source.md"),
            links: vec![(
                1,
                Link {
                    target: "Web/Foo".to_string(),
                    label: None,
                    kind: LinkKind::Wikilink,
                },
            )],
        }];

        let report = detect_broken_links(tmp.path(), &file_links, None, Some(&idx), false);

        // Regardless of FS case sensitivity: if there are case_mismatches,
        // they must use the LinkCaseMismatch strategy and confidence 1.0.
        for fix in &report.case_mismatches {
            assert!(
                matches!(fix.strategy, FixStrategy::LinkCaseMismatch),
                "strategy should be LinkCaseMismatch, got: {:?}",
                fix.strategy
            );
            assert!(
                (fix.confidence - 1.0).abs() < f64::EPSILON,
                "confidence should be 1.0"
            );
            assert_eq!(
                fix.old_target, "Web/Foo",
                "old_target should preserve original casing"
            );
        }
    }

    #[test]
    fn short_form_wikilink_with_stem_case_mismatch_reports_link_case_mismatch() {
        // Regression for iter-137: a short-form wikilink whose stem casing
        // differs from the on-disk file must classify as `LinkCaseMismatch`,
        // not the legacy `ShortFormStemMismatch`. macOS APFS hid this on
        // local dev runs (the early `is_file()` resolution succeeded
        // case-insensitively), but on case-sensitive filesystems the stem
        // path was taken and emitted the wrong strategy label.
        use crate::case_index::CaseInsensitiveIndex;
        use crate::link_graph::FileLinks;
        use crate::links::{Link, LinkKind};

        let tmp = vault_with_files(&[("iteration_protocols.md", ""), ("source.md", "")]);

        let mut idx = CaseInsensitiveIndex::new();
        idx.set_case_insensitive_paths(true);
        idx.insert("iteration_protocols.md");
        idx.insert("source.md");

        let file_links = vec![FileLinks {
            source: PathBuf::from("source.md"),
            links: vec![(
                1,
                Link {
                    target: "Iteration_Protocols".to_string(),
                    label: None,
                    kind: LinkKind::Wikilink,
                },
            )],
        }];

        let report = detect_broken_links(tmp.path(), &file_links, None, Some(&idx), false);

        assert_eq!(
            report.case_mismatches.len(),
            1,
            "expected one case-mismatch fix; report: {report:#?}"
        );
        let fix = &report.case_mismatches[0];
        assert!(
            matches!(fix.strategy, FixStrategy::LinkCaseMismatch),
            "strategy must be LinkCaseMismatch (was: {:?})",
            fix.strategy
        );
        assert_eq!(fix.old_target, "Iteration_Protocols");
        // `new_target` may be either the canonical short-form stem
        // (`iteration_protocols`) on case-sensitive filesystems or the
        // canonical path (`iteration_protocols.md`) on case-insensitive
        // ones — both are valid case-fix proposals. The invariant under
        // test is the *strategy label*, which must be `LinkCaseMismatch`
        // either way.
        assert!(
            fix.new_target.eq_ignore_ascii_case("iteration_protocols")
                || fix
                    .new_target
                    .eq_ignore_ascii_case("iteration_protocols.md"),
            "new_target should canonicalize to iteration_protocols[.md]; got: {:?}",
            fix.new_target
        );
    }

    // --- Finding 1: bare-basename intra-folder links not flagged as case-mismatches ---

    /// `a/foo.md` links to `[x](bar.md)` and `a/bar.md` exists.
    /// The link should resolve via source-relative lookup and produce no case-mismatch.
    #[test]
    fn bare_basename_markdown_link_in_subfolder_not_flagged() {
        use crate::link_graph::FileLinks;
        use crate::links::{Link, LinkKind};

        let tmp = vault_with_files(&[("a/foo.md", "[x](bar.md)\n"), ("a/bar.md", "# Bar\n")]);

        let file_links = vec![FileLinks {
            source: PathBuf::from("a/foo.md"),
            links: vec![(
                1,
                Link {
                    target: "bar.md".to_string(),
                    label: Some("x".to_string()),
                    kind: LinkKind::Markdown,
                },
            )],
        }];

        let report = detect_broken_links(tmp.path(), &file_links, None, None, false);

        assert_eq!(
            report.case_mismatches.len(),
            0,
            "intra-folder bare-basename markdown link should not be a case-mismatch"
        );
        assert_eq!(
            report.broken.len(),
            0,
            "intra-folder bare-basename markdown link should not be broken"
        );
    }

    /// Same scenario via the index-based detection path.
    #[test]
    fn bare_basename_markdown_link_in_subfolder_not_flagged_from_index() {
        use crate::index::{ScanOptions, ScannedIndex};

        let tmp = vault_with_files(&[
            ("a/foo.md", "---\ntitle: Foo\n---\n[x](bar.md)\n"),
            ("a/bar.md", "---\ntitle: Bar\n---\n# Bar\n"),
        ]);

        let files = vec![
            (tmp.path().join("a/foo.md"), "a/foo.md".to_string()),
            (tmp.path().join("a/bar.md"), "a/bar.md".to_string()),
        ];
        let built = ScannedIndex::build(
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

        let report = detect_broken_links_from_index(tmp.path(), &built.index, None, None, false);

        assert_eq!(
            report.case_mismatches.len(),
            0,
            "intra-folder bare-basename markdown link should not be a case-mismatch (index path)"
        );
        assert_eq!(
            report.broken.len(),
            0,
            "intra-folder bare-basename markdown link should not be broken (index path)"
        );
    }

    #[test]
    fn detect_broken_links_no_index_no_case_mismatches() {
        use crate::link_graph::FileLinks;
        use crate::links::{Link, LinkKind};

        let tmp = vault_with_files(&[("web/foo.md", ""), ("source.md", "")]);

        let file_links = vec![FileLinks {
            source: PathBuf::from("source.md"),
            links: vec![(
                1,
                Link {
                    target: "Web/Foo".to_string(),
                    label: None,
                    kind: LinkKind::Wikilink,
                },
            )],
        }];

        // Without case index: case_mismatches must always be empty.
        let report = detect_broken_links(tmp.path(), &file_links, None, None, false);
        assert!(
            report.case_mismatches.is_empty(),
            "case_mismatches must be empty when no index is provided"
        );
    }

    // ---------------------------------------------------------------------------
    // Short-form wikilink resolution (iter-134)
    // ---------------------------------------------------------------------------

    /// `[[Corina]]` resolving to `sub/Corina.md` must NOT be broken or a case-mismatch.
    #[test]
    fn short_form_wikilink_in_subdir_is_valid() {
        use crate::index::{ScanOptions, ScannedIndex};

        let tmp = vault_with_files(&[
            ("sub/Corina.md", "---\ntitle: Corina\n---\n"),
            ("index.md", "---\ntitle: Index\n---\nSee [[Corina]] here.\n"),
        ]);

        let files = vec![
            (
                tmp.path().join("sub/Corina.md"),
                "sub/Corina.md".to_string(),
            ),
            (tmp.path().join("index.md"), "index.md".to_string()),
        ];
        let built = ScannedIndex::build(
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

        let report = detect_broken_links_from_index(tmp.path(), &built.index, None, None, false);

        assert_eq!(
            report.broken.len(),
            0,
            "[[Corina]] pointing to sub/Corina.md must not be broken; report: {report:?}"
        );
        assert_eq!(
            report.case_mismatches.len(),
            0,
            "[[Corina]] pointing to sub/Corina.md must not be a case-mismatch; report: {report:?}"
        );
        assert_eq!(
            report.ambiguous.len(),
            0,
            "[[Corina]] with one stem match must not be ambiguous; report: {report:?}"
        );
    }

    /// `[[corina]]` for `sub/Corina.md` is a stem-case mismatch — fix to `[[Corina]]`.
    #[test]
    fn short_form_stem_case_mismatch_detected_and_short_form_preserved() {
        use crate::index::{ScanOptions, ScannedIndex};

        let tmp = vault_with_files(&[
            ("sub/Corina.md", "---\ntitle: Corina\n---\n"),
            ("index.md", "---\ntitle: Index\n---\nSee [[corina]] here.\n"),
        ]);

        let files = vec![
            (
                tmp.path().join("sub/Corina.md"),
                "sub/Corina.md".to_string(),
            ),
            (tmp.path().join("index.md"), "index.md".to_string()),
        ];
        let built = ScannedIndex::build(
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

        let report = detect_broken_links_from_index(tmp.path(), &built.index, None, None, false);

        assert_eq!(
            report.broken.len(),
            0,
            "stem-case-mismatch must not be broken; report: {report:?}"
        );
        assert_eq!(
            report.case_mismatches.len(),
            1,
            "stem-case-mismatch must appear in case_mismatches; report: {report:?}"
        );
        let fix = &report.case_mismatches[0];
        assert_eq!(fix.old_target, "corina");
        // new_target must be the short-form stem, not a full path
        assert_eq!(
            fix.new_target, "Corina",
            "new_target must be the stem only, not a full path; fix: {fix:?}"
        );
        assert!(
            !fix.new_target.contains('/'),
            "new_target must not contain a path separator; fix: {fix:?}"
        );
    }

    /// Two files with the same stem produce an `ambiguous` entry; nothing in broken/case_mismatches.
    #[test]
    fn short_form_ambiguous_detected() {
        use crate::index::{ScanOptions, ScannedIndex};

        let tmp = vault_with_files(&[
            ("a/Corina.md", "---\ntitle: Corina A\n---\n"),
            ("b/Corina.md", "---\ntitle: Corina B\n---\n"),
            ("index.md", "---\ntitle: Index\n---\nSee [[Corina]] here.\n"),
        ]);

        let files = vec![
            (tmp.path().join("a/Corina.md"), "a/Corina.md".to_string()),
            (tmp.path().join("b/Corina.md"), "b/Corina.md".to_string()),
            (tmp.path().join("index.md"), "index.md".to_string()),
        ];
        let built = ScannedIndex::build(
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

        let report = detect_broken_links_from_index(tmp.path(), &built.index, None, None, false);

        assert_eq!(
            report.broken.len(),
            0,
            "ambiguous short-form link must not be broken; report: {report:?}"
        );
        assert_eq!(
            report.case_mismatches.len(),
            0,
            "ambiguous short-form link must not be a case-mismatch; report: {report:?}"
        );
        assert_eq!(
            report.ambiguous.len(),
            1,
            "ambiguous short-form link must appear in ambiguous; report: {report:?}"
        );
        assert_eq!(report.ambiguous[0].target, "Corina");
    }

    /// With `expand_short_form=true`, short-form wikilinks fall back to path-based
    /// classification (old behavior), allowing plan_fixes to expand them.
    #[test]
    fn expand_short_form_flag_uses_path_based_classification() {
        use crate::index::{ScanOptions, ScannedIndex};

        let tmp = vault_with_files(&[
            ("sub/Corina.md", "---\ntitle: Corina\n---\n"),
            ("index.md", "---\ntitle: Index\n---\nSee [[Corina]] here.\n"),
        ]);

        let files = vec![
            (
                tmp.path().join("sub/Corina.md"),
                "sub/Corina.md".to_string(),
            ),
            (tmp.path().join("index.md"), "index.md".to_string()),
        ];
        let built = ScannedIndex::build(
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

        // expand_short_form=true: [[Corina]] is not found at vault root → broken
        let report = detect_broken_links_from_index(tmp.path(), &built.index, None, None, true);

        assert_eq!(
            report.broken.len(),
            1,
            "with expand_short_form, [[Corina]] not at vault root must be broken; report: {report:?}"
        );
        assert_eq!(report.broken[0].target, "Corina");
    }
}
