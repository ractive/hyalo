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

use crate::discovery::canonicalize_vault_dir;
use crate::discovery::resolve_target;
use crate::index::VaultIndex;
use crate::link_graph::{FileLinks, normalize_target};
use crate::link_rewrite::{Replacement, RewritePlan, apply_replacements, execute_plans};
use crate::links::{LinkKind, extract_link_spans_with_original};
use crate::scanner::{FenceTracker, is_comment_fence, strip_inline_code, strip_inline_comments};
use crate::types::BrokenLinkInfo;

// ---------------------------------------------------------------------------
// Report types
// ---------------------------------------------------------------------------

/// Summary of broken link detection across the vault.
#[derive(Debug, Clone, Serialize)]
pub struct BrokenLinkReport {
    pub total_links: usize,
    pub broken: Vec<BrokenLinkInfo>,
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
// Broken link detection
// ---------------------------------------------------------------------------

/// Detect broken links from pre-collected file links data.
///
/// `file_links` is a slice of [`FileLinks`] (from `link_graph.rs`).
/// Uses [`resolve_target`] to check if each link target exists.
pub fn detect_broken_links(
    dir: &Path,
    file_links: &[FileLinks],
    site_prefix: Option<&str>,
) -> BrokenLinkReport {
    let canonical = match canonicalize_vault_dir(dir) {
        Ok(p) => p,
        Err(_) => {
            return BrokenLinkReport {
                total_links: 0,
                broken: Vec::new(),
            };
        }
    };

    let mut total_links = 0usize;
    let mut broken: Vec<BrokenLinkInfo> = Vec::new();

    for fl in file_links {
        let source_str = fl.source.to_string_lossy().replace('\\', "/");

        for (line, link) in &fl.links {
            total_links += 1;

            // Normalize the target before resolution.
            // Wikilinks are vault-relative; markdown links may be relative to
            // the source file's directory.
            let resolved_target = match link.kind {
                LinkKind::Wikilink => link.target.clone(),
                LinkKind::Markdown => {
                    if link.target.starts_with('/') {
                        link.target.clone()
                    } else if link.target.contains('/') || link.target.contains('\\') {
                        normalize_target(Path::new(&source_str), &link.target)
                    } else {
                        link.target.clone()
                    }
                }
            };

            if resolve_target(&canonical, &resolved_target, site_prefix).is_none() {
                broken.push(BrokenLinkInfo {
                    source: source_str.clone(),
                    line: *line,
                    target: link.target.clone(),
                });
            }
        }
    }

    broken.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.line.cmp(&b.line)));

    BrokenLinkReport {
        total_links,
        broken,
    }
}

