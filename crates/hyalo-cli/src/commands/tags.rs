#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::Path;

use crate::commands::{FilesOrOutcome, collect_files};
use crate::output::{CommandOutcome, Format};
use hyalo_core::filter::{extract_tags, tag_matches};
use hyalo_core::frontmatter;
use hyalo_core::index::VaultIndex;
use hyalo_core::types::TagSummaryEntry;
use serde::Serialize;
use serde_json::Value;

// ---------------------------------------------------------------------------
// Tag format validation
// ---------------------------------------------------------------------------

/// Returns `true` if `c` is a permitted tag character.
///
/// Permitted set:
/// - Unicode alphabetic characters (`char::is_alphabetic`) — covers ASCII
///   letters as well as CJK, Cyrillic, Greek, Arabic, etc.
/// - Unicode numeric characters (`char::is_numeric`).
/// - The punctuation `_`, `-`, `/` (the last separates tag hierarchy levels).
/// - Emoji and related pictographs (see [`is_emoji_like`]). This matches the
///   tag conventions used by Mastodon, Bluesky and GitHub labels and keeps
///   the write and query paths symmetric for non-Latin / non-ASCII users.
///
/// Tag identity is codepoint-equal — no NFC/NFD normalisation is performed.
#[must_use]
pub fn is_valid_tag_char(c: char) -> bool {
    c.is_alphabetic() || c.is_numeric() || matches!(c, '_' | '-' | '/') || is_emoji_like(c)
}

/// Conservative emoji-range check used by [`is_valid_tag_char`].
///
/// We deliberately avoid pulling in a Unicode-emoji table crate. The ranges
/// below cover the most commonly used emoji blocks plus the zero-width
/// joiner and variation-selector codepoints that appear inside
/// multi-codepoint emoji sequences. This is **not exhaustive** — emoji
/// presentation forms of codepoints outside these ranges (e.g. © U+00A9,
/// the combining enclosing keycap U+20E3 used in keycap sequences) are
/// not accepted. New emoji assigned inside the listed blocks in future
/// Unicode revisions are picked up automatically.
#[must_use]
pub fn is_emoji_like(c: char) -> bool {
    matches!(c as u32,
        0x2600..=0x27BF        // Misc symbols and dingbats (☀ ✨ ✅ …)
        | 0x2300..=0x23FF      // Misc technical (⌛ ⏰ …)
        | 0x2B00..=0x2BFF      // Misc symbols and arrows
        | 0x200D               // Zero-width joiner (emoji sequences)
        | 0xFE0F               // Variation selector-16 (emoji presentation)
        | 0x1F000..=0x1FFFF    // Supplementary symbols/pictographs (🎉 🚀 …)
    )
}

