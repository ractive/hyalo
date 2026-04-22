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

use crate::discovery::{canonicalize_vault_dir, ensure_within_vault, match_globs};
use crate::fs_util::atomic_write;
use crate::index::{IndexEntry, VaultIndex};
use crate::links::extract_link_spans_with_original;
use crate::scanner::{
    FenceTracker, MAX_FILE_SIZE, is_comment_fence, strip_inline_code, strip_inline_comments,
};

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

/// A single proposed auto-link replacement.
#[derive(Debug, Clone, Serialize)]
pub struct AutoLinkMatch {
    /// Vault-relative path of the file containing the unlinked mention.
    pub file: String,
    /// 1-based line number.
    pub line: usize,
    /// Column offset (0-based byte offset within the line).
    pub col: usize,
    /// The matched text as it appears in the file.
    pub matched_text: String,
    /// The wikilink target (file stem or title) to link to.
    pub link_target: String,
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
    let mut target_sources: HashMap<String, HashSet<String>> = HashMap::new();
    for entry in title_map.values() {
        target_sources
            .entry(entry.link_target.clone())
            .or_default()
            .insert(entry.source_rel.clone());
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

/// Returns `true` if the byte at position `idx` in `s` is NOT an
/// alphanumeric ASCII character or underscore.
///
/// If `idx` is out of bounds (i.e. at the start/end of the string), that
/// boundary is considered to be a word boundary (returns `true`).
fn is_word_boundary_byte(s: &str, idx: usize) -> bool {
    match s.as_bytes().get(idx) {
        None => true,
        Some(&b) => !b.is_ascii_alphanumeric() && b != b'_',
    }
}

/// Verify that a match at `[start, end)` in `line` sits on word boundaries.
fn has_word_boundaries(line: &str, start: usize, end: usize) -> bool {
    // The byte before `start` (if any).
    let before_ok = if start == 0 {
        true
    } else {
        is_word_boundary_byte(line, start - 1)
    };
    // The byte at `end` (first byte after the match).
    let after_ok = is_word_boundary_byte(line, end);
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

        // Only retain file content when we'll actually need it for writing.
        let has_matches = !file_matches.is_empty();
        all_matches.extend(file_matches);
        if opts.apply && has_matches {
            scanned_content.insert(rel_path.clone(), content);
        }
    }

    // 4b. Apply --first-only: keep only the lowest-offset match per (source_file, target_title).
    //     Two-pass approach: intern strings as integer IDs to avoid cloning per match,
    //     then filter using the precomputed keep-mask.
    if opts.first_only {
        let mut file_ids: HashMap<&str, usize> = HashMap::new();
        let mut target_ids: HashMap<&str, usize> = HashMap::new();
        let mut seen: HashSet<(usize, usize)> = HashSet::new();

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
    if opts.apply {
        apply_matches(dir, &all_matches, &scanned_content)?;
    }

    let total = all_matches.len();
    Ok(AutoLinkReport {
        scanned,
        total,
        matches: all_matches,
        ambiguous_titles,
        applied: opts.apply,
    })
}

/// Scan a single file's content for unlinked title mentions, returning all
/// proposed [`AutoLinkMatch`] values.
fn scan_file_for_matches(
    content: &str,
    rel_path: &str,
    ac: &AhoCorasick,
    patterns_sorted: &[(&str, &TitleEntry)],
) -> Vec<AutoLinkMatch> {
    let mut results = Vec::new();

    let mut fence = FenceTracker::new();
    let mut in_comment_fence = false;
    let mut in_frontmatter = false;
    let mut frontmatter_done = false;
    let mut line_num = 0usize;

    for line in content.split('\n') {
        line_num += 1;

        // ---- Frontmatter handling ----
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
            // No frontmatter block; mark done.
            frontmatter_done = true;
        }

        // ---- Fenced code block ----
        if fence.process_line(line) {
            continue;
        }

        // ---- Comment fence (Obsidian %% blocks) ----
        // Must come after code-fence check: a `%%` inside a fenced code block
        // is literal text, not an Obsidian comment delimiter.
        if !fence.in_fence() && is_comment_fence(line) {
            in_comment_fence = !in_comment_fence;
            continue;
        }
        if in_comment_fence {
            continue;
        }

        // ---- Skip heading lines ----
        if line.trim_start().starts_with('#') {
            continue;
        }

        // ---- Strip inline code and inline comments ----
        let stripped_code = strip_inline_code(line);
        let cleaned = strip_inline_comments(stripped_code.as_ref());
        let cleaned_str: &str = cleaned.as_ref();

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

            // Use original line text for the matched_text (preserves casing).
            let matched_text = line.get(start..end).unwrap_or(&cleaned_str[start..end]);

            results.push(AutoLinkMatch {
                file: rel_path.to_owned(),
                line: line_num,
                col: start,
                matched_text: matched_text.to_owned(),
                link_target: entry.link_target.clone(),
            });
        }
    }

