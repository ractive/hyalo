//! Broken link detection and auto-repair with fuzzy matching.
//!
//! # Overview
//!
//! 1. [`detect_broken_links_from_index`] — scan a vault for links that cannot
//!    be resolved to an existing file and return a [`BrokenLinkReport`]. It
//!    classifies each link via the shared Classify-mode entry point
//!    [`crate::discovery::classify_link_from_source`].
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

use anyhow::Result;
use serde::Serialize;

use crate::case_index::CaseInsensitiveIndex;
use crate::discovery::canonicalize_vault_dir;
use crate::discovery::{LinkResolution, StemIndex, classify_link_from_source};
use crate::index::VaultIndex;
use crate::link_graph::normalize_target;
use crate::link_rewrite::{
    Replacement, RewritePlan, apply_replacements, execute_plans_partial,
    find_frontmatter_wikilinks, rewrite_frontmatter_wikilink_text,
};
use crate::links::{
    LinkKind, extract_link_spans_with_original, parse_wikilink, strip_wikilink_md_suffix,
};
use crate::scanner::{LineClass, LineScanner, MAX_FILE_SIZE, lines_with_rest};
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

/// A fix whose source file's on-disk write failed during `--apply` (L-11).
///
/// Distinct from an *unapplied* fix (whose on-disk text no longer matched what
/// detection saw, so no `Replacement` was built): a failed fix produced a valid
/// plan but the durable write itself failed (e.g. read-only target, I/O error).
#[derive(Debug, Clone, Serialize)]
pub struct FailedFix {
    /// The fix that could not be written.
    #[serde(flatten)]
    pub fix: FixPlan,
    /// Human-readable failure reason from the write layer.
    pub error: String,
}