/// Validate an Obsidian-compatible tag name.
///
/// Rules:
/// - Must not be empty.
/// - Every character must satisfy [`is_valid_tag_char`].
/// - Must contain at least one non-numeric character (so a tag like `1984`
///   is rejected — write `y1984` instead).
///
/// This single validator is called from both write paths (`set --tag`,
/// `new --tag`, `tags rename`) and read paths (`find --tag`,
/// `--where-tag`) so the two directions can never drift.
pub fn validate_tag(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("tag name must not be empty".to_owned());
    }

    for ch in name.chars() {
        if !is_valid_tag_char(ch) {
            return Err(format!(
                "invalid character '{ch}' in tag name; allowed: Unicode letters and digits, _, -, /, and emoji"
            ));
        }
    }

    // Must contain at least one non-digit character (Unicode-aware).
    if name.chars().all(char::is_numeric) {
        return Err(format!(
            "tag '{name}' is all numeric; tags must contain at least one non-numeric character (e.g. 'y{name}')"
        ));
    }

    // Reject tags composed entirely of invisible joiner / variation-selector
    // codepoints — they are permitted *inside* emoji sequences but a tag
    // made only of them would be effectively blank.
    if name.chars().all(|c| matches!(c, '\u{200D}' | '\u{FE0F}')) {
        return Err(format!(
            "tag '{name}' contains only invisible joiner/variation-selector codepoints; tags must contain a visible character"
        ));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// `hyalo tags` — aggregate: unique tags with counts
// ---------------------------------------------------------------------------

/// Aggregate tag summary using pre-scanned index data.
///
/// `file_filter` is an optional list of vault-relative paths to include.
/// When `None` (or an empty slice), all index entries are used.
/// `limit` caps how many entries are returned (`None` = no cap).
pub fn tags_summary(
    index: &dyn VaultIndex,
    file_filter: Option<&[String]>,
    format: Format,
    limit: Option<usize>,
) -> Result<CommandOutcome> {
    // Aggregate case-insensitively: use lowercase key, preserve first-seen casing for display
    let mut counts: BTreeMap<String, (String, usize)> = BTreeMap::new();

    for entry in index.entries() {
        // Apply optional file-level filter
        if let Some(filter) = file_filter
            && !filter.is_empty()
            && !filter.iter().any(|f| f == &entry.rel_path)
        {
            continue;
        }
        for tag in &entry.tags {
            let key = tag.to_ascii_lowercase();
            counts
                .entry(key)
                .and_modify(|e| e.1 += 1)
                .or_insert_with(|| (tag.clone(), 1));
        }
    }

    let mut tags: Vec<TagSummaryEntry> = counts
        .into_iter()
        .map(|(_, (name, count))| TagSummaryEntry { name, count })
        .collect();

    let total = tags.len() as u64;
    if let Some(n) = limit.filter(|n| *n > 0) {
        tags.truncate(n);
    }
    let _ = format; // format is applied by the output pipeline
    Ok(CommandOutcome::success_with_total(
        serde_json::to_string_pretty(&tags).context("failed to serialize")?,
        total,
    ))
}

// ---------------------------------------------------------------------------
// `hyalo tags rename` — rename a tag across matched files
// ---------------------------------------------------------------------------

/// One concrete tag the rename expanded to (iter-266 TAG-1, DEC-282).
///
/// `tags rename --from music --to audio` renames `music` *and* every nested
/// `music/…` tag, so the pair the user typed is not the whole story. Each
/// actually-renamed tag gets an entry here with the number of files it
/// appeared in.
#[derive(Debug, Serialize)]
pub struct RenamedTag {
    pub from: String,
    pub to: String,
    pub files: usize,
}

/// Result of a `tags rename` operation.
#[derive(Debug, Serialize)]
pub struct RenameTagResult {
    pub from: String,
    pub to: String,
    pub dry_run: bool,
    pub modified: Vec<String>,
    /// Every tag the rename actually touched, parent and children alike.
    pub renamed_tags: Vec<RenamedTag>,
    pub skipped_count: usize,
    pub total: usize,
    pub scanned: usize,
}

/// Rewrite `tag` under a `from` → `to` rename, preserving the nested suffix.
///
/// `tag` must already satisfy [`tag_matches(tag, from)`](tag_matches), i.e. it
/// is either `from` itself or a `from/…` child; the suffix (`/genres`) is
/// carried over verbatim.
fn rename_nested_tag(tag: &str, from: &str, to: &str) -> String {
    debug_assert!(tag.len() >= from.len());
    format!("{to}{}", &tag[from.len()..])
}

/// Rename a tag across all matched files.
///
/// Obsidian semantics (DEC-282): renaming `music` also renames `music/genres`
/// and every other nested child, and `music` never matches `musical` — the
/// match must land on a `/` boundary.
///
/// - Atomic per-file: if the new tag already exists, the renamed duplicate is
///   dropped rather than written twice
/// - Skips files where neither the source tag nor any of its children appear
#[allow(clippy::too_many_arguments)]
pub fn tags_rename(
    dir: &Path,
    from: &str,
    to: &str,
    globs: &[String],
    dry_run: bool,
    format: Format,
    journal: &mut crate::commands::journal::MutationJournal<'_>,
) -> Result<CommandOutcome> {
    // Validate both tag names
    if let Err(msg) = validate_tag(from) {
        let out = crate::output::format_error(format, &msg, None, Some("invalid --from tag"), None);
        return Ok(CommandOutcome::UserError(out));
    }
    if let Err(msg) = validate_tag(to) {
        let out = crate::output::format_error(format, &msg, None, Some("invalid --to tag"), None);
        return Ok(CommandOutcome::UserError(out));
    }
    if from.eq_ignore_ascii_case(to) {
        let out = crate::output::format_error(
            format,
            "source and target tag names are identical (case-insensitive)",
            None,
            None,
            None,
        );
        return Ok(CommandOutcome::UserError(out));
    }

    let file_vec: Vec<String> = Vec::new();
    let files = collect_files(dir, &file_vec, globs, format)?;
    let files = match files {
        FilesOrOutcome::Files(f) => f,
        FilesOrOutcome::Outcome(o) => return Ok(o),
    };
    let scanned = files.len();

    let mut modified = Vec::new();
    let mut skipped_count: usize = 0;
    // lowercase old tag → (old tag as written, new tag, file count)
    let mut renamed_tags: BTreeMap<String, (String, String, usize)> = BTreeMap::new();

    for (full_path, rel_path) in &files {
        let mut props = match frontmatter::read_frontmatter(full_path) {
            Ok(p) => p,
            Err(e) if frontmatter::is_parse_error(&e) => {
                hyalo_core::warn::record_skip(
                    rel_path,
                    e.to_string(),
                    hyalo_core::warn::SkipKind::Frontmatter,
                );
                continue;
            }
            Err(e) => return Err(e),
        };

        let tags = extract_tags(&props);
        // DEC-282: the parent tag and every nested child are in scope, and the
        // parent itself need not be present for the children to be renamed.
        if !tags.iter().any(|t| tag_matches(t, from)) {
            skipped_count += 1;
            continue;
        }

        // Tag names this file's rename produces — used to drop a renamed tag
        // that would collide with a tag the file already carries.
        let produced: Vec<String> = tags
            .iter()
            .filter(|t| tag_matches(t, from))
            .map(|t| rename_nested_tag(t, from, to))
            .collect();

        // Remove old tag and add new tag, handling both sequence and scalar forms
        let mut remove_tags_key = false;
        let mut file_renames: Vec<(String, String)> = Vec::new();
        match props.get_mut("tags") {
            Some(Value::Array(seq)) => {
                let mut emitted: Vec<String> = Vec::new();
                let mut out_items: Vec<Value> = Vec::with_capacity(seq.len());
                for item in seq.iter() {
                    let Value::String(s) = item else {
                        out_items.push(item.clone());
                        continue;
                    };
                    let renamed = tag_matches(s, from);
                    let new_tag = if renamed {
                        rename_nested_tag(s, from, to)
                    } else {
                        s.clone()
                    };
                    // Collapse a collision the rename created (either the
                    // renamed tag duplicating one already emitted, or an
                    // untouched tag the rename is about to duplicate).
                    let collides = emitted.iter().any(|e| e.eq_ignore_ascii_case(&new_tag));
                    let rename_related =
                        renamed || produced.iter().any(|p| p.eq_ignore_ascii_case(&new_tag));
                    if collides && rename_related {
                        if renamed {
                            file_renames.push((s.clone(), new_tag));
                        }
                        continue;
                    }
                    if renamed {
                        file_renames.push((s.clone(), new_tag.clone()));
                    }
                    emitted.push(new_tag.clone());
                    out_items.push(Value::String(new_tag));
                }
                *seq = out_items;
                if seq.is_empty() {
                    remove_tags_key = true;
                }
            }
            Some(Value::String(s)) if tag_matches(s, from) => {
                let new_tag = rename_nested_tag(s, from, to);
                file_renames.push((s.clone(), new_tag.clone()));
                *s = new_tag;
            }
            _ => {}
        }
        if remove_tags_key {
            props.shift_remove("tags");
        }
        for (old, new) in file_renames {
            let counter = renamed_tags
                .entry(old.to_ascii_lowercase())
                .or_insert_with(|| (old, new, 0usize));
            counter.2 += 1;
        }

        if !dry_run {
            frontmatter::write_frontmatter_within(dir, full_path, &props)?;
            // Journal refresh covers entry AND link graph — the pre-journal
            // code only patched the entry (stale-graph bug class, ARCH-3).
            journal.update_entry(rel_path, props, full_path)?;
        }
        modified.push(rel_path.clone());
    }

    if !dry_run {
        journal.flush()?;
    }

    let total = modified.len() + skipped_count;
    let mut renamed_tags: Vec<RenamedTag> = renamed_tags
        .into_values()
        .map(|(from, to, files)| RenamedTag { from, to, files })
        .collect();
    renamed_tags.sort_by(|a, b| a.from.cmp(&b.from));
    let result = RenameTagResult {
        from: from.to_owned(),
        to: to.to_owned(),
        dry_run,
        modified,
        renamed_tags,
        skipped_count,
        total,
        scanned,
    };

    Ok(CommandOutcome::success(crate::output::format_output(
        format, &result,
    )))
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::items_after_test_module)] // dispatch handler appended below (ARCH-1, iter-225)
mod tests {
    use super::*;
    use crate::commands::journal::MutationJournal;
    use hyalo_core::filter::tag_matches;
    use hyalo_core::index::{ScanOptions, ScannedIndex};
    use indexmap::IndexMap;
    use serde_json::Value;
    use std::fs;

