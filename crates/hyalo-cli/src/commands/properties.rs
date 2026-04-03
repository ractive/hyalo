#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result};
use std::path::Path;

use crate::commands::{FilesOrOutcome, collect_files};
use crate::output::{CommandOutcome, Format};
use hyalo_core::filter::extract_tags;
use hyalo_core::frontmatter;
use hyalo_core::index::{SnapshotIndex, VaultIndex, format_modified};
use hyalo_core::types::PropertySummaryEntry;
use serde::Serialize;

/// Aggregate property summary using pre-scanned index data.
///
/// `file_filter` is an optional list of vault-relative paths to include.
/// When `None` (or an empty slice), all index entries are used.
pub fn properties_summary(
    index: &dyn VaultIndex,
    file_filter: Option<&[String]>,
    format: Format,
) -> Result<CommandOutcome> {
    let mut agg: std::collections::BTreeMap<(String, String), usize> =
        std::collections::BTreeMap::new();

    for entry in index.entries() {
        // Apply optional file-level filter
        if let Some(filter) = file_filter
            && !filter.is_empty()
            && !filter.iter().any(|f| f == &entry.rel_path)
        {
            continue;
        }
        for (key, value) in entry
            .properties
            .iter()
            .filter(|(k, _)| k.as_str() != "tags")
        {
            let prop_type = frontmatter::infer_type(value).to_owned();
            *agg.entry((key.clone(), prop_type)).or_insert(0) += 1;
        }
    }

    let mut result: Vec<PropertySummaryEntry> = agg
        .into_iter()
        .map(|((name, prop_type), count)| PropertySummaryEntry {
            name,
            prop_type,
            count,
        })
        .collect();
    result.sort_by(|a, b| a.name.cmp(&b.name).then(a.prop_type.cmp(&b.prop_type)));

    let total = result.len() as u64;
    let _ = format;
    Ok(CommandOutcome::success_with_total(
        serde_json::to_string_pretty(&result).context("failed to serialize")?,
        total,
    ))
}

/// Result of a `properties rename` operation.
#[derive(Debug, Serialize)]
pub struct RenamePropertyResult {
    pub from: String,
    pub to: String,
    pub modified: Vec<String>,
    pub skipped: Vec<String>,
    pub conflicts: Vec<String>,
    pub total: usize,
    pub scanned: usize,
}

/// Rename a property key across all matched files.
///
/// - Preserves value and type (moves the `Value` in the IndexMap)
/// - Skips files where the source key is absent (reported in `skipped`)
/// - Skips files where the target key already exists (reported in `conflicts`)
pub fn properties_rename(
    dir: &Path,
    from: &str,
    to: &str,
    globs: &[String],
    format: Format,
    snapshot_index: &mut Option<SnapshotIndex>,
    index_path: Option<&Path>,
) -> Result<CommandOutcome> {
    if from == to {
        let out = crate::output::format_error(
            format,
            "source and target property names are identical",
            None,
            None,
            None,
        );
        return Ok(CommandOutcome::UserError(out));
    }

    let files = collect_files(dir, &[], globs, format)?;
    let files = match files {
        FilesOrOutcome::Files(f) => f,
        FilesOrOutcome::Outcome(o) => return Ok(o),
    };
    let scanned = files.len();

    let mut modified = Vec::new();
    let mut skipped = Vec::new();
    let mut conflicts = Vec::new();
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

        // Source key not present -- skip
        let Some(value) = props.shift_remove(from) else {
            skipped.push(rel_path.clone());
            continue;
        };

        // Target key already exists -- conflict, put the source back
        if props.contains_key(to) {
            props.insert(from.to_owned(), value);
            conflicts.push(rel_path.clone());
            continue;
        }

        props.insert(to.to_owned(), value);
        frontmatter::write_frontmatter(full_path, &props)?;
        if let Some(idx) = snapshot_index.as_mut()
            && let Some(entry) = idx.get_mut(rel_path)
        {
            let new_tags = extract_tags(&props);
            entry.properties = props;
            entry.tags = new_tags;
            entry.modified = format_modified(full_path)?;
            index_dirty = true;
        }
        modified.push(rel_path.clone());
    }

    if index_dirty && let (Some(idx), Some(idx_path)) = (snapshot_index.as_mut(), index_path) {
        idx.save_to(idx_path)?;
    }

    let total = modified.len() + skipped.len() + conflicts.len();
    let result = RenamePropertyResult {
        from: from.to_owned(),
        to: to.to_owned(),
        modified,
        skipped,
        conflicts,
        total,
        scanned,
    };

    let _ = format;
    Ok(CommandOutcome::success(
        serde_json::to_string_pretty(&result).context("failed to serialize")?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyalo_core::index::{ScanOptions, ScannedIndex};
    use std::fs;

    macro_rules! md {
        ($s:expr) => {
            $s.strip_prefix('\n').unwrap_or($s)
        };
    }

    /// Build a `ScannedIndex` from `dir` and call `properties_summary`.
    /// Mirrors the old disk-scan helper signature used in pre-Phase-5 tests.
    fn run_properties_summary(
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
        properties_summary(&build.index, file_filter.as_deref(), format)
    }

    fn setup_dir() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
title: Test
status: draft
priority: 3
tags:
  - rust
  - cli
---
# Hello
"),
        )
        .unwrap();
        fs::write(tmp.path().join("empty.md"), "No frontmatter here.\n").unwrap();
        tmp
    }

    /// Extract the output string from a `CommandOutcome`.
    fn unwrap_output(outcome: CommandOutcome) -> (String, bool) {
        match outcome {
            CommandOutcome::Success { output: s, .. } | CommandOutcome::RawOutput(s) => (s, true),
            CommandOutcome::UserError(s) => (s, false),
        }
    }

    #[test]
    fn properties_summary_aggregates() {
        let tmp = setup_dir();
        let (out, ok) =
            unwrap_output(run_properties_summary(tmp.path(), None, Format::Json).unwrap());
        assert!(ok);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap();
        assert!(!parsed.is_empty());
        let names: Vec<&str> = parsed.iter().map(|v| v["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"title"));
        assert!(names.contains(&"status"));
    }

    #[test]
    fn properties_rename_basic() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
title: Note
keywords: test
---
"),
        )
        .unwrap();

        let outcome = properties_rename(
            tmp.path(),
            "keywords",
            "Keywords",
            &[],
            Format::Json,
            &mut None,
            None,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["from"], "keywords");
        assert_eq!(parsed["to"], "Keywords");
        assert_eq!(parsed["modified"].as_array().unwrap().len(), 1);

        let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
        assert!(content.contains("Keywords:"));
        assert!(!content.contains("keywords:"));
    }

    #[test]
    fn properties_rename_skips_missing() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
