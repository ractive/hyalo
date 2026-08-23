//! Auto-link: scan markdown body text for unlinked mentions of known page
//! titles and propose (or apply) `[[wikilink]]` replacements.
//!
//! The public entry point is [`auto_link`].

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use aho_corasick::{AhoCorasick, MatchKind};
use anyhow::{Context, Result};
use serde::Serialize;

use globset::{GlobBuilder, GlobSetBuilder};

use crate::discovery::match_globs;
use crate::index::{IndexEntry, VaultIndex};
use crate::link_rewrite::{Replacement, RewritePlan, apply_replacements, execute_plans_partial};
use crate::links::{extract_link_spans_with_original, strip_wikilink_md_suffix};
use crate::scanner::{LineClass, LineScanner, MAX_FILE_SIZE, lines_with_rest};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Options for the [`auto_link`] function.
pub struct AutoLinkOptions<'a> {
    pub apply: bool,
    pub min_length: usize,
    pub exclude_titles: &'a [String],
    pub first_only: bool,
    pub exclude_target_globs: &'a [String],
    pub file_filter: Option<&'a str>,
    pub glob_filter: &'a [String],
}

/// Serialize a 0-based in-memory offset as a 1-based JSON position (L-15).
///
/// `serde`'s `serialize_with` contract fixes the by-reference signature, so the
/// trivially-copy lint does not apply here.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn serialize_one_based<S>(value: &usize, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_u64(u64::try_from(value.saturating_add(1)).unwrap_or(u64::MAX))
}

/// A single proposed auto-link replacement.
#[derive(Debug, Clone, Serialize)]
pub struct AutoLinkMatch {
    /// Vault-relative path of the file containing the unlinked mention.
    pub file: String,
    /// 1-based line number.
    pub line: usize,
    /// Byte offset of the match within its line, 0-based.
    ///
    /// Internal only — it indexes straight into the line's bytes when building
    /// the replacement, so it must stay a byte offset. It is **not**
    /// serialized: JSON reports [`AutoLinkMatch::col`] instead, which counts
    /// Unicode scalars like lint's `column` does (iter-210).
    #[serde(skip)]
    pub byte_col: usize,
    /// Column of the match within its line, counted in Unicode scalar values.
    ///
    /// Held 0-based in memory but **serialized 1-based** under `col`, so every
    /// position in hyalo's JSON — `line` and `col` alike — counts from one
    /// (L-15) and counts *characters*, matching `lint`'s `column` (iter-210).
    /// Before iter-210 this was a byte index, so a match after a multibyte
    /// character reported a column no editor agreed with.
    #[serde(serialize_with = "serialize_one_based")]
    pub col: usize,
    /// The matched text as it appears in the file.
    pub matched_text: String,
    /// The wikilink target (file stem or title) to link to.
    pub link_target: String,
}

/// Per-file outcome when `--apply` runs (L-11 honest envelope).
///
/// One record per file that had at least one proposed match. `applied` files
/// were durably rewritten; `skipped`/`failed` files carry a human-readable
/// `reason`. Previously the skip and failure conditions only surfaced as
/// stderr warnings and never appeared in the JSON envelope.
#[derive(Debug, Clone, Serialize)]
pub struct AutoApplyOutcome {
    /// Vault-relative path (forward slashes) of the file.
    pub file: String,
    /// What happened to this file: `"applied"`, `"skipped"`, or `"failed"`.
    pub status: AutoApplyStatus,
    /// Human-readable reason for a skip or failure; `None` when applied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Discriminant for [`AutoApplyOutcome`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AutoApplyStatus {
    /// The file was durably rewritten on disk.
    Applied,
    /// The file was skipped before writing (not in scan cache, or its on-disk
    /// content changed after the scan).
    Skipped,
    /// The write itself failed (e.g. read-only target, I/O error).
    Failed,
}

/// Result of the auto-link scan.
#[derive(Debug, Serialize)]
pub struct AutoLinkReport {
    /// Number of files scanned.
    pub scanned: usize,
    /// Total number of proposed auto-link replacements.
    pub total: usize,
    /// The proposed replacements, grouped or flat.
    pub matches: Vec<AutoLinkMatch>,
    /// Titles that were skipped due to ambiguity (multiple files share the
    /// same title).
    pub ambiguous_titles: Vec<String>,
    /// Whether changes were applied to disk.
    pub applied: bool,
    /// Per-file apply outcomes (empty in preview mode). Includes applied,
    /// skipped, and failed records so the caller can emit an honest envelope
    /// (L-11).
    pub apply_outcomes: Vec<AutoApplyOutcome>,
}

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

/// One entry in the title inventory: maps a lowercased title to the
/// wikilink target and the source file (to detect self-links).
#[derive(Debug)]
struct TitleEntry {
    /// The wikilink target: the filename stem (without `.md`, without dir).
    link_target: String,
    /// The source file's vault-relative path (to detect self-links).
    source_rel: String,
}

// ---------------------------------------------------------------------------
// Title inventory builder
// ---------------------------------------------------------------------------

/// Build a title inventory from index entries.
///
/// Returns `(title_to_target map, ambiguous_titles list)`.
/// `title_to_target` maps `lowercased_title` → `TitleEntry`.
/// When two different files produce the same lowercased title the entry is
/// removed and the title is placed in `ambiguous_titles`.
fn build_title_inventory(
    entries: &[IndexEntry],
    min_length: usize,
    exclude_titles: &[String],
    exclude_target_globs: &[String],
) -> Result<(HashMap<String, TitleEntry>, Vec<String>)> {
    // Build --exclude-target-glob filter up front so excluded entries never
    // participate in ambiguity detection (fixes false ambiguities when one of
    // two conflicting pages is glob-excluded).
    let glob_set = if exclude_target_globs.is_empty() {
        None
    } else {
        let mut builder = GlobSetBuilder::new();
        for pat in exclude_target_globs {
            builder.add(
                GlobBuilder::new(pat)
                    .literal_separator(true)
                    // Match case-insensitively to mirror `--exclude-title`,
                    // which lowercases both sides before comparing. Without
                    // this, `--exclude-target-glob 'Templates/*'` would fail to
                    // exclude `templates/foo.md` — a surprising asymmetry.
                    .case_insensitive(true)
                    .build()
                    .context("invalid --exclude-target-glob pattern")?,
            );
        }
        Some(
            builder
                .build()
                .context("failed to build --exclude-target-glob globset")?,
        )
    };

    let exclude_lower: HashSet<String> = exclude_titles
        .iter()
        .map(|s| s.to_ascii_lowercase())
        .collect();

    // Track ambiguity: a title key that maps to `None` was seen from 2+ different source files.
    let mut map: HashMap<String, Option<TitleEntry>> = HashMap::new();
    let mut ambiguous: Vec<String> = Vec::new();

    let mut try_insert = |title: &str, entry: TitleEntry| {
        if title.len() < min_length {
            return;
        }
        let key = title.to_ascii_lowercase();
        if exclude_lower.contains(&key) {
            return;
        }
        match map.get(&key) {
            None => {
                map.insert(key, Some(entry));
            }
            Some(Some(existing)) if existing.source_rel != entry.source_rel => {
                // Conflict: two different files produce the same lowercased title.
                ambiguous.push(title.to_owned());
                map.insert(key, None);
            }
            Some(_) => {
                // Same source file — keep whichever was first (stem usually wins,
                // but the order of iteration doesn't matter for correctness).
            }
        }
    };

    for entry in entries {
        let rel = &entry.rel_path;

        // Skip entries whose path matches an --exclude-target-glob pattern.
        if let Some(ref gs) = glob_set
            && gs.is_match(rel)
        {
            continue;
        }

        // 1. Filename stem.
        let stem = stem_from_rel(rel);
        try_insert(
            stem,
            TitleEntry {
                link_target: stem.to_owned(),
                source_rel: rel.clone(),
            },
        );

        // 2. Frontmatter `title` property.
        if let Some(title_val) = entry.properties.get("title")
            && let Some(title_str) = title_val.as_str()
        {
            let title_str = title_str.trim();
            if !title_str.is_empty() {
                try_insert(
                    title_str,
                    TitleEntry {
                        link_target: stem.to_owned(),
                        source_rel: rel.clone(),
                    },
                );
            }
        }

        // 3. Frontmatter `aliases` property (list of alternate names).
        if let Some(aliases_val) = entry.properties.get("aliases") {
            let aliases: Vec<&str> = if let Some(arr) = aliases_val.as_array() {
                arr.iter().filter_map(|v| v.as_str()).collect()
            } else if let Some(s) = aliases_val.as_str() {
                // YAML scalar: treat as a single alias.
                vec![s]
            } else {
                vec![]
            };

            for alias in aliases {
                let alias = alias.trim();
                if !alias.is_empty() {
                    try_insert(
                        alias,
                        TitleEntry {
                            link_target: stem.to_owned(),
                            source_rel: rel.clone(),
                        },
                    );
                }
            }
        }
    }

    // Flatten: keep only unambiguous entries.
    let mut title_map: HashMap<String, TitleEntry> = map
        .into_iter()
        .filter_map(|(k, v)| v.map(|entry| (k, entry)))
        .collect();

    // Second pass: a `link_target` (stem) that maps to 2+ different source
    // files is ambiguous even when the *title keys* are distinct.  For
    // example, `projects/apple.md` (title "Apple Inc") and
    // `companies/apple.md` (title "Apple Company") both generate
    // `link_target = "apple"` — emitting `[[apple]]` would be wrong.
    //
    // Computed from `entries` directly (every non-excluded file's own stem),
    // not from `title_map.values()` (iter-217, NEW-4 real-corpus gap): a
    // conflicting file's own title-map entries can vanish from `title_map`
    // for a completely unrelated reason — e.g. its title collides
    // first-pass with a *third* file's identical title — which would hide
    // that file's stem contribution from a check keyed off survivors alone.
    // On GitHub Docs, `rest/pulls/pulls.md` and `rest/pulls/index.md` share
    // the exact title "REST API endpoints for pull requests", so
    // `rest/pulls/pulls.md`'s title entry never reached `title_map` — but
    // its plain stem `pulls` still collided with `graphql/reference/pulls.md`,
    // and that collision has to be visible regardless of what else happened
    // to either file's title.
    let mut target_sources: HashMap<String, HashSet<String>> = HashMap::new();
    for entry in entries {
        let rel = &entry.rel_path;
        if let Some(ref gs) = glob_set
            && gs.is_match(rel)
        {
            continue;
        }
        target_sources
            .entry(stem_from_rel(rel).to_owned())
            .or_default()
            .insert(rel.clone());
    }
    let ambiguous_targets: HashSet<String> = target_sources
        .into_iter()
        .filter(|(_, sources)| sources.len() > 1)
        .map(|(target, _)| target)
        .collect();

    if !ambiguous_targets.is_empty() {
        title_map.retain(|_, entry| !ambiguous_targets.contains(&entry.link_target));
        for target in &ambiguous_targets {
            if !ambiguous.iter().any(|a| a.eq_ignore_ascii_case(target)) {
                ambiguous.push(target.clone());
            }
        }
    }

    Ok((title_map, ambiguous))
}

