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
//!    shortest-path → fuzzy) and produce a [`FixReport`]. Fuzzy candidacy is
//!    gated by a Jaro-Winkler stem score, but the reported *confidence* comes
//!    from [`crate::link_score::candidate_confidence`] (iter-212).
//!
//! 3. [`apply_fixes`] — convert [`FixPlan`]s to [`RewritePlan`]s and write
//!    the corrected link text back to disk.

#![allow(clippy::missing_errors_doc)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use anyhow::Result;
use serde::Serialize;

use crate::case_index::CaseInsensitiveIndex;
use crate::discovery::canonicalize_vault_dir;
use crate::discovery::{LinkResolution, StemIndex, classify_link_from_source};
use crate::index::VaultIndex;
use crate::link_graph::{normalize_target, relative_path_between, strip_site_prefix};
use crate::link_rewrite::{
    Replacement, RewritePlan, apply_replacements, execute_plans_partial,
    find_frontmatter_wikilinks, rewrite_frontmatter_wikilink_text,
};
use crate::link_score::{self, candidate_confidence_with_claim};
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
    /// Links whose exact path failed to resolve but whose bare stem matched a
    /// file somewhere else in the vault — [`FixStrategy::ShortestPath`].
    ///
    /// NEW-13 (dogfood pre3): before this bucket existed these landed in
    /// [`Self::case_mismatches`] alongside genuine [`FixStrategy::LinkCaseMismatch`]
    /// casing fixes. A user reading "Case mismatches: N" reasonably assumes a
    /// cosmetic count; a relocation (`target.md` → `sub/target.md`) is a
    /// different kind of change and gets its own bucket and section.
    pub relocations: Vec<FixPlan>,
    /// Short-form wikilinks (no `/`) whose stem matches ≥2 files in the vault.
    /// These are left untouched by `--apply` because the correct target is
    /// ambiguous and auto-picking would be wrong.
    pub ambiguous: Vec<BrokenLinkInfo>,
    /// Links whose target resolves *above* the scanned vault root (`../..`
    /// walks out of the directory hyalo was pointed at). They cannot be
    /// checked — the file they name is out of scope — so they are reported
    /// separately instead of inflating the headline `broken` count
    /// (iter-193; same treatment iter-184 gave broken anchors).
    ///
    /// Site-absolute targets (`/src/foo.md`) deliberately stay in `broken`:
    /// a vault that is itself the site root makes those genuine misses.
    pub out_of_vault: Vec<BrokenLinkInfo>,
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
#[derive(Debug, Clone, Copy, Serialize)]
pub enum FixStrategy {
    /// The target matched an existing file path case-insensitively.
    CaseInsensitive,
    /// The target was written with or without `.md` and the other form matched.
    ExtensionMismatch,
    /// The target was a bare basename (no directory part) and its stem
    /// matched exactly one file anywhere in the vault — the Obsidian
    /// short-form resolution rule, applied to a link that was already written
    /// short-form.
    ///
    /// DEC-076: a bare stem asserts no location, so this is a *resolution*,
    /// not a guess, and plain `--apply` writes it. The moment the author
    /// writes any directory component the verdict becomes
    /// [`BasenameFallback`] instead, whatever the leading character.
    ///
    /// iter-211 / BUG-12 also routes the read-side stem rescue here: a link
    /// whose exact path fails but whose bare stem resolves elsewhere used to
    /// be reported as [`LinkCaseMismatch`] — a *relocation* dressed up as a
    /// casing fix, printed as `[link-case-mismatch]` next to an old and new
    /// target that differ by a whole directory.
    ShortestPath,
    /// A target that **wrote a directory** (`/actions`, `guides/actions`)
    /// matched a file only by its last path segment (iter-200 / dogfood M-1,
    /// widened to relative paths by DEC-076 in iter-211).
    ///
    /// This is a guess, not a resolution: the location the author asserted is
    /// thrown away and a same-named file from somewhere else is substituted
    /// (`/actions` → `graphql/reference/actions.md`). It therefore carries a
    /// reduced confidence and is grouped with fuzzy matches, so plain
    /// `--apply` never writes it — `--apply-fuzzy` / `--min-confidence` opt
    /// in.
    BasenameFallback,
    /// Jaro-Winkler similarity above the configured threshold.
    FuzzyMatch,
    /// The target resolves to an existing file but with different casing.
    ///
    /// Rule code: `link-case-mismatch`. The `new_target` in the [`FixPlan`]
    /// holds either the canonical on-disk path (for path-form links and
    /// markdown links) or the canonical short-form stem (for bare wikilinks
    /// whose stem lookup succeeded with a case-only difference).
    LinkCaseMismatch,
}

impl FixStrategy {
    /// Stable kebab-case rule code for human-readable output.
    ///
    /// `links fix` used to print a hard-coded `[link-case-mismatch]` against
    /// every entry in the case-mismatch bucket, so a bare-stem relocation was
    /// labelled a casing fix (iter-211 / BUG-12). Callers should render this
    /// instead. The JSON `strategy` field keeps its PascalCase variant name —
    /// this is an additional, presentation-level spelling.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            FixStrategy::CaseInsensitive => "case-insensitive",
            FixStrategy::ExtensionMismatch => "extension-mismatch",
            FixStrategy::ShortestPath => "shortest-path",
            FixStrategy::BasenameFallback => "basename-fallback",
            FixStrategy::FuzzyMatch => "fuzzy-match",
            FixStrategy::LinkCaseMismatch => "link-case-mismatch",
        }
    }
}

/// Result of planning fixes for a set of broken links.
#[derive(Debug, Clone, Serialize)]
pub struct FixReport {
    /// Broken links for which a candidate fix was found.
    pub fixes: Vec<FixPlan>,
    /// Broken links for which no suitable candidate could be found.
    pub unfixable: Vec<BrokenLinkInfo>,
    /// Broken links whose target is a *template expression*, not a path
    /// (iter-207, BUG-4). Never matched, never rewritten — see
    /// [`is_templated_target`].
    pub templated: Vec<BrokenLinkInfo>,
}

/// Whether `target` is a template expression rather than a literal path
/// (iter-207, BUG-4).
///
/// Site generators emit link destinations containing conditionals and
/// variables — `{% ifversion ghes %}/admin{% endif %}/guides`,
/// `{{ site.baseurl }}/x`, `${BASE}/y`. hyalo cannot know what those render
/// to, but the *literal* text fuzzy-matches real files well enough to clear
/// the 0.95 threshold, so `links fix --apply` used to rewrite them and
/// silently drop the conditional. The round-trip guard cannot catch this: the
/// rewritten target genuinely resolves, and the corruption is semantic.
///
/// Such targets are reported in [`FixReport::templated`] and never fixed.
#[must_use]
pub fn is_templated_target(target: &str) -> bool {
    target.contains("{%") || target.contains("{{") || target.contains("${")
}

/// What one `--apply` pass did with the fixes it was handed.
///
/// Tuple order: `(applied_plans, unapplied, failed, rejected)` — see
/// [`apply_fixes`] for what each bucket means. Named so the four-way split
/// stays readable at the call site.
pub type ApplyOutcome = (Vec<RewritePlan>, Vec<FixPlan>, Vec<FailedFix>, Vec<FixPlan>);

/// What one dry-run pass would do: `(would_modify, unapplied, rejected)` —
/// see [`plan_fixes_dry_run`].
pub type DryRunOutcome = (Vec<String>, Vec<FixPlan>, Vec<FixPlan>);

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