    /// Build a `ScannedIndex` from `dir` and call `tags_summary`.
    /// Mirrors the old disk-scan helper signature used in pre-Phase-5 tests.
    fn run_tags_summary(
        dir: &std::path::Path,
        file: Option<&str>,
        format: Format,
    ) -> anyhow::Result<CommandOutcome> {
        let all = hyalo_core::discovery::discover_files(dir)?;
        let file_pairs: Vec<(std::path::PathBuf, String)> = all
            .into_iter()
            .map(|p| {
                let rel = hyalo_core::discovery::relative_path(dir, &p);
                (p, rel)
            })
            .collect();
        let build = ScannedIndex::build(
            &file_pairs,
            None,
            &ScanOptions {
                scan_body: false,
                bm25_tokenize: false,
                default_language: None,
                frontmatter_link_props: None,
            },
        )?;
        let file_filter: Option<Vec<String>> = file.map(|f| vec![f.to_owned()]);
        tags_summary(&build.index, file_filter.as_deref(), format, None)
    }

    macro_rules! md {
        ($s:expr) => {
            $s.strip_prefix('\n').unwrap_or($s)
        };
    }

    // --- Tag validation ---

    #[test]
    fn valid_tag_simple() {
        assert!(validate_tag("inbox").is_ok());
        assert!(validate_tag("my-tag").is_ok());
        assert!(validate_tag("my_tag").is_ok());
        assert!(validate_tag("MyTag").is_ok());
        assert!(validate_tag("tag123").is_ok());
        assert!(validate_tag("y1984").is_ok());
    }