/// Auto-link exclusion inputs from one source (CLI flags, or the
/// `[links.auto]` section of `.hyalo.toml`).
///
/// Used by [`count_config_excluded_titles`] to attribute suppressed candidate
/// titles to the config rather than to the flags.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExclusionSets<'a> {
    /// Titles never linked (case-insensitive), like `--exclude-title`.
    pub exclude_titles: &'a [String],
    /// Vault-relative globs whose pages are never link targets, like
    /// `--exclude-target-glob`.
    pub exclude_target_globs: &'a [String],
}

impl ExclusionSets<'_> {
    /// `true` when neither list carries an entry (nothing to attribute).
    pub fn is_empty(&self) -> bool {
        self.exclude_titles.is_empty() && self.exclude_target_globs.is_empty()
    }
}

/// Count the auto-link candidate titles that persisted config exclusions
/// remove, i.e. the `config_excluded` figure in the `links auto` envelope.
///
/// `cli` carries only the exclusions the user typed on this invocation;
/// `effective` carries the union actually handed to [`auto_link`]. The result
/// is the number of title keys that are candidates under `cli` alone but not
/// under `effective` — exactly the candidates the config took away.
///
/// Builds the title inventory twice, but only over index metadata (no file
/// I/O), so callers should still skip the call when the config contributes no
/// exclusions.
pub fn count_config_excluded_titles(
    index: &dyn VaultIndex,
    min_length: usize,
    cli: ExclusionSets<'_>,
    effective: ExclusionSets<'_>,
) -> Result<usize> {
    let entries = index.entries();
    let (cli_titles, _) = build_title_inventory(
        entries,
        min_length,
        cli.exclude_titles,
        cli.exclude_target_globs,
    )?;
    if cli_titles.is_empty() {
        return Ok(0);
    }
    let (effective_titles, _) = build_title_inventory(
        entries,
        min_length,
        effective.exclude_titles,
        effective.exclude_target_globs,
    )?;
    Ok(cli_titles
        .keys()
        .filter(|k| !effective_titles.contains_key(*k))
        .count())
}

/// Extract the filename stem from a vault-relative path.
///
/// `"notes/sprint-planning.md"` → `"sprint-planning"`.
fn stem_from_rel(rel: &str) -> &str {
    let fname = rel.rsplit('/').next().unwrap_or(rel);
    if fname.len() > 3
        && fname
            .as_bytes()
            .get(fname.len() - 3..)
            .is_some_and(|s| s.eq_ignore_ascii_case(b".md"))
    {
        &fname[..fname.len() - 3]
    } else {
        fname
    }
}

// ---------------------------------------------------------------------------
// Word boundary helpers
// ---------------------------------------------------------------------------

/// Returns `true` if `c` is a "word" character for boundary purposes:
/// any Unicode alphanumeric character or an underscore.
///
/// This is Unicode-aware (not per-byte ASCII) so that a title abutting a CJK
/// ideograph, an accented letter, or a non-ASCII digit is correctly treated as
/// *not* on a word boundary. Punctuation-class connectors such as U+2011
/// (non-breaking hyphen) are non-alphanumeric and therefore *are* boundaries.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Verify that a match at byte range `[start, end)` in `line` sits on word
/// boundaries, inspecting the full Unicode scalar values immediately before
/// `start` and at/after `end` rather than raw bytes.
fn has_word_boundaries(line: &str, start: usize, end: usize) -> bool {
    // The character immediately preceding `start` (if any).
    let before_ok = line[..start]
        .chars()
        .next_back()
        .is_none_or(|c| !is_word_char(c));
    // The character starting at `end` (first char after the match), if any.
    let after_ok = line[end..].chars().next().is_none_or(|c| !is_word_char(c));
    before_ok && after_ok
}

// ---------------------------------------------------------------------------
// Scanning helpers
// ---------------------------------------------------------------------------

/// Returns `true` if `[match_start, match_end)` overlaps any link span in `spans`.
fn overlaps_any_link(
    spans: &[crate::links::LinkSpan],
    match_start: usize,
    match_end: usize,
) -> bool {
    spans
        .iter()
        .any(|s| match_start < s.full_end && match_end > s.full_start)
}

// ---------------------------------------------------------------------------
// Main public function
// ---------------------------------------------------------------------------

/// Scan the vault for unlinked mentions of known page titles.
///
/// When `opts.apply` is true, write the `[[wikilinks]]` into the files.
/// When false (dry-run), just report what would change.
pub fn auto_link(
    index: &dyn VaultIndex,
    dir: &Path,
    opts: &AutoLinkOptions<'_>,
) -> Result<AutoLinkReport> {
    let entries = index.entries();

    // Validate --file for path traversal or absolute paths.
    if let Some(filter) = opts.file_filter {
        anyhow::ensure!(
            !crate::discovery::has_parent_traversal(filter),
            "--file path must not contain '..' components: {filter}"
        );
        // `Path::is_absolute()` on Windows requires a drive prefix, so `/foo`
        // and `\foo` (root-relative paths) slip through.  `has_root()` catches
        // all non-relative paths on every platform.
        anyhow::ensure!(
            !std::path::Path::new(filter).has_root(),
            "--file path must be vault-relative, not absolute: {filter}"
        );
    }

    // 1. Build the title inventory.
    let (title_map, ambiguous_titles) = build_title_inventory(
        entries,
        opts.min_length,
        opts.exclude_titles,
        opts.exclude_target_globs,
    )?;

    if title_map.is_empty() {
        return Ok(AutoLinkReport {
            scanned: 0,
            total: 0,
            matches: Vec::new(),
            ambiguous_titles,
            applied: false,
            apply_outcomes: Vec::new(),
        });
    }

    // 2. Build the Aho-Corasick automaton.
    //    We need to keep track of which pattern index maps to which TitleEntry.
    //    Sort for determinism.
    let mut patterns_sorted: Vec<(&str, &TitleEntry)> =
        title_map.iter().map(|(k, v)| (k.as_str(), v)).collect();
    patterns_sorted.sort_by_key(|(k, _)| *k);

    let ac = AhoCorasick::builder()
        .match_kind(MatchKind::LeftmostLongest)
        .ascii_case_insensitive(true)
        .build(patterns_sorted.iter().map(|(k, _)| k))
        .context("failed to build Aho-Corasick automaton")?;

    // 3. Determine which files to scan (respecting file_filter and glob_filter).
    let all_paths: Vec<PathBuf> = entries.iter().map(|e| dir.join(&e.rel_path)).collect();

    let paths_to_scan: Vec<(PathBuf, String)> = if !opts.glob_filter.is_empty() {
        match_globs(dir, &all_paths, opts.glob_filter)?
    } else if let Some(filter) = opts.file_filter {
        // Exact vault-relative path match (normalise separators for Windows).
        let normalised = filter.replace('\\', "/");
        entries
            .iter()
            .filter(|e| e.rel_path == normalised)
            .map(|e| (dir.join(&e.rel_path), e.rel_path.clone()))
            .collect()
    } else {
        entries
            .iter()
            .map(|e| (dir.join(&e.rel_path), e.rel_path.clone()))
            .collect()
    };

    // 4. Scan each file.
    let mut all_matches: Vec<AutoLinkMatch> = Vec::new();
    let mut scanned_content: HashMap<String, String> = HashMap::new();
    // Per-file set of link_targets that already have an existing [[wikilink]]
    // somewhere in that file. Only populated when --first-only is active,
    // since that's the only consumer (see step 4b).
    let mut existing_linked_targets: HashMap<String, HashSet<String>> = HashMap::new();
    let scanned = paths_to_scan.len();

    for (abs_path, rel_path) in &paths_to_scan {
        // Enforce the same size cap used elsewhere (scanner, link_fix, etc.).
        if let Ok(meta) = std::fs::metadata(abs_path)
            && meta.len() > MAX_FILE_SIZE
        {
            eprintln!(
                "warning: skipping {} ({} MiB exceeds {} MiB limit)",
                abs_path.display(),
                meta.len() / (1024 * 1024),
                MAX_FILE_SIZE / (1024 * 1024),
            );
            continue;
        }

        let content = std::fs::read_to_string(abs_path)
            .with_context(|| format!("failed to read {}", abs_path.display()))?;

        let file_matches = scan_file_for_matches(&content, rel_path, &ac, &patterns_sorted);

        if opts.first_only {
            let targets = resolve_existing_link_targets(&content, &title_map);
            if !targets.is_empty() {
                existing_linked_targets.insert(rel_path.clone(), targets);
            }
        }

        // Only retain file content when we'll actually need it for writing.
        let has_matches = !file_matches.is_empty();
        all_matches.extend(file_matches);
        if opts.apply && has_matches {
            scanned_content.insert(rel_path.clone(), content);
        }
    }

    // 4b. Apply --first-only: keep only the lowest-offset match per (source_file, target_title).
    //     Two-pass approach: intern strings as integer IDs to avoid cloning per match,
    //     then filter using the precomputed keep-mask. A target that already has an
    //     existing [[wikilink]] anywhere in the file has its slot pre-seeded as "seen" —
    //     the existing link counts as the first mention, so no new match is kept for it.
    if opts.first_only {
        let mut file_ids: HashMap<&str, usize> = HashMap::new();
        let mut target_ids: HashMap<&str, usize> = HashMap::new();
        let mut seen: HashSet<(usize, usize)> = HashSet::new();

        for (file, targets) in &existing_linked_targets {
            let n = file_ids.len();
            let fid = *file_ids.entry(file.as_str()).or_insert(n);
            for target in targets {
                let n = target_ids.len();
                let tid = *target_ids.entry(target.as_str()).or_insert(n);
                seen.insert((fid, tid));
            }
        }

        let keep: Vec<bool> = all_matches
            .iter()
            .map(|m| {
                let n = file_ids.len();
                let fid = *file_ids.entry(&m.file).or_insert(n);
                let n = target_ids.len();
                let tid = *target_ids.entry(&m.link_target).or_insert(n);
                seen.insert((fid, tid))
            })
            .collect();

        let mut keep_iter = keep.into_iter();
        all_matches.retain(|_| keep_iter.next().unwrap_or(false));
    }

    // 5. Apply changes if requested.
    let apply_outcomes = if opts.apply {
        apply_matches(dir, &all_matches, &scanned_content)?
    } else {
        Vec::new()
    };

    let total = all_matches.len();
    Ok(AutoLinkReport {
        scanned,
        total,
        matches: all_matches,
        ambiguous_titles,
        applied: opts.apply,
        apply_outcomes,
    })
}

