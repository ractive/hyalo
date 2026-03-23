#![allow(clippy::missing_errors_doc)]
use anyhow::Result;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::SystemTime;

use crate::commands::{FilesOrOutcome, collect_files};
use crate::output::{CommandOutcome, Format};
use hyalo_core::filter::extract_tags;
use hyalo_core::frontmatter::{infer_type, yaml_to_json};
use hyalo_core::scanner::{FrontmatterCollector, scan_file_multi};
use hyalo_core::tasks::TaskCounter;
use hyalo_core::types::{
    DirectoryCount, FileCounts, PropertySummaryEntry, RecentFile, StatusGroup, TagSummary,
    TagSummaryEntry, TaskCount, VaultSummary,
};

/// Show a high-level vault summary.
pub fn summary(
    dir: &Path,
    glob: Option<&str>,
    recent: usize,
    format: Format,
) -> Result<CommandOutcome> {
    let files = collect_files(dir, None, glob, format)?;
    let files = match files {
        FilesOrOutcome::Files(f) => f,
        FilesOrOutcome::Outcome(o) => return Ok(o),
    };

    // Aggregation state
    let mut total_files: usize = 0;
    // directory -> count
    let mut dir_counts: BTreeMap<String, usize> = BTreeMap::new();
    // (name, type) -> count
    let mut property_counts: BTreeMap<(String, String), usize> = BTreeMap::new();
    // tag (lowercase) -> (display_name, count)
    let mut tag_counts: BTreeMap<String, (String, usize)> = BTreeMap::new();
    // status_value -> Vec<rel_path>
    let mut status_groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    // aggregate task counts
    let mut total_tasks: usize = 0;
    let mut done_tasks: usize = 0;
    // (mtime_secs_as_i64_negated, rel_path) for sorting most-recent first
    let mut recent_entries: Vec<(i64, String)> = Vec::new();

    for (full_path, rel_path) in &files {
        total_files += 1;

        // Count by parent directory (relative to vault root)
        let dir_key = {
            use std::path::Path as P;
            let rel = P::new(rel_path.as_str());
            match rel.parent() {
                Some(p) if !p.as_os_str().is_empty() => p.to_string_lossy().into_owned(),
                _ => ".".to_owned(),
            }
        };
        *dir_counts.entry(dir_key).or_insert(0) += 1;

        // Single-pass scan: collect frontmatter + count tasks
        let mut fm = FrontmatterCollector::new(true);
        let mut counter = TaskCounter::new();
        scan_file_multi(full_path, &mut [&mut fm, &mut counter])?;
        let props = fm.into_props();
        let TaskCount { total, done } = counter.into_count();
        total_tasks += total;
        done_tasks += done;

        // Properties aggregation (skip "tags" — they have a dedicated section)
        for (name, value) in props.iter().filter(|(n, _)| n.as_str() != "tags") {
            let prop_type = infer_type(value).to_owned();
            *property_counts
                .entry((name.clone(), prop_type))
                .or_insert(0) += 1;
        }

        // Tags aggregation (case-insensitive, preserve first-seen casing)
        for tag in extract_tags(&props) {
            let key = tag.to_ascii_lowercase();
            tag_counts
                .entry(key)
                .and_modify(|e| e.1 += 1)
                .or_insert((tag, 1));
        }

        // Status grouping
        if let Some(status_val) = props.get("status") {
            let status_str = match yaml_to_json(status_val) {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            };
            status_groups
                .entry(status_str)
                .or_default()
                .push(rel_path.clone());
        }

        // Recent files: get mtime
        if let Ok(meta) = std::fs::metadata(full_path)
            && let Ok(mtime) = meta.modified()
        {
            // Negate so that sorting ascending gives most-recent first
            let secs = mtime
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            recent_entries.push((-secs, rel_path.clone()));
        }
    }

    // Build FileCounts
    let mut by_directory: Vec<DirectoryCount> = dir_counts
        .into_iter()
        .map(|(directory, count)| DirectoryCount { directory, count })
        .collect();
    by_directory.sort_by(|a, b| a.directory.cmp(&b.directory));

    let file_counts = FileCounts {
        total: total_files,
        by_directory,
    };

    // Build properties summary
    let mut properties: Vec<PropertySummaryEntry> = property_counts
        .into_iter()
        .map(|((name, prop_type), count)| PropertySummaryEntry {
            name,
            prop_type,
            count,
        })
        .collect();
    properties.sort_by(|a, b| a.name.cmp(&b.name).then(a.prop_type.cmp(&b.prop_type)));

    // Build tag summary
    let mut tags_vec: Vec<TagSummaryEntry> = tag_counts
        .into_iter()
        .map(|(_, (name, count))| TagSummaryEntry { name, count })
        .collect();
    tags_vec.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
    let tags_total = tags_vec.len();
    let tags = TagSummary {
        tags: tags_vec,
        total: tags_total,
    };

    // Build status groups (sorted by value)
    let status: Vec<StatusGroup> = status_groups
        .into_iter()
        .map(|(value, files)| StatusGroup { value, files })
        .collect();
    // Already sorted because BTreeMap is ordered by key

    // Build task count
    let tasks = TaskCount {
        total: total_tasks,
        done: done_tasks,
    };

    // Build recent files (sort most recent first, take top N)
    recent_entries.sort_by_key(|(neg_secs, _)| *neg_secs);
    let recent_files: Vec<RecentFile> = recent_entries
        .into_iter()
        .take(recent)
        .map(|(neg_secs, path)| {
            let secs = (-neg_secs) as u64;
            let modified = format_iso8601(secs);
            RecentFile { path, modified }
        })
        .collect();

    let vault_summary = VaultSummary {
        files: file_counts,
        properties,
        tags,
        status,
        tasks,
        recent_files,
    };

    let json_value =
        serde_json::to_value(&vault_summary).expect("derived Serialize impl should not fail");
    Ok(CommandOutcome::Success(crate::output::format_success(
        format,
        &json_value,
    )))
}

