#![allow(clippy::missing_errors_doc)]
use anyhow::Result;
use std::path::Path;

use crate::commands::{FilesOrOutcome, collect_files};
use crate::output::{CommandOutcome, Format, format_output};
use hyalo_core::filter::extract_tags;
use hyalo_core::frontmatter;
use hyalo_core::index::{SnapshotIndex, VaultIndex, format_modified};
use hyalo_core::types::PropertySummaryEntry;
use serde::Serialize;

/// Aggregate summary: unique property names with types and file counts.
/// Scope is filtered by `--file` / `--glob` (or all files if both are None).
pub fn properties_summary(
    dir: &Path,
    file: Option<&str>,
    globs: &[String],
    format: Format,
) -> Result<CommandOutcome> {
    let file_vec: Vec<String> = file.map(|f| vec![f.to_owned()]).unwrap_or_default();
    let files = collect_files(dir, &file_vec, globs, format)?;
    let files = match files {
        FilesOrOutcome::Files(f) => f,
        FilesOrOutcome::Outcome(o) => return Ok(o),
    };

    // Aggregate: (name, type) -> count -- same key as summary command so both agree.
    let mut agg: std::collections::BTreeMap<(String, String), usize> =
        std::collections::BTreeMap::new();

    for (fp, rel) in &files {
        let props = match frontmatter::read_frontmatter(fp) {
            Ok(p) => p,
            Err(e) if frontmatter::is_parse_error(&e) => {
                eprintln!("warning: skipping {rel}: {e}");
                continue;
            }
            Err(e) => return Err(e),
        };
        for (key, value) in props.iter().filter(|(k, _)| k.as_str() != "tags") {
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

    Ok(CommandOutcome::Success(format_output(format, &result)))
}

/// Aggregate property summary using pre-scanned index data.
///
/// `file_filter` is an optional list of vault-relative paths to include.
/// When `None` (or an empty slice), all index entries are used.
pub fn properties_summary_from_index(
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

    Ok(CommandOutcome::Success(format_output(format, &result)))
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
/// - Preserves value and type (moves the `Value` in the BTreeMap)
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
                eprintln!("warning: skipping {rel_path}: {e}");
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

    Ok(CommandOutcome::Success(crate::output::format_output(
        format, &result,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    macro_rules! md {
        ($s:expr) => {
            $s.strip_prefix('\n').unwrap_or($s)
        };
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
            CommandOutcome::Success(s) => (s, true),
            CommandOutcome::UserError(s) => (s, false),
        }
    }

    #[test]
    fn properties_summary_aggregates() {
        let tmp = setup_dir();
        let (out, ok) =
            unwrap_output(properties_summary(tmp.path(), None, &[], Format::Json).unwrap());
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
        let CommandOutcome::Success(out) = outcome else {
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
        let CommandOutcome::Success(out) = outcome else {
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
        let CommandOutcome::Success(out) = outcome else {
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
            unwrap_output(properties_summary(tmp.path(), None, &[], Format::Json).unwrap());
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

        let outcome = properties_summary(tmp.path(), None, &[], Format::Json).unwrap();
        let (out, ok) = unwrap_output(outcome);
        assert!(ok, "expected Success, got UserError: {out}");

        let parsed: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap();
        let names: Vec<&str> = parsed.iter().map(|v| v["name"].as_str().unwrap()).collect();
        // The valid file's property must appear.
        assert!(names.contains(&"title"), "missing 'title' in {names:?}");
    }
}