    results
}

// ---------------------------------------------------------------------------
// Apply changes
// ---------------------------------------------------------------------------

/// Apply a batch of matches to the vault by rewriting files atomically.
///
/// Replacements are applied to the content collected during the scan phase
/// (not re-read from disk), so byte offsets stay consistent.  A single
/// verification re-read is still performed per file to detect concurrent
/// modifications; if the on-disk content differs from the scanned snapshot
/// the file is skipped with a warning.
///
/// Every write path is guarded by a vault-boundary check so that no path
/// constructed from user-controlled data can escape the vault root.
fn apply_matches(
    dir: &Path,
    matches: &[AutoLinkMatch],
    scanned_content: &HashMap<String, String>,
) -> Result<()> {
    // Canonicalize the vault root once so every write can be checked cheaply.
    let canonical_vault = canonicalize_vault_dir(dir)
        .context("failed to canonicalize vault directory for write safety check")?;

    // Group matches by file.
    let mut by_file: HashMap<&str, Vec<&AutoLinkMatch>> = HashMap::new();
    for m in matches {
        by_file.entry(&m.file).or_default().push(m);
    }

    for (rel_path, file_matches) in by_file {
        let abs_path = dir.join(rel_path);

        // Vault-boundary safety check: refuse to write outside the vault root.
        let within = ensure_within_vault(&canonical_vault, &abs_path)
            .with_context(|| format!("could not verify {} is within vault", abs_path.display()))?;
        anyhow::ensure!(
            within,
            "refusing to write outside vault: {}",
            abs_path.display()
        );

        // Use content from the scan phase.  If the entry is missing, the
        // concurrent-modification check below would compare disk-to-disk and
        // always pass, so skip the file with a warning instead.
        let Some(content) = scanned_content.get(rel_path).map(String::as_str) else {
            eprintln!(
                "warning: {rel_path} not in scan cache, skipping (possible internal bug)"
            );
            continue;
        };

        // Detect concurrent modification: skip the file if it changed after scan.
        // A read failure (deleted, permissions, non-UTF-8) is treated as
        // "changed" — we must not overwrite what we cannot verify.
        let disk_content = match std::fs::read_to_string(&abs_path) {
            Ok(c) => c,
            Err(err) => {
                eprintln!("warning: could not verify {rel_path} after scan ({err}), skipping");
                continue;
            }
        };
        if disk_content != content {
            eprintln!("warning: {rel_path} was modified after scan, skipping");
            continue;
        }

        // Build per-line replacements: (line_idx (0-based), col, matched_text, link_target).
        // We need to apply them from last to first within each line (and last to first line).
        // Sort descending by (line, col).
        let mut sorted_matches: Vec<&AutoLinkMatch> = file_matches;
        sorted_matches.sort_by(|a, b| b.line.cmp(&a.line).then(b.col.cmp(&a.col)));

        // Work on a per-line basis. We need to reconstruct the full content after edits.
        // Split lines while preserving line endings.
        let mut lines: Vec<String> = split_lines_preserving_endings(content);

        for m in sorted_matches {
            let line_idx = m.line.saturating_sub(1);
            if let Some(line) = lines.get_mut(line_idx) {
                let start = m.col;
                let end = start + m.matched_text.len();
                if end <= line.len() && line.get(start..end) == Some(&m.matched_text) {
                    let replacement = format!("[[{}]]", m.link_target);
                    line.replace_range(start..end, &replacement);
                }
            }
        }

        let new_content = lines.concat();
        atomic_write(&abs_path, new_content.as_bytes())
            .with_context(|| format!("failed to write {}", abs_path.display()))?;
    }

    Ok(())
}

/// Split content into lines while preserving line endings (`\n` or `\r\n`).
///
/// This allows `concat()` of the result to reconstruct the original content
/// byte-for-byte (modulo applied replacements).
fn split_lines_preserving_endings(content: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut remaining = content;
    while let Some(pos) = remaining.find('\n') {
        // Include the `\n` (and possible preceding `\r`) in the line string.
        lines.push(remaining[..=pos].to_owned());
        remaining = &remaining[pos + 1..];
    }
    if !remaining.is_empty() {
        lines.push(remaining.to_owned());
    }
    lines
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

        let written = std::fs::read_to_string(tmp.path().join("notes.md")).unwrap();
        assert!(
            written.contains("[[target]]"),
            "written content should contain wikilink: {written}"
        );
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
}