title: Note
---
"),
        )
        .unwrap();

        let outcome = properties_rename(
            tmp.path(),
            "keywords",
            "Keywords",
            &[],
            Format::Json,
            &mut None,
            None,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["skipped"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["modified"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn properties_rename_conflict() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("note.md"),
            md!(r"
---
title: Note
keywords: test
Keywords: other
---
"),
        )
        .unwrap();

        let outcome = properties_rename(
            tmp.path(),
            "keywords",
            "Keywords",
            &[],
            Format::Json,
            &mut None,
            None,
        )
        .unwrap();
        let CommandOutcome::Success { output: out, .. } = outcome else {
            panic!("expected success")
        };
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["conflicts"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["modified"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn properties_rename_same_name_error() {
        let tmp = tempfile::tempdir().unwrap();
        let outcome =
            properties_rename(tmp.path(), "foo", "foo", &[], Format::Json, &mut None, None)
                .unwrap();
        assert!(matches!(outcome, CommandOutcome::UserError(_)));
    }

    #[test]
    fn properties_summary_distinguishes_types() {
        // Same property name with different types should produce separate entries
        // (consistent with summary command's (name, type) keying)
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("text.md"), "---\npriority: high\n---\n").unwrap();
        fs::write(tmp.path().join("number.md"), "---\npriority: 3\n---\n").unwrap();

        let (out, ok) =
            unwrap_output(run_properties_summary(tmp.path(), None, Format::Json).unwrap());
        assert!(ok);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap();
        let priority_entries: Vec<&serde_json::Value> =
            parsed.iter().filter(|p| p["name"] == "priority").collect();
        // Two entries: one text, one number -- not collapsed into a single entry
        assert_eq!(
            priority_entries.len(),
            2,
            "expected 2 entries for 'priority', got: {priority_entries:?}"
        );
        assert_eq!(priority_entries[0]["count"], 1);
        assert_eq!(priority_entries[1]["count"], 1);
    }

    #[test]
    fn properties_summary_skips_malformed_yaml() {
        let tmp = tempfile::tempdir().unwrap();
        // Valid file with a known property.
        fs::write(
            tmp.path().join("good.md"),
            md!(r"
---
title: Good Note
---
# Hello
"),
        )
        .unwrap();
        // Malformed YAML: a bare colon key is rejected by serde_saphyr.
        fs::write(
            tmp.path().join("bad.md"),
            "---\n: invalid yaml [[[{\n---\n# Bad\n",
        )
        .unwrap();

        let outcome = run_properties_summary(tmp.path(), None, Format::Json).unwrap();
        let (out, ok) = unwrap_output(outcome);
        assert!(ok, "expected Success, got UserError: {out}");

        let parsed: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap();
        let names: Vec<&str> = parsed.iter().map(|v| v["name"].as_str().unwrap()).collect();
        // The valid file's property must appear.
        assert!(names.contains(&"title"), "missing 'title' in {names:?}");
    }

    /// A file whose entire content is a bare `---` (no closing delimiter) must be
    /// skipped by `properties_summary`, matching the behaviour of `summary`.
    ///
    /// Before the fix, `read_frontmatter` returned `Ok(empty)` for this edge case
    /// while `scan_file_multi` correctly returned an "unclosed frontmatter" error,
    /// causing the two commands to diverge on total file counts.
    #[test]
    fn properties_summary_skips_bare_opening_delimiter() {
        let tmp = tempfile::tempdir().unwrap();
        // File with a valid title — must be counted.
        fs::write(tmp.path().join("good.md"), "---\ntitle: Present\n---\n").unwrap();
        // File that is just `---` — no closing delimiter, no content.
        // scan_file_multi treats this as an unclosed-frontmatter parse error;
        // properties_summary must do the same (not silently count it as empty).
        fs::write(tmp.path().join("bare.md"), "---\n").unwrap();

        let outcome = run_properties_summary(tmp.path(), None, Format::Json).unwrap();
        let (out, ok) = unwrap_output(outcome);
        assert!(ok, "expected Success: {out}");

        let parsed: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap();
        let title_entry = parsed.iter().find(|p| p["name"] == "title").unwrap();
        // Only the good file contributes — count must be 1.
        assert_eq!(
            title_entry["count"], 1,
            "bare `---` file must not inflate the count: {parsed:?}"
        );
    }
}