    #[test]
    fn valid_tag_nested() {
        assert!(validate_tag("inbox/processing").is_ok());
        assert!(validate_tag("project/hyalo/iteration").is_ok());
    }

    #[test]
    fn valid_tag_unicode_letters() {
        // CJK, Cyrillic, Greek, Arabic — all accepted on both write and query.
        assert!(validate_tag("日本語").is_ok());
        assert!(validate_tag("проект").is_ok());
        assert!(validate_tag("Ελληνικά").is_ok());
        assert!(validate_tag("مشروع").is_ok());
        assert!(validate_tag("café").is_ok());
        // Hierarchy with non-ASCII parents.
        assert!(validate_tag("проект/задача").is_ok());
    }

    #[test]
    fn valid_tag_emoji() {
        assert!(validate_tag("emoji-🎉").is_ok());
        assert!(validate_tag("🚀").is_ok());
        assert!(validate_tag("✨sparkle✨").is_ok());
    }

    #[test]
    fn invalid_tag_empty() {
        assert!(validate_tag("").is_err());
    }

    #[test]
    fn invalid_tag_numeric_only() {
        let err = validate_tag("1984").unwrap_err();
        assert!(err.contains("non-numeric"), "got: {err}");
    }

    #[test]
    fn invalid_tag_with_space() {
        let err = validate_tag("my tag").unwrap_err();
        assert!(err.contains("invalid character"), "got: {err}");
    }