/// Scan a file's content for existing `[[wikilinks]]` (and `[markdown](links)`)
/// and resolve each one's target to a known `link_target` (file stem), used to
/// pre-seed the `--first-only` keep-mask so an existing link counts as the
/// first mention of its target.
///
/// Mirrors the zone-skipping in [`scan_file_for_matches`] for frontmatter,
/// fenced code blocks, and comment fences (`%%`) — link syntax inside those is
/// inert. Unlike that function, heading lines are *not* skipped: a link inside
/// a heading is a real, rendered link.
fn resolve_existing_link_targets(
    content: &str,
    title_map: &HashMap<String, TitleEntry>,
) -> HashSet<String> {
    let mut targets = HashSet::new();

    // Shared, cross-line-aware line classifier (iter-183 Phase B). Frontmatter
    // lines are ignored here; only body links count toward "already linked"
    // targets.
    let mut scanner = LineScanner::new();

    for (line, rest) in lines_with_rest(content) {
        let LineClass::Body(body) = scanner.classify(line, rest) else {
            continue;
        };

        // ---- Strip inline code, inline/HTML comments (cross-line aware) ----
        let cleaned = body.cleaned(line, rest);
        let cleaned_str: &str = cleaned.as_ref();

        for span in extract_link_spans_with_original(cleaned_str, line) {
            // Resolve the written target the same way plain-text mentions are
            // resolved: case-insensitive lookup against the title inventory
            // (titles, stems, aliases). Also try the last path segment as a
            // stem, so `[[dir/target]]` resolves like `[[target]]` does.
            //
            // Markdown links (`[text](target.md)`) keep their `.md` suffix —
            // `parse_markdown_link` only strips the fragment — while wikilink
            // targets already had it stripped by `parse_wikilink`. Stripping
            // it again here is a no-op for wikilinks and required for
            // markdown links, since `title_map` keys are extension-less.
            let raw = strip_wikilink_md_suffix(&span.link.target);
            if let Some(entry) = title_map.get(&raw.to_ascii_lowercase()) {
                targets.insert(entry.link_target.clone());
            } else if let Some(stem) = raw.rsplit('/').next()
                && stem != raw
                && let Some(entry) = title_map.get(&stem.to_ascii_lowercase())
            {
                targets.insert(entry.link_target.clone());
            }
        }
    }

    targets
}

// ---------------------------------------------------------------------------
// Document-scoped paragraph blocks (iter-217)
// ---------------------------------------------------------------------------

/// One physical body line inside a [`LineBlock`].
struct BlockLine<'a> {
    /// 1-based line number in the source file.
    line_num: usize,
    /// The original (unblanked) line text.
    line: &'a str,
    /// Start offset of this line's cleaned text within `LineBlock::text`.
    block_offset: usize,
    /// Whether this line is itself a CommonMark reference-link definition
    /// (`[ref]: url "title"`) — the whole line is inert either way.
    is_definition: bool,
}

/// A maximal run of consecutive non-blank, non-heading body lines: the
/// paragraph-like unit a wikilink, markdown link, or raw HTML tag can wrap
/// across (iter-217), mirroring the block-scoping iter-207 already applies to
/// cross-line inline code spans (BUG-1). A construct never reaches across a
/// blank line, heading, or fence.
struct LineBlock<'a> {
    lines: Vec<BlockLine<'a>>,
    /// Cleaned (code-span/comment-blanked) text of every line in the block,
    /// `'\n'`-joined so a construct spanning lines can be matched with a
    /// single scan. Each line's contribution is exactly its original byte
    /// length (blanking only substitutes spaces), so `block_offset + len()`
    /// always bounds that line's slice correctly.
    text: String,
}

/// Split `content` into [`LineBlock`]s, marking every line that is itself a
/// CommonMark reference-link definition (`[ref]: url "title"`).
///
/// Frontmatter, fenced code, and comment-fence lines are excluded from every
/// block, as are blank and heading lines (which end the current block
/// without joining it, matching [`crate::scanner::is_block_boundary`]).
/// Reuses [`LineScanner`] for cross-line code-span / HTML-comment blanking
/// (iter-183 L-3/L-15).
fn build_blocks(content: &str) -> Vec<LineBlock<'_>> {
    let mut blocks = Vec::new();
    let mut scanner = LineScanner::new();
    let mut current: Option<LineBlock<'_>> = None;

    for (line, rest) in lines_with_rest(content) {
        let LineClass::Body(body) = scanner.classify(line, rest) else {
            if let Some(b) = current.take() {
                blocks.push(b);
            }
            continue;
        };

        if crate::scanner::is_block_boundary(line) {
            if let Some(b) = current.take() {
                blocks.push(b);
            }
            continue;
        }

        let cleaned = body.cleaned(line, rest);
        let cleaned_str: &str = cleaned.as_ref();
        let is_definition = crate::links::parse_reference_definition_label(cleaned_str).is_some();

        let block = current.get_or_insert_with(|| LineBlock {
            lines: Vec::new(),
            text: String::new(),
        });
        if !block.text.is_empty() {
            block.text.push('\n');
        }
        let block_offset = block.text.len();
        block.text.push_str(cleaned_str);
        block.lines.push(BlockLine {
            line_num: scanner.line_num(),
            line,
            block_offset,
            is_definition,
        });
    }
    if let Some(b) = current.take() {
        blocks.push(b);
    }

    blocks
}

/// Scan a single file's content for unlinked title mentions, returning all
/// proposed [`AutoLinkMatch`] values.
///
/// Zones are computed once per paragraph [`LineBlock`] (iter-217) so a
/// wikilink, markdown link, or raw HTML tag that wraps across a line
/// boundary is recognised as inert across its whole span, not just the
/// physical line the scanner happens to be on — the root cause of NEW-2
/// (wrapped links corrupted) and NEW-10 (multi-line HTML tags leaking).
/// [`crate::links::inert_link_zones`]'s generic `[...]` fallback also covers
/// every CommonMark reference-link usage (NEW-1) as a side effect — any
/// well-formed bracket span is inert, referenced-defined or not (see that
/// function's doc comment for why). Definition *lines* still get their own
/// explicit whole-line treatment below, since the destination/title text
/// after `[ref]:` sits outside any bracket.
fn scan_file_for_matches(
    content: &str,
    rel_path: &str,
    ac: &AhoCorasick,
    patterns_sorted: &[(&str, &TitleEntry)],
) -> Vec<AutoLinkMatch> {
    let mut results = Vec::new();
    let blocks = build_blocks(content);

    for block in &blocks {
        // ---- Zones that are syntactically part of a link or tag
        // (iter-200 H-2, iter-207 BUG-2/BUG-3, iter-217 NEW-1/NEW-2/NEW-10) ----
        let mut zones_block = crate::links::inert_link_zones(&block.text);
        // A reference-link definition line is inert in its entirety, label
        // through title, even when its shape doesn't exactly match the
        // lenient parser's expectations for the destination/title spans.
        for bl in &block.lines {
            if bl.is_definition {
                zones_block.push((bl.block_offset, bl.block_offset + bl.line.len()));
            }
        }

        for bl in &block.lines {
            let line = bl.line;
            let line_num = bl.line_num;
            let cleaned_str = &block.text[bl.block_offset..bl.block_offset + line.len()];

            // ---- Extract existing link spans to avoid overlapping them ----
            let link_spans = extract_link_spans_with_original(cleaned_str, line);

            // ---- Run Aho-Corasick on the cleaned line ----
            for mat in ac.find_iter(cleaned_str) {
                let start = mat.start();
                let end = mat.end();
                let pat_idx = mat.pattern().as_usize();
                let (_, entry) = patterns_sorted[pat_idx];

                // Self-link check: skip if the match belongs to the current file.
                if entry.source_rel == rel_path {
                    continue;
                }

                // Word boundary check.
                if !has_word_boundaries(cleaned_str, start, end) {
                    continue;
                }

                // Existing link overlap check.
                if overlaps_any_link(&link_spans, start, end) {
                    continue;
                }

                // Link-syntax zone check, in block-relative coordinates.
                let block_start = bl.block_offset + start;
                let block_end = bl.block_offset + end;
                if crate::links::overlaps_zone(&zones_block, block_start, block_end) {
                    continue;
                }

                // Use original line text for the matched_text (preserves casing).
                let matched_text = line.get(start..end).unwrap_or(&cleaned_str[start..end]);

                // `col` counts Unicode scalars, not bytes (iter-210). `start`
                // indexes the cleaned line, which preserves the original line's
                // byte layout, so the original text is the right thing to count
                // when it is available.
                let char_col = line
                    .get(..start)
                    .unwrap_or_else(|| cleaned_str.get(..start).unwrap_or_default())
                    .chars()
                    .count();

                results.push(AutoLinkMatch {
                    file: rel_path.to_owned(),
                    line: line_num,
                    byte_col: start,
                    col: char_col,
                    matched_text: matched_text.to_owned(),
                    link_target: entry.link_target.clone(),
                });
            }
        }
    }

    results
}

// ---------------------------------------------------------------------------
// Apply changes
// ---------------------------------------------------------------------------

/// Build the `[[wikilink]]` (or `[[target|alias]]`) text to splice in for a
/// match (iter-217, NEW-3).
///
/// Plain `[[link_target]]` only when `matched_text` is byte-identical to
/// `link_target` — any difference, including a bare case difference
/// (`Pulls` vs `pulls`), means substituting the target alone would change
/// what the page renders, so the original surface text is preserved as an
/// explicit alias instead: `[[link_target|matched_text]]`.
fn wikilink_replacement_text(matched_text: &str, link_target: &str) -> String {
    if matched_text == link_target {
        format!("[[{link_target}]]")
    } else {
        format!("[[{link_target}|{matched_text}]]")
    }
}