/// Count how many links in `index` are site-absolute (`/foo/bar`) and how
/// many of those look plausibly resolvable — after `site_prefix` stripping,
/// the remaining path's first segment names a real top-level entry in the
/// vault — versus ones that don't.
///
/// NEW-9 (dogfood pre3): a `site_prefix` that strips *something* but not
/// *enough* looks, from a naive "did the string change" check, identical to
/// one that worked. On a real MDN checkout the auto-derived prefix (`en-us`,
/// the last path segment of `--dir`) case-insensitively strips the `en-US/`
/// segment from every `/en-US/docs/Web/...` link (iter-204 made that match
/// case-insensitive) — but MDN's on-disk layout has no top-level `docs/`
/// directory at all, only `web/`, `mdn/`, `glossary/`, etc., so the result
/// (`docs/Web/...`) still resolves nowhere. Comparing against the vault's own
/// top-level entries catches this in one pass over the index, without a
/// second full link-resolution run: measured against this repo's real MDN
/// checkout, the derived prefix leaves ~0% of site-absolute links
/// plausible, the correct two-segment `en-US/docs` prefix ~100%.
pub fn site_prefix_plausible_resolution_stats(
    index: &dyn VaultIndex,
    site_prefix: Option<&str>,
) -> (usize, usize) {
    // PR #251 review N13: `split('/').next()` on any `&str` (including
    // empty) always yields `Some`, so `filter_map` never actually filters
    // anything here — `map` says that plainly.
    let top_level: std::collections::HashSet<String> = index
        .entries()
        .iter()
        .map(|e| {
            e.rel_path
                .split('/')
                .next()
                .unwrap_or_default()
                .to_lowercase()
        })
        .collect();

    let mut absolute = 0usize;
    let mut plausible = 0usize;
    for entry in index.entries() {
        for (_, link) in &entry.links {
            let normalized = link.target.replace('\\', "/");
            if !normalized.starts_with('/') {
                continue;
            }
            let stripped = strip_site_prefix(&normalized, site_prefix);
            let first_segment = stripped.split('/').next().unwrap_or("");
            // PR #251 review L5: a bare `/` (site-root link, e.g. `[home](/)`)
            // has no path segment at all to check plausibility against —
            // stripping the leading slash always leaves an empty string,
            // regardless of `site_prefix`. Counting it in `absolute` only
            // padded the denominator toward a false "stripped 0 of N"; it
            // carries no signal either way, so it is excluded entirely
            // rather than counted as "not plausible".
            if first_segment.is_empty() {
                continue;
            }
            absolute += 1;
            if top_level.contains(&first_segment.to_lowercase()) {
                plausible += 1;
            }
        }
    }
    (absolute, plausible)
}

/// Count links whose TARGET resolves to a real vault file but whose
/// `#fragment` does not name any heading there — a broken *anchor*, distinct
/// from a broken target ([`detect_broken_links_from_index`] never reports
/// these; it only checks whether the target file exists).
///
/// NEW-15 / UX-2 (dogfood pre3): `summary` and `find --broken-links` used to
/// disagree on what "broken" counts — `summary` said "0 broken" on a vault
/// `find --broken-links` reported 3 files for, because `summary`'s notion of
/// broken never looked at anchors at all. Mirrors `find`'s own
/// `LinkInfo::broken_anchor` computation so every caller counts the same
/// thing. Same-file fragments (`[b](#nope)`, indexed separately as
/// `entry.self_anchors`) are not included — this counts only links that
/// point *at another file's* heading, matching what `links fix`'s target
/// resolution already covers.
///
/// Returns `None` when `dir` cannot be canonicalized (matching
/// [`detect_broken_links_from_index`]'s own empty-report fallback for the
/// same failure) — PR #251 review L6: the first cut returned `0` here, which
/// a caller cannot distinguish from "genuinely checked, found none." `None`
/// says "could not check" honestly instead of asserting a clean bill for a
/// vault this function never actually looked at.
pub fn count_broken_anchors(
    dir: &Path,
    index: &dyn VaultIndex,
    site_prefix: Option<&str>,
    case_index: Option<&CaseInsensitiveIndex>,
) -> Option<usize> {
    let canonical = canonicalize_vault_dir(dir).ok()?;
    let mut count = 0usize;
    for entry in index.entries() {
        for (_, link) in &entry.links {
            let Some(fragment) = &link.fragment else {
                continue;
            };
            let resolved = crate::discovery::resolve_link_from_source(
                &canonical,
                &entry.rel_path,
                link.kind,
                &link.target,
                site_prefix,
                case_index,
            );
            if let Some(target_path) = resolved
                && let Some(target_entry) = index.get(&target_path)
                && !crate::anchor::fragment_matches_headings(fragment, &target_entry.sections)
            {
                count += 1;
            }
        }
    }
    Some(count)
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
            relocations: Vec::new(),
            ambiguous: Vec::new(),
            out_of_vault: Vec::new(),
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
    let mut relocations: Vec<FixPlan> = Vec::new();
    let mut ambiguous: Vec<BrokenLinkInfo> = Vec::new();
    let mut out_of_vault: Vec<BrokenLinkInfo> = Vec::new();

    for entry in index.entries() {
        for (line, link) in &entry.links {
            total_links += 1;

            let (resolved_target, resolution) = classify_link_from_source(
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
                LinkResolution::StemRelocation(canonical_str) => {
                    // iter-211 / BUG-12: the exact path failed and the bare
                    // stem resolved somewhere else — a relocation, not a
                    // casing fix. Report it under its real strategy name and
                    // confidence so `links fix` stops printing
                    // `[link-case-mismatch]` next to two targets that differ
                    // by a directory. Gating is unchanged and consistent with
                    // DEC-076: the written target carried no directory (that
                    // is the only way the stem fallback fires), so this is the
                    // documented short-form rule, not a guess.
                    //
                    // NEW-13 (dogfood pre3): reported in its own `relocations`
                    // bucket, not `case_mismatches` — a relocation is not a
                    // cosmetic casing fix, and lumping the two made the "Case
                    // mismatches" count lie about what changed.
                    relocations.push(FixPlan {
                        source: entry.rel_path.clone(),
                        line: *line,
                        old_target: link.target.clone(),
                        new_target: canonical_str,
                        strategy: FixStrategy::ShortestPath,
                        confidence: SHORTEST_PATH_CONFIDENCE,
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
                    let info = BrokenLinkInfo {
                        source: entry.rel_path.clone(),
                        line: *line,
                        target: link.target.clone(),
                    };
                    // A target that still starts with `..` after normalization
                    // names a file above the vault root — out of scope, not
                    // broken (iter-193).
                    if crate::discovery::normalized_target_escapes_vault(&resolved_target) {
                        out_of_vault.push(info);
                    } else {
                        broken.push(info);
                    }
                }
            }
        }
    }

    broken.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.line.cmp(&b.line)));
    case_mismatches.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.line.cmp(&b.line)));
    relocations.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.line.cmp(&b.line)));
    ambiguous.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.line.cmp(&b.line)));
    out_of_vault.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.line.cmp(&b.line)));

    BrokenLinkReport {
        total_links,
        broken,
        case_mismatches,
        relocations,
        ambiguous,
        out_of_vault,
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
/// 4. Fuzzy match — Jaro-Winkler on the filename stem decides *candidacy*
///    (`--threshold`), [`crate::link_score::candidate_confidence`] decides
///    ranking and the reported confidence.
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
    /// Minimum Jaro-Winkler stem score for a file to be considered a fuzzy
    /// candidate at all (`--threshold`). Candidates that clear it are then
    /// ranked by [`crate::link_score::candidate_confidence`].
    threshold: f64,
    /// Site prefix stripped from site-absolute targets before matching, so a
    /// link written `/docs/a/b.md` is compared against the vault path
    /// `a/b.md` (iter-200).
    site_prefix: Option<String>,
    /// Per-file filename stems (`.md` stripped), parallel to `files` —
    /// precomputed at build so the fuzzy pass (and its iter-206 shortlist
    /// cache) never re-derives them per broken link.
    stems: Vec<String>,
    /// Lazy threshold-gated fuzzy candidate shortlist per distinct target
    /// stem (iter-206). The Jaro-Winkler candidacy gate over the whole vault
    /// is by far the dominant cost of `links fix` on link-heavy corpora
    /// (profiled at ~87% of samples in `find_match`: broken-link count ×
    /// vault-size calls to `strsim::jaro_winkler`). Broken targets repeat
    /// heavily across a vault (the same site-absolute URL appears in many
    /// pages), so the gate is computed once per *distinct* target stem and
    /// shared by every broken link with that stem. The per-link work that
    /// remains — self-link filtering and `candidate_confidence` ranking over
    /// the shortlist — still runs per link because it depends on `source`.
    fuzzy_shortlists: RefCell<HashMap<String, Rc<Vec<usize>>>>,
}

/// Result of a single match attempt.
pub(crate) struct MatchResult {
    /// Vault-relative path of the matched file.
    pub matched_file: String,
    pub strategy: FixStrategy,
    pub confidence: f64,
}

/// Confidence reported for [`FixStrategy::ShortestPath`] matches — a unique
/// bare-stem resolution. Below 1.0 because the vault could gain a second file
/// with the same stem, but well above the fuzzy gate: DEC-076 treats it as a
/// certain fix.
pub(crate) const SHORTEST_PATH_CONFIDENCE: f64 = 0.95;