/// Detect broken links from index entries.
///
/// Each [`IndexEntry`](crate::index::IndexEntry) has
/// `links: Vec<(usize, Link)>` and `rel_path: String`.
pub fn detect_broken_links_from_index(
    dir: &Path,
    index: &dyn VaultIndex,
    site_prefix: Option<&str>,
) -> BrokenLinkReport {
    let canonical = match canonicalize_vault_dir(dir) {
        Ok(p) => p,
        Err(_) => {
            return BrokenLinkReport {
                total_links: 0,
                broken: Vec::new(),
            };
        }
    };

    let mut total_links = 0usize;
    let mut broken: Vec<BrokenLinkInfo> = Vec::new();

    for entry in index.entries() {
        for (line, link) in &entry.links {
            total_links += 1;

            let resolved_target = match link.kind {
                LinkKind::Wikilink => link.target.clone(),
                LinkKind::Markdown => {
                    if link.target.starts_with('/') {
                        link.target.clone()
                    } else if link.target.contains('/') || link.target.contains('\\') {
                        normalize_target(Path::new(&entry.rel_path), &link.target)
                    } else {
                        link.target.clone()
                    }
                }
            };

            if resolve_target(&canonical, &resolved_target, site_prefix).is_none() {
                broken.push(BrokenLinkInfo {
                    source: entry.rel_path.clone(),
                    line: *line,
                    target: link.target.clone(),
                });
            }
        }
    }

    broken.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.line.cmp(&b.line)));

    BrokenLinkReport {
        total_links,
        broken,
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
pub struct MatchResult {
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
                    .map(|s| s.to_string())
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
    pub fn find_match(&self, raw_target: &str, source: &str) -> Option<MatchResult> {
        let target_filename = raw_target.rsplit('/').next().unwrap_or(raw_target);
        let target_stem = target_filename
            .strip_suffix(".md")
            .unwrap_or(target_filename);

        // --- Strategy 1: Case-insensitive exact match ---
        // `target_lower` is also used for the exact-case alt computation below.
        let target_lower = raw_target.to_ascii_lowercase();

        // Precompute the exact-case alt form so strategy 1 doesn't steal strategy 2 hits.
        // Check the .md suffix on the lowercased form to avoid a case-sensitive comparison.
        let exact_alt = if target_lower.ends_with(".md") {
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
        // 0.01 of each other the match is ambiguous and we return None rather than
        // silently picking the first.
        const TIE_DELTA: f64 = 0.01;
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
/// for every fix in that file, applies them via [`apply_replacements`], and
/// writes back via [`execute_plans`].
///
/// Returns the list of [`RewritePlan`]s for reporting.
pub fn apply_fixes(
    dir: &Path,
    fixes: &[FixPlan],
    site_prefix: Option<&str>,
) -> Result<Vec<RewritePlan>> {
    // Group fixes by source file.
    let mut by_source: HashMap<&str, Vec<&FixPlan>> = HashMap::new();
    for fix in fixes {
        by_source.entry(fix.source.as_str()).or_default().push(fix);
    }

    let mut plans: Vec<RewritePlan> = Vec::new();

    for (source_rel, file_fixes) in &by_source {
        let abs_path = dir.join(source_rel.replace('\\', "/"));
        let content = std::fs::read_to_string(&abs_path)
            .with_context(|| format!("reading {}", abs_path.display()))?;

        let replacements =
            build_replacements_for_file(&content, source_rel, file_fixes, site_prefix);

        if !replacements.is_empty() {
            let rewritten_content = apply_replacements(&content, &replacements);
            plans.push(RewritePlan {
                path: abs_path,
                rel_path: source_rel.to_string(),
                replacements,
                rewritten_content,
            });
        }
    }

    execute_plans(dir, &plans)?;

    Ok(plans)
}

/// Walk `content` line by line (skipping frontmatter, code fences, comment
/// fences) and build [`Replacement`]s for all link fixes that apply to this
/// file.
fn build_replacements_for_file(
    content: &str,
    source_rel: &str,
    fixes: &[&FixPlan],
    _site_prefix: Option<&str>,
) -> Vec<Replacement> {
    // Index fixes by line number for O(1) lookup during the scan.
    let mut fixes_by_line: HashMap<usize, Vec<&FixPlan>> = HashMap::new();
    for fix in fixes {
        fixes_by_line.entry(fix.line).or_default().push(fix);
    }

    let mut replacements = Vec::new();
    let mut fence = FenceTracker::new();
    let mut in_comment_fence = false;
    let mut in_frontmatter = false;
    let mut frontmatter_done = false;
    let mut line_num = 0usize;

    for line in content.split('\n') {
        line_num += 1;

        // --- Frontmatter ---
        if !frontmatter_done {
            if line_num == 1 && line.trim() == "---" {
                in_frontmatter = true;
                continue;
            }
            if in_frontmatter {
                if line.trim() == "---" {
                    in_frontmatter = false;
                    frontmatter_done = true;
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

            // Find the fix for this particular span.
            let Some(fix) = line_fixes.iter().find(|f| {
                f.old_target == normalized_span_target || f.old_target == span.link.target
            }) else {
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
        }
    }

    replacements
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
        names.iter().map(|s| s.to_string()).collect()
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
        assert_eq!(result.confidence, 1.0);
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
        assert_eq!(result.confidence, 0.95);
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

        let report = detect_broken_links(tmp.path(), &file_links, None);

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

        let report = detect_broken_links(tmp.path(), &file_links, None);

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

        let plans = apply_fixes(tmp.path(), &fixes, None).unwrap();

        assert_eq!(plans.len(), 1);
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

        let plans = apply_fixes(tmp.path(), &fixes, None).unwrap();

        assert_eq!(plans.len(), 1);
        let written = fs::read_to_string(tmp.path().join("index.md")).unwrap();
        assert!(
            written.contains("[text](correct.md)"),
            "expected rewritten link, got: {written}"
        );
    }
}