    #[test]
    fn invalid_tag_special_chars() {
        assert!(validate_tag("tag!").is_err());
        assert!(validate_tag("tag@name").is_err());
        assert!(validate_tag("#tag").is_err());
    }

    #[test]
    fn invalid_tag_only_joiners() {
        // ZWJ / VS16 alone — permitted inside an emoji sequence, but a tag
        // made entirely of these invisible codepoints is rejected.
        let err = validate_tag("\u{200D}").unwrap_err();
        assert!(err.contains("invisible"), "got: {err}");
        let err = validate_tag("\u{FE0F}\u{200D}").unwrap_err();
        assert!(err.contains("invisible"), "got: {err}");
        // But a real emoji that uses them internally still passes.
        assert!(validate_tag("emoji-🎉").is_ok());
    }

    // --- Nested tag matching ---

    #[test]
    fn tag_matches_exact() {
        assert!(tag_matches("inbox", "inbox"));
    }

    #[test]
    fn tag_matches_child() {
        assert!(tag_matches("inbox/processing", "inbox"));
        assert!(tag_matches("inbox/to-read", "inbox"));
    }

    #[test]
    fn tag_no_match_prefix_without_slash() {
        assert!(!tag_matches("inboxes", "inbox"));
        assert!(!tag_matches("my-inbox", "inbox"));
    }

    #[test]
    fn tag_matches_case_insensitive() {
        assert!(tag_matches("Inbox", "inbox"));
        assert!(tag_matches("INBOX/PROCESSING", "inbox"));
        assert!(tag_matches("inbox", "INBOX"));
    }

    #[test]
    fn tag_no_match_different_tag() {
        assert!(!tag_matches("project", "inbox"));
    }

    // --- Tag extraction ---

    fn make_props(yaml: &str) -> IndexMap<String, Value> {
        serde_saphyr::from_str_with_options(yaml, hyalo_core::frontmatter::hyalo_options()).unwrap()
    }