/// Lower bound on the confidence reported for a [`FixStrategy::BasenameFallback`]
/// match — the value a candidate gets when its basename matches exactly but its
/// directory shares nothing at all with the path the author wrote.
///
/// Deliberately below the 0.95 of a genuine short-form stem match: the only
/// evidence is the filename, and the directory the author actually wrote
/// contradicts it. Since iter-212 the reported confidence is no longer this
/// flat constant — it is [`candidate_confidence`], which adds up to
/// [`link_score::DIR_WEIGHT`] back for directory overlap, so a same-basename
/// *relocation* inside a related subtree outranks a cross-tree substitution.
/// The floor equals [`link_score::BASENAME_WEIGHT`] by construction.
pub const BASENAME_FALLBACK_CONFIDENCE: f64 = link_score::BASENAME_WEIGHT;

impl LinkMatcher {
    /// Build a matcher from a list of vault-relative file paths.
    ///
    /// Equivalent to [`LinkMatcher::with_site_prefix`] with no site prefix.
    pub fn new(files: Vec<String>, threshold: f64) -> Self {
        Self::with_site_prefix(files, threshold, None)
    }

    /// Build a matcher that understands site-absolute targets.
    ///
    /// Broken-link targets reach [`find_match`](Self::find_match) exactly as
    /// the author wrote them, but the index is keyed by vault-relative paths.
    /// Without the prefix strip, a site-absolute target could never satisfy
    /// the exact/case-insensitive/extension strategies — every site-absolute
    /// link fell through to the basename guess (dogfood H-1/M-1).
    pub fn with_site_prefix(files: Vec<String>, threshold: f64, site_prefix: Option<&str>) -> Self {
        let mut lower_to_idx = HashMap::with_capacity(files.len());
        let mut exact_to_idx = HashMap::with_capacity(files.len());
        let mut stem_to_indices: HashMap<String, Vec<usize>> = HashMap::new();
        let mut stems = Vec::with_capacity(files.len());

        for (i, f) in files.iter().enumerate() {
            // Filename stem, precomputed for the fuzzy pass (iter-206).
            let fname0 = f.rsplit('/').next().unwrap_or(f.as_str());
            stems.push(fname0.strip_suffix(".md").unwrap_or(fname0).to_string());
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
            site_prefix: site_prefix.map(std::string::ToString::to_string),
            stems,
            fuzzy_shortlists: RefCell::new(HashMap::new()),
        }
    }

