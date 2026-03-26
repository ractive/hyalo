#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::commands::{FilesOrOutcome, collect_files};
use crate::output::{CommandOutcome, Format};
use hyalo_core::filter::extract_tags;
use hyalo_core::frontmatter::{infer_type, yaml_to_json};
use hyalo_core::link_graph::{FileLinks, LinkGraph, LinkGraphVisitor};
use hyalo_core::scanner::{FrontmatterCollector, scan_file_multi};
use hyalo_core::tasks::TaskCounter;
use hyalo_core::types::{
    DirectoryCount, FileCounts, OrphanSummary, PropertySummaryEntry, RecentFile, StatusGroup,
    TagSummary, TagSummaryEntry, TaskCount, VaultSummary,
};

/// Show a high-level vault summary.
pub fn summary(
    dir: &Path,
    glob: Option<&str>,
    recent: usize,
    depth: Option<usize>,
    format: Format,
) -> Result<CommandOutcome> {
    let files = collect_files(dir, &[], glob, format)?;
    let files = match files {
        FilesOrOutcome::Files(f) => f,
        FilesOrOutcome::Outcome(o) => return Ok(o),
    };

    // When no glob filter is active, we collect links alongside frontmatter+tasks
    // in a single pass per file, then build the LinkGraph from collected data.
    // This avoids a second full-vault scan for orphan detection.
    // When a glob IS active, orphan detection needs a vault-wide link graph
    // (links from outside the glob count), so we do a separate LinkGraph::build.
    let collect_links = glob.is_none();

    // Aggregation state
    let mut total_files: usize = 0;
    let mut dir_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut property_counts: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut tag_counts: BTreeMap<String, (String, usize)> = BTreeMap::new();
    let mut status_groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut total_tasks: usize = 0;
    let mut done_tasks: usize = 0;
    let mut recent_entries: Vec<(i64, String)> = Vec::new();
    let mut good_files: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut collected_links: Vec<FileLinks> = Vec::new();

    for (full_path, rel_path) in &files {
        total_files += 1;

        let dir_key = {
            use std::path::Path as P;
            let rel = P::new(rel_path.as_str());
            match rel.parent() {
                Some(p) if !p.as_os_str().is_empty() => p.to_string_lossy().into_owned(),
                _ => ".".to_owned(),
            }
        };

        // Single-pass scan: collect frontmatter + tasks + links (when applicable)
        let mut fm = FrontmatterCollector::new(true);
        let mut counter = TaskCounter::new();
        let mut link_visitor = if collect_links {
            Some(LinkGraphVisitor::new(PathBuf::from(rel_path)))
        } else {
            None
        };

        let scan_result = match link_visitor.as_mut() {
            Some(lv) => scan_file_multi(full_path, &mut [&mut fm, &mut counter, lv]),
            None => scan_file_multi(full_path, &mut [&mut fm, &mut counter]),
        };
        match scan_result {
            Ok(()) => {}
            Err(e) if hyalo_core::frontmatter::is_parse_error(&e) => {
                eprintln!("warning: skipping {rel_path}: {e}");
                total_files -= 1;
                continue;
            }
            Err(e) => return Err(e),
        }

        good_files.push((full_path.clone(), PathBuf::from(rel_path)));
        if let Some(lv) = link_visitor {
            collected_links.push(lv.into_file_links());
        }
        *dir_counts.entry(dir_key).or_insert(0) += 1;
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
            let secs = mtime
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            recent_entries.push((-secs, rel_path.clone()));
        }
    }

    // Apply depth limit: collapse deeper directories into their nearest visible ancestor
    if let Some(max_depth) = depth {
        let original: Vec<(String, usize)> = dir_counts.into_iter().collect();
        dir_counts = BTreeMap::new();
        for (dir_key, count) in original {
            let target = truncate_to_depth(&dir_key, max_depth);
            *dir_counts.entry(target).or_insert(0) += count;
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

    // Build orphan list: files with no inbound AND no outbound links (fully isolated).
    // When no glob: use pre-collected link data (single pass, no re-read).
    // When glob: build vault-wide link graph so links from outside the glob count.
    let orphans = {
        let build = if collect_links {
            LinkGraph::from_file_links(collected_links, None)
        } else {
            LinkGraph::build(dir, None)
                .context("failed to build link graph for orphan detection")?
        };
        let targets = build.graph.all_targets();
        let sources = build.graph.all_sources();
        let mut orphan_files: Vec<String> = good_files
            .iter()
            .map(|(_, rel)| rel.to_string_lossy())
            .filter(|rel| {
                let rel_str: &str = rel.as_ref();
                let without_md = rel_str.strip_suffix(".md").unwrap_or(rel_str);
                let has_inbound = targets.contains(rel_str) || targets.contains(without_md);
                let has_outbound = sources.contains(rel_str);
                !has_inbound && !has_outbound
            })
            .map(|rel| rel.into_owned())
            .collect();
        orphan_files.sort();
        let total = orphan_files.len();
        OrphanSummary {
            total,
            files: orphan_files,
        }
    };

    let vault_summary = VaultSummary {
        files: file_counts,
        orphans,
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

/// Truncate a directory path to at most `max_depth` components.
///
/// - `"."` always stays `"."`
/// - `max_depth == 0` collapses everything to `"."`
/// - `"notes/sub/deep"` with `max_depth == 1` returns `"notes"`
fn truncate_to_depth(dir: &str, max_depth: usize) -> String {
    if dir == "." {
        return ".".to_owned();
    }
    if max_depth == 0 {
        return ".".to_owned();
    }
    let parts: Vec<&str> = dir.split('/').collect();
    if parts.len() <= max_depth {
        dir.to_owned()
    } else {
        parts[..max_depth].join("/")
    }
}

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
        let val = unwrap_success(summary(tmp.path(), None, 10, None, Format::Json).unwrap());
        assert_eq!(val["files"]["total"], 3);
    }

    #[test]
    fn summary_directory_counts() {
        let tmp = setup_vault();
        let val = unwrap_success(summary(tmp.path(), None, 10, None, Format::Json).unwrap());
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
        let val = unwrap_success(summary(tmp.path(), None, 10, None, Format::Json).unwrap());
        // a.md: 2 tasks (1 done), b.md: 1 task (0 done), c.md: 0 tasks
        assert_eq!(val["tasks"]["total"], 3);
        assert_eq!(val["tasks"]["done"], 1);
    }

    #[test]
    fn summary_property_aggregation() {
        let tmp = setup_vault();
        let val = unwrap_success(summary(tmp.path(), None, 10, None, Format::Json).unwrap());
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
        let val = unwrap_success(summary(tmp.path(), None, 10, None, Format::Json).unwrap());
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
        let val = unwrap_success(summary(tmp.path(), None, 10, None, Format::Json).unwrap());
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
        let val = unwrap_success(summary(tmp.path(), None, 2, None, Format::Json).unwrap());
        let recent = val["recent_files"].as_array().unwrap();
        // With limit=2, at most 2 recent files
        assert!(recent.len() <= 2);
    }

    #[test]
    fn summary_recent_files_have_iso8601_timestamps() {
        let tmp = setup_vault();
        let val = unwrap_success(summary(tmp.path(), None, 10, None, Format::Json).unwrap());
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
        let val =
            unwrap_success(summary(tmp.path(), Some("*.md"), 10, None, Format::Json).unwrap());
        assert_eq!(val["files"]["total"], 2);
    }

    #[test]
    fn summary_text_format() {
        let tmp = setup_vault();
        let outcome = summary(tmp.path(), None, 10, None, Format::Text).unwrap();
        match outcome {
            CommandOutcome::Success(s) => {
                assert!(s.contains("Files:"), "expected 'Files:' in: {s}");
                assert!(s.contains("Tasks:"), "expected 'Tasks:' in: {s}");
            }
            CommandOutcome::UserError(s) => panic!("expected success, got: {s}"),
        }
    }

    /// Vault with three nesting levels: ".", "notes", "notes/sub"
    fn setup_vault_nested() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("notes/sub")).unwrap();

        let simple = |title: &str| format!("---\ntitle: {title}\n---\n");

        fs::write(tmp.path().join("root.md"), simple("Root")).unwrap();
        fs::write(tmp.path().join("notes/n.md"), simple("N")).unwrap();
        fs::write(tmp.path().join("notes/sub/s.md"), simple("S")).unwrap();

        tmp
    }

    #[test]
    fn summary_depth_zero_collapses_all() {
        let tmp = setup_vault_nested();
        let val = unwrap_success(summary(tmp.path(), None, 10, Some(0), Format::Json).unwrap());
        let by_dir = val["files"]["by_directory"].as_array().unwrap();
        assert_eq!(by_dir.len(), 1);
        assert_eq!(by_dir[0]["directory"], ".");
        assert_eq!(by_dir[0]["count"], 3);
    }

    #[test]
    fn summary_depth_one_shows_top_level() {
        let tmp = setup_vault_nested();
        let val = unwrap_success(summary(tmp.path(), None, 10, Some(1), Format::Json).unwrap());
        let by_dir = val["files"]["by_directory"].as_array().unwrap();
        // "." (1 file) and "notes" (2 files collapsed from notes/ and notes/sub/)
        assert_eq!(by_dir.len(), 2);
        let dot = by_dir.iter().find(|d| d["directory"] == ".").unwrap();
        assert_eq!(dot["count"], 1);
        let notes = by_dir.iter().find(|d| d["directory"] == "notes").unwrap();
        assert_eq!(notes["count"], 2);
    }

    #[test]
    fn summary_depth_none_shows_all() {
        let tmp = setup_vault_nested();
        let val = unwrap_success(summary(tmp.path(), None, 10, None, Format::Json).unwrap());
        let by_dir = val["files"]["by_directory"].as_array().unwrap();
        assert_eq!(by_dir.len(), 3);
        assert!(by_dir.iter().any(|d| d["directory"] == "."));
        assert!(by_dir.iter().any(|d| d["directory"] == "notes"));
        assert!(by_dir.iter().any(|d| d["directory"] == "notes/sub"));
    }

    #[test]
    fn summary_depth_stats_unaffected_by_depth() {
        // Stats (tasks, tags, properties) must be computed from all files regardless of depth
        let tmp = setup_vault_nested();
        let val_no_depth =
            unwrap_success(summary(tmp.path(), None, 10, None, Format::Json).unwrap());
        let val_depth0 =
            unwrap_success(summary(tmp.path(), None, 10, Some(0), Format::Json).unwrap());
        assert_eq!(val_no_depth["files"]["total"], val_depth0["files"]["total"]);
        assert_eq!(val_no_depth["tasks"], val_depth0["tasks"]);
        assert_eq!(val_no_depth["tags"], val_depth0["tags"]);
        assert_eq!(val_no_depth["properties"], val_depth0["properties"]);
    }

    /// Orphan detection: a.md links to b.md, orphan.md has no links in or out.
    /// Both code paths (no glob = single-pass, glob = separate scan) must agree.
    #[test]
    fn summary_orphan_detection_no_glob() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a.md"), "---\ntitle: A\n---\n[[b]]\n").unwrap();
        fs::write(tmp.path().join("b.md"), "---\ntitle: B\n---\nNo links.\n").unwrap();
        fs::write(
            tmp.path().join("orphan.md"),
            "---\ntitle: Orphan\n---\nNo links.\n",
        )
        .unwrap();

        // No glob: single-pass code path
        let val = unwrap_success(summary(tmp.path(), None, 10, None, Format::Json).unwrap());
        let orphan_files: Vec<&str> = val["orphans"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();

        // orphan.md has no inbound and no outbound links
        assert!(
            orphan_files.iter().any(|f| f.contains("orphan")),
            "orphan.md must appear in orphans: {orphan_files:?}"
        );
        // a.md links out, so it is not an orphan
        assert!(
            !orphan_files
                .iter()
                .any(|f| f.contains("/a") || *f == "a.md"),
            "a.md must NOT appear in orphans: {orphan_files:?}"
        );
        // b.md has an inbound link from a.md, so it is not an orphan
        assert!(
            !orphan_files
                .iter()
                .any(|f| f.contains("/b") || *f == "b.md"),
            "b.md must NOT appear in orphans: {orphan_files:?}"
        );
    }

    /// Same assertion via the glob code path (separate vault-wide scan).
    #[test]
    fn summary_orphan_detection_with_glob() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a.md"), "---\ntitle: A\n---\n[[b]]\n").unwrap();
        fs::write(tmp.path().join("b.md"), "---\ntitle: B\n---\nNo links.\n").unwrap();
        fs::write(
            tmp.path().join("orphan.md"),
            "---\ntitle: Orphan\n---\nNo links.\n",
        )
        .unwrap();

        // Passing "*.md" glob activates the separate LinkGraph::build code path.
        let val =
            unwrap_success(summary(tmp.path(), Some("*.md"), 10, None, Format::Json).unwrap());
        let orphan_files: Vec<&str> = val["orphans"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();

        assert!(
            orphan_files.iter().any(|f| f.contains("orphan")),
            "orphan.md must appear in orphans (glob path): {orphan_files:?}"
        );
        assert!(
            !orphan_files
                .iter()
                .any(|f| f.contains("/a") || *f == "a.md"),
            "a.md must NOT appear in orphans (glob path): {orphan_files:?}"
        );
        assert!(
            !orphan_files
                .iter()
                .any(|f| f.contains("/b") || *f == "b.md"),
            "b.md must NOT appear in orphans (glob path): {orphan_files:?}"
        );
    }

    #[test]
    fn summary_skips_broken_frontmatter_file() {
        let tmp = setup_vault();
        // Add a file with unclosed frontmatter
        fs::write(
            tmp.path().join("broken.md"),
            "---\ntitle: Broken\nNo closing delimiter.\n",
        )
        .unwrap();
        let val = unwrap_success(summary(tmp.path(), None, 10, None, Format::Json).unwrap());
        // Only the 3 good files should be counted
        assert_eq!(val["files"]["total"], 3);
    }
}