    #[test]
    fn extract_tags_from_list() {
        let props = make_props(md!(r"
tags:
  - rust
  - cli
"));
        let tags = extract_tags(&props);
        assert_eq!(tags, vec!["rust", "cli"]);
    }

    #[test]
    fn extract_tags_from_scalar_string() {
        let props = make_props("tags: rust\n");
        let tags = extract_tags(&props);
        assert_eq!(tags, vec!["rust"]);
    }

    #[test]
    fn extract_tags_missing_key() {
        let props = make_props("title: Note\n");
        let tags = extract_tags(&props);
        assert!(tags.is_empty());
    }

    #[test]
    fn extract_tags_empty_list() {
        let props = make_props("tags: []\n");
        let tags = extract_tags(&props);
        assert!(tags.is_empty());
    }

    #[test]
    fn extract_tags_null() {
        let props = make_props("tags: ~\n");
        let tags = extract_tags(&props);
        assert!(tags.is_empty());
    }

    // --- tags_list command ---

    fn setup_vault() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("a.md"),
            md!(r"
---
tags:
  - rust
  - cli
---
# A
"),
        )
        .unwrap();
        fs::write(
            tmp.path().join("b.md"),
            md!(r"
---
tags:
  - rust
  - iteration
---
# B
"),
        )
        .unwrap();
        fs::write(tmp.path().join("c.md"), "No frontmatter.\n").unwrap();
        tmp
    }

    #[test]
    fn tags_summary_all_files() {
        let tmp = setup_vault();
        let outcome = run_tags_summary(tmp.path(), None, Format::Json).unwrap();
        let out = match outcome {
            CommandOutcome::Success { output: s, .. } | CommandOutcome::RawOutput(s) => s,
            CommandOutcome::RawBytes(b) => String::from_utf8_lossy(&b).into_owned(),
            CommandOutcome::UserError(s) => panic!("unexpected error: {s}"),
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let tags = parsed.as_array().unwrap();
        assert_eq!(tags.len(), 3); // rust, cli, iteration
        let rust = tags.iter().find(|t| t["name"] == "rust").unwrap();
        assert_eq!(rust["count"], 2);
    }

    #[test]
    fn tags_summary_single_file() {
        let tmp = setup_vault();
        let outcome = run_tags_summary(tmp.path(), Some("a.md"), Format::Json).unwrap();
        let out = match outcome {
            CommandOutcome::Success { output: s, .. } | CommandOutcome::RawOutput(s) => s,
            CommandOutcome::RawBytes(b) => String::from_utf8_lossy(&b).into_owned(),
            CommandOutcome::UserError(s) => panic!("unexpected error: {s}"),
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 2);
    }

    // --- discover_files used by tags_summary (read-only) still works without file/glob ---

    #[test]
    fn tags_summary_no_file_or_glob_reads_all() {
        let tmp = setup_vault();
        // tags_summary (read-only) still accepts no --file/--glob
        let outcome = run_tags_summary(tmp.path(), None, Format::Json).unwrap();
        assert!(matches!(outcome, CommandOutcome::Success { .. }));
    }

    #[test]
    fn tags_rename_basic() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
tags:
  - filtering
  - cli
---
"),
        )
        .unwrap();

        let outcome = tags_rename(
            tmp.path(),
            "filtering",
            "filters",
            &[],
            false,
            Format::Json,
            &mut MutationJournal::new(&mut None, None),
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["from"], "filtering");
        assert_eq!(parsed["to"], "filters");
        assert_eq!(parsed["modified"].as_array().unwrap().len(), 1);

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains("filters"));
        assert!(!content.contains("filtering"));
    }

    #[test]
    fn tags_rename_already_has_new_tag() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
tags:
  - filtering
  - filters
---
"),
        )
        .unwrap();

        let outcome = tags_rename(
            tmp.path(),
            "filtering",
            "filters",
            &[],
            false,
            Format::Json,
            &mut MutationJournal::new(&mut None, None),
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["modified"].as_array().unwrap().len(), 1);

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains("filters"));
        assert!(!content.contains("filtering"));
        // Should not have duplicate "filters"
        let count = content.matches("filters").count();
        assert_eq!(count, 1, "should not duplicate the new tag");
    }

    #[test]
    fn tags_rename_skips_missing() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
tags:
  - cli