/// Apply a batch of matches to the vault by building [`RewritePlan`]s and
/// executing them through the shared [`execute_plans_partial`] machinery
/// (iter-187: single write path).
///
/// Replacements are computed from the content collected during the scan phase
/// (not re-read from disk), so byte offsets stay consistent. A verification
/// re-read is still performed per file to detect concurrent modifications: if
/// the on-disk content differs from the scanned snapshot the file is skipped.
/// This full-content compare is the stronger TOCTOU guard (vs. the mtime+size
/// pair on `RewritePlan.mtime`), so plans are handed to `execute_plans_partial`
/// with `mtime: None` — the content compare happened immediately before.
///
/// Returns one [`AutoApplyOutcome`] per file that had matches, recording
/// applied / skipped / failed so the caller can emit an honest envelope (L-11).
/// The vault-boundary safety check inside `execute_plans_partial` guards against
/// any plan path escaping the vault root.
fn apply_matches(
    dir: &Path,
    matches: &[AutoLinkMatch],
    scanned_content: &HashMap<String, String>,
) -> Result<Vec<AutoApplyOutcome>> {
    // Group matches by file, preserving first-seen order for deterministic output.
    let mut order: Vec<&str> = Vec::new();
    let mut by_file: HashMap<&str, Vec<&AutoLinkMatch>> = HashMap::new();
    for m in matches {
        by_file
            .entry(&m.file)
            .or_insert_with(|| {
                order.push(&m.file);
                Vec::new()
            })
            .push(m);
    }

    let mut outcomes: Vec<AutoApplyOutcome> = Vec::new();
    let mut plans: Vec<RewritePlan> = Vec::new();

    for rel_path in order {
        let file_matches = &by_file[rel_path];
        let abs_path = dir.join(rel_path);

        // Use content from the scan phase.  If the entry is missing, the
        // concurrent-modification check below would compare disk-to-disk and
        // always pass, so skip the file instead.
        let Some(content) = scanned_content.get(rel_path).map(String::as_str) else {
            outcomes.push(AutoApplyOutcome {
                file: rel_path.to_owned(),
                status: AutoApplyStatus::Skipped,
                reason: Some("not in scan cache (possible internal bug)".to_owned()),
            });
            continue;
        };

        // Detect concurrent modification: skip the file if it changed after scan.
        // A read failure (deleted, permissions, non-UTF-8) is treated as
        // "changed" — we must not overwrite what we cannot verify.
        let disk_content = match std::fs::read_to_string(&abs_path) {
            Ok(c) => c,
            Err(err) => {
                outcomes.push(AutoApplyOutcome {
                    file: rel_path.to_owned(),
                    status: AutoApplyStatus::Skipped,
                    reason: Some(format!("could not verify after scan ({err})")),
                });
                continue;
            }
        };
        if disk_content != content {
            outcomes.push(AutoApplyOutcome {
                file: rel_path.to_owned(),
                status: AutoApplyStatus::Skipped,
                reason: Some("modified after scan".to_owned()),
            });
            continue;
        }

        // Build a `Replacement` per match. `col` is the 0-based byte offset
        // within the line; `apply_replacements` sorts right-to-left per line so
        // earlier offsets stay valid.
        let replacements: Vec<Replacement> = file_matches
            .iter()
            .map(|m| Replacement {
                line: m.line,
                byte_offset: m.byte_col,
                old_text: m.matched_text.clone(),
                new_text: wikilink_replacement_text(&m.matched_text, &m.link_target),
            })
            .collect();

        let rewritten_content = apply_replacements(content, &replacements);
        plans.push(RewritePlan {
            path: abs_path,
            rel_path: rel_path.to_owned(),
            replacements,
            rewritten_content,
            // Full-content compare above is the stronger guard; skip the
            // weaker mtime+size check in `write_single_plan`.
            mtime: None,
            original_content: None,
        });
    }

    // Execute all buildable plans, continuing past per-file write failures.
    let report = execute_plans_partial(dir, &plans)?;
    for outcome in report.outcomes {
        outcomes.push(AutoApplyOutcome {
            file: outcome.rel_path,
            status: if outcome.applied {
                AutoApplyStatus::Applied
            } else {
                AutoApplyStatus::Failed
            },
            reason: outcome.error,
        });
    }

    Ok(outcomes)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{IndexEntry, VaultIndex};
    use crate::link_graph::LinkGraph;
    use indexmap::IndexMap;
    use serde_json::Value;
    use tempfile::TempDir;

    // ---- Mock VaultIndex ----

    struct MockIndex {
        entries: Vec<IndexEntry>,
        graph: LinkGraph,
    }

    impl MockIndex {
        fn new(entries: Vec<IndexEntry>) -> Self {
            Self {
                entries,
                graph: LinkGraph::default(),
            }
        }
    }

    impl VaultIndex for MockIndex {
        fn entries(&self) -> &[IndexEntry] {
            &self.entries
        }

        fn get(&self, rel_path: &str) -> Option<&IndexEntry> {
            self.entries.iter().find(|e| e.rel_path == rel_path)
        }

        fn link_graph(&self) -> &LinkGraph {
            &self.graph
        }
    }

    // ---- Builder helpers ----

    fn make_entry(rel_path: &str, props: Vec<(&str, Value)>) -> IndexEntry {
        let mut properties = IndexMap::new();
        for (k, v) in props {
            properties.insert(k.to_owned(), v);
        }
        IndexEntry {
            rel_path: rel_path.to_owned(),
            modified: String::new(),
            properties,
            tags: Vec::new(),
            sections: Vec::new(),
            tasks: Vec::new(),
            links: Vec::new(),
            self_anchors: Vec::new(),
            bm25_tokens: None,
            bm25_language: None,
        }
    }

    /// Write a file to a temp dir and return its abs path.
    fn write_file(dir: &TempDir, rel: &str, content: &str) -> PathBuf {
        let path = dir.path().join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, content).unwrap();
        path
    }

    // ---- Tests ----

    #[test]
    fn test_title_inventory_basic() {
        let entries = vec![make_entry(
            "sprint-planning.md",
            vec![
                ("title", Value::String("Sprint Planning".to_owned())),
                (
                    "aliases",
                    Value::Array(vec![Value::String("SP".to_owned())]),
                ),
            ],
        )];
        let (map, ambiguous) = build_title_inventory(&entries, 2, &[], &[]).unwrap();
        assert!(ambiguous.is_empty());
        // Stem
        assert!(map.contains_key("sprint-planning"), "stem missing");
        // Title
        assert!(map.contains_key("sprint planning"), "title missing");
        // Alias "SP" (len 2 >= min_length 2)
        assert!(map.contains_key("sp"), "alias missing");

        let title_entry = map.get("sprint planning").unwrap();
        assert_eq!(title_entry.link_target, "sprint-planning");
        assert_eq!(title_entry.source_rel, "sprint-planning.md");
    }

    #[test]
    fn test_title_inventory_ambiguous() {
        let entries = vec![
            make_entry(
                "planning/sprint.md",
                vec![("title", Value::String("Sprint".to_owned()))],
            ),
            make_entry(
                "notes/sprint.md",
                vec![("title", Value::String("Sprint".to_owned()))],
            ),
        ];
        let (map, ambiguous) = build_title_inventory(&entries, 3, &[], &[]).unwrap();
        // Both "sprint.md" produce the same stem "sprint" and same title "sprint"
        assert!(
            !map.contains_key("sprint"),
            "ambiguous title should be absent from map"
        );
        assert!(!ambiguous.is_empty(), "should have ambiguous entries");
    }

    #[test]
    fn test_title_inventory_min_length() {
        let entries = vec![make_entry(
            "go.md",
            vec![("title", Value::String("Go".to_owned()))],
        )];
        // min_length = 3: "go" (len 2) should be filtered
        let (map, _) = build_title_inventory(&entries, 3, &[], &[]).unwrap();
        assert!(!map.contains_key("go"), "short title should be filtered");
    }

    #[test]
    fn test_title_inventory_exclude() {
        let entries = vec![make_entry(
            "the.md",
            vec![("title", Value::String("The".to_owned()))],
        )];
        let (map, _) =
            build_title_inventory(&entries, 2, &["the".to_owned(), "The".to_owned()], &[]).unwrap();
        assert!(!map.contains_key("the"), "excluded title should not appear");
    }

    #[test]
    fn test_word_boundary() {
        // "Sprint" should not match inside "Sprinting"
        let entry = make_entry(
            "sprint.md",
            vec![("title", Value::String("Sprint".to_owned()))],
        );
        let other = make_entry("other.md", vec![]);
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "sprint.md", "---\ntitle: Sprint\n---\n");
        write_file(&tmp, "other.md", "Sprinting is fun but Sprint is better.\n");

        let index = MockIndex::new(vec![entry, other]);
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("other.md"),
                glob_filter: &[],
            },
        )
        .unwrap();

        let matches: Vec<_> = report
            .matches
            .iter()
            .filter(|m| m.matched_text == "Sprint")
            .collect();
        // "Sprint" at the beginning of "Sprinting" should NOT match (no word boundary after)
        // "Sprint" as standalone word SHOULD match
        assert_eq!(matches.len(), 1, "only standalone 'Sprint' should match");
        assert_eq!(matches[0].matched_text, "Sprint");
    }

    #[test]
    fn word_boundary_unicode_cjk_adjacency() {
        // L-12: a title abutting a CJK ideograph must NOT be treated as sitting
        // on a word boundary. Byte-level ASCII checks would (wrongly) see the
        // multi-byte lead byte as a non-word byte and accept the match.
        let line = "私はSprintが好き"; // "Sprint" flanked by CJK, no ASCII boundary
        let start = line.find("Sprint").unwrap();
        let end = start + "Sprint".len();
        assert!(
            !has_word_boundaries(line, start, end),
            "CJK-adjacent match must not count as a word boundary"
        );

        // A CJK char is alphanumeric, so a title made of CJK sitting between
        // ASCII letters is likewise not on a boundary.
        let line2 = "x東京y";
        let s2 = line2.find('東').unwrap();
        let e2 = s2 + "東京".len();
        assert!(!has_word_boundaries(line2, s2, e2));
    }

    #[test]
    fn word_boundary_non_breaking_hyphen_is_boundary() {
        // L-12: U+2011 (non-breaking hyphen) is punctuation, not a word char,
        // so a match flanked by it IS on a word boundary and should match.
        let line = "foo\u{2011}Sprint\u{2011}bar";
        let start = line.find("Sprint").unwrap();
        let end = start + "Sprint".len();
        assert!(
            has_word_boundaries(line, start, end),
            "U+2011 flanks must count as word boundaries"
        );
    }

    #[test]
    fn word_boundary_accented_letter_adjacency() {
        // An accented Unicode letter is a word char; a match glued to it is not
        // on a boundary.
        let line = "café Sprint"; // space before Sprint → boundary OK here
        let start = line.find("Sprint").unwrap();
        let end = start + "Sprint".len();
        assert!(has_word_boundaries(line, start, end));

        let glued = "caféSprint";
        let gs = glued.find("Sprint").unwrap();
        let ge = gs + "Sprint".len();
        assert!(
            !has_word_boundaries(glued, gs, ge),
            "match glued to an accented letter is not on a boundary"
        );
    }

    #[test]
    fn exclude_target_glob_is_case_insensitive() {
        // L-24: --exclude-target-glob must match case-insensitively, mirroring
        // --exclude-title. `Templates/*` should exclude `templates/note.md`.
        let entries = vec![
            make_entry("templates/note.md", vec![]),
            make_entry("real.md", vec![]),
        ];
        let (map, _) =
            build_title_inventory(&entries, 2, &[], &["Templates/*".to_owned()]).unwrap();
        assert!(
            !map.contains_key("note"),
            "case-insensitive glob should exclude templates/note.md"
        );
        assert!(map.contains_key("real"), "unrelated file still included");
    }

    #[test]
    fn test_skip_headings() {
        let page = make_entry(
            "sprint-planning.md",
            vec![("title", Value::String("Sprint Planning".to_owned()))],
        );
        let other = make_entry("notes.md", vec![]);
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "sprint-planning.md", "# Sprint Planning\n");
        write_file(
            &tmp,
            "notes.md",
            "## Sprint Planning\n\nSee Sprint Planning for details.\n",
        );

        let index = MockIndex::new(vec![page, other]);
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("notes.md"),
                glob_filter: &[],
            },
        )
        .unwrap();

        // Heading line should be skipped; body line should match.
        assert_eq!(
            report.matches.len(),
            1,
            "only the body mention should match"
        );
        assert_eq!(report.matches[0].line, 3);
    }

    // -----------------------------------------------------------------
    // iter-200 H-2: link-syntax zones are never auto-linked
    // -----------------------------------------------------------------

    /// Run auto-link (dry-run) over a single `notes.md` body against a vault
    /// containing one linkable page named `net`, returning the matched lines.
    fn net_matches(body: &str) -> Vec<usize> {
        let page = make_entry("net.md", vec![]);
        let other = make_entry("notes.md", vec![]);
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "net.md", "");
        write_file(&tmp, "notes.md", body);

        let index = MockIndex::new(vec![page, other]);
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("notes.md"),
                glob_filter: &[],
            },
        )
        .unwrap();
        report.matches.iter().map(|m| m.line).collect()
    }

    #[test]
    fn no_match_inside_an_external_link_destination() {
        // The dogfood H-2 repro: a page titled `net` rewrote
        // `…summerwind.net/v1` inside a working URL.
        assert!(
            net_matches("Link: [x](https://pkg.go.dev/x/actions.summerwind.net/v1)\n").is_empty(),
            "a URL destination must never be auto-linked"
        );
    }

    #[test]
    fn no_match_inside_a_bare_url() {
        assert!(
            net_matches("Bare: https://example.net/path is a URL.\n").is_empty(),
            "a bare URL must never be auto-linked"
        );
        assert!(
            net_matches("Auto: <https://example.net/path>\n").is_empty(),
            "an autolink must never be auto-linked"
        );
    }

    #[test]
    fn no_match_inside_an_existing_link_label() {
        assert!(
            net_matches("Label: [the net thing](https://example.com/a)\n").is_empty(),
            "an external link's label must never be auto-linked"
        );
        assert!(
            net_matches("Label: [the net thing](other.md)\n").is_empty(),
            "an internal link's label must never be auto-linked"
        );
    }

    #[test]
    fn plain_prose_mention_next_to_a_url_still_matches() {
        // The exclusions must not swallow real candidates on the same line.
        let matches = net_matches("See https://example.com/x and also net here.\n");
        assert_eq!(matches, vec![1], "the bare prose mention must still match");
    }

    #[test]
    fn test_skip_code_blocks() {
        let page = make_entry("target.md", vec![]);
        let other = make_entry("notes.md", vec![]);
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "target.md", "");
        write_file(
            &tmp,
            "notes.md",
            "```\ntarget text mentioning target\n```\n",
        );

        let index = MockIndex::new(vec![page, other]);
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("notes.md"),
                glob_filter: &[],
            },
        )
        .unwrap();

        // Matches inside fenced code block should be skipped.
        assert!(
            report.matches.is_empty(),
            "code block content should not match"
        );
    }

    #[test]
    fn test_skip_inline_code() {
        let page = make_entry("target.md", vec![]);
        let other = make_entry("notes.md", vec![]);
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "target.md", "");
        write_file(&tmp, "notes.md", "Use `target` sparingly.\n");

        let index = MockIndex::new(vec![page, other]);
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("notes.md"),
                glob_filter: &[],
            },
        )
        .unwrap();

        assert!(
            report.matches.is_empty(),
            "inline code span should not match"
        );
    }

    #[test]
    fn test_skip_existing_links() {
        let page = make_entry("target.md", vec![]);
        let other = make_entry("notes.md", vec![]);
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "target.md", "");
        write_file(
            &tmp,
            "notes.md",
            "See [[target]] and [target](target.md) for details.\n",
        );

        let index = MockIndex::new(vec![page, other]);
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("notes.md"),
                glob_filter: &[],
            },
        )
        .unwrap();

        assert!(
            report.matches.is_empty(),
            "matches overlapping existing links should be skipped"
        );
    }

    #[test]
    fn test_skip_self_links() {
        let page = make_entry(
            "sprint.md",
            vec![("title", Value::String("Sprint".to_owned()))],
        );
        let tmp = TempDir::new().unwrap();
        write_file(
            &tmp,
            "sprint.md",
            "---\ntitle: Sprint\n---\n\nThis is the Sprint page.\n",
        );

        let index = MockIndex::new(vec![page]);
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("sprint.md"),
                glob_filter: &[],
            },
        )
        .unwrap();

        assert!(
            report.matches.is_empty(),
            "self-links (file's own title in its own body) should be skipped"
        );
    }

    #[test]
    fn test_case_insensitive() {
        let page = make_entry("target.md", vec![]);
        let other = make_entry("notes.md", vec![]);
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "target.md", "");
        write_file(&tmp, "notes.md", "See Target or TARGET or target here.\n");

        let index = MockIndex::new(vec![page, other]);
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("notes.md"),
                glob_filter: &[],
            },
        )
        .unwrap();

        assert_eq!(report.matches.len(), 3, "all case variants should match");
    }

    #[test]
    fn test_longest_match() {
        // "Sprint Planning" should win over "Sprint" when both are in inventory.
        let sprint = make_entry(
            "sprint.md",
            vec![("title", Value::String("Sprint".to_owned()))],
        );
        let sp = make_entry(
            "sprint-planning.md",
            vec![("title", Value::String("Sprint Planning".to_owned()))],
        );
        let other = make_entry("notes.md", vec![]);
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "sprint.md", "");
        write_file(&tmp, "sprint-planning.md", "");
        write_file(&tmp, "notes.md", "Sprint Planning kicks off tomorrow.\n");

        let index = MockIndex::new(vec![sprint, sp, other]);
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("notes.md"),
                glob_filter: &[],
            },
        )
        .unwrap();

        // Should produce exactly one match: "Sprint Planning"
        assert_eq!(report.matches.len(), 1);
        assert_eq!(report.matches[0].matched_text, "Sprint Planning");
        assert_eq!(report.matches[0].link_target, "sprint-planning");
    }

    #[test]
    fn test_skip_frontmatter() {
        let page = make_entry("target.md", vec![]);
        let other = make_entry("notes.md", vec![]);
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "target.md", "");
        write_file(
            &tmp,
            "notes.md",
            "---\ntitle: target mentions\n---\n\nNo mention in body.\n",
        );

        let index = MockIndex::new(vec![page, other]);
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("notes.md"),
                glob_filter: &[],
            },
        )
        .unwrap();

        // Frontmatter content should be skipped.
        assert!(
            report.matches.is_empty(),
            "frontmatter mentions should be skipped"
        );
    }

    #[test]
    fn test_skip_comment_fences() {
        let page = make_entry("target.md", vec![]);
        let other = make_entry("notes.md", vec![]);
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "target.md", "");
        write_file(
            &tmp,
            "notes.md",
            "%%\ntarget is mentioned here inside comment\n%%\n",
        );

        let index = MockIndex::new(vec![page, other]);
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("notes.md"),
                glob_filter: &[],
            },
        )
        .unwrap();

        assert!(
            report.matches.is_empty(),
            "comment fence blocks should be skipped"
        );
    }

    #[test]
    fn test_apply_writes_wikilinks() {
        let page = make_entry("target.md", vec![]);
        let other = make_entry("notes.md", vec![]);
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "target.md", "");
        write_file(&tmp, "notes.md", "See target for details.\n");

        let index = MockIndex::new(vec![page, other]);
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: true,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("notes.md"),
                glob_filter: &[],
            },
        )
        .unwrap();

        assert_eq!(report.matches.len(), 1);
        assert!(report.applied);

        // L-11: the file is reported as applied in the per-file envelope.
        assert_eq!(report.apply_outcomes.len(), 1);
        assert_eq!(report.apply_outcomes[0].file, "notes.md");
        assert_eq!(report.apply_outcomes[0].status, AutoApplyStatus::Applied);
        assert!(report.apply_outcomes[0].reason.is_none());

        let written = std::fs::read_to_string(tmp.path().join("notes.md")).unwrap();
        assert!(
            written.contains("[[target]]"),
            "written content should contain wikilink: {written}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_apply_read_only_file_reports_failed() {
        // L-11: a mid-batch write failure (read-only target) must surface as a
        // `Failed` per-file outcome, not a silently-swallowed error.
        use std::os::unix::fs::PermissionsExt;

        let page = make_entry("target.md", vec![]);
        let other = make_entry("notes.md", vec![]);
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "target.md", "");
        let notes = write_file(&tmp, "notes.md", "See target for details.\n");

        // Make the file read-only so the atomic rename-over fails.
        let mut perms = std::fs::metadata(&notes).unwrap().permissions();
        perms.set_mode(0o444);
        std::fs::set_permissions(&notes, perms).unwrap();
        // Also make the parent dir read-only so atomic_write's temp create fails.
        let mut dir_perms = std::fs::metadata(tmp.path()).unwrap().permissions();
        dir_perms.set_mode(0o555);
        std::fs::set_permissions(tmp.path(), dir_perms).unwrap();

        let index = MockIndex::new(vec![page, other]);
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: true,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("notes.md"),
                glob_filter: &[],
            },
        );

        // Restore dir perms so TempDir cleanup can run regardless of outcome.
        let mut restore = std::fs::metadata(tmp.path()).unwrap().permissions();
        restore.set_mode(0o755);
        let _ = std::fs::set_permissions(tmp.path(), restore);

        let report = report.expect("auto_link should not abort on a per-file write failure");
        assert_eq!(report.apply_outcomes.len(), 1);
        assert_eq!(report.apply_outcomes[0].file, "notes.md");
        assert_eq!(report.apply_outcomes[0].status, AutoApplyStatus::Failed);
        assert!(report.apply_outcomes[0].reason.is_some());
    }

    #[test]
    fn test_first_only_dedup() {
        let tmp = TempDir::new().unwrap();
        let entries = vec![
            make_entry("alice.md", vec![("title", Value::String("Alice".into()))]),
            make_entry("notes.md", vec![("title", Value::String("Notes".into()))]),
        ];
        write_file(&tmp, "alice.md", "---\ntitle: Alice\n---\nAlice bio.\n");
        write_file(
            &tmp,
            "notes.md",
            "---\ntitle: Notes\n---\nAlice went to the park. Later Alice came back.\n",
        );
        let index = MockIndex::new(entries);

        // Without first_only: should get 2 matches for "Alice" in notes.md
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: None,
                glob_filter: &[],
            },
        )
        .unwrap();
        let alice_matches: Vec<_> = report
            .matches
            .iter()
            .filter(|m| m.file == "notes.md" && m.link_target == "alice")
            .collect();
        assert_eq!(
            alice_matches.len(),
            2,
            "without first_only, expected 2 Alice matches"
        );

        // With first_only: should get only 1 match for "Alice" in notes.md
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: true,
                exclude_target_globs: &[],
                file_filter: None,
                glob_filter: &[],
            },
        )
        .unwrap();
        let alice_matches: Vec<_> = report
            .matches
            .iter()
            .filter(|m| m.file == "notes.md" && m.link_target == "alice")
            .collect();
        assert_eq!(
            alice_matches.len(),
            1,
            "with first_only, expected 1 Alice match"
        );
    }

    #[test]
    fn test_first_only_existing_link_suppresses_new_matches() {
        // A file that already contains [[alice]] plus a later plain-text
        // mention: --first-only must emit zero new matches, since the
        // existing link is the first mention (regression test for the
        // "the [[fake-login]] envVars block from [[fake-login]]" bug).
        let tmp = TempDir::new().unwrap();
        let entries = vec![
            make_entry("alice.md", vec![("title", Value::String("Alice".into()))]),
            make_entry("notes.md", vec![("title", Value::String("Notes".into()))]),
        ];
        write_file(&tmp, "alice.md", "---\ntitle: Alice\n---\nAlice bio.\n");
        write_file(
            &tmp,
            "notes.md",
            "---\ntitle: Notes\n---\nThe [[alice]] page mentions Alice again in this sentence.\n",
        );
        let index = MockIndex::new(entries);

        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: true,
                exclude_target_globs: &[],
                file_filter: None,
                glob_filter: &[],
            },
        )
        .unwrap();

        let alice_matches: Vec<_> = report
            .matches
            .iter()
            .filter(|m| m.file == "notes.md" && m.link_target == "alice")
            .collect();
        assert!(
            alice_matches.is_empty(),
            "existing [[alice]] link should suppress the later plain mention, got: {alice_matches:?}"
        );
    }

    #[test]
    fn test_first_only_no_existing_link_unaffected() {
        // A file with no pre-existing link to the target: behavior should be
        // unchanged from the pre-fix first-only semantics (first plain mention
        // is still linked, later ones are suppressed).
        let tmp = TempDir::new().unwrap();
        let entries = vec![
            make_entry("alice.md", vec![("title", Value::String("Alice".into()))]),
            make_entry("notes.md", vec![("title", Value::String("Notes".into()))]),
        ];
        write_file(&tmp, "alice.md", "---\ntitle: Alice\n---\nAlice bio.\n");
        write_file(
            &tmp,
            "notes.md",
            "---\ntitle: Notes\n---\nAlice went to the park. Later Alice came back.\n",
        );
        let index = MockIndex::new(entries);

        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: true,
                exclude_target_globs: &[],
                file_filter: None,
                glob_filter: &[],
            },
        )
        .unwrap();

        let alice_matches: Vec<_> = report
            .matches
            .iter()
            .filter(|m| m.file == "notes.md" && m.link_target == "alice")
            .collect();
        assert_eq!(
            alice_matches.len(),
            1,
            "without a pre-existing link, the first plain mention should still be linked"
        );
    }

    #[test]
    fn test_first_only_existing_link_case_insensitive() {
        // An existing [[Fake-Login]] (different case) should still suppress
        // mentions of "fake-login" — matches the case-insensitive Aho-Corasick
        // matching used for plain-text mentions.
        let tmp = TempDir::new().unwrap();
        let entries = vec![
            make_entry(
                "fake-login.md",
                vec![("title", Value::String("Fake Login".into()))],
            ),
            make_entry("notes.md", vec![("title", Value::String("Notes".into()))]),
        ];
        write_file(
            &tmp,
            "fake-login.md",
            "---\ntitle: Fake Login\n---\nFake login page.\n",
        );
        write_file(
            &tmp,
            "notes.md",
            "---\ntitle: Notes\n---\nThe [[Fake-Login]] page and the fake-login flow are related.\n",
        );
        let index = MockIndex::new(entries);

        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: true,
                exclude_target_globs: &[],
                file_filter: None,
                glob_filter: &[],
            },
        )
        .unwrap();

        let matches: Vec<_> = report
            .matches
            .iter()
            .filter(|m| m.file == "notes.md" && m.link_target == "fake-login")
            .collect();
        assert!(
            matches.is_empty(),
            "existing [[Fake-Login]] link should suppress 'fake-login' mention case-insensitively, got: {matches:?}"
        );
    }

    #[test]
    fn test_first_only_aliased_existing_link_counts() {
        // An aliased existing link [[alice|A]] still counts as an existing
        // link to "alice" and should suppress a later plain mention.
        let tmp = TempDir::new().unwrap();
        let entries = vec![
            make_entry("alice.md", vec![("title", Value::String("Alice".into()))]),
            make_entry("notes.md", vec![("title", Value::String("Notes".into()))]),
        ];
        write_file(&tmp, "alice.md", "---\ntitle: Alice\n---\nAlice bio.\n");
        write_file(
            &tmp,
            "notes.md",
            "---\ntitle: Notes\n---\nSee [[alice|A]] for details. Alice is also mentioned here.\n",
        );
        let index = MockIndex::new(entries);

        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: true,
                exclude_target_globs: &[],
                file_filter: None,
                glob_filter: &[],
            },
        )
        .unwrap();

        let alice_matches: Vec<_> = report
            .matches
            .iter()
            .filter(|m| m.file == "notes.md" && m.link_target == "alice")
            .collect();
        assert!(
            alice_matches.is_empty(),
            "aliased existing link [[alice|A]] should suppress the later plain mention, got: {alice_matches:?}"
        );
    }

    #[test]
    fn test_first_only_existing_markdown_link_counts() {
        // A markdown-style existing link [Text](alice.md) keeps its `.md`
        // suffix through parsing (parse_markdown_link only strips the
        // fragment) — resolve_existing_link_targets must strip it before
        // the title_map lookup, or the link is invisible to --first-only.
        let tmp = TempDir::new().unwrap();
        let entries = vec![
            make_entry("alice.md", vec![("title", Value::String("Alice".into()))]),
            make_entry("notes.md", vec![("title", Value::String("Notes".into()))]),
        ];
        write_file(&tmp, "alice.md", "---\ntitle: Alice\n---\nAlice bio.\n");
        write_file(
            &tmp,
            "notes.md",
            "---\ntitle: Notes\n---\nSee [Alice](alice.md) for details. Alice is also mentioned here.\n",
        );
        let index = MockIndex::new(entries);

        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: true,
                exclude_target_globs: &[],
                file_filter: None,
                glob_filter: &[],
            },
        )
        .unwrap();

        let alice_matches: Vec<_> = report
            .matches
            .iter()
            .filter(|m| m.file == "notes.md" && m.link_target == "alice")
            .collect();
        assert!(
            alice_matches.is_empty(),
            "existing markdown link [Alice](alice.md) should suppress the later plain mention, got: {alice_matches:?}"
        );
    }

    #[test]
    fn test_first_only_existing_markdown_link_path_form_counts() {
        // Same as above but with a directory-qualified target
        // [Text](dir/alice.md) — must fall back to the last path segment
        // (after stripping .md) to resolve against the bare-stem title_map key.
        let tmp = TempDir::new().unwrap();
        let entries = vec![
            make_entry(
                "people/alice.md",
                vec![("title", Value::String("Alice".into()))],
            ),
            make_entry("notes.md", vec![("title", Value::String("Notes".into()))]),
        ];
        write_file(
            &tmp,
            "people/alice.md",
            "---\ntitle: Alice\n---\nAlice bio.\n",
        );
        write_file(
            &tmp,
            "notes.md",
            "---\ntitle: Notes\n---\nSee [Alice](people/alice.md) for details. Alice is also mentioned here.\n",
        );
        let index = MockIndex::new(entries);

        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: true,
                exclude_target_globs: &[],
                file_filter: None,
                glob_filter: &[],
            },
        )
        .unwrap();

        let alice_matches: Vec<_> = report
            .matches
            .iter()
            .filter(|m| m.file == "notes.md" && m.link_target == "alice")
            .collect();
        assert!(
            alice_matches.is_empty(),
            "existing markdown link [Alice](people/alice.md) should suppress the later plain mention, got: {alice_matches:?}"
        );
    }

    #[test]
    fn test_exclude_target_glob() {
        let tmp = TempDir::new().unwrap();
        let entries = vec![
            make_entry(
                "templates/start.md",
                vec![("title", Value::String("Start".into()))],
            ),
            make_entry(
                "people/alice.md",
                vec![("title", Value::String("Alice".into()))],
            ),
            make_entry("notes.md", vec![("title", Value::String("Notes".into()))]),
        ];
        write_file(
            &tmp,
            "templates/start.md",
            "---\ntitle: Start\n---\nStart template.\n",
        );
        write_file(
            &tmp,
            "people/alice.md",
            "---\ntitle: Alice\n---\nAlice bio.\n",
        );
        write_file(
            &tmp,
            "notes.md",
            "---\ntitle: Notes\n---\nWe Start with Alice today.\n",
        );
        let index = MockIndex::new(entries);

        // Without exclusion: both "Start" and "Alice" match in notes.md
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: None,
                glob_filter: &[],
            },
        )
        .unwrap();
        let has_start = report.matches.iter().any(|m| m.link_target == "start");
        let has_alice = report.matches.iter().any(|m| m.link_target == "alice");
        assert!(has_start, "without exclusion, Start should match");
        assert!(has_alice, "without exclusion, Alice should match");

        // With --exclude-target-glob 'templates/*': "Start" should be excluded
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &["templates/*".to_owned()],
                file_filter: None,
                glob_filter: &[],
            },
        )
        .unwrap();
        let has_start = report.matches.iter().any(|m| m.link_target == "start");
        let has_alice = report.matches.iter().any(|m| m.link_target == "alice");
        assert!(!has_start, "with exclusion, Start should NOT match");
        assert!(has_alice, "with exclusion, Alice should still match");
    }

    #[test]
    fn test_exclude_target_glob_multiple() {
        let tmp = TempDir::new().unwrap();
        let entries = vec![
            make_entry(
                "templates/start.md",
                vec![("title", Value::String("Start".into()))],
            ),
            make_entry(
                "archive/old.md",
                vec![("title", Value::String("Old".into()))],
            ),
            make_entry(
                "people/alice.md",
                vec![("title", Value::String("Alice".into()))],
            ),
            make_entry("notes.md", vec![("title", Value::String("Notes".into()))]),
        ];
        write_file(
            &tmp,
            "templates/start.md",
            "---\ntitle: Start\n---\nStart.\n",
        );
        write_file(&tmp, "archive/old.md", "---\ntitle: Old\n---\nOld.\n");
        write_file(&tmp, "people/alice.md", "---\ntitle: Alice\n---\nAlice.\n");
        write_file(
            &tmp,
            "notes.md",
            "---\ntitle: Notes\n---\nStart and Old and Alice today.\n",
        );
        let index = MockIndex::new(entries);

        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &["templates/*".to_owned(), "archive/*".to_owned()],
                file_filter: None,
                glob_filter: &[],
            },
        )
        .unwrap();
        let targets: Vec<&str> = report
            .matches
            .iter()
            .map(|m| m.link_target.as_str())
            .collect();
        assert!(
            !targets.contains(&"start"),
            "templates/* should be excluded"
        );
        assert!(!targets.contains(&"old"), "archive/* should be excluded");
        assert!(
            targets.contains(&"alice"),
            "people/alice should NOT be excluded"
        );
    }

    #[test]
    fn test_exclude_target_glob_resolves_ambiguity() {
        // Two pages share the same title "Sprint" — normally ambiguous.
        // Excluding one via --exclude-target-glob should resolve the ambiguity.
        let entries = vec![
            make_entry(
                "templates/sprint.md",
                vec![("title", Value::String("Sprint".into()))],
            ),
            make_entry(
                "planning/sprint.md",
                vec![("title", Value::String("Sprint".into()))],
            ),
            make_entry("notes.md", vec![("title", Value::String("Notes".into()))]),
        ];

        // Without exclusion: "sprint" is ambiguous.
        let (map, ambiguous) = build_title_inventory(&entries, 3, &[], &[]).unwrap();
        assert!(
            !map.contains_key("sprint"),
            "without exclusion, sprint should be ambiguous"
        );
        assert!(!ambiguous.is_empty());

        // With exclusion: templates/* removed, only planning/sprint.md remains — no ambiguity.
        let (map, ambiguous) =
            build_title_inventory(&entries, 3, &[], &["templates/*".to_owned()]).unwrap();
        assert!(
            map.contains_key("sprint"),
            "with templates/* excluded, sprint should be unambiguous and present"
        );
        let entry = map.get("sprint").unwrap();
        assert_eq!(entry.source_rel, "planning/sprint.md");
        assert!(
            !ambiguous.iter().any(|a| a.eq_ignore_ascii_case("sprint")),
            "sprint should not be in the ambiguous list"
        );
    }

    #[test]
    fn file_filter_rejects_parent_traversal() {
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "a.md", "---\ntitle: A\n---\n");
        let index = MockIndex::new(vec![make_entry("a.md", vec![])]);

        let err = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("../etc/passwd"),
                glob_filter: &[],
            },
        )
        .unwrap_err();
        assert!(
            format!("{err:?}").contains(".."),
            "error should mention '..' component: {err:?}"
        );
    }

    #[test]
    fn file_filter_rejects_absolute_path() {
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "a.md", "---\ntitle: A\n---\n");
        let index = MockIndex::new(vec![make_entry("a.md", vec![])]);

        let err = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("/etc/passwd"),
                glob_filter: &[],
            },
        )
        .unwrap_err();
        assert!(
            format!("{err:?}").contains("absolute"),
            "error should mention 'absolute': {err:?}"
        );
    }

    // -----------------------------------------------------------------------
    // count_config_excluded_titles (iter-195a)
    // -----------------------------------------------------------------------

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| (*s).to_owned()).collect()
    }

    /// Vault with three candidate pages, one of them under `templates/`.
    fn attribution_index() -> MockIndex {
        MockIndex::new(vec![
            make_entry(
                "permissions.md",
                vec![("title", Value::from("Permissions"))],
            ),
            make_entry("daily.md", vec![("title", Value::from("Daily"))]),
            make_entry(
                "templates/widget.md",
                vec![("title", Value::from("Widget"))],
            ),
        ])
    }

    #[test]
    fn config_excluded_counts_titles_only_the_config_removed() {
        let index = attribution_index();
        let config_titles = strings(&["permissions"]);
        let count = count_config_excluded_titles(
            &index,
            3,
            ExclusionSets::default(),
            ExclusionSets {
                exclude_titles: &config_titles,
                exclude_target_globs: &[],
            },
        )
        .unwrap();
        // `permissions.md` contributes the stem and the title, which share one
        // lowercased key — one candidate title lost.
        assert_eq!(count, 1);
    }

    #[test]
    fn config_excluded_ignores_titles_the_cli_already_excluded() {
        let index = attribution_index();
        let cli_titles = strings(&["permissions"]);
        let effective = strings(&["permissions", "daily"]);
        let count = count_config_excluded_titles(
            &index,
            3,
            ExclusionSets {
                exclude_titles: &cli_titles,
                exclude_target_globs: &[],
            },
            ExclusionSets {
                exclude_titles: &effective,
                exclude_target_globs: &[],
            },
        )
        .unwrap();
        assert_eq!(count, 1, "only `daily` is attributable to the config");
    }

    #[test]
    fn config_excluded_counts_glob_excluded_target_pages() {
        let index = attribution_index();
        let globs = strings(&["templates/*"]);
        let count = count_config_excluded_titles(
            &index,
            3,
            ExclusionSets::default(),
            ExclusionSets {
                exclude_titles: &[],
                exclude_target_globs: &globs,
            },
        )
        .unwrap();
        assert_eq!(count, 1, "templates/widget.md contributes one title key");
    }

    #[test]
    fn config_excluded_is_zero_when_nothing_matches() {
        let index = attribution_index();
        let titles = strings(&["nonexistent"]);
        let count = count_config_excluded_titles(
            &index,
            3,
            ExclusionSets::default(),
            ExclusionSets {
                exclude_titles: &titles,
                exclude_target_globs: &[],
            },
        )
        .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn config_excluded_is_zero_for_an_empty_vault() {
        let index = MockIndex::new(vec![]);
        let titles = strings(&["permissions"]);
        let count = count_config_excluded_titles(
            &index,
            3,
            ExclusionSets::default(),
            ExclusionSets {
                exclude_titles: &titles,
                exclude_target_globs: &[],
            },
        )
        .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn config_excluded_does_not_count_titles_unambiguated_by_a_glob() {
        // `a/dup.md` and `b/dup.md` are ambiguous together; excluding one via a
        // glob makes the other linkable. That is a candidate *gained*, so the
        // count stays at the pages the glob actually removed.
        let index = MockIndex::new(vec![
            make_entry("a/dup.md", vec![("title", Value::from("Dup"))]),
            make_entry("b/dup.md", vec![("title", Value::from("Dup"))]),
        ]);
        let globs = strings(&["b/*"]);
        let count = count_config_excluded_titles(
            &index,
            3,
            ExclusionSets::default(),
            ExclusionSets {
                exclude_titles: &[],
                exclude_target_globs: &globs,
            },
        )
        .unwrap();
        assert_eq!(
            count, 0,
            "both `dup` keys were ambiguous (no candidates) before the glob"
        );
    }

    // -----------------------------------------------------------------
    // iter-217 NEW-4: ambiguity must be checked in the emitted (stem)
    // namespace, not just the title namespace.
    // -----------------------------------------------------------------

    #[test]
    fn same_stem_different_dirs_and_titles_is_ambiguous() {
        // `graphql/reference/pulls.md` and `rest/pulls/pulls.md`: distinct
        // titles, but both are literally named `pulls.md`, so both produce
        // the shared stem "pulls". Emitting `[[pulls]]` for either title
        // would be ambiguous — hyalo's own resolver could not tell which
        // file it points to.
        let entries = vec![
            make_entry(
                "graphql/reference/pulls.md",
                vec![("title", Value::String("Pull requests".to_owned()))],
            ),
            make_entry(
                "rest/pulls/pulls.md",
                vec![(
                    "title",
                    Value::String("REST API endpoints for pull requests".to_owned()),
                )],
            ),
        ];
        let (map, ambiguous) = build_title_inventory(&entries, 3, &[], &[]).unwrap();
        assert!(
            !map.contains_key("pull requests"),
            "title colliding on the emitted stem must be excluded, not just the identical-key case"
        );
        assert!(
            !map.contains_key("rest api endpoints for pull requests"),
            "the other file's title must also be excluded: same emitted stem"
        );
        assert!(
            !map.values().any(|e| e.link_target == "pulls"),
            "no surviving entry may still emit the ambiguous stem"
        );
        assert!(
            ambiguous.iter().any(|a| a.eq_ignore_ascii_case("pulls")),
            "the ambiguity must be reported: {ambiguous:?}"
        );
    }

    #[test]
    fn stem_collision_survives_even_when_one_side_title_collides_elsewhere() {
        // Real GitHub Docs corpus shape found during iter-217 verification:
        // `rest/pulls/pulls.md` and `rest/pulls/index.md` share the exact
        // title "REST API endpoints for pull requests" — an *unrelated*
        // first-pass key collision that removes `rest/pulls/pulls.md`'s own
        // title-map entry entirely before the target-collision pass ever
        // runs. A target_sources computation keyed off `title_map.values()`
        // (the post-pass-1 survivors) never sees `rest/pulls/pulls.md`
        // again, so it can no longer prove `graphql/reference/pulls.md`'s
        // "Pull requests" title collides on the shared stem "pulls" — and
        // wrote `[[pulls]]` for it on the real corpus (1,429 insertions
        // hyalo's own resolver then called ambiguous). The check must be
        // computed from `entries` directly, not from collision survivors.
        let entries = vec![
            make_entry(
                "graphql/reference/pulls.md",
                vec![("title", Value::String("Pull requests".to_owned()))],
            ),
            make_entry(
                "rest/pulls/pulls.md",
                vec![(
                    "title",
                    Value::String("REST API endpoints for pull requests".to_owned()),
                )],
            ),
            make_entry(
                "rest/pulls/index.md",
                vec![(
                    "title",
                    Value::String("REST API endpoints for pull requests".to_owned()),
                )],
            ),
        ];
        let (map, ambiguous) = build_title_inventory(&entries, 3, &[], &[]).unwrap();
        assert!(
            !map.contains_key("pull requests"),
            "graphql/reference/pulls.md's title must stay excluded even though \
             rest/pulls/pulls.md's own title entry vanished via an unrelated collision"
        );
        assert!(
            !map.values().any(|e| e.link_target == "pulls"),
            "no surviving entry may emit the shared stem 'pulls'"
        );
        assert!(
            ambiguous.iter().any(|a| a.eq_ignore_ascii_case("pulls")),
            "the stem collision must still be reported: {ambiguous:?}"
        );
    }

    #[test]
    fn same_stem_different_dirs_ambiguity_blocks_actual_writes() {
        // End-to-end: a third file mentions "Pull requests" in prose. Neither
        // ambiguous title may be auto-linked.
        let entries = vec![
            make_entry(
                "graphql/reference/pulls.md",
                vec![("title", Value::String("Pull requests".to_owned()))],
            ),
            make_entry(
                "rest/pulls/pulls.md",
                vec![(
                    "title",
                    Value::String("REST API endpoints for pull requests".to_owned()),
                )],
            ),
            make_entry("notes.md", vec![]),
        ];
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "graphql/reference/pulls.md", "");
        write_file(&tmp, "rest/pulls/pulls.md", "");
        write_file(
            &tmp,
            "notes.md",
            "See Pull requests and REST API endpoints for pull requests docs.\n",
        );

        let index = MockIndex::new(entries);
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("notes.md"),
                glob_filter: &[],
            },
        )
        .unwrap();

        assert!(
            report.matches.is_empty(),
            "ambiguous-stem titles must never be auto-linked: {:?}",
            report.matches
        );
        assert!(
            report
                .ambiguous_titles
                .iter()
                .any(|a| a.eq_ignore_ascii_case("pulls")),
            "ambiguity must surface in the report: {:?}",
            report.ambiguous_titles
        );
    }

    // -----------------------------------------------------------------
    // iter-217 NEW-2 / NEW-10: document-scoped (cross-line) inert zones.
    // -----------------------------------------------------------------

    #[test]
    fn wrapped_wikilink_target_is_inert_across_the_line_break() {
        // The real own-KB repro (dogfood-v0210-pre3): a wikilink's alias text
        // wraps onto the next line, leaving the target — which also happens
        // to be a real page title — unprotected by a line-scoped zone scan.
        let page = make_entry("release-pipeline-unification.md", vec![]);
        let other = make_entry("notes.md", vec![]);
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "release-pipeline-unification.md", "");
        let body =
            "See [[research/release-pipeline-unification|reusable\nrelease process]] docs.\n";
        write_file(&tmp, "notes.md", body);

        let index = MockIndex::new(vec![page, other]);
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: true,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("notes.md"),
                glob_filter: &[],
            },
        )
        .unwrap();

        assert!(
            report.matches.is_empty(),
            "the wrapped wikilink's own target must not be re-matched: {:?}",
            report.matches
        );
        let written = std::fs::read_to_string(tmp.path().join("notes.md")).unwrap();
        assert_eq!(written, body, "the file must be byte-identical after apply");
    }

    #[test]
    fn wrapped_markdown_link_label_is_inert_across_the_line_break() {
        // CommonMark: a link's label text may contain a soft line break.
        // `permissions` (a real title) sits inside the wrapped label.
        let page = make_entry("permissions.md", vec![]);
        let other = make_entry("notes.md", vec![]);
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "permissions.md", "");
        let body = "See [file permissions\nguide](other.md) for details.\n";
        write_file(&tmp, "notes.md", body);

        let index = MockIndex::new(vec![page, other]);
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: true,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("notes.md"),
                glob_filter: &[],
            },
        )
        .unwrap();

        assert!(
            report.matches.is_empty(),
            "a title mention inside a wrapped link label must not match: {:?}",
            report.matches
        );
        let written = std::fs::read_to_string(tmp.path().join("notes.md")).unwrap();
        assert_eq!(written, body);
    }

    #[test]
    fn wrapped_markdown_link_destination_alone_on_a_line_is_inert() {
        // CommonMark whitespace (including a line ending) is allowed between
        // `(` and the destination. `target` is a real title sitting inside
        // the destination text on its own continuation line.
        let page = make_entry("target.md", vec![]);
        let other = make_entry("notes.md", vec![]);
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "target.md", "");
        let body = "See [docs](\ntarget.md\n) for details.\n";
        write_file(&tmp, "notes.md", body);

        let index = MockIndex::new(vec![page, other]);
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: true,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("notes.md"),
                glob_filter: &[],
            },
        )
        .unwrap();

        assert!(
            report.matches.is_empty(),
            "a title mention inside a wrapped destination must not match: {:?}",
            report.matches
        );
        let written = std::fs::read_to_string(tmp.path().join("notes.md")).unwrap();
        assert_eq!(written, body);
    }

    #[test]
    fn same_line_baseline_still_matches_after_block_scoping() {
        // Regression guard: block-scoping the zone scan must not make
        // ordinary same-line prose mentions stop matching.
        let page = make_entry("target.md", vec![]);
        let other = make_entry("notes.md", vec![]);
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "target.md", "");
        write_file(&tmp, "notes.md", "See target for details.\n");

        let index = MockIndex::new(vec![page, other]);
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("notes.md"),
                glob_filter: &[],
            },
        )
        .unwrap();
        assert_eq!(report.matches.len(), 1);
        assert_eq!(report.matches[0].matched_text, "target");
    }

    #[test]
    fn multiline_html_tag_attributes_are_inert() {
        // NEW-10: a tag's attributes wrapped onto a continuation line are
        // still part of the tag, not prose.
        let page = make_entry("intellisense.md", vec![]);
        let other = make_entry("notes.md", vec![]);
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "intellisense.md", "");
        let body = "<video\n  title=\"Demo of navigation and intellisense features\">\n</video>\n";
        write_file(&tmp, "notes.md", body);

        let index = MockIndex::new(vec![page, other]);
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: true,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("notes.md"),
                glob_filter: &[],
            },
        )
        .unwrap();

        assert!(
            report.matches.is_empty(),
            "a wrapped HTML tag's attributes must stay inert: {:?}",
            report.matches
        );
        let written = std::fs::read_to_string(tmp.path().join("notes.md")).unwrap();
        assert_eq!(written, body);
    }

    #[test]
    fn cross_line_zone_does_not_reach_across_a_blank_line() {
        // An unclosed `[[` at the end of a paragraph must not swallow the
        // next paragraph across a blank-line boundary.
        let page = make_entry("target.md", vec![]);
        let other = make_entry("notes.md", vec![]);
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "target.md", "");
        write_file(
            &tmp,
            "notes.md",
            "Stray [[ opener with no closer.\n\nMention target here.\n",
        );

        let index = MockIndex::new(vec![page, other]);
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("notes.md"),
                glob_filter: &[],
            },
        )
        .unwrap();

        assert_eq!(
            report.matches.len(),
            1,
            "the next paragraph's real mention must still match: {:?}",
            report.matches
        );
        assert_eq!(report.matches[0].line, 3);
    }

    // -----------------------------------------------------------------
    // iter-217 NEW-1: CommonMark reference-link inert zones.
    // -----------------------------------------------------------------

    #[test]
    fn reference_link_forms_are_all_inert() {
        let page = make_entry(
            "gamma.md",
            vec![("title", Value::String("Gamma".to_owned()))],
        );
        let other = make_entry("notes.md", vec![]);
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "gamma.md", "");
        let body = concat!(
            "Full: [click here][Gamma]\n",
            "Collapsed: [Gamma][]\n",
            "Shortcut: [Gamma]\n",
            "Image: ![Gamma][Gamma]\n",
            "\n",
            "[Gamma]: https://example.com/gamma \"Gamma page\"\n",
        );
        write_file(&tmp, "notes.md", body);

        let index = MockIndex::new(vec![page, other]);
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: true,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("notes.md"),
                glob_filter: &[],
            },
        )
        .unwrap();

        assert!(
            report.matches.is_empty(),
            "every reference form and the definition line must stay inert: {:?}",
            report.matches
        );
        let written = std::fs::read_to_string(tmp.path().join("notes.md")).unwrap();
        assert_eq!(written, body, "the file must be byte-identical after apply");
    }

    #[test]
    fn bracketed_mention_without_a_reference_definition_stays_inert() {
        // `[Gamma]` with no `[Gamma]: url` definition anywhere in the
        // document is not a CommonMark reference link at all — per spec it's
        // literal prose with literal brackets. An earlier design only made
        // brackets inert when a matching definition existed, reasoning that
        // otherwise every bracketed mention becomes permanently unlinkable.
        // Real-corpus verification overturned that: GitHub Docs and
        // vscode-docs both use plain, undefined `[...]` bracket conventions
        // for things that are not links at all — style-guide placeholders
        // (`[ACCOUNT ROLE]`), PR area tags
        // (`[typescript-language-features]`) — and inserting `[[target]]`
        // touching or inside those brackets produced nested bracket soup
        // (`[[[typescript]]-language-features]`) that hyalo's own resolver
        // then reported as a broken link. `inert_link_zones` now treats any
        // well-formed `[...]` span as inert unconditionally; recall is
        // traded for the same "corruption is worse than a missed candidate"
        // safety margin the rest of this scanner already uses.
        let page = make_entry(
            "gamma.md",
            vec![("title", Value::String("Gamma".to_owned()))],
        );
        let other = make_entry("notes.md", vec![]);
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "gamma.md", "");
        write_file(&tmp, "notes.md", "Mentioned as [Gamma] in prose.\n");

        let index = MockIndex::new(vec![page, other]);
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("notes.md"),
                glob_filter: &[],
            },
        )
        .unwrap();

        assert!(
            report.matches.is_empty(),
            "a bracketed mention must never be auto-linked, definition or not: {:?}",
            report.matches
        );
    }

    #[test]
    fn placeholder_style_bracket_text_is_never_corrupted() {
        // The real GitHub Docs / vscode-docs shapes: a title-length word
        // sitting at the START of a longer bracketed run, touching the
        // opening bracket but not co-extensive with it.
        let page = make_entry("account.md", vec![]);
        let other = make_entry("notes.md", vec![]);
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "account.md", "");
        let body = "People with [ACCOUNT ROLE] can do this.\n";
        write_file(&tmp, "notes.md", body);

        let index = MockIndex::new(vec![page, other]);
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: true,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("notes.md"),
                glob_filter: &[],
            },
        )
        .unwrap();

        assert!(
            report.matches.is_empty(),
            "a word touching a bracket edge must not be linked: {:?}",
            report.matches
        );
        let written = std::fs::read_to_string(tmp.path().join("notes.md")).unwrap();
        assert_eq!(written, body, "the file must be byte-identical after apply");
    }

    #[test]
    fn reference_definition_later_in_the_document_still_protects_earlier_shortcuts() {
        // Definitions are commonly grouped at the end of a document. Once a
        // definition exists, the shortcut-form usage is doubly inert: as a
        // generic bracket span, and (redundantly, harmlessly) as a
        // recognised reference. This regression guard is really about the
        // definition *line* itself staying untouched regardless of where in
        // the document it sits.
        let page = make_entry(
            "gamma.md",
            vec![("title", Value::String("Gamma".to_owned()))],
        );
        let other = make_entry("notes.md", vec![]);
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "gamma.md", "");
        let body = concat!(
            "Early mention: [Gamma] appears here.\n",
            "\n",
            "Much later:\n",
            "\n",
            "[Gamma]: https://example.com/gamma\n",
        );
        write_file(&tmp, "notes.md", body);

        let index = MockIndex::new(vec![page, other]);
        let report = auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: false,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("notes.md"),
                glob_filter: &[],
            },
        )
        .unwrap();

        assert!(
            report.matches.is_empty(),
            "the early shortcut reference must still be protected: {:?}",
            report.matches
        );
    }

    // -----------------------------------------------------------------
    // iter-217 NEW-3: alias emission when matched text differs from the
    // emitted target (including by case).
    // -----------------------------------------------------------------

    #[test]
    fn alias_emitted_when_matched_text_differs_by_case() {
        let page = make_entry("pulls.md", vec![]);
        let other = make_entry("notes.md", vec![]);
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "pulls.md", "");
        write_file(&tmp, "notes.md", "See Pulls for details.\n");

        let index = MockIndex::new(vec![page, other]);
        auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: true,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("notes.md"),
                glob_filter: &[],
            },
        )
        .unwrap();

        let written = std::fs::read_to_string(tmp.path().join("notes.md")).unwrap();
        assert_eq!(
            written, "See [[pulls|Pulls]] for details.\n",
            "a case-only difference must be preserved as an alias, not silently rewritten"
        );
    }

    #[test]
    fn alias_emitted_when_matched_text_differs_from_stem() {
        // "revocation" is a page alias for `revoke.md`; matching on the alias
        // text must preserve that surface form rather than silently
        // substituting the stem ("revoke").
        let other = make_entry("notes.md", vec![]);
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "revoke.md", "");
        write_file(&tmp, "notes.md", "See revocation for the process.\n");

        let page_with_alias = make_entry(
            "revoke.md",
            vec![(
                "aliases",
                Value::Array(vec![Value::String("revocation".to_owned())]),
            )],
        );
        let index = MockIndex::new(vec![page_with_alias, other]);
        auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: true,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("notes.md"),
                glob_filter: &[],
            },
        )
        .unwrap();

        let written = std::fs::read_to_string(tmp.path().join("notes.md")).unwrap();
        assert_eq!(
            written, "See [[revoke|revocation]] for the process.\n",
            "matched alias text must be preserved, not silently replaced by the stem"
        );
    }

    #[test]
    fn plain_wikilink_when_matched_text_equals_target() {
        let page = make_entry("target.md", vec![]);
        let other = make_entry("notes.md", vec![]);
        let tmp = TempDir::new().unwrap();
        write_file(&tmp, "target.md", "");
        write_file(&tmp, "notes.md", "See target for details.\n");

        let index = MockIndex::new(vec![page, other]);
        auto_link(
            &index,
            tmp.path(),
            &AutoLinkOptions {
                apply: true,
                min_length: 3,
                exclude_titles: &[],
                first_only: false,
                exclude_target_globs: &[],
                file_filter: Some("notes.md"),
                glob_filter: &[],
            },
        )
        .unwrap();

        let written = std::fs::read_to_string(tmp.path().join("notes.md")).unwrap();
        assert_eq!(
            written, "See [[target]] for details.\n",
            "an exact match must stay a plain wikilink, no alias noise"
        );
    }
}