use super::format_iso8601;

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    macro_rules! md {
        ($s:expr) => {
            $s.strip_prefix('\n').unwrap_or($s)
        };
    }

    fn setup_vault() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();

        // Create a subdirectory
        fs::create_dir(tmp.path().join("notes")).unwrap();

        fs::write(
            tmp.path().join("a.md"),
            md!(r"
---
title: A
status: draft
tags:
  - rust
  - cli
---
- [ ] Open task
- [x] Done task
"),
        )
        .unwrap();

        fs::write(
            tmp.path().join("b.md"),
            md!(r"
---
title: B
status: done
tags:
  - rust
---
- [ ] Another task
"),
        )
        .unwrap();

        fs::write(
            tmp.path().join("notes/c.md"),
            md!(r"
---
title: C
status: draft
---
No tasks here.
"),
        )
        .unwrap();

        tmp
    }

    fn unwrap_success(outcome: CommandOutcome) -> serde_json::Value {
        match outcome {
            CommandOutcome::Success(s) => serde_json::from_str(&s).unwrap(),
            CommandOutcome::UserError(s) => panic!("expected success, got: {s}"),
        }
    }

    #[test]
    fn summary_file_counts() {
        let tmp = setup_vault();
        let val = unwrap_success(summary(tmp.path(), None, 10, Format::Json).unwrap());
        assert_eq!(val["files"]["total"], 3);
    }

    #[test]
    fn summary_directory_counts() {
        let tmp = setup_vault();
        let val = unwrap_success(summary(tmp.path(), None, 10, Format::Json).unwrap());
        let by_dir = val["files"]["by_directory"].as_array().unwrap();
        // Should have "." and "notes"
        assert!(
            by_dir
                .iter()
                .any(|d| d["directory"] == "." && d["count"] == 2)
        );
        assert!(
            by_dir
                .iter()
                .any(|d| d["directory"] == "notes" && d["count"] == 1)
        );
    }

    #[test]
    fn summary_task_counts() {
        let tmp = setup_vault();
        let val = unwrap_success(summary(tmp.path(), None, 10, Format::Json).unwrap());
        // a.md: 2 tasks (1 done), b.md: 1 task (0 done), c.md: 0 tasks
        assert_eq!(val["tasks"]["total"], 3);
        assert_eq!(val["tasks"]["done"], 1);
    }

    #[test]
    fn summary_property_aggregation() {
        let tmp = setup_vault();
        let val = unwrap_success(summary(tmp.path(), None, 10, Format::Json).unwrap());
        let props = val["properties"].as_array().unwrap();
        // title appears in all 3 files, status in all 3 files, tags in 2 files
        let title = props.iter().find(|p| p["name"] == "title").unwrap();
        assert_eq!(title["count"], 3);
        let status = props.iter().find(|p| p["name"] == "status").unwrap();
        assert_eq!(status["count"], 3);
    }

    #[test]
    fn summary_tag_aggregation() {
        let tmp = setup_vault();
        let val = unwrap_success(summary(tmp.path(), None, 10, Format::Json).unwrap());
        let total_tags = val["tags"]["total"].as_u64().unwrap();
        // rust and cli are unique tag names
        assert_eq!(total_tags, 2);
        let tags = val["tags"]["tags"].as_array().unwrap();
        let rust = tags.iter().find(|t| t["name"] == "rust").unwrap();
        assert_eq!(rust["count"], 2);
        let cli = tags.iter().find(|t| t["name"] == "cli").unwrap();
        assert_eq!(cli["count"], 1);
    }

    #[test]
    fn summary_status_grouping() {
        let tmp = setup_vault();
        let val = unwrap_success(summary(tmp.path(), None, 10, Format::Json).unwrap());
        let status_groups = val["status"].as_array().unwrap();
        let draft = status_groups
            .iter()
            .find(|g| g["value"] == "draft")
            .unwrap();
        // a.md and notes/c.md have status=draft
        assert_eq!(draft["files"].as_array().unwrap().len(), 2);
        let done = status_groups.iter().find(|g| g["value"] == "done").unwrap();
        assert_eq!(done["files"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn summary_recent_files_respects_limit() {
        let tmp = setup_vault();
        let val = unwrap_success(summary(tmp.path(), None, 2, Format::Json).unwrap());
        let recent = val["recent_files"].as_array().unwrap();
        // With limit=2, at most 2 recent files
        assert!(recent.len() <= 2);
    }

    #[test]
    fn summary_recent_files_have_iso8601_timestamps() {
        let tmp = setup_vault();
        let val = unwrap_success(summary(tmp.path(), None, 10, Format::Json).unwrap());
        let recent = val["recent_files"].as_array().unwrap();
        for entry in recent {
            let modified = entry["modified"].as_str().unwrap();
            // Should look like 2024-01-15T10:30:00Z
            assert!(
                modified.contains('T') && modified.ends_with('Z'),
                "unexpected timestamp: {modified}"
            );
        }
    }

    #[test]
    fn summary_glob_filter() {
        let tmp = setup_vault();
        // Only scan root files, not notes/
        let val = unwrap_success(summary(tmp.path(), Some("*.md"), 10, Format::Json).unwrap());
        assert_eq!(val["files"]["total"], 2);
    }

    #[test]
    fn summary_text_format() {
        let tmp = setup_vault();
        let outcome = summary(tmp.path(), None, 10, Format::Text).unwrap();
        match outcome {
            CommandOutcome::Success(s) => {
                assert!(s.contains("Files:"), "expected 'Files:' in: {s}");
                assert!(s.contains("Tasks:"), "expected 'Tasks:' in: {s}");
            }
            CommandOutcome::UserError(s) => panic!("expected success, got: {s}"),
        }
    }
}