---
"),
        )
        .unwrap();

        let outcome = tags_rename(
            tmp.path(),
            "filtering",
            "filters",
            &[],
            false,
            Format::Json,
            &mut MutationJournal::new(&mut None, None),
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["skipped_count"].as_u64().unwrap(), 1);
        assert_eq!(parsed["modified"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn tags_rename_same_name_error() {
        let tmp = tempfile::tempdir().unwrap();
        let outcome = tags_rename(
            tmp.path(),
            "foo",
            "foo",
            &[],
            false,
            Format::Json,
            &mut MutationJournal::new(&mut None, None),
        )
        .unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn tags_rename_invalid_tag_error() {
        let tmp = tempfile::tempdir().unwrap();
        let outcome = tags_rename(
            tmp.path(),
            "1984",
            "filters",
            &[],
            false,
            Format::Json,
            &mut MutationJournal::new(&mut None, None),
        )
        .unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn tags_summary_skips_malformed_yaml() {
        let tmp = tempfile::tempdir().unwrap();
        // Valid tagged file.
        fs::write(
            tmp.path().join("good.md"),
            md!(r"
---
tags:
  - rust
---
# Good
"),
        )
        .unwrap();
        // Malformed YAML: a bare colon key is rejected by serde_saphyr.
        fs::write(
            tmp.path().join("bad.md"),
            "---\n: invalid yaml [[[{\n---\n# Bad\n",
        )
        .unwrap();

        let outcome = run_tags_summary(tmp.path(), None, Format::Json).unwrap();
        let out = match outcome {
            CommandOutcome::Success { output: s, .. } | CommandOutcome::RawOutput(s) => s,
            CommandOutcome::RawBytes(b) => String::from_utf8_lossy(&b).into_owned(),
            CommandOutcome::UserError(s) => panic!("unexpected UserError: {s}"),
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let tags = parsed.as_array().unwrap();
        // The valid file's tag must appear.
        assert!(
            tags.iter().any(|t| t["name"] == "rust"),
            "expected 'rust' tag in {parsed}"
        );
    }
}

// ---------------------------------------------------------------------------
// Dispatch handler (ARCH-1, iter-225)
// ---------------------------------------------------------------------------

/// The `hyalo tags` dispatch arm, extracted verbatim from `dispatch.rs`.
/// `index_flags` was consumed earlier in `run.rs` (snapshot loading).
use crate::cli::args::TagsAction;

#[allow(clippy::items_after_statements)] // extracted handler keeps its mid-fn imports (ARCH-1, iter-225)
pub(crate) fn run(
    ctx: &mut crate::dispatch::CommandContext<'_>,
    bare_glob: Vec<String>,
    bare_limit: Option<usize>,
    action: Option<TagsAction>,
) -> Result<CommandOutcome> {
    let dir = ctx.dir;
    let site_prefix = ctx.site_prefix;
    let effective_format = ctx.effective_format;
    let snapshot_index = &mut *ctx.snapshot_index;
    use crate::cli::args::{IndexFlags, TagsAction};
    use crate::commands::find as find_commands;
    use crate::commands::{IndexResolution, ResolvedIndex, resolve_index};
    use crate::dispatch::resolve_limit;
    use hyalo_core::index::ScanOptions;

    // M-8: see the `properties` arm — bare `hyalo tags` is `tags summary`.
    let action = action.unwrap_or(TagsAction::Summary {
        glob: bare_glob,
        limit: bare_limit,
        index_flags: IndexFlags::default(),
    });
    match action {
        TagsAction::Summary {
            ref glob,
            limit: cli_limit,
            index_flags: _, // consumed in run.rs before dispatch
        } => match resolve_index(
            snapshot_index.as_ref(),
            dir,
            &[],
            glob,
            effective_format,
            site_prefix,
            false,
            &ScanOptions {
                scan_body: false,
                bm25_tokenize: false,
                default_language: None,
                frontmatter_link_props: ctx.frontmatter_link_props,
            },
        )? {
            IndexResolution::Resolved(ResolvedIndex::Snapshot(idx)) => {
                let filtered = find_commands::filter_index_entries(idx.entries(), &[], glob);
                match filtered {
                    Err(e) => Err(e),
                    Ok(filtered) => {
                        let paths: Vec<String> =
                            filtered.iter().map(|e| e.rel_path.clone()).collect();
                        let file_filter = if glob.is_empty() {
                            None
                        } else {
                            Some(paths.as_slice())
                        };
                        tags_summary(
                            idx,
                            file_filter,
                            effective_format,
                            resolve_limit(
                                cli_limit,
                                ctx.config_default_limit,
                                ctx.programmatic_output,
                            ),
                        )
                    }
                }
            }
            IndexResolution::Resolved(ResolvedIndex::Scanned(build)) => tags_summary(
                &build.index,
                None,
                effective_format,
                resolve_limit(cli_limit, ctx.config_default_limit, ctx.programmatic_output),
            ),
            IndexResolution::Outcome(outcome) => Ok(outcome),
        },
        TagsAction::Rename {
            from,
            to,
            glob,
            dry_run,
            index_flags: _, // consumed in run.rs before dispatch
        } => tags_rename(
            dir,
            &from,
            &to,
            &glob,
            dry_run,
            effective_format,
            &mut crate::commands::journal::MutationJournal::new(
                &mut *ctx.snapshot_index,
                ctx.index_path,
            ),
        ),
    }
}