// ---------------------------------------------------------------------------
// Broken link detection
// ---------------------------------------------------------------------------

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

            let (_resolved_target, resolution) = classify_link_from_source(
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
    ///
    /// L-17: uses the shared [`strip_wikilink_md_suffix`] instead of a private
    /// `strip_md`. Both strip a trailing `.md`; the shared helper additionally
    /// requires at least one character before `.md`, so a pathological bare
    /// `.md` candidate is compared verbatim (it is never a real vault path).
    fn is_self_link(source: &str, candidate: &str) -> bool {
        strip_wikilink_md_suffix(source).eq_ignore_ascii_case(strip_wikilink_md_suffix(candidate))
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
        //
        // L-9: seed both scores at NEG_INFINITY (NOT `self.threshold`) so the
        // threshold never acts as a phantom second candidate. Previously,
        // seeding `best_score = self.threshold` meant a lone real candidate
        // scoring just inside `(threshold, threshold + TIE_DELTA]` would push
        // the threshold value into `second_score` and be wrongly rejected as
        // ambiguous. The threshold is now applied once, as a pure floor, after
        // the loop.
        let mut best_score = f64::NEG_INFINITY;
        let mut second_score = f64::NEG_INFINITY;
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

        // Floor check: the best candidate must clear the acceptance threshold.
        let best_idx = best_idx.filter(|_| best_score >= self.threshold)?;

        // If a real runner-up is within TIE_DELTA of the winner the match is
        // ambiguous — decline rather than guessing. When there is no second
        // candidate, `second_score` is still NEG_INFINITY so the gap is
        // effectively infinite and the unique match is accepted.
        if best_score - second_score <= TIE_DELTA {
            return None;
        }

        Some(MatchResult {
            matched_file: self.files[best_idx].clone(),
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
/// Returns `(applied_plans, unapplied, failed)` where:
/// - `applied_plans` are the [`RewritePlan`]s that were durably written to disk.
/// - `unapplied` lists input [`FixPlan`]s that produced no [`Replacement`]
///   (e.g. because the on-disk text no longer matched what detection saw, or
///   the file exceeded the size limit).
/// - `failed` lists fixes whose file produced a valid plan but the durable
///   write failed mid-batch (L-11); remaining files still get written.
///
/// Callers must treat both `unapplied` and `failed` fixes as NOT applied when
/// reporting results, and set a non-zero exit code when `failed` is non-empty.
pub fn apply_fixes(
    dir: &Path,
    fixes: &[FixPlan],
    site_prefix: Option<&str>,
) -> Result<(Vec<RewritePlan>, Vec<FixPlan>, Vec<FailedFix>)> {
    // Group fixes by source file.
    let mut by_source: HashMap<&str, Vec<&FixPlan>> = HashMap::new();
    for fix in fixes {
        by_source.entry(fix.source.as_str()).or_default().push(fix);
    }

    let mut plans: Vec<RewritePlan> = Vec::new();
    let mut unapplied: Vec<FixPlan> = Vec::new();
    // I/O failures (stat/read) encountered while building plans, keyed by the
    // fixes they belong to — reported as `failed`, not `unapplied`, since
    // these are genuine errors rather than stale-text mismatches. Fixes for a
    // file whose read fails do not abort the batch; the remaining source
    // files still get their plans built and applied.
    let mut io_failed: Vec<FailedFix> = Vec::new();
    // Map each plan's rel_path → the fixes it carries, so a mid-batch write
    // failure can be reported against the specific fixes that did not land.
    let mut fixes_by_plan: HashMap<String, Vec<FixPlan>> = HashMap::new();

    for (source_rel, file_fixes) in &by_source {
        let abs_path = dir.join(source_rel.replace('\\', "/"));
        let (content, file_mtime) = match read_source_file(&abs_path) {
            SourceRead::Ok { content, mtime } => (content, mtime),
            SourceRead::TooLarge { size } => {
                eprintln!(
                    "warning: skipping {} ({} MiB exceeds {} MiB limit)",
                    abs_path.display(),
                    size / (1024 * 1024),
                    MAX_FILE_SIZE / (1024 * 1024)
                );
                unapplied.extend(file_fixes.iter().map(|f| (*f).clone()));
                continue;
            }
            SourceRead::Failed(error) => {
                // L-11: a per-file stat/read failure (e.g. the file was
                // deleted between detection and apply) must not abort the
                // whole batch — record it as failed and keep processing the
                // remaining source files.
                eprintln!("warning: failed to read {}: {error}", abs_path.display());
                io_failed.extend(file_fixes.iter().map(|f| FailedFix {
                    fix: (*f).clone(),
                    error: error.clone(),
                }));
                continue;
            }
        };

        let (replacements, satisfied) =
            build_replacements_for_file(&content, source_rel, file_fixes, site_prefix);

        let mut satisfied_fixes: Vec<FixPlan> = Vec::new();
        for (idx, fix) in file_fixes.iter().enumerate() {
            if satisfied.contains(&idx) {
                satisfied_fixes.push((*fix).clone());
            } else {
                unapplied.push((*fix).clone());
            }
        }

        if !replacements.is_empty() {
            let rewritten_content = apply_replacements(&content, &replacements);
            fixes_by_plan.insert((*source_rel).to_string(), satisfied_fixes);
            plans.push(RewritePlan {
                path: abs_path,
                rel_path: (*source_rel).to_string(),
                replacements,
                rewritten_content,
                mtime: file_mtime,
                original_content: None,
            });
        }
    }

    // Execute all plans, continuing past per-file write failures so the caller
    // gets an honest applied/failed split even on a mid-batch failure (L-11).
    let report = execute_plans_partial(dir, &plans)?;

    let mut failed: Vec<FailedFix> = io_failed;
    let mut applied_plans: Vec<RewritePlan> = Vec::new();
    let mut outcome_by_rel: HashMap<&str, (bool, Option<String>)> = HashMap::new();
    for o in &report.outcomes {
        outcome_by_rel.insert(o.rel_path.as_str(), (o.applied, o.error.clone()));
    }
    for plan in plans {
        // A missing outcome (should not happen) is treated as applied — the
        // failure path only fires on an explicit `applied == false` record.
        if let Some((false, err)) = outcome_by_rel.get(plan.rel_path.as_str()) {
            let reason = err.clone().unwrap_or_else(|| "write failed".to_string());
            if let Some(fs) = fixes_by_plan.remove(&plan.rel_path) {
                for fix in fs {
                    failed.push(FailedFix {
                        fix,
                        error: reason.clone(),
                    });
                }
            }
        } else {
            applied_plans.push(plan);
        }
    }

    Ok((applied_plans, unapplied, failed))
}

/// Outcome of reading a source file's on-disk content for fix planning.
///
/// Shared by [`apply_fixes`] and [`plan_fixes_dry_run`] so both run the
/// identical per-file I/O prelude (stat, size-limit check, read) — the two
/// functions differ only in how they react to each outcome (`apply_fixes`
/// routes a [`SourceRead::Failed`] into the `failed` bucket, while
/// `plan_fixes_dry_run` treats it the same as a stale/vanished file and adds
/// it to `unapplied`).
enum SourceRead {
    /// File was read successfully. `mtime` is `None` if the modified time
    /// could not be determined (still usable — callers just skip the
    /// mtime-based concurrent-edit check for this plan).
    Ok {
        content: String,
        mtime: Option<(std::time::SystemTime, u64)>,
    },
    /// File exceeds [`MAX_FILE_SIZE`]; skipped as a matter of policy, not an
    /// I/O error.
    TooLarge { size: u64 },
    /// `stat` or `read_to_string` failed (e.g. the file was deleted or
    /// became unreadable between detection and this call). Carries a
    /// human-readable error string.
    Failed(String),
}

/// Stat and read `abs_path`, classifying the outcome for fix planning.
fn read_source_file(abs_path: &Path) -> SourceRead {
    let meta = match std::fs::metadata(abs_path) {
        Ok(m) => m,
        Err(e) => return SourceRead::Failed(format!("failed to stat {}: {e}", abs_path.display())),
    };
    let file_size = meta.len();
    if file_size > MAX_FILE_SIZE {
        return SourceRead::TooLarge { size: file_size };
    }
    let mtime = meta.modified().ok().map(|t| (t, file_size));
    match std::fs::read_to_string(abs_path) {
        Ok(content) => SourceRead::Ok { content, mtime },
        Err(e) => SourceRead::Failed(format!("reading {}: {e}", abs_path.display())),
    }
}

/// Dry-run counterpart of [`apply_fixes`]: build the same [`RewritePlan`]s
/// against on-disk text but write nothing (L-25).
///
/// Running the identical plan-building phase means dry-run's `unapplied` set is
/// exactly what `--apply` would refuse — a fix whose on-disk text no longer
/// matches what detection saw (stale index / concurrent edit) is reported as
/// unapplied in *both* modes. Without this, dry-run always reported an empty
/// `unapplied` and could promise fixes that a subsequent `--apply` would drop.
///
/// Returns `(would_modify, unapplied)` where `would_modify` is the set of
/// vault-relative paths that would receive at least one rewrite, and
/// `unapplied` lists the fixes whose on-disk text no longer matches.
pub fn plan_fixes_dry_run(
    dir: &Path,
    fixes: &[FixPlan],
    site_prefix: Option<&str>,
) -> Result<(Vec<String>, Vec<FixPlan>)> {
    let mut by_source: HashMap<&str, Vec<&FixPlan>> = HashMap::new();
    for fix in fixes {
        by_source.entry(fix.source.as_str()).or_default().push(fix);
    }

    let mut would_modify: Vec<String> = Vec::new();
    let mut unapplied: Vec<FixPlan> = Vec::new();

    for (source_rel, file_fixes) in &by_source {
        let abs_path = dir.join(source_rel.replace('\\', "/"));
        // File vanished/unreadable since detection, or exceeds the size
        // limit — every fix for it is stale/unapplied. Dry-run treats a
        // genuine I/O failure the same as a stale file (unlike `apply_fixes`,
        // which distinguishes them into `failed`): nothing was written
        // either way, so from a preview's point of view both are simply
        // "this fix will not land."
        let content = match read_source_file(&abs_path) {
            SourceRead::Ok { content, .. } => content,
            SourceRead::TooLarge { .. } | SourceRead::Failed(_) => {
                unapplied.extend(file_fixes.iter().map(|f| (*f).clone()));
                continue;
            }
        };

        let (replacements, satisfied) =
            build_replacements_for_file(&content, source_rel, file_fixes, site_prefix);

        for (idx, fix) in file_fixes.iter().enumerate() {
            if !satisfied.contains(&idx) {
                unapplied.push((*fix).clone());
            }
        }

        if !replacements.is_empty() {
            would_modify.push((*source_rel).to_string());
        }
    }

    would_modify.sort();
    unapplied.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.line.cmp(&b.line)));
    Ok((would_modify, unapplied))
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
    // Shared, cross-line-aware line classifier (iter-183 Phase B): one lexer
    // for frontmatter, fences, `%%` comments, and cross-line code/HTML spans.
    let mut scanner = LineScanner::new();

    // Frontmatter-derived FixPlans always carry `line: 1` (see
    // `LinkGraphVisitor::extract_frontmatter_wikilinks`, which has no
    // meaningful per-line info once YAML is parsed into a `Value`). Look
    // them up once and match by `old_target` against every `[[...]]`
    // occurrence anywhere in the frontmatter block, regardless of which
    // physical line it sits on.
    let frontmatter_fixes: &[(usize, &FixPlan)] = fixes_by_line.get(&1).map_or(&[], Vec::as_slice);

    for (line, rest) in lines_with_rest(content) {
        let class = scanner.classify(line, rest);
        let line_num = scanner.line_num();

        // --- Frontmatter ---
        match class {
            LineClass::FrontmatterOpen | LineClass::FrontmatterClose | LineClass::Skip => continue,
            LineClass::Frontmatter => {
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

                        // Preserve alias (`path|Label`), the `#fragment`
                        // anchor (L-7: repairs must keep `[[log#DEC-041]]`'s
                        // anchor), and written form (path-form vs bare stem)
                        // via the shared `mv`/`links fix` frontmatter rewriter.
                        if let Some(new_text) =
                            rewrite_frontmatter_wikilink_text(occ.target, &fix.new_target)
                        {
                            replacements.push(Replacement {
                                line: line_num,
                                byte_offset: occ.full_start,
                                old_text: line[occ.full_start..occ.full_end].to_string(),
                                new_text,
                            });
                        }
                        satisfied.insert(fix_idx);
                    }
                }
                continue;
            }
            LineClass::Body(_) => {}
        }

        // Body line (`LineClass::Body`). The shared scanner already handled
        // fences, `%%` comment blocks, and cross-line code/HTML suppression.
        let LineClass::Body(body) = class else {
            unreachable!("all non-Body classes were handled above")
        };

        // If there are no fixes on this line, skip expensive span extraction.
        let Some(line_fixes) = fixes_by_line.get(&line_num) else {
            continue;
        };

        // Extract link spans (inline code, `%%` comments, cross-line code
        // spans, and HTML comments are already blanked by the shared scanner).
        let cleaned = body.cleaned(line, rest);
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

    /// Minimal in-memory [`VaultIndex`] built from hand-specified
    /// `(rel_path, links)` pairs. Used to exercise
    /// [`detect_broken_links_from_index`] with precisely-controlled outbound
    /// links (line numbers, targets, kinds) without going through the scanner —
    /// the direct successor to the retired `detect_broken_links(&[FileLinks])`
    /// test path (iter-189 task 4).
    struct MockIndex {
        entries: Vec<crate::index::IndexEntry>,
        graph: crate::link_graph::LinkGraph,
    }

    impl MockIndex {
        fn new(files: &[(&str, Vec<(usize, crate::links::Link)>)]) -> Self {
            let mut entries: Vec<crate::index::IndexEntry> = files
                .iter()
                .map(|(rel, links)| crate::index::IndexEntry {
                    rel_path: (*rel).to_string(),
                    modified: String::new(),
                    properties: indexmap::IndexMap::default(),
                    tags: Vec::new(),
                    sections: Vec::new(),
                    tasks: Vec::new(),
                    links: links.clone(),
                    bm25_tokens: None,
                    bm25_language: None,
                })
                .collect();
            entries.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
            Self {
                entries,
                graph: crate::link_graph::LinkGraph::default(),
            }
        }
    }

    impl VaultIndex for MockIndex {
        fn entries(&self) -> &[crate::index::IndexEntry] {
            &self.entries
        }
        fn get(&self, rel_path: &str) -> Option<&crate::index::IndexEntry> {
            self.entries.iter().find(|e| e.rel_path == rel_path)
        }
        fn link_graph(&self) -> &crate::link_graph::LinkGraph {
            &self.graph
        }
    }

    /// Build a single-source [`MockIndex`] from a source path and its links,
    /// ensuring the source file itself is present as an entry too so that the
    /// stem index sees it (mirrors the old `FileLinks { source, links }` shape).
    fn mock_index(
        source: &str,
        links: Vec<(usize, crate::links::Link)>,
        extra_files: &[&str],
    ) -> MockIndex {
        let mut files: Vec<(&str, Vec<(usize, crate::links::Link)>)> = vec![(source, links)];
        for f in extra_files {
            files.push((f, Vec::new()));
        }
        MockIndex::new(&files)
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

    #[test]
    fn matcher_single_candidate_inside_tie_delta_above_threshold_accepted() {
        // L-9: with exactly ONE candidate whose score sits just inside
        // (threshold, threshold + TIE_DELTA], the phantom-tie bug used to
        // reject it as "ambiguous" because the seeded threshold became the
        // runner-up. A lone valid candidate must now be accepted.
        // Mirrors the private `TIE_DELTA` in `find_match`.
        const TIE_DELTA: f64 = 0.01;
        let target = "authentcation";
        let stem = "authentication";
        let score = strsim::jaro_winkler(target, stem);
        // Threshold half a TIE_DELTA below the real score → score is inside
        // (threshold, threshold + TIE_DELTA].
        let threshold = score - TIE_DELTA / 2.0;
        assert!(
            score - threshold <= TIE_DELTA && score >= threshold,
            "test setup: score {score} must be within TIE_DELTA above threshold {threshold}"
        );
        let matcher = LinkMatcher::new(make_files(&[&format!("{stem}.md")]), threshold);
        let result = matcher
            .find_match(target, "__test__")
            .expect("lone valid candidate must not be rejected as a phantom tie");
        assert_eq!(result.matched_file, format!("{stem}.md"));
        assert!(matches!(result.strategy, FixStrategy::FuzzyMatch));
    }

    #[test]
    fn matcher_two_genuine_ties_still_rejected() {
        // Guard: two real candidates scoring within TIE_DELTA of each other are
        // still ambiguous and rejected (the fix must not accept genuine ties).
        let matcher = LinkMatcher::new(make_files(&["report-a.md", "report-b.md"]), 0.7);
        assert!(
            matcher.find_match("report-x", "__test__").is_none(),
            "two near-identical candidates should stay ambiguous"
        );
    }

    // --- L-7: frontmatter link repair keeps the `#anchor` ---

    fn fm_fix(old_target: &str, new_target: &str) -> FixPlan {
        FixPlan {
            source: "a.md".to_string(),
            line: 1, // frontmatter fixes always carry line 1
            old_target: old_target.to_string(),
            new_target: new_target.to_string(),
            strategy: FixStrategy::CaseInsensitive,
            confidence: 1.0,
        }
    }

    #[test]
    fn build_replacements_frontmatter_repair_preserves_anchor() {
        // L-7: repairing a broken anchored frontmatter wikilink must keep the
        // `#fragment` — previously it was dropped, turning
        // `[[decision-log#DEC-041]]` into `[[decision-log-archive]]`.
        let content = "---\nrelated:\n  - \"[[decision-log#DEC-041]]\"\n---\nBody\n";
        let fix = fm_fix("decision-log", "decision-log-archive.md");
        let (repls, _) = build_replacements_for_file(content, "a.md", &[&fix], None);
        assert_eq!(repls.len(), 1, "one frontmatter link repaired: {repls:?}");
        assert_eq!(repls[0].old_text, "[[decision-log#DEC-041]]");
        assert_eq!(repls[0].new_text, "[[decision-log-archive#DEC-041]]");
    }

    #[test]
    fn build_replacements_frontmatter_repair_preserves_anchor_and_alias() {
        let content = "---\nrelated:\n  - \"[[decision-log#DEC-041|Log]]\"\n---\nBody\n";
        let fix = fm_fix("decision-log", "decision-log-archive.md");
        let (repls, _) = build_replacements_for_file(content, "a.md", &[&fix], None);
        assert_eq!(repls.len(), 1);
        assert_eq!(repls[0].new_text, "[[decision-log-archive#DEC-041|Log]]");
    }

    // --- L-8: `%%` inside a fenced code block is literal ---

    #[test]
    fn build_replacements_literal_percent_in_code_fence_does_not_desync() {
        // L-8: a literal `%%` inside a fenced code block must NOT toggle the
        // comment-fence state; a real broken link AFTER the block must still
        // be rewritten (previously the stray `%%` opened a phantom comment and
        // swallowed everything until the next `%%`).
        // A bare `%%` line inside a fenced code block. With the buggy ordering
        // (comment toggle before code-fence processing) this opened a phantom
        // comment fence that swallowed the link below.
        let content = "\
# Title

```text
%%
```

See [broken](old-name.md) here.
";
        let fix = FixPlan {
            source: "a.md".to_string(),
            line: 7,
            old_target: "old-name.md".to_string(),
            new_target: "new-name.md".to_string(),
            strategy: FixStrategy::CaseInsensitive,
            confidence: 1.0,
        };
        let (repls, _) = build_replacements_for_file(content, "a.md", &[&fix], None);
        assert_eq!(
            repls.len(),
            1,
            "link after a code-fenced `%%` must still be rewritten: {repls:?}"
        );
        assert_eq!(repls[0].old_text, "[broken](old-name.md)");
        assert_eq!(repls[0].new_text, "[broken](new-name.md)");
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

    // --- detect_broken_links_from_index: basic ---
    // (Ported from the retired FileLinks-based `detect_broken_links` in
    //  iter-189 task 4; assertions preserved verbatim.)

    #[test]
    fn detect_broken_links_finds_missing() {
        use crate::links::{Link, LinkKind};

        let tmp = vault_with_files(&[("index.md", "[[existing]]"), ("existing.md", "")]);

        let index = mock_index(
            "index.md",
            vec![
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
            &["existing.md"],
        );

        let report = detect_broken_links_from_index(tmp.path(), &index, None, None, false);

        assert_eq!(report.total_links, 2);
        assert_eq!(report.broken.len(), 1);
        assert_eq!(report.broken[0].target, "missing");
    }

    // --- detect_broken_links_from_index: sorted output ---

    #[test]
    fn detect_broken_links_sorted() {
        use crate::links::{Link, LinkKind};

        let tmp = vault_with_files(&[("a.md", ""), ("b.md", "")]);

        let index = MockIndex::new(&[
            (
                "b.md",
                vec![(
                    3,
                    Link {
                        target: "gone".to_string(),
                        label: None,
                        kind: LinkKind::Wikilink,
                    },
                )],
            ),
            (
                "a.md",
                vec![
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
            ),
        ]);

        let report = detect_broken_links_from_index(tmp.path(), &index, None, None, false);

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

        let (plans, unapplied, _failed) = apply_fixes(tmp.path(), &fixes, None).unwrap();

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

        let (plans, unapplied, _failed) = apply_fixes(tmp.path(), &fixes, None).unwrap();

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

        let (plans, unapplied, _failed) = apply_fixes(tmp.path(), &fixes, None).unwrap();

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

        let (plans, unapplied, _failed) = apply_fixes(tmp.path(), &fixes, None).unwrap();

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

        let (plans, unapplied, _failed) = apply_fixes(tmp.path(), &fixes, None).unwrap();

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

        let (plans, unapplied, _failed) = apply_fixes(tmp.path(), &fixes, None).unwrap();

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

        let (plans, unapplied, _failed) = apply_fixes(tmp.path(), &fixes, None).unwrap();

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

        let (plans, unapplied, _failed) = apply_fixes(tmp.path(), &fixes, None).unwrap();

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

        let (plans, unapplied, _failed) = apply_fixes(tmp.path(), &fixes, None).unwrap();

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

        let (plans, unapplied, _failed) = apply_fixes(tmp.path(), &fixes, None).unwrap();

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

        let (plans, unapplied, _failed) = apply_fixes(tmp.path(), &fixes, None).unwrap();

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
        use crate::links::{Link, LinkKind};

        // On-disk: `web/foo.md` (lowercase). Link written as `Web/Foo` (PascalCase).
        let tmp = vault_with_files(&[("web/foo.md", ""), ("source.md", "[[Web/Foo]]")]);

        // Build a case index containing the real path.
        let mut idx = CaseInsensitiveIndex::new();
        idx.insert("web/foo.md");

        let index = mock_index(
            "source.md",
            vec![(
                1,
                Link {
                    target: "Web/Foo".to_string(),
                    label: None,
                    kind: LinkKind::Wikilink,
                },
            )],
            &["web/foo.md"],
        );

        // Without index: case_mismatches is always empty regardless of FS type.
        // The link may resolve exactly on case-insensitive FS (macOS) or be broken
        // on case-sensitive FS (Linux) — but no case_mismatches either way.
        let report_no_idx = detect_broken_links_from_index(tmp.path(), &index, None, None, false);
        assert_eq!(report_no_idx.total_links, 1);
        assert!(
            report_no_idx.case_mismatches.is_empty(),
            "case_mismatches must always be empty when no index is provided"
        );

        // With index: total_links is still 1 and accounting is consistent.
        // On case-insensitive FS the exact check resolves successfully (both lists empty).
        // On case-sensitive FS the link is reported as a case_mismatch (not broken).
        let report_with_idx =
            detect_broken_links_from_index(tmp.path(), &index, None, Some(&idx), false);
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
        use crate::links::{Link, LinkKind};

        // Build a case-sensitive vault setup by checking the actual FS behavior.
        let tmp = vault_with_files(&[("web/foo.md", ""), ("source.md", "")]);

        let mut idx = CaseInsensitiveIndex::new();
        idx.insert("web/foo.md");

        let index = mock_index(
            "source.md",
            vec![(
                1,
                Link {
                    target: "Web/Foo".to_string(),
                    label: None,
                    kind: LinkKind::Wikilink,
                },
            )],
            &["web/foo.md"],
        );

        let report = detect_broken_links_from_index(tmp.path(), &index, None, Some(&idx), false);

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
        use crate::links::{Link, LinkKind};

        let tmp = vault_with_files(&[("iteration_protocols.md", ""), ("source.md", "")]);

        let mut idx = CaseInsensitiveIndex::new();
        idx.set_case_insensitive_paths(true);
        idx.insert("iteration_protocols.md");
        idx.insert("source.md");

        let index = mock_index(
            "source.md",
            vec![(
                1,
                Link {
                    target: "Iteration_Protocols".to_string(),
                    label: None,
                    kind: LinkKind::Wikilink,
                },
            )],
            &["iteration_protocols.md"],
        );

        let report = detect_broken_links_from_index(tmp.path(), &index, None, Some(&idx), false);

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
        use crate::links::{Link, LinkKind};

        let tmp = vault_with_files(&[("a/foo.md", "[x](bar.md)\n"), ("a/bar.md", "# Bar\n")]);

        let index = mock_index(
            "a/foo.md",
            vec![(
                1,
                Link {
                    target: "bar.md".to_string(),
                    label: Some("x".to_string()),
                    kind: LinkKind::Markdown,
                },
            )],
            &["a/bar.md"],
        );

        let report = detect_broken_links_from_index(tmp.path(), &index, None, None, false);

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
        use crate::links::{Link, LinkKind};

        let tmp = vault_with_files(&[("web/foo.md", ""), ("source.md", "")]);

        let index = mock_index(
            "source.md",
            vec![(
                1,
                Link {
                    target: "Web/Foo".to_string(),
                    label: None,
                    kind: LinkKind::Wikilink,
                },
            )],
            &["web/foo.md"],
        );

        // Without case index: case_mismatches must always be empty.
        let report = detect_broken_links_from_index(tmp.path(), &index, None, None, false);
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

    // --- L-25: dry-run / apply parity ---

    #[test]
    fn plan_fixes_dry_run_matches_apply_on_fresh_text() {
        // A fix that would apply cleanly must be reported as would-modify by
        // dry-run and produce no unapplied entries — matching what apply does.
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

        let (would_modify, unapplied) = plan_fixes_dry_run(tmp.path(), &fixes, None).unwrap();
        assert_eq!(would_modify, vec!["index.md"]);
        assert!(unapplied.is_empty(), "fresh text: nothing stale");

        // Dry-run must not have written anything.
        let on_disk = fs::read_to_string(tmp.path().join("index.md")).unwrap();
        assert!(
            on_disk.contains("[[wrongname]]"),
            "dry-run must not mutate disk"
        );
    }

    #[test]
    fn plan_fixes_dry_run_reports_stale_fix_like_apply() {
        // L-25: when the on-disk text no longer matches what detection saw
        // (stale index / concurrent edit), the fix must show up as unapplied in
        // BOTH dry-run and apply — one code path, guaranteed parity.
        let tmp = vault_with_files(&[
            // On disk the link is already gone — the plan below is stale.
            ("index.md", "Nothing to see here.\n"),
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

        let (would_modify_dry, unapplied_dry) =
            plan_fixes_dry_run(tmp.path(), &fixes, None).unwrap();
        assert!(would_modify_dry.is_empty(), "stale fix modifies nothing");
        assert_eq!(
            unapplied_dry.len(),
            1,
            "stale fix must be reported unapplied"
        );

        // apply must report the identical unapplied set.
        let (plans, unapplied_apply, failed) = apply_fixes(tmp.path(), &fixes, None).unwrap();
        assert!(plans.is_empty());
        assert!(failed.is_empty());
        assert_eq!(unapplied_apply.len(), unapplied_dry.len());
        assert_eq!(unapplied_apply[0].old_target, unapplied_dry[0].old_target);
    }

    // --- Finding 2 (PR #221 review): apply_fixes records-and-continues on
    // per-file I/O failure instead of aborting the whole batch ---

    #[test]
    fn apply_fixes_continues_past_deleted_source_file() {
        // A source file deleted between detection and apply must not abort
        // the whole batch: its fixes land in `failed`, and fixes for other
        // files in the same batch are still applied.
        let tmp = vault_with_files(&[
            ("gone.md", "See [[wrongname]] here.\n"),
            ("still-here.md", "See [[wrongname]] here too.\n"),
            ("correct-name.md", ""),
        ]);

        // Delete the file after "detection" (which would have scanned it)
        // but before apply runs.
        fs::remove_file(tmp.path().join("gone.md")).unwrap();

        let fixes = vec![
            FixPlan {
                source: "gone.md".to_string(),
                line: 1,
                old_target: "wrongname".to_string(),
                new_target: "correct-name.md".to_string(),
                strategy: FixStrategy::FuzzyMatch,
                confidence: 0.9,
            },
            FixPlan {
                source: "still-here.md".to_string(),
                line: 1,
                old_target: "wrongname".to_string(),
                new_target: "correct-name.md".to_string(),
                strategy: FixStrategy::FuzzyMatch,
                confidence: 0.9,
            },
        ];

        let (plans, unapplied, failed) = apply_fixes(tmp.path(), &fixes, None)
            .expect("apply_fixes must not abort on a per-file I/O error");

        assert_eq!(
            failed.len(),
            1,
            "the deleted file's fix must land in `failed`, not abort the batch: {failed:?}"
        );
        assert_eq!(failed[0].fix.source, "gone.md");
        assert!(
            unapplied.is_empty(),
            "the deleted file's fix belongs in `failed`, not `unapplied`: {unapplied:?}"
        );

        assert_eq!(
            plans.len(),
            1,
            "the still-existing file's fix must still be applied: {plans:?}"
        );
        assert_eq!(plans[0].rel_path, "still-here.md");
        let written = fs::read_to_string(tmp.path().join("still-here.md")).unwrap();
        assert!(
            written.contains("[[correct-name]]") || written.contains("correct-name.md"),
            "still-here.md must have been rewritten despite gone.md's failure: {written}"
        );
    }

    // --- Finding 4c (PR #221 review): dry-run's vanished/oversized branches,
    // exercised directly rather than only via apply's equivalents ---

    #[test]
    fn plan_fixes_dry_run_reports_unapplied_for_vanished_file() {
        let tmp = vault_with_files(&[
            ("index.md", "See [[wrongname]] for details.\n"),
            ("correct-name.md", ""),
        ]);
        fs::remove_file(tmp.path().join("index.md")).unwrap();

        let fixes = vec![FixPlan {
            source: "index.md".to_string(),
            line: 1,
            old_target: "wrongname".to_string(),
            new_target: "correct-name.md".to_string(),
            strategy: FixStrategy::FuzzyMatch,
            confidence: 0.9,
        }];

        let (would_modify, unapplied) = plan_fixes_dry_run(tmp.path(), &fixes, None).unwrap();
        assert!(
            would_modify.is_empty(),
            "a vanished file must modify nothing"
        );
        assert_eq!(unapplied.len(), 1, "the fix must be reported unapplied");
        assert_eq!(unapplied[0].old_target, "wrongname");
    }

    #[test]
    fn plan_fixes_dry_run_reports_unapplied_for_oversized_file() {
        let tmp = vault_with_files(&[("correct-name.md", "")]);
        // Write a file that exceeds MAX_FILE_SIZE so dry-run's size-limit
        // branch fires directly (previously only covered via apply_fixes).
        let big_path = tmp.path().join("big.md");
        let mut f = fs::File::create(&big_path).unwrap();
        let chunk = vec![b'a'; 1024 * 1024];
        let mut written = 0u64;
        while written <= MAX_FILE_SIZE {
            std::io::Write::write_all(&mut f, &chunk).unwrap();
            written += chunk.len() as u64;
        }

        let fixes = vec![FixPlan {
            source: "big.md".to_string(),
            line: 1,
            old_target: "wrongname".to_string(),
            new_target: "correct-name.md".to_string(),
            strategy: FixStrategy::FuzzyMatch,
            confidence: 0.9,
        }];

        let (would_modify, unapplied) = plan_fixes_dry_run(tmp.path(), &fixes, None).unwrap();
        assert!(
            would_modify.is_empty(),
            "an oversized file must modify nothing"
        );
        assert_eq!(unapplied.len(), 1, "the fix must be reported unapplied");
        assert_eq!(unapplied[0].old_target, "wrongname");
    }
}
