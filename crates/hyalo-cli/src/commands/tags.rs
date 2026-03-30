#![allow(clippy::missing_errors_doc)]
use anyhow::Result;
use std::collections::BTreeMap;
use std::path::Path;

use crate::commands::{FilesOrOutcome, collect_files};
use crate::output::{CommandOutcome, Format};
use hyalo_core::filter::extract_tags;
use hyalo_core::frontmatter;
use hyalo_core::index::{SnapshotIndex, VaultIndex, format_modified};
use hyalo_core::types::{TagSummary, TagSummaryEntry};
use serde::Serialize;
use serde_json::Value;

// ---------------------------------------------------------------------------
// Tag format validation
// ---------------------------------------------------------------------------

/// Validate an Obsidian-compatible tag name.
/// Rules:
/// - Only letters, digits, underscores (`_`), hyphens (`-`), forward slashes (`/`)
/// - Must contain at least one non-numeric character
/// - Must not be empty
/// - Forward slashes are allowed for hierarchy (e.g. `inbox/processing`)
pub fn validate_tag(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("tag name must not be empty".to_owned());
    }

    for ch in name.chars() {
        if !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-' && ch != '/' {
            return Err(format!(
                "invalid character '{ch}' in tag name; allowed: letters, digits, _, -, /"
            ));
        }
    }

    // Must contain at least one non-digit character
    if name.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!(
            "tag '{name}' is all numeric; tags must contain at least one non-numeric character (e.g. 'y{name}')"
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
pub fn tags_summary(
    index: &dyn VaultIndex,
    file_filter: Option<&[String]>,
    format: Format,
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

    let tags: Vec<TagSummaryEntry> = counts
        .into_iter()
        .map(|(_, (name, count))| TagSummaryEntry { name, count })
        .collect();

    let total = tags.len();
    let result = TagSummary { tags, total };

    Ok(CommandOutcome::Success(crate::output::format_output(
        format, &result,
    )))
}

// ---------------------------------------------------------------------------
// `hyalo tags rename` — rename a tag across matched files
// ---------------------------------------------------------------------------

/// Result of a `tags rename` operation.
#[derive(Debug, Serialize)]
pub struct RenameTagResult {
    pub from: String,
    pub to: String,
    pub modified: Vec<String>,
    pub skipped: Vec<String>,
    pub total: usize,
    pub scanned: usize,
}

/// Rename a tag across all matched files.
///
/// - Atomic per-file: if new tag already exists, only old one is removed
/// - Skips files where the source tag is absent
pub fn tags_rename(
    dir: &Path,
    from: &str,
    to: &str,
    globs: &[String],
    format: Format,
    snapshot_index: &mut Option<SnapshotIndex>,
    index_path: Option<&Path>,
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
    let mut skipped = Vec::new();
    let mut index_dirty = false;

    for (full_path, rel_path) in &files {
        let mut props = match frontmatter::read_frontmatter(full_path) {
            Ok(p) => p,
            Err(e) if frontmatter::is_parse_error(&e) => {
                crate::warn::warn(format!("skipping {rel_path}: {e}"));
                continue;
            }
            Err(e) => return Err(e),
        };

        let tags = extract_tags(&props);
        let has_old = tags.iter().any(|t| t.eq_ignore_ascii_case(from));
        if !has_old {
            skipped.push(rel_path.clone());
            continue;
        }

        let has_new = tags.iter().any(|t| t.eq_ignore_ascii_case(to));

        // Remove old tag and add new tag, handling both sequence and scalar forms
        let mut remove_tags_key = false;
        match props.get_mut("tags") {
            Some(Value::Array(seq)) => {
                seq.retain(|v| match v {
                    Value::String(s) => !s.eq_ignore_ascii_case(from),
                    _ => true,
                });
                if !has_new {
                    seq.push(Value::String(to.to_owned()));
                }
                if seq.is_empty() {
                    remove_tags_key = true;
                }
            }
            Some(Value::String(s)) if s.eq_ignore_ascii_case(from) => {
                if has_new {
                    remove_tags_key = true;
                } else {
                    *s = to.to_owned();
                }
            }
            _ => {}
        }
        if remove_tags_key {
            props.shift_remove("tags");
        }

        frontmatter::write_frontmatter(full_path, &props)?;
        if let Some(idx) = snapshot_index.as_mut()
            && let Some(entry) = idx.get_mut(rel_path)
        {
            entry.properties = props.clone();
            entry.tags = extract_tags(&props);
            entry.modified = format_modified(full_path)?;
            index_dirty = true;
        }
        modified.push(rel_path.clone());
    }

    if index_dirty && let (Some(idx), Some(idx_path)) = (snapshot_index.as_mut(), index_path) {
        idx.save_to(idx_path)?;
    }

    let total = modified.len() + skipped.len();
    let result = RenameTagResult {
        from: from.to_owned(),
        to: to.to_owned(),
        modified,
        skipped,
        total,
        scanned,
    };

    Ok(CommandOutcome::Success(crate::output::format_output(
        format, &result,
    )))
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
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
        let build = ScannedIndex::build(&file_pairs, None, &ScanOptions { scan_body: false })?;
        let file_filter: Option<Vec<String>> = file.map(|f| vec![f.to_owned()]);
        tags_summary(&build.index, file_filter.as_deref(), format)
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
            CommandOutcome::Success(s) => s,
            CommandOutcome::UserError(s) => panic!("unexpected error: {s}"),
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let tags = parsed["tags"].as_array().unwrap();
        assert_eq!(parsed["total"], 3); // rust, cli, iteration
        let rust = tags.iter().find(|t| t["name"] == "rust").unwrap();
        assert_eq!(rust["count"], 2);
    }

    #[test]
    fn tags_summary_single_file() {
        let tmp = setup_vault();
        let outcome = run_tags_summary(tmp.path(), Some("a.md"), Format::Json).unwrap();
        let out = match outcome {
            CommandOutcome::Success(s) => s,
            CommandOutcome::UserError(s) => panic!("unexpected error: {s}"),
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["total"], 2);
    }

    // --- discover_files used by tags_summary (read-only) still works without file/glob ---

    #[test]
    fn tags_summary_no_file_or_glob_reads_all() {
        let tmp = setup_vault();
        // tags_summary (read-only) still accepts no --file/--glob
        let outcome = run_tags_summary(tmp.path(), None, Format::Json).unwrap();
        assert!(matches!(outcome, CommandOutcome::Success(_)));
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
            Format::Json,
            &mut None,
            None,
        )
        .unwrap();
        let CommandOutcome::Success(out) = outcome else {
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
            Format::Json,
            &mut None,
            None,
        )
        .unwrap();
        let CommandOutcome::Success(out) = outcome else {
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
            Format::Json,
            &mut None,
            None,
        )
        .unwrap();
        let CommandOutcome::Success(out) = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["skipped"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["modified"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn tags_rename_same_name_error() {
        let tmp = tempfile::tempdir().unwrap();
        let outcome =
            tags_rename(tmp.path(), "foo", "foo", &[], Format::Json, &mut None, None).unwrap();
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
            Format::Json,
            &mut None,
            None,
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
            CommandOutcome::Success(s) => s,
            CommandOutcome::UserError(s) => panic!("unexpected UserError: {s}"),
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let tags = parsed["tags"].as_array().unwrap();
        // The valid file's tag must appear.
        assert!(
            tags.iter().any(|t| t["name"] == "rust"),
            "expected 'rust' tag in {parsed}"
        );
    }
}