    /// Build a matcher from an index (avoids rescanning the directory).
    pub fn from_index(index: &dyn VaultIndex, threshold: f64, site_prefix: Option<&str>) -> Self {
        let files: Vec<String> = index.entries().iter().map(|e| e.rel_path.clone()).collect();
        Self::with_site_prefix(files, threshold, site_prefix)
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

    /// Threshold-gated fuzzy candidate indices for `target_stem` (iter-206).
    ///
    /// Computes (once per distinct target stem, then cached) the indices of
    /// every file whose stem clears `--threshold` under Jaro-Winkler. This is
    /// the expensive full-vault pass; sharing it across the many broken links
    /// that share a target stem turns the O(broken × vault) gate into
    /// O(distinct stems × vault).
    fn fuzzy_shortlist(&self, target_stem: &str) -> Rc<Vec<usize>> {
        if let Some(hit) = self.fuzzy_shortlists.borrow().get(target_stem) {
            return Rc::clone(hit);
        }
        let shortlist: Vec<usize> = self
            .stems
            .iter()
            .enumerate()
            .filter(|(_, fstem)| {
                strsim::jaro_winkler(target_stem, fstem.as_str()) >= self.threshold
            })
            .map(|(i, _)| i)
            .collect();
        let rc = Rc::new(shortlist);
        self.fuzzy_shortlists
            .borrow_mut()
            .insert(target_stem.to_string(), Rc::clone(&rc));
        rc
    }

    /// Try to find a matching file for a broken link target.
    ///
    /// `source` is the vault-relative path of the file that contains the
    /// broken link.  Candidates that resolve back to `source` are skipped so
    /// the matcher never proposes a self-referential fix.
    ///
    /// Returns `None` if no match is found above the configured threshold.
    pub(crate) fn find_match(&self, written_target: &str, source: &str) -> Option<MatchResult> {
        // Minimum score difference to avoid ambiguous fuzzy matches.
        const TIE_DELTA: f64 = 0.01;

        // A site-absolute target (`/docs/a/b.md`) names a path from the site
        // root; the index is keyed by vault-relative paths, so strip the
        // leading `/` and any configured site prefix before matching.
        // Everything below then works on vault-relative text.
        let stripped;
        let raw_target = if written_target.starts_with('/') {
            stripped = strip_site_prefix(written_target, self.site_prefix.as_deref());
            stripped.as_str()
        } else {
            written_target
        };

        let target_filename = raw_target.rsplit('/').next().unwrap_or(raw_target);
        let target_stem = target_filename
            .strip_suffix(".md")
            .unwrap_or(target_filename);

        // DEC-076: a written directory component is a *location claim*. Both
        // the strategy-3 gate and the iter-212 confidence scorer key on it, so
        // decide once, on the target exactly as the author wrote it — `/actions`
        // claims the site root even though stripping the prefix leaves a bare
        // `actions` behind.
        let asserts_path = written_target.contains('/') || written_target.contains('\\');

        // Coordinate system for scoring: the candidate list is vault-relative,
        // so a relative target has to be resolved against its source directory
        // before its directories mean anything. `../c/target.md` written in
        // `a/b/page.md` is a claim about `a/c/`, not about a directory literally
        // named `..`.
        let score_target: std::borrow::Cow<'_, str> =
            if asserts_path && !written_target.starts_with('/') {
                std::borrow::Cow::Owned(normalize_target(Path::new(source), raw_target))
            } else {
                std::borrow::Cow::Borrowed(raw_target)
            };

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
        //
        // Split by how the broken target was written (M-1). A *site-absolute*
        // target (`/actions`) asserts a path from the site root; matching it
        // by basename alone means discarding that assertion and substituting a
        // same-named file from somewhere else entirely — on GitHub Docs this
        // turned `[GitHub Actions](/actions)` into
        // `graphql/reference/actions.md`, 17 times, under a plain `--apply`.
        // That is a guess and belongs behind the fuzzy gate. Bare and
        // relative targets keep the long-standing shortest-path treatment:
        // for them the basename is the reliable signal (short-form semantics,
        // or a stale relative path after a move), and the H-1 round-trip
        // guard now guarantees whatever gets written actually resolves.
        let target_stem_lower = target_stem.to_ascii_lowercase();
        if let Some(indices) = self.stem_to_indices.get(&target_stem_lower)
            && indices.len() == 1
            && !Self::is_self_link(source, &self.files[indices[0]])
        {
            // DEC-076 (iter-211 / BUG-12): the discriminator is whether the
            // author wrote a *directory*, not whether the path happened to
            // start with `/`. Splitting on the leading slash meant
            // `[x](guides/actions)` was rewritten to `reference/actions.md`
            // by a plain `--apply` while the byte-identical `/guides/actions`
            // needed `--apply-fuzzy` — the same guess, two gates, impossible
            // to explain. A bare stem (`[[actions]]`, `[x](actions.md)`)
            // carries no location claim, so resolving it by basename is the
            // documented short-form rule and stays a certain fix; any written
            // directory component is a location claim, and discarding it is a
            // guess that belongs behind the fuzzy gate.
            // `asserts_path` is decided above on `written_target`, NOT on the
            // prefix-stripped `raw_target`: `/actions` still names a location
            // (the site root) even though stripping leaves a bare `actions`.
            let (strategy, confidence) = if asserts_path {
                // iter-212: the confidence is no longer the flat
                // BASENAME_FALLBACK_CONFIDENCE. The basename matches exactly
                // (that is why we are here), so `candidate_confidence` reduces
                // to `BASENAME_WEIGHT + DIR_WEIGHT * directory_similarity`:
                // a relocation within a related subtree
                // (`code-security/how-tos/a/x` → `code-security/how-tos/b/x`)
                // now outranks a cross-tree substitution (`/actions` →
                // `graphql/reference/actions.md`), which stays at exactly the
                // 0.7 floor and therefore below the default apply floor.
                (
                    FixStrategy::BasenameFallback,
                    candidate_confidence_with_claim(&score_target, &self.files[indices[0]], true),
                )
            } else {
                (FixStrategy::ShortestPath, SHORTEST_PATH_CONFIDENCE)
            };
            return Some(MatchResult {
                matched_file: self.files[indices[0]].clone(),
                strategy,
                confidence,
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
        // ambiguous. Since iter-212 the threshold is applied as a per-candidate
        // admission gate before scoring, so it can never enter either slot.
        //
        // iter-212: candidacy is still gated by the raw Jaro-Winkler stem
        // score against `--threshold` (unchanged semantics, and a cheap filter
        // that keeps the composite scorer off the ~99% of the vault that could
        // never win), but *ranking* and the reported confidence now come from
        // [`candidate_confidence`], which weights the basename above the
        // directory instead of rewarding a shared prefix.
        // iter-206: the expensive full-vault Jaro-Winkler candidacy gate now
        // runs once per *distinct* target stem (`fuzzy_shortlist` cache);
        // this loop only ranks the survivors. Per-link semantics are
        // unchanged: candidacy is still gated by the raw stem score against
        // `--threshold`, and ranking/confidence still come from
        // [`candidate_confidence`] (iter-212). The shortlist is *not*
        // filtered for self-links here — self-link filtering depends on
        // `source`, which the cache cannot see — so it is applied per link
        // below, exactly as before.
        let shortlist = self.fuzzy_shortlist(target_stem);
        let mut best_score = f64::NEG_INFINITY;
        let mut second_score = f64::NEG_INFINITY;
        let mut best_idx: Option<usize> = None;

        for &i in shortlist.iter() {
            let candidate = &self.files[i];
            if Self::is_self_link(source, candidate) {
                continue;
            }
            let score = candidate_confidence_with_claim(&score_target, candidate, asserts_path);
            if score > best_score {
                second_score = best_score;
                best_score = score;
                best_idx = Some(i);
            } else if score > second_score {
                second_score = score;
            }
        }

        // Every surviving candidate already cleared `--threshold` on the stem
        // gate above, so there is no second floor to apply here.
        let best_idx = best_idx?;

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
/// `threshold` is the minimum Jaro-Winkler stem score (0.0–1.0) for a file to
/// be considered a fuzzy candidate; the confidence attached to the winning
/// candidate is [`crate::link_score::candidate_confidence`].
pub fn plan_fixes(broken: &[BrokenLinkInfo], matcher: &LinkMatcher) -> FixReport {
    let mut fixes = Vec::new();
    let mut unfixable = Vec::new();
    let mut templated = Vec::new();

    for info in broken {
        // Template expressions are dynamic destinations, not broken paths
        // (iter-207, BUG-4): never offer a rewrite for them.
        if is_templated_target(&info.target) {
            templated.push(info.clone());
        } else if let Some(result) = matcher.find_match(&info.target, &info.source) {
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

    FixReport {
        fixes,
        unfixable,
        templated,
    }
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
/// Returns `(applied_plans, unapplied, failed, rejected)` where:
/// - `applied_plans` are the [`RewritePlan`]s that were durably written to disk.
/// - `unapplied` lists input [`FixPlan`]s that produced no [`Replacement`]
///   (e.g. because the on-disk text no longer matched what detection saw, or
///   the file exceeded the size limit).
/// - `failed` lists fixes whose file produced a valid plan but the durable
///   write failed mid-batch (L-11); remaining files still get written.
/// - `rejected` lists fixes refused by the H-1 round-trip guard: the emitted
///   target would not have resolved, so nothing was written and the caller
///   must report them as unfixable rather than fixed.
///
/// Callers must treat `unapplied`, `failed`, and `rejected` fixes as NOT
/// applied when reporting results, and set a non-zero exit code when `failed`
/// is non-empty.
pub fn apply_fixes(
    dir: &Path,
    fixes: &[FixPlan],
    site_prefix: Option<&str>,
) -> Result<ApplyOutcome> {
    // Group fixes by source file.
    let mut by_source: HashMap<&str, Vec<&FixPlan>> = HashMap::new();
    for fix in fixes {
        by_source.entry(fix.source.as_str()).or_default().push(fix);
    }

    let mut plans: Vec<RewritePlan> = Vec::new();
    let mut unapplied: Vec<FixPlan> = Vec::new();
    // Fixes refused by the H-1 round-trip guard (emitted target would not
    // resolve) — reported to the caller as unfixable, never written.
    let mut rejected: Vec<FixPlan> = Vec::new();
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

        let (replacements, satisfied, guard_rejected) =
            build_replacements_for_file(&content, source_rel, file_fixes, site_prefix);

        let mut satisfied_fixes: Vec<FixPlan> = Vec::new();
        for (idx, fix) in file_fixes.iter().enumerate() {
            if guard_rejected.contains(&idx) {
                rejected.push((*fix).clone());
            } else if satisfied.contains(&idx) {
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

    rejected.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.line.cmp(&b.line)));
    Ok((applied_plans, unapplied, failed, rejected))
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
/// Returns `(would_modify, unapplied, rejected)` where `would_modify` is the
/// set of vault-relative paths that would receive at least one rewrite,
/// `unapplied` lists the fixes whose on-disk text no longer matches, and
/// `rejected` lists the fixes the H-1 round-trip guard refuses (their emitted
/// target would not resolve, so `--apply` would report them as unfixable).
pub fn plan_fixes_dry_run(
    dir: &Path,
    fixes: &[FixPlan],
    site_prefix: Option<&str>,
) -> Result<DryRunOutcome> {
    let mut by_source: HashMap<&str, Vec<&FixPlan>> = HashMap::new();
    for fix in fixes {
        by_source.entry(fix.source.as_str()).or_default().push(fix);
    }

    let mut would_modify: Vec<String> = Vec::new();
    let mut unapplied: Vec<FixPlan> = Vec::new();
    let mut rejected: Vec<FixPlan> = Vec::new();

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

        let (replacements, satisfied, guard_rejected) =
            build_replacements_for_file(&content, source_rel, file_fixes, site_prefix);

        for (idx, fix) in file_fixes.iter().enumerate() {
            if guard_rejected.contains(&idx) {
                rejected.push((*fix).clone());
            } else if !satisfied.contains(&idx) {
                unapplied.push((*fix).clone());
            }
        }

        if !replacements.is_empty() {
            would_modify.push((*source_rel).to_string());
        }
    }

    would_modify.sort();
    unapplied.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.line.cmp(&b.line)));
    rejected.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.line.cmp(&b.line)));
    Ok((would_modify, unapplied, rejected))
}

// ---------------------------------------------------------------------------
// Emission: turning a vault-relative fix target into on-page link text
// ---------------------------------------------------------------------------

/// Strip a trailing `.md` (case-insensitively) from a target string.
fn strip_md_suffix(target: &str) -> &str {
    if target.len() > 3 && target.as_bytes()[target.len() - 3..].eq_ignore_ascii_case(b".md") {
        &target[..target.len() - 3]
    } else {
        target
    }
}

/// Compute the destination text to write for a fixed **markdown** link.
///
/// [`FixPlan::new_target`] is always *vault-relative*, but the read-side
/// resolver reads a bare markdown destination as *file-relative* and a
/// leading-`/` destination as *site-absolute*. Writing the vault-relative path
/// verbatim therefore only round-trips when the source file happens to sit at
/// the vault root — everywhere else the rewritten link cannot resolve, and on
/// a site-absolute corpus every single rewrite was corruption (dogfood H-1:
/// 1,097 GitHub Docs files modified, broken count 6,565 → 6,582).
///
/// Emission rules, mirroring [`crate::link_write::LinkWriter`]:
/// - site-absolute in ⇒ site-absolute out (re-attaching `site_prefix`);
/// - otherwise a path relative to the *source file's directory*;
/// - the original's `.md` presence/absence is preserved either way.
fn emit_markdown_fix_target(
    raw_target: &str,
    new_vault_rel: &str,
    source_rel: &str,
    site_prefix: Option<&str>,
) -> String {
    let had_md = raw_target.len() > 3
        && raw_target.as_bytes()[raw_target.len() - 3..].eq_ignore_ascii_case(b".md");

    if raw_target.starts_with('/') {
        let body = if had_md {
            new_vault_rel
        } else {
            strip_md_suffix(new_vault_rel)
        };
        // Re-attach the site prefix only when the *original* link carried it.
        // `site_prefix` is auto-derived from the vault directory name when
        // nothing is configured, so injecting it unconditionally would invent
        // a path segment the author never wrote (dogfood L-11).
        let author_used_prefix = site_prefix.is_some_and(|prefix| {
            let prefix = prefix.trim_matches('/');
            !prefix.is_empty()
                && raw_target[1..]
                    .strip_prefix(prefix)
                    .is_some_and(|rest| rest.starts_with('/'))
        });
        return match site_prefix.filter(|_| author_used_prefix) {
            Some(prefix) => format!("/{}/{}", prefix.trim_matches('/'), body),
            None => format!("/{body}"),
        };
    }

    let rel = relative_path_between(source_rel, new_vault_rel);
    if had_md {
        rel
    } else {
        strip_md_suffix(&rel).to_string()
    }
}

/// Whether the emitted markdown destination reads back — through the exact
/// normalization the read-side resolver applies — as `new_vault_rel`.
///
/// This is the H-1 *guard*: any writer/resolver asymmetry (present or future)
/// turns into a refused fix and a visible `unfixable` count instead of a
/// silently corrupted link. `.md` presence is ignored on both sides because
/// [`crate::discovery::resolve_target`] appends the extension itself.
fn markdown_fix_round_trips(
    emitted: &str,
    new_vault_rel: &str,
    source_rel: &str,
    site_prefix: Option<&str>,
) -> bool {
    let normalized = if emitted.starts_with('/') {
        strip_site_prefix(emitted, site_prefix)
    } else {
        normalize_target(Path::new(source_rel), emitted)
    };
    strip_md_suffix(&normalized) == strip_md_suffix(new_vault_rel)
}

/// Whether the emitted wikilink target reads back as `new_vault_rel`.
///
/// Wikilink targets are vault-relative as written, so a path-form emission has
/// to match exactly; a bare stem is accepted when it is the basename of the
/// target (Obsidian short-form, which detection only proposes for stems that
/// are unique in the vault).
fn wikilink_fix_round_trips(emitted: &str, new_vault_rel: &str) -> bool {
    let emitted = strip_md_suffix(emitted);
    let target = strip_md_suffix(new_vault_rel);
    emitted == target
        || (!emitted.contains('/')
            && !emitted.contains('\\')
            && target.rsplit('/').next() == Some(emitted))
}

/// Walk `content` line by line and build [`Replacement`]s for all link fixes
/// that apply to this file — both `[[wikilink]]`s inside YAML frontmatter
/// link properties and links in the document body (code fences and Obsidian
/// comment fences are skipped for the latter).
///
/// Returns `(replacements, satisfied, rejected)` where `satisfied` holds the
/// indices (into `fixes`) of plans that were matched to an on-disk occurrence
/// and `rejected` the subset of those whose emitted target failed the H-1
/// round-trip guard (see [`markdown_fix_round_trips`]) and was therefore *not*
/// turned into a `Replacement`. Tracking is per-occurrence: each on-disk match
/// consumes the first not-yet-satisfied plan with that target, so duplicate
/// plans for the same `(line, old_target)` — a legitimate case when the same
/// broken target appears twice — are only satisfied by distinct occurrences.
/// Callers use the unsatisfied remainder to detect fixes whose on-disk text no
/// longer matches what detection saw (stale plan) so they are never
/// misreported as applied, and `rejected` to report them as unfixable.
fn build_replacements_for_file(
    content: &str,
    source_rel: &str,
    fixes: &[&FixPlan],
    site_prefix: Option<&str>,
) -> (
    Vec<Replacement>,
    std::collections::HashSet<usize>,
    std::collections::HashSet<usize>,
) {
    // Index fixes by line number for O(1) lookup during the scan, carrying
    // each plan's index into `fixes` for per-occurrence satisfaction
    // tracking.
    let mut fixes_by_line: HashMap<usize, Vec<(usize, &FixPlan)>> = HashMap::new();
    for (idx, fix) in fixes.iter().enumerate() {
        fixes_by_line.entry(fix.line).or_default().push((idx, fix));
    }

    let mut replacements = Vec::new();
    let mut satisfied: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut rejected: std::collections::HashSet<usize> = std::collections::HashSet::new();
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

            // Compute new target text based on link kind, then verify it
            // round-trips through the read-side resolver (H-1 guard).
            let (new_target_text, round_trips) = match span.kind {
                LinkKind::Wikilink => {
                    // Use stem (without .md) for wikilinks; wikilink targets
                    // are vault-relative as written, so the plan's target is
                    // already in the right coordinate system.
                    let emitted = strip_md_suffix(&fix.new_target).to_string();
                    let ok = wikilink_fix_round_trips(&emitted, &fix.new_target);
                    (emitted, ok)
                }
                LinkKind::Markdown => {
                    // The plan's target is vault-relative; a markdown
                    // destination is read as site-absolute or file-relative.
                    let emitted = emit_markdown_fix_target(
                        &span.link.target,
                        &fix.new_target,
                        source_rel,
                        site_prefix,
                    );
                    let ok = markdown_fix_round_trips(
                        &emitted,
                        &fix.new_target,
                        source_rel,
                        site_prefix,
                    );
                    (emitted, ok)
                }
            };

            // A fix whose emitted target would not resolve is never written:
            // it is consumed (so a duplicate plan is not re-matched) and
            // reported as unfixable instead of corrupting the link.
            if !round_trips {
                satisfied.insert(fix_idx);
                rejected.insert(fix_idx);
                continue;
            }

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

    (replacements, satisfied, rejected)
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
                    size: 0,
                    lines: 0,
                    properties: indexmap::IndexMap::default(),
                    tags: Vec::new(),
                    sections: Vec::new(),
                    tasks: Vec::new(),
                    links: links.clone(),
                    self_anchors: Vec::new(),
                    bm25_tokens: None,
                    bm25_language: None,
                    bm25_tokenizer_version: None,
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

    // --- iter-206: fuzzy shortlist cache semantics ---

    #[test]
    fn matcher_shortlist_cache_repeated_target_same_result() {
        // The same broken target asked twice (as happens across many source
        // files in a real vault) must produce byte-identical match results —
        // the cached candidacy shortlist cannot change per-link ranking,
        // self-link filtering, or tie detection.
        let matcher = LinkMatcher::new(
            make_files(&["authentication.md", "authentication-backup.md", "other.md"]),
            0.7,
        );
        let first = matcher.find_match("authentcation", "__test__").unwrap();
        for source in ["__test__", "other.md", "authentication.md"] {
            let again = matcher.find_match("authentcation", source);
            // The self-link filter depends on `source`, so `again` may be None
            // for `authentication.md` — but for the non-self sources it must
            // equal the first result exactly.
            if source != "authentication.md" {
                let again = again.unwrap();
                assert_eq!(again.matched_file, first.matched_file);
                assert_eq!(again.confidence.to_bits(), first.confidence.to_bits());
            } else if let Some(m) = again {
                assert_ne!(m.matched_file, first.matched_file);
            }
        }
    }

    #[test]
    fn matcher_shortlist_cache_does_not_leak_self_link_filter() {
        // The cached shortlist is shared across sources, but the self-link
        // filter is per-source: asking for a target whose best fuzzy
        // candidate IS the source file must still skip that candidate, even
        // when the shortlist was already cached by an earlier call from a
        // different source that legitimately matched it.
        let files = make_files(&["authentication.md", "totally-unrelated.md"]);
        let matcher = LinkMatcher::new(files, 0.7);
        // Warm the cache from a different source: "authentication" matches.
        let warm = matcher.find_match("authentication-typo", "totally-unrelated.md");
        assert!(warm.is_some());
        // Now the same target from `authentication.md` itself: the cached
        // shortlist still contains it, but the per-link self-link filter must
        // exclude it (only the unrelated file may win, or nothing at all).
        if let Some(m) = matcher.find_match("authentication-typo", "authentication.md") {
            assert_ne!(m.matched_file, "authentication.md");
        }
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
        let (repls, _, _) = build_replacements_for_file(content, "a.md", &[&fix], None);
        assert_eq!(repls.len(), 1, "one frontmatter link repaired: {repls:?}");
        assert_eq!(repls[0].old_text, "[[decision-log#DEC-041]]");
        assert_eq!(repls[0].new_text, "[[decision-log-archive#DEC-041]]");
    }

    #[test]
    fn build_replacements_frontmatter_repair_preserves_anchor_and_alias() {
        let content = "---\nrelated:\n  - \"[[decision-log#DEC-041|Log]]\"\n---\nBody\n";
        let fix = fm_fix("decision-log", "decision-log-archive.md");
        let (repls, _, _) = build_replacements_for_file(content, "a.md", &[&fix], None);
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
        let (repls, _, _) = build_replacements_for_file(content, "a.md", &[&fix], None);
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

    // --- iter-207 BUG-4: templated destinations are never rewritten ---

    #[test]
    fn is_templated_target_recognizes_the_three_marker_forms() {
        assert!(is_templated_target(
            "{% ifversion ghes %}/admin{% endif %}/guides"
        ));
        assert!(is_templated_target("{{ site.baseurl }}/guides"));
        assert!(is_templated_target("${BASE}/guides"));
        assert!(!is_templated_target("guides/index.md"));
        // A bare brace is a legal (if odd) filename character, not a template.
        assert!(!is_templated_target("weird{name}.md"));
    }

    /// The literal text of a Liquid conditional fuzzy-matches a real file well
    /// above the 0.95 threshold, so `links fix --apply` used to rewrite it and
    /// silently drop the version conditional. The round-trip guard cannot see
    /// this: the rewritten target genuinely resolves.
    #[test]
    fn plan_fixes_routes_templated_targets_to_their_own_bucket() {
        let matcher = LinkMatcher::new(make_files(&["guides.md"]), 0.7);
        let broken_links = vec![
            broken("src.md", 1, "{% ifversion ghes %}/admin{% endif %}/guides"),
            broken("src.md", 2, "{{ site.baseurl }}/guides"),
            broken("src.md", 3, "${BASE}/guides"),
            broken("src.md", 4, "guidez"),
        ];
        let report = plan_fixes(&broken_links, &matcher);
        assert_eq!(report.templated.len(), 3, "{:?}", report.templated);
        assert!(
            report.templated.iter().all(|b| b.target.contains("guides")),
            "templated links keep their original target text"
        );
        assert_eq!(report.fixes.len(), 1, "the real typo is still fixable");
        assert_eq!(report.fixes[0].old_target, "guidez");
        assert!(report.unfixable.is_empty());
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
                        fragment: None,
                        query: None,
                        embed: false,
                        external: false,
                    },
                ),
                (
                    2,
                    Link {
                        target: "missing".to_string(),
                        label: None,
                        kind: LinkKind::Wikilink,
                        fragment: None,
                        query: None,
                        embed: false,
                        external: false,
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

    // --- detect_broken_links_from_index: out-of-vault bucket (iter-193) ---

    #[test]
    fn detect_broken_links_buckets_out_of_vault_targets() {
        use crate::links::{Link, LinkKind};

        let tmp = vault_with_files(&[("sub/a.md", ""), ("existing.md", "")]);

        let index = mock_index(
            "sub/a.md",
            vec![
                // Walks above the vault root — out of scope, not broken.
                (
                    1,
                    Link {
                        target: "../../outside/thing.md".to_string(),
                        label: None,
                        kind: LinkKind::Markdown,
                        fragment: None,
                        query: None,
                        embed: false,
                        external: false,
                    },
                ),
                // Stays inside the vault and simply misses — genuinely broken.
                (
                    2,
                    Link {
                        target: "../gone.md".to_string(),
                        label: None,
                        kind: LinkKind::Markdown,
                        fragment: None,
                        query: None,
                        embed: false,
                        external: false,
                    },
                ),
            ],
            &["existing.md"],
        );

        let report = detect_broken_links_from_index(tmp.path(), &index, None, None, false);

        assert_eq!(report.total_links, 2);
        assert_eq!(
            report.out_of_vault.len(),
            1,
            "escaping target belongs in out_of_vault: {:?}",
            report.out_of_vault
        );
        assert_eq!(report.out_of_vault[0].target, "../../outside/thing.md");
        assert_eq!(
            report.broken.len(),
            1,
            "in-vault miss must stay broken: {:?}",
            report.broken
        );
        assert_eq!(report.broken[0].target, "../gone.md");
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
                        fragment: None,
                        query: None,
                        embed: false,
                        external: false,
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
                            fragment: None,
                            query: None,
                            embed: false,
                            external: false,
                        },
                    ),
                    (
                        1,
                        Link {
                            target: "nope".to_string(),
                            label: None,
                            kind: LinkKind::Wikilink,
                            fragment: None,
                            query: None,
                            embed: false,
                            external: false,
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

    // -----------------------------------------------------------------
    // iter-200 H-1: emission must round-trip through the read-side resolver
    // -----------------------------------------------------------------

    #[test]
    fn emit_site_absolute_target_stays_site_absolute() {
        // Dogfood H-1 minimal repro: `/how-tos/old-home/moved-page` in
        // `docs/page.md`, real file at `how-tos/new-home/moved-page.md`.
        // Dropping the leading `/` produced a target the resolver reads as
        // relative to `docs/`, i.e. permanently broken.
        let emitted = emit_markdown_fix_target(
            "/how-tos/old-home/moved-page",
            "how-tos/new-home/moved-page.md",
            "docs/page.md",
            None,
        );
        assert_eq!(emitted, "/how-tos/new-home/moved-page");
        assert!(markdown_fix_round_trips(
            &emitted,
            "how-tos/new-home/moved-page.md",
            "docs/page.md",
            None
        ));
    }

    #[test]
    fn emit_site_absolute_preserves_authors_site_prefix() {
        let emitted = emit_markdown_fix_target(
            "/docs/old/page.md",
            "new/page.md",
            "sub/linker.md",
            Some("docs"),
        );
        assert_eq!(emitted, "/docs/new/page.md");
        assert!(markdown_fix_round_trips(
            &emitted,
            "new/page.md",
            "sub/linker.md",
            Some("docs")
        ));
    }

    #[test]
    fn emit_site_absolute_does_not_inject_a_derived_site_prefix() {
        // `site_prefix` is auto-derived from the vault directory name when
        // nothing is configured; a link the author wrote without it must not
        // grow one (dogfood L-11).
        let emitted = emit_markdown_fix_target(
            "/how-tos/old/page",
            "how-tos/new/page.md",
            "index.md",
            Some("my-vault"),
        );
        assert_eq!(emitted, "/how-tos/new/page");
    }

    #[test]
    fn emit_relative_target_is_relative_to_the_source_directory() {
        // A vault-relative target written verbatim into a nested file is the
        // same H-1 asymmetry without the leading slash.
        let emitted =
            emit_markdown_fix_target("../c/target.md", "z/target.md", "a/b/page.md", None);
        assert_eq!(emitted, "../../z/target.md");
        assert!(markdown_fix_round_trips(
            &emitted,
            "z/target.md",
            "a/b/page.md",
            None
        ));
    }

    #[test]
    fn emit_preserves_md_suffix_style() {
        assert_eq!(
            emit_markdown_fix_target("wrong", "sub/right.md", "index.md", None),
            "sub/right"
        );
        assert_eq!(
            emit_markdown_fix_target("wrong.md", "sub/right.md", "index.md", None),
            "sub/right.md"
        );
    }

    #[test]
    fn round_trip_guard_rejects_vault_relative_emission_from_a_nested_source() {
        // The pre-iter-200 writer emitted exactly this: the vault-relative
        // path, verbatim, from a nested source. The guard must reject it.
        assert!(!markdown_fix_round_trips(
            "how-tos/new-home/moved-page",
            "how-tos/new-home/moved-page.md",
            "docs/page.md",
            None
        ));
    }

    #[test]
    fn round_trip_guard_accepts_wikilink_path_and_short_forms() {
        assert!(wikilink_fix_round_trips("sub/note", "sub/note.md"));
        assert!(wikilink_fix_round_trips("note", "sub/note.md"));
        assert!(!wikilink_fix_round_trips("other", "sub/note.md"));
    }

    #[test]
    fn apply_fixes_site_absolute_link_resolves_after_rewrite() {
        let tmp = vault_with_files(&[
            (
                "docs/page.md",
                "See [AUTOTITLE](/how-tos/old-home/moved-page) here.\n",
            ),
            ("how-tos/new-home/moved-page.md", ""),
        ]);

        let fixes = vec![FixPlan {
            source: "docs/page.md".to_string(),
            line: 1,
            old_target: "/how-tos/old-home/moved-page".to_string(),
            new_target: "how-tos/new-home/moved-page.md".to_string(),
            strategy: FixStrategy::BasenameFallback,
            confidence: BASENAME_FALLBACK_CONFIDENCE,
        }];

        let (plans, unapplied, _failed, rejected) = apply_fixes(tmp.path(), &fixes, None).unwrap();
        assert_eq!(plans.len(), 1);
        assert!(unapplied.is_empty(), "unexpected unapplied: {unapplied:?}");
        assert!(rejected.is_empty(), "unexpected rejected: {rejected:?}");

        let written = fs::read_to_string(tmp.path().join("docs").join("page.md")).unwrap();
        assert!(
            written.contains("[AUTOTITLE](/how-tos/new-home/moved-page)"),
            "site-absolute form must survive the rewrite, got: {written}"
        );

        // The rewritten link must actually resolve.
        let canonical = crate::discovery::canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            crate::discovery::resolve_target(
                &canonical,
                "/how-tos/new-home/moved-page",
                None,
                None
            )
            .as_deref(),
            Some("how-tos/new-home/moved-page.md"),
            "rewritten target must resolve"
        );
    }

    #[test]
    fn apply_fixes_refuses_a_fix_whose_emitted_target_would_not_resolve() {
        // Stand-in for any future writer/resolver asymmetry: a `new_target`
        // whose text the resolver normalizes to something else. The guard must
        // turn that into a refusal (reported as unfixable) rather than a
        // corrupted link.
        let tmp = vault_with_files(&[
            ("index.md", "See [text](wrong.md) for details.\n"),
            ("b.md", ""),
        ]);

        let fixes = vec![FixPlan {
            source: "index.md".to_string(),
            line: 1,
            old_target: "wrong.md".to_string(),
            new_target: "a/../b.md".to_string(),
            strategy: FixStrategy::ShortestPath,
            confidence: 0.95,
        }];

        let (plans, unapplied, _failed, rejected) = apply_fixes(tmp.path(), &fixes, None).unwrap();
        assert!(plans.is_empty(), "nothing may be written: {plans:?}");
        assert!(unapplied.is_empty(), "not a stale-text case: {unapplied:?}");
        assert_eq!(rejected.len(), 1, "guard must reject the fix: {rejected:?}");

        let written = fs::read_to_string(tmp.path().join("index.md")).unwrap();
        assert_eq!(
            written, "See [text](wrong.md) for details.\n",
            "file must be untouched"
        );
    }

    #[test]
    fn dry_run_reports_the_same_guard_rejection_as_apply() {
        let tmp = vault_with_files(&[
            ("index.md", "See [text](wrong.md) for details.\n"),
            ("b.md", ""),
        ]);

        let fixes = vec![FixPlan {
            source: "index.md".to_string(),
            line: 1,
            old_target: "wrong.md".to_string(),
            new_target: "a/../b.md".to_string(),
            strategy: FixStrategy::ShortestPath,
            confidence: 0.95,
        }];

        let (would_modify, unapplied, rejected) =
            plan_fixes_dry_run(tmp.path(), &fixes, None).unwrap();
        assert!(would_modify.is_empty(), "nothing would change");
        assert!(unapplied.is_empty());
        assert_eq!(rejected.len(), 1);
    }

    // -----------------------------------------------------------------
    // iter-200 M-1: site-absolute basename guesses are gated
    // -----------------------------------------------------------------

    #[test]
    fn site_absolute_basename_match_is_a_gated_fallback() {
        let matcher = LinkMatcher::new(
            make_files(&["actions/index.md", "graphql/reference/actions.md"]),
            0.85,
        );
        let result = matcher
            .find_match("/actions", "index.md")
            .expect("basename match still found");
        assert!(
            matches!(result.strategy, FixStrategy::BasenameFallback),
            "site-absolute basename guess must not masquerade as a certain fix: {:?}",
            result.strategy
        );
        assert!(
            result.confidence < 1.0,
            "a guess must not claim confidence 1.0"
        );
    }

    #[test]
    fn relative_path_basename_match_is_a_basename_fallback() {
        // DEC-076 (iter-211): a written directory is a location claim whatever
        // the leading character, so discarding it is the same guess as for a
        // site-absolute target and lands in the same gated bucket.
        let matcher = LinkMatcher::new(make_files(&["z/target.md"]), 0.85);
        let result = matcher
            .find_match("../c/target.md", "a/b/page.md")
            .expect("basename match found");
        assert!(
            matches!(result.strategy, FixStrategy::BasenameFallback),
            "expected a gated guess, got {:?}",
            result.strategy
        );
        assert!(result.confidence < 0.95);
    }

    #[test]
    fn bare_stem_match_stays_shortest_path() {
        // The other half of DEC-076: no directory written ⇒ the Obsidian
        // short-form rule ⇒ a certain fix.
        let matcher = LinkMatcher::new(make_files(&["z/target.md"]), 0.85);
        let result = matcher
            .find_match("target.md", "a/b/page.md")
            .expect("basename match found");
        assert!(
            matches!(result.strategy, FixStrategy::ShortestPath),
            "expected a certain short-form fix, got {:?}",
            result.strategy
        );
        assert!((result.confidence - SHORTEST_PATH_CONFIDENCE).abs() < f64::EPSILON);
    }

    #[test]
    fn site_absolute_case_mismatch_is_a_certain_fix() {
        // Before iter-200 the leading `/` made every strategy but the basename
        // guess unreachable for site-absolute targets.
        let matcher = LinkMatcher::new(make_files(&["how-tos/moved-page.md"]), 0.85);
        let result = matcher
            .find_match("/how-tos/Moved-Page", "a/b/page.md")
            .expect("case-insensitive match found");
        assert!(
            matches!(result.strategy, FixStrategy::CaseInsensitive),
            "expected a certain case fix, got {:?}",
            result.strategy
        );
        assert_eq!(result.matched_file, "how-tos/moved-page.md");
    }

    #[test]
    fn site_prefix_is_stripped_before_matching() {
        let matcher =
            LinkMatcher::with_site_prefix(make_files(&["how-tos/page.md"]), 0.85, Some("docs"));
        let result = matcher
            .find_match("/docs/how-tos/Page", "index.md")
            .expect("prefixed site-absolute target must match");
        assert_eq!(result.matched_file, "how-tos/page.md");
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

        let (plans, unapplied, _failed, _rejected) = apply_fixes(tmp.path(), &fixes, None).unwrap();

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

        let (plans, unapplied, _failed, _rejected) = apply_fixes(tmp.path(), &fixes, None).unwrap();

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

        let (plans, unapplied, _failed, _rejected) = apply_fixes(tmp.path(), &fixes, None).unwrap();

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

        let (plans, unapplied, _failed, _rejected) = apply_fixes(tmp.path(), &fixes, None).unwrap();

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

        let (plans, unapplied, _failed, _rejected) = apply_fixes(tmp.path(), &fixes, None).unwrap();

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

        let (plans, unapplied, _failed, _rejected) = apply_fixes(tmp.path(), &fixes, None).unwrap();

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

        let (plans, unapplied, _failed, _rejected) = apply_fixes(tmp.path(), &fixes, None).unwrap();

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

        let (plans, unapplied, _failed, _rejected) = apply_fixes(tmp.path(), &fixes, None).unwrap();

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

        let (plans, unapplied, _failed, _rejected) = apply_fixes(tmp.path(), &fixes, None).unwrap();

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

        let (plans, unapplied, _failed, _rejected) = apply_fixes(tmp.path(), &fixes, None).unwrap();

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

        let (plans, unapplied, _failed, _rejected) = apply_fixes(tmp.path(), &fixes, None).unwrap();

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
                    fragment: None,
                    query: None,
                    embed: false,
                    external: false,
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

    // --- NEW-9 (dogfood pre3): site_prefix plausible-resolution stats ---

    /// Mirrors the real MDN repro: on-disk layout has a top-level `web/` (no
    /// `docs/`), links are spelled `/en-US/docs/Web/...`. The single-segment
    /// derived prefix `en-us` case-insensitively strips `en-US/` (iter-204),
    /// leaving `docs/Web/...` — `docs` is not a real top-level entry, so this
    /// must not count as plausibly resolved even though the string changed.
    #[test]
    fn site_prefix_plausible_resolution_distinguishes_under_and_correctly_stripped() {
        use crate::links::{Link, LinkKind};

        let link = |target: &str| Link {
            target: target.to_string(),
            label: None,
            kind: LinkKind::Markdown,
            fragment: None,
            query: None,
            embed: false,
            external: false,
        };
        let index = mock_index(
            "source.md",
            vec![
                (1, link("/en-US/docs/Web/A")),
                (2, link("/en-US/docs/Web/B")),
                (3, link("relative.md")),
            ],
            &["web/a.md", "web/b.md"],
        );

        // Under-stripped: `docs` is not a real top-level vault entry.
        let (absolute, plausible) = site_prefix_plausible_resolution_stats(&index, Some("en-us"));
        assert_eq!(absolute, 2, "two site-absolute links, one relative");
        assert_eq!(
            plausible, 0,
            "the single-segment prefix leaves a `docs` segment nothing in the vault has"
        );

        // The correct multi-segment prefix leaves `Web/...`, which matches
        // the real top-level `web/` entry case-insensitively.
        let (absolute, plausible) =
            site_prefix_plausible_resolution_stats(&index, Some("en-US/docs"));
        assert_eq!(absolute, 2);
        assert_eq!(plausible, 2);
    }

    #[test]
    fn site_prefix_plausible_resolution_zero_absolute_links_is_not_a_misconfiguration() {
        use crate::links::{Link, LinkKind};

        let index = mock_index(
            "source.md",
            vec![(
                1,
                Link {
                    target: "relative.md".to_string(),
                    label: None,
                    kind: LinkKind::Markdown,
                    fragment: None,
                    query: None,
                    embed: false,
                    external: false,
                },
            )],
            &[],
        );
        let (absolute, plausible) = site_prefix_plausible_resolution_stats(&index, Some("en-us"));
        assert_eq!(
            absolute, 0,
            "a vault with no site-absolute links must not be flagged"
        );
        assert_eq!(plausible, 0);
    }

    // --- NEW-15 / UX-2 (dogfood pre3): count_broken_anchors ---

    /// A minimal [`VaultIndex`] over hand-built entries, for tests that need
    /// real `sections`/`self_anchors` data `mock_index` does not expose.
    struct HeadingsMockIndex(Vec<crate::index::IndexEntry>);
    impl VaultIndex for HeadingsMockIndex {
        fn entries(&self) -> &[crate::index::IndexEntry] {
            &self.0
        }
        fn get(&self, rel_path: &str) -> Option<&crate::index::IndexEntry> {
            self.0.iter().find(|e| e.rel_path == rel_path)
        }
        fn link_graph(&self) -> &crate::link_graph::LinkGraph {
            unreachable!("not exercised by count_broken_anchors tests")
        }
    }

    /// A link whose target resolves but whose `#fragment` names no heading
    /// there must be counted; a link to a real heading must not.
    #[test]
    fn count_broken_anchors_counts_only_dead_fragments_on_resolving_targets() {
        use crate::links::{Link, LinkKind};

        let link = |target: &str, fragment: &str| Link {
            target: target.to_string(),
            label: None,
            kind: LinkKind::Markdown,
            fragment: Some(fragment.to_string()),
            query: None,
            embed: false,
            external: false,
        };
        let make_entry = |rel_path: &str, links: Vec<(usize, Link)>| crate::index::IndexEntry {
            rel_path: rel_path.to_string(),
            modified: String::new(),
            size: 0,
            lines: 0,
            properties: indexmap::IndexMap::default(),
            tags: Vec::new(),
            sections: Vec::new(),
            tasks: Vec::new(),
            links,
            self_anchors: Vec::new(),
            bm25_tokens: None,
            bm25_language: None,
            bm25_tokenizer_version: None,
        };

        let mut target = make_entry("target.md", Vec::new());
        target.sections = vec![crate::types::OutlineSection {
            level: 1,
            heading: Some("Real".to_string()),
            line: 1,
            links: Vec::new(),
            tasks: None,
            code_blocks: Vec::new(),
        }];
        let source = make_entry(
            "source.md",
            vec![
                (1, link("target.md", "Real")),
                (2, link("target.md", "nope")),
            ],
        );

        let index = HeadingsMockIndex(vec![source, target]);
        let tmp = vault_with_files(&[("source.md", ""), ("target.md", "")]);

        let count = count_broken_anchors(tmp.path(), &index, None, None);
        assert_eq!(
            count,
            Some(1),
            "only the dead fragment (#nope) must count, not the resolving one (#Real)"
        );
    }

    #[test]
    fn count_broken_anchors_ignores_same_file_fragments() {
        // Same-file fragments live in `entry.self_anchors`, never
        // `entry.links` — count_broken_anchors only walks `entry.links`, so a
        // dead same-file anchor (find's own broken_anchor concern) must not
        // be double-counted here regardless of how many self_anchors exist.
        let entry = crate::index::IndexEntry {
            rel_path: "source.md".to_string(),
            modified: String::new(),
            size: 0,
            lines: 0,
            properties: indexmap::IndexMap::default(),
            tags: Vec::new(),
            sections: Vec::new(),
            tasks: Vec::new(),
            links: Vec::new(),
            self_anchors: vec![(1, "nope".to_string())],
            bm25_tokens: None,
            bm25_language: None,
            bm25_tokenizer_version: None,
        };

        let index = HeadingsMockIndex(vec![entry]);
        let tmp = vault_with_files(&[("source.md", "")]);
        assert_eq!(
            count_broken_anchors(tmp.path(), &index, None, None),
            Some(0)
        );
    }

    /// PR #251 review L6: a vault directory that cannot be canonicalized
    /// must report "could not check" (`None`), not a false `Some(0)` clean
    /// bill — mirrors `detect_broken_links_from_index`'s own empty-report
    /// fallback for the identical failure.
    #[test]
    fn count_broken_anchors_returns_none_when_dir_cannot_be_canonicalized() {
        let index = HeadingsMockIndex(Vec::new());
        let nonexistent = std::path::Path::new("/definitely/does/not/exist/anywhere");
        assert_eq!(
            count_broken_anchors(nonexistent, &index, None, None),
            None,
            "an uncanonicalizable directory must report 'could not check', not a clean zero"
        );
    }

    /// NEW-13 (dogfood pre3): a bare-stem relocation — the exact path fails,
    /// the stem resolves to a *different directory* — must land in
    /// `relocations`, not `case_mismatches`. Before the fix both were counted
    /// as "Case mismatches", presenting a move as a cosmetic casing fix.
    #[test]
    fn detect_broken_links_stem_relocation_is_not_a_case_mismatch() {
        use crate::case_index::CaseInsensitiveIndex;
        use crate::links::{Link, LinkKind};

        // On-disk: `sub/target.md`. Link written as bare `target.md` — the
        // exact path (`target.md` at vault root) does not exist, so
        // resolution falls back to the bare-stem lookup and finds it in a
        // different directory: a relocation, not a casing difference.
        let tmp = vault_with_files(&[("sub/target.md", ""), ("source.md", "[a](target.md)")]);

        let mut idx = CaseInsensitiveIndex::new();
        idx.insert("sub/target.md");

        let index = mock_index(
            "source.md",
            vec![(
                1,
                Link {
                    target: "target.md".to_string(),
                    label: Some("a".to_string()),
                    kind: LinkKind::Markdown,
                    fragment: None,
                    query: None,
                    embed: false,
                    external: false,
                },
            )],
            &["sub/target.md"],
        );

        let report = detect_broken_links_from_index(tmp.path(), &index, None, Some(&idx), false);

        assert_eq!(
            report.case_mismatches.len(),
            0,
            "a directory relocation must not be counted as a case mismatch; report: {report:#?}"
        );
        assert_eq!(
            report.relocations.len(),
            1,
            "the relocation must appear in its own bucket; report: {report:#?}"
        );
        let fix = &report.relocations[0];
        assert!(
            matches!(fix.strategy, FixStrategy::ShortestPath),
            "relocation must use the ShortestPath strategy, got: {:?}",
            fix.strategy
        );
        assert_eq!(fix.old_target, "target.md");
        assert_eq!(fix.new_target, "sub/target.md");
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
                    fragment: None,
                    query: None,
                    embed: false,
                    external: false,
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
                    fragment: None,
                    query: None,
                    embed: false,
                    external: false,
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
                    fragment: None,
                    query: None,
                    embed: false,
                    external: false,
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
                    fragment: None,
                    query: None,
                    embed: false,
                    external: false,
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

        let (would_modify, unapplied, _rejected) =
            plan_fixes_dry_run(tmp.path(), &fixes, None).unwrap();
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

        let (would_modify_dry, unapplied_dry, _rejected_dry) =
            plan_fixes_dry_run(tmp.path(), &fixes, None).unwrap();
        assert!(would_modify_dry.is_empty(), "stale fix modifies nothing");
        assert_eq!(
            unapplied_dry.len(),
            1,
            "stale fix must be reported unapplied"
        );

        // apply must report the identical unapplied set.
        let (plans, unapplied_apply, failed, _rejected) =
            apply_fixes(tmp.path(), &fixes, None).unwrap();
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

        let (plans, unapplied, failed, _rejected) = apply_fixes(tmp.path(), &fixes, None)
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

        let (would_modify, unapplied, _rejected) =
            plan_fixes_dry_run(tmp.path(), &fixes, None).unwrap();
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

        let (would_modify, unapplied, _rejected) =
            plan_fixes_dry_run(tmp.path(), &fixes, None).unwrap();
        assert!(
            would_modify.is_empty(),
            "an oversized file must modify nothing"
        );
        assert_eq!(unapplied.len(), 1, "the fix must be reported unapplied");
        assert_eq!(unapplied[0].old_target, "wrongname");
    }
}
