#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::Path;

use hyalo_core::util::levenshtein;

use crate::commands::lint::lint_counts_from_properties;
use crate::output::{CommandOutcome, Format};
use hyalo_core::frontmatter::infer_type;
use hyalo_core::index::VaultIndex;
use hyalo_core::link_fix::detect_broken_links_from_index;
use hyalo_core::schema::SchemaConfig;
use hyalo_core::types::{
    DirectoryCount, FileCounts, LinkHealthSummary, LintSummary, PropertySummaryEntry, RecentFile,
    StatusGroup, TagSummary, TagSummaryEntry, TaskCount, VaultSummary,
};

// ---------------------------------------------------------------------------
// Rare-value inconsistency detection
// ---------------------------------------------------------------------------

/// Emit warnings for property values that appear in very few files and closely
/// resemble a much more common value (likely typos or inconsistencies).
///
/// A value is flagged when:
/// - It appears in at most `rare_threshold` files, AND
/// - There exists another value appearing in at least `dominant_min` files, AND
/// - The Levenshtein distance between the two values is at most `max_distance`.
fn warn_rare_values(
    prop_name: &str,
    value_counts: &BTreeMap<String, usize>,
    rare_threshold: usize,
    dominant_min: usize,
    max_distance: usize,
) {
    // Sort by count descending so the most-frequent value is first.
    // This makes it easy to find the dominant candidate and allows us to
    // short-circuit the rare-value search from the end.
    let mut sorted: Vec<(&str, usize)> =
        value_counts.iter().map(|(v, &c)| (v.as_str(), c)).collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));

    // Iterate in reverse (least-frequent first) to visit rare values.
    for (rare_val, rare_count) in sorted.iter().rev() {
        if *rare_count > rare_threshold {
            // Everything from here toward the front of the reversed iteration
            // is above the threshold — stop early.
            break;
        }
        // Find the most-frequent value (other than this one) that qualifies
        // as dominant. `sorted` is still in descending order, so `.find`
        // returns the highest-count match first.
        if let Some((dominant_val, dominant_count)) = sorted
            .iter()
            .find(|(v, c)| *c >= dominant_min && *v != *rare_val)
        {
            let dist = levenshtein(rare_val, dominant_val);
            if dist <= max_distance {
                let file_word = if *rare_count == 1 { "file" } else { "files" };
                crate::warn::warn(format!(
                    "property \"{prop_name}\" value \"{rare_val}\" appears in {rare_count} {file_word} — did you mean \"{dominant_val}\" ({dominant_count} files)?"
                ));
            }
        }
    }
}

/// Emit rare-value inconsistency warnings for all string-valued properties
/// collected during a summary scan.
///
/// `string_prop_values` maps `property_name -> (value -> file_count)`.
fn warn_inconsistent_properties(string_prop_values: &BTreeMap<String, BTreeMap<String, usize>>) {
    for (prop_name, value_counts) in string_prop_values {
        // Skip properties where no value reaches the dominant threshold —
        // warn_rare_values would find nothing to compare against anyway.
        let dominant_min = 3;
        let max_count = value_counts.values().copied().max().unwrap_or(0);
        if max_count < dominant_min {
            continue;
        }
        warn_rare_values(
            prop_name,
            value_counts,
            /* rare_threshold */ 1,
            /* dominant_min */ 3,
            /* max_distance */ 2,
        );
    }
}

/// Show a high-level vault summary using pre-scanned index data.
///
/// All aggregation (file counts, properties, tags, status groups, tasks, recent
/// files, orphans) is derived from `IndexEntry` values rather than scanning
/// files from disk.
///
/// `globs` optionally narrows which entries are included (same semantics as the
/// `--glob` flag on the `summary` command).
#[allow(clippy::too_many_arguments)]
pub fn summary(
    dir: &Path,
    index: &dyn VaultIndex,
    globs: &[String],
    recent: usize,
    depth: Option<usize>,
    site_prefix: Option<&str>,
    format: Format,
    schema: &SchemaConfig,
) -> Result<CommandOutcome> {
    use crate::commands::find::filter_index_entries;
    let scoped: Vec<_> = filter_index_entries(index.entries(), &[], globs)?;
    // Warn if globs matched 0 files and may redundantly include the --dir path
    crate::warn::warn_glob_dir_overlap(dir, globs, scoped.len());
    // Work with a slice of &IndexEntry references.
    let entries: Vec<_> = scoped;

    let mut total_files: usize = 0;
    let mut dir_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut property_counts: BTreeMap<(String, String), usize> = BTreeMap::new();
    // string_prop_values: for inconsistency detection — property_name -> (value -> count)
    // Only tracks text-typed string properties (not date/datetime/number/checkbox/list).
    let mut string_prop_values: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut tag_counts: BTreeMap<String, (String, usize)> = BTreeMap::new();
    let mut status_groups: BTreeMap<String, usize> = BTreeMap::new();
    let mut total_tasks: usize = 0;
    let mut done_tasks: usize = 0;
    let mut recent_entries: Vec<(String, String)> = Vec::new();

    for entry in &entries {
        total_files += 1;

        let dir_key = {
            let rel = std::path::Path::new(&entry.rel_path);
            match rel.parent() {
                Some(p) if !p.as_os_str().is_empty() => p.to_string_lossy().replace('\\', "/"),
                _ => ".".to_owned(),
            }
        };
        *dir_counts.entry(dir_key).or_insert(0) += 1;

        // Properties aggregation (skip "tags")
        for (name, value) in entry
            .properties
            .iter()
            .filter(|(n, _)| n.as_str() != "tags")
        {
            let prop_type = infer_type(value).to_owned();
            *property_counts
                .entry((name.clone(), prop_type.clone()))
                .or_insert(0) += 1;
            // Track string (text) values for rare-value inconsistency detection.
            // Cap at 50 distinct values per property (same as disk-scan path).
            if prop_type == "text"
                && let serde_json::Value::String(s) = value
            {
                let entry = string_prop_values.entry(name.clone()).or_default();
                if entry.len() < 50 || entry.contains_key(s.as_str()) {
                    *entry.entry(s.clone()).or_insert(0) += 1;
                }
            }
        }

        // Tags aggregation (case-insensitive, preserve first-seen casing)
        for tag in &entry.tags {
            let key = tag.to_ascii_lowercase();
            tag_counts
                .entry(key)
                .and_modify(|e| e.1 += 1)
                .or_insert_with(|| (tag.clone(), 1));
        }

        // Status grouping — flatten arrays so each element becomes its own group.
        // Deduplicate within a single entry to avoid counting the same file twice
        // when an array contains duplicate values.
        if let Some(status_val) = entry.properties.get("status") {
            let mut seen = std::collections::HashSet::new();
            let mut push_status = |s: String| {
                if seen.insert(s.clone()) {
                    *status_groups.entry(s).or_insert(0) += 1;
                }
            };
            match status_val {
                serde_json::Value::Array(arr) => {
                    for item in arr {
                        let s = match item {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        push_status(s);
                    }
                }
                serde_json::Value::String(s) => push_status(s.clone()),
                other => push_status(other.to_string()),
            }
        }

        // Task counts from pre-indexed tasks
        for task in &entry.tasks {
            total_tasks += 1;
            if task.done {
                done_tasks += 1;
            }
        }

        // Recent files: use the pre-indexed ISO 8601 modified timestamp.
        // Store as (modified_desc_sort_key, rel_path) for descending sort.
        // Since the timestamp is ISO 8601, lexicographic desc == chronological desc.
        recent_entries.push((entry.modified.clone(), entry.rel_path.clone()));
    }

    // Apply depth limit (default to 1 for compact output)
    let effective_depth = depth.unwrap_or(1);
    {
        let original: Vec<(String, usize)> = dir_counts.into_iter().collect();
        dir_counts = BTreeMap::new();
        for (dir_key, count) in original {
            let target = truncate_to_depth(&dir_key, effective_depth);
            *dir_counts.entry(target).or_insert(0) += count;
        }
    }

    // Build FileCounts — sort by count descending for compact output
    let mut directories: Vec<DirectoryCount> = dir_counts
        .into_iter()
        .map(|(directory, count)| DirectoryCount { directory, count })
        .collect();
    directories.sort_by(|a, b| b.count.cmp(&a.count).then(a.directory.cmp(&b.directory)));

    let file_counts = FileCounts {
        total: total_files,
        directories,
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

    // Build status groups (counts only)
    let mut status: Vec<StatusGroup> = status_groups
        .into_iter()
        .map(|(value, count)| StatusGroup { value, count })
        .collect();
    status.sort_by(|a, b| b.count.cmp(&a.count).then(a.value.cmp(&b.value)));

    let tasks = TaskCount {
        total: total_tasks,
        done: done_tasks,
    };

    // Build recent files (sort most-recent first by ISO 8601 timestamp desc, take top N)
    recent_entries.sort_by(|a, b| b.0.cmp(&a.0));
    let recent_files: Vec<RecentFile> = recent_entries
        .into_iter()
        .take(recent)
        .map(|(modified, path)| RecentFile { path, modified })
        .collect();

    // Count orphans and dead-ends using the pre-built link graph from the index.
    // The link graph is vault-wide so links from outside the scoped set still count.
    let (orphans, dead_ends) = {
        let graph = index.link_graph();
        let targets = graph.all_targets();
        let sources = graph.all_sources();

        let mut orphan_count: usize = 0;
        let mut dead_end_count: usize = 0;

        for entry in &entries {
            let rel_str: &str = &entry.rel_path;
            let without_md = rel_str.strip_suffix(".md").unwrap_or(rel_str);
            let has_inbound = targets.contains(rel_str) || targets.contains(without_md);
            let has_outbound = sources.contains(rel_str);
            if !has_inbound && !has_outbound {
                orphan_count += 1;
            } else if has_inbound && !has_outbound {
                dead_end_count += 1;
            }
        }

        (orphan_count, dead_end_count)
    };

    // Emit warnings for any property value that looks like a typo of a dominant value.
    warn_inconsistent_properties(&string_prop_values);

    // Link health is intentionally vault-wide: detect_broken_links_from_index
    // scans all entries in the index regardless of any --glob scope.  This is
    // consistent with the disk-scan path and ensures the report is meaningful
    // (scoped results would produce misleadingly low counts).
    let link_health = {
        let report = detect_broken_links_from_index(dir, index, site_prefix);
        LinkHealthSummary {
            total: report.total_links,
            broken: report.broken.len(),
        }
    };

    // Compute schema lint counts from index data (no disk re-read needed).
    let lint_summary: Option<LintSummary> = if schema.is_empty() {
        None
    } else {
        let lint_entries = entries.iter().map(|e| {
            let has_tags = e.properties.contains_key("tags") || !e.tags.is_empty();
            (e.rel_path.as_str(), &e.properties, has_tags)
        });
        let counts = lint_counts_from_properties(lint_entries, schema);
        Some(LintSummary {
            errors: counts.errors,
            warnings: counts.warnings,
            files_with_issues: counts.files_with_issues,
        })
    };

    let vault_summary = VaultSummary {
        files: file_counts,
        orphans,
        dead_ends,
        links: link_health,
        properties,
        tags,
        status,
        tasks,
        recent_files,
        schema: lint_summary,
    };

    let json_value = serde_json::to_value(&vault_summary).context("failed to serialize summary")?;
    Ok(CommandOutcome::success(crate::output::format_success(
        format,
        &json_value,
    )))
}

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
    use hyalo_core::index::{ScanOptions, ScannedIndex};
    use std::fs;

    /// Build a `ScannedIndex` from `dir` and call `summary`.
    /// Mirrors the old disk-scan helper signature used in pre-Phase-5 tests.
    fn run_summary(
        dir: &std::path::Path,
        globs: &[String],
        recent: usize,
        depth: Option<usize>,
        site_prefix: Option<&str>,
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
            site_prefix,
            &ScanOptions {
                scan_body: true,
                bm25_tokenize: false,
                default_language: None,
                frontmatter_link_props: None,
            },
        )?;
        let schema = hyalo_core::schema::SchemaConfig::default();
        summary(
            dir,
            &build.index,
            globs,
            recent,
            depth,
            site_prefix,
            format,
            &schema,
        )
    }

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
            CommandOutcome::Success { output: s, .. } | CommandOutcome::RawOutput(s) => {
                serde_json::from_str(&s).unwrap()
            }
            CommandOutcome::UserError(s) => panic!("expected success, got: {s}"),
        }
    }

    #[test]
    fn summary_file_counts() {
        let tmp = setup_vault();
        let val =
            unwrap_success(run_summary(tmp.path(), &[], 10, None, None, Format::Json).unwrap());
        assert_eq!(val["files"]["total"], 3);
    }

    #[test]
    fn summary_directory_counts() {
        let tmp = setup_vault();
        let val =
            unwrap_success(run_summary(tmp.path(), &[], 10, None, None, Format::Json).unwrap());
        let by_dir = val["files"]["directories"].as_array().unwrap();
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
        let val =
            unwrap_success(run_summary(tmp.path(), &[], 10, None, None, Format::Json).unwrap());
        // a.md: 2 tasks (1 done), b.md: 1 task (0 done), c.md: 0 tasks
        assert_eq!(val["tasks"]["total"], 3);
        assert_eq!(val["tasks"]["done"], 1);
    }

    #[test]
    fn summary_property_aggregation() {
        let tmp = setup_vault();
        let val =
            unwrap_success(run_summary(tmp.path(), &[], 10, None, None, Format::Json).unwrap());
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
        let val =
            unwrap_success(run_summary(tmp.path(), &[], 10, None, None, Format::Json).unwrap());
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
        let val =
            unwrap_success(run_summary(tmp.path(), &[], 10, None, None, Format::Json).unwrap());
        let status_groups = val["status"].as_array().unwrap();
        let draft = status_groups
            .iter()
            .find(|g| g["value"] == "draft")
            .unwrap();
        // a.md and notes/c.md have status=draft
        assert_eq!(draft["count"], 2);
        let done = status_groups.iter().find(|g| g["value"] == "done").unwrap();
        assert_eq!(done["count"], 1);
    }

    #[test]
    fn summary_status_grouping_flattens_arrays() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("a.md"),
            md!(r"
---
title: A
status:
  - deprecated
  - non-standard
---
Body.
"),
        )
        .unwrap();
        fs::write(
            tmp.path().join("b.md"),
            md!(r"
---
title: B
status: deprecated
---
Body.
"),
        )
        .unwrap();
        let val =
            unwrap_success(run_summary(tmp.path(), &[], 10, None, None, Format::Json).unwrap());
        let status_groups = val["status"].as_array().unwrap();

        // "deprecated" should appear as its own group with 2 files (a.md array + b.md scalar).
        let deprecated = status_groups
            .iter()
            .find(|g| g["value"] == "deprecated")
            .expect("expected 'deprecated' status group");
        assert_eq!(deprecated["count"], 2);

        // "non-standard" should appear as its own group with 1 file.
        let non_standard = status_groups
            .iter()
            .find(|g| g["value"] == "non-standard")
            .expect("expected 'non-standard' status group");
        assert_eq!(non_standard["count"], 1);

        // No stringified array group should exist.
        assert!(
            !status_groups
                .iter()
                .any(|g| g["value"].as_str().unwrap_or("").starts_with('[')),
            "should not have a stringified array as a status group"
        );
    }

    #[test]
    fn summary_recent_files_respects_limit() {
        let tmp = setup_vault();
        let val =
            unwrap_success(run_summary(tmp.path(), &[], 2, None, None, Format::Json).unwrap());
        let recent = val["recent_files"].as_array().unwrap();
        // With limit=2, at most 2 recent files
        assert!(recent.len() <= 2);
    }

    #[test]
    fn summary_recent_files_have_iso8601_timestamps() {
        let tmp = setup_vault();
        let val =
            unwrap_success(run_summary(tmp.path(), &[], 10, None, None, Format::Json).unwrap());
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
        let val = unwrap_success(
            run_summary(
                tmp.path(),
                &["*.md".to_owned()],
                10,
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        assert_eq!(val["files"]["total"], 2);
    }

    #[test]
    fn summary_text_format() {
        let tmp = setup_vault();
        let outcome = run_summary(tmp.path(), &[], 10, None, None, Format::Text).unwrap();
        match outcome {
            CommandOutcome::Success { output: s, .. } | CommandOutcome::RawOutput(s) => {
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
        let val =
            unwrap_success(run_summary(tmp.path(), &[], 10, Some(0), None, Format::Json).unwrap());
        let by_dir = val["files"]["directories"].as_array().unwrap();
        assert_eq!(by_dir.len(), 1);
        assert_eq!(by_dir[0]["directory"], ".");
        assert_eq!(by_dir[0]["count"], 3);
    }

    #[test]
    fn summary_depth_one_shows_top_level() {
        let tmp = setup_vault_nested();
        let val =
            unwrap_success(run_summary(tmp.path(), &[], 10, Some(1), None, Format::Json).unwrap());
        let by_dir = val["files"]["directories"].as_array().unwrap();
        // "." (1 file) and "notes" (2 files collapsed from notes/ and notes/sub/)
        assert_eq!(by_dir.len(), 2);
        let dot = by_dir.iter().find(|d| d["directory"] == ".").unwrap();
        assert_eq!(dot["count"], 1);
        let notes = by_dir.iter().find(|d| d["directory"] == "notes").unwrap();
        assert_eq!(notes["count"], 2);
    }

    #[test]
    fn summary_depth_none_defaults_to_depth_one() {
        let tmp = setup_vault_nested();
        let val =
            unwrap_success(run_summary(tmp.path(), &[], 10, None, None, Format::Json).unwrap());
        let by_dir = val["files"]["directories"].as_array().unwrap();
        // Default depth is 1, so "notes/sub" is collapsed into "notes"
        assert_eq!(by_dir.len(), 2);
        assert!(by_dir.iter().any(|d| d["directory"] == "."));
        let notes = by_dir.iter().find(|d| d["directory"] == "notes").unwrap();
        assert_eq!(notes["count"], 2);
    }

    #[test]
    fn summary_depth_stats_unaffected_by_depth() {
        // Stats (tasks, tags, properties) must be computed from all files regardless of depth
        let tmp = setup_vault_nested();
        let val_no_depth =
            unwrap_success(run_summary(tmp.path(), &[], 10, None, None, Format::Json).unwrap());
        let val_depth0 =
            unwrap_success(run_summary(tmp.path(), &[], 10, Some(0), None, Format::Json).unwrap());
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
        let val =
            unwrap_success(run_summary(tmp.path(), &[], 10, None, None, Format::Json).unwrap());
        // orphan.md has no inbound and no outbound links → 1 orphan
        assert_eq!(val["orphans"], 1, "expected exactly 1 orphan");
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
        let val = unwrap_success(
            run_summary(
                tmp.path(),
                &["*.md".to_owned()],
                10,
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        // orphan.md is the only orphan
        assert_eq!(val["orphans"], 1, "expected exactly 1 orphan (glob path)");
    }

    /// Disk-scan and snapshot-index must produce identical orphan lists.
    /// Uses absolute links with `site_prefix` to exercise the resolution path
    /// that was previously inconsistent (disk scan hardcoded `None`).
    #[test]
    fn summary_orphan_parity_disk_vs_index() {
        use hyalo_core::index::{ScanOptions, ScannedIndex, SnapshotIndex};

        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        // a.md links to b via wikilink
        fs::write(dir.join("a.md"), "---\ntitle: A\n---\n[[b]]\n").unwrap();
        // b.md links to c via absolute markdown link (needs site_prefix)
        fs::write(
            dir.join("b.md"),
            "---\ntitle: B\n---\n[see c](/docs/c.md)\n",
        )
        .unwrap();
        // c.md has an inbound link from b (only with correct site_prefix)
        fs::write(dir.join("c.md"), "---\ntitle: C\n---\nNo links.\n").unwrap();
        // orphan.md has no links in or out
        fs::write(
            dir.join("orphan.md"),
            "---\ntitle: Orphan\n---\nNo links.\n",
        )
        .unwrap();

        let prefix = Some("docs");

        // Disk-scan path
        let disk_val =
            unwrap_success(run_summary(dir, &[], 10, None, prefix, Format::Json).unwrap());
        let disk_orphans = disk_val["orphans"].as_u64().unwrap();

        // Index path: build a SnapshotIndex and query via summary
        let all = hyalo_core::discovery::discover_files(dir).unwrap();
        let files: Vec<(std::path::PathBuf, String)> = all
            .into_iter()
            .map(|p| {
                let rel = hyalo_core::discovery::relative_path(dir, &p);
                (p, rel)
            })
            .collect();
        let build = ScannedIndex::build(
            &files,
            prefix,
            &ScanOptions {
                scan_body: true,
                bm25_tokenize: false,
                default_language: None,
                frontmatter_link_props: None,
            },
        )
        .unwrap();
        let index_path = dir.join(".hyalo-index");
        SnapshotIndex::save(
            &build.index,
            &index_path,
            &dir.display().to_string(),
            prefix,
            None,
        )
        .unwrap();
        let loaded = SnapshotIndex::load(&index_path).unwrap().unwrap();
        let schema = hyalo_core::schema::SchemaConfig::default();
        let index_val = unwrap_success(
            summary(dir, &loaded, &[], 10, None, prefix, Format::Json, &schema).unwrap(),
        );
        let index_orphans = index_val["orphans"].as_u64().unwrap();

        assert_eq!(
            disk_orphans, index_orphans,
            "disk scan and index must produce identical orphan counts"
        );
        // orphan.md should be the only orphan
        assert_eq!(disk_orphans, 1);
    }

    /// Dead-end detection:
    /// - a.md links to b.md and c.md (has outbound, no inbound → not a dead-end, not an orphan)
    /// - b.md links to c.md (has inbound from a, has outbound to c → not a dead-end)
    /// - c.md has no links (has inbound from a and b, no outbound → dead-end)
    /// - d.md has no links and nothing links to it → orphan
    #[test]
    fn summary_dead_end_detection() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("a.md"),
            "---\ntitle: A\n---\n[[b]]\n[[c]]\n",
        )
        .unwrap();
        fs::write(tmp.path().join("b.md"), "---\ntitle: B\n---\n[[c]]\n").unwrap();
        fs::write(tmp.path().join("c.md"), "---\ntitle: C\n---\nNo links.\n").unwrap();
        fs::write(tmp.path().join("d.md"), "---\ntitle: D\n---\nNo links.\n").unwrap();

        let val =
            unwrap_success(run_summary(tmp.path(), &[], 10, None, None, Format::Json).unwrap());

        // c.md is the only dead-end
        assert_eq!(val["dead_ends"], 1, "expected 1 dead-end");
        // d.md is the only orphan
        assert_eq!(val["orphans"], 1, "expected 1 orphan");
    }

    /// Dead-end parity: disk scan and index path must agree on dead-end lists.
    #[test]
    fn summary_dead_end_parity_disk_vs_index() {
        use hyalo_core::index::{ScanOptions, ScannedIndex, SnapshotIndex};

        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        // a.md links to b and c
        fs::write(dir.join("a.md"), "---\ntitle: A\n---\n[[b]]\n[[c]]\n").unwrap();
        // b.md links to c
        fs::write(dir.join("b.md"), "---\ntitle: B\n---\n[[c]]\n").unwrap();
        // c.md is a dead-end (inbound from a and b, no outbound)
        fs::write(dir.join("c.md"), "---\ntitle: C\n---\nNo links.\n").unwrap();
        // d.md is an orphan (no links in or out)
        fs::write(dir.join("d.md"), "---\ntitle: D\n---\nNo links.\n").unwrap();

        // Disk-scan path
        let disk_val = unwrap_success(run_summary(dir, &[], 10, None, None, Format::Json).unwrap());
        let disk_dead_ends = disk_val["dead_ends"].as_u64().unwrap();

        // Index path
        let all = hyalo_core::discovery::discover_files(dir).unwrap();
        let files: Vec<(std::path::PathBuf, String)> = all
            .into_iter()
            .map(|p| {
                let rel = hyalo_core::discovery::relative_path(dir, &p);
                (p, rel)
            })
            .collect();
        let build = ScannedIndex::build(
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
        let index_path = dir.join(".hyalo-index");
        SnapshotIndex::save(
            &build.index,
            &index_path,
            &dir.display().to_string(),
            None,
            None,
        )
        .unwrap();
        let loaded = SnapshotIndex::load(&index_path).unwrap().unwrap();
        let schema = hyalo_core::schema::SchemaConfig::default();
        let index_val = unwrap_success(
            summary(dir, &loaded, &[], 10, None, None, Format::Json, &schema).unwrap(),
        );
        let index_dead_ends = index_val["dead_ends"].as_u64().unwrap();

        assert_eq!(
            disk_dead_ends, index_dead_ends,
            "disk scan and index must produce identical dead-end counts"
        );
        assert_eq!(disk_dead_ends, 1);
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
        let val =
            unwrap_success(run_summary(tmp.path(), &[], 10, None, None, Format::Json).unwrap());
        // Only the 3 good files should be counted
        assert_eq!(val["files"]["total"], 3);
    }

    // -----------------------------------------------------------------------
    // levenshtein tests
    // -----------------------------------------------------------------------

    #[test]
    fn levenshtein_equal_strings() {
        assert_eq!(levenshtein("completed", "completed"), 0);
    }

    #[test]
    fn levenshtein_empty_strings() {
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("", "abc"), 3);
    }

    #[test]
    fn levenshtein_single_substitution() {
        // "done" vs "dune" — one substitution
        assert_eq!(levenshtein("done", "dune"), 1);
    }

    #[test]
    fn levenshtein_typical_typos() {
        // "done" vs "completed" — clearly different
        assert_eq!(levenshtein("done", "completed"), 7);
        // "planed" vs "planned" — one insertion
        assert_eq!(levenshtein("planed", "planned"), 1);
        // "in-progres" vs "in-progress" — one insertion
        assert_eq!(levenshtein("in-progres", "in-progress"), 1);
    }

    #[test]
    fn levenshtein_commutative() {
        let a = "kitten";
        let b = "sitting";
        assert_eq!(levenshtein(a, b), levenshtein(b, a));
    }

    // -----------------------------------------------------------------------
    // warn_rare_values tests
    // -----------------------------------------------------------------------

    // Helper: build a BTreeMap<String, usize> from key/value pairs.
    fn counts(pairs: &[(&str, usize)]) -> BTreeMap<String, usize> {
        pairs.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect()
    }

    #[test]
    fn warn_rare_values_emits_for_typo() {
        let _guard = crate::warn::WARN_TEST_LOCK.lock().unwrap();
        crate::warn::reset_for_test();
        crate::warn::init(false);

        // "complted" (missing 'e') vs "completed" — Levenshtein 1, should warn.
        let vc = counts(&[("complted", 1), ("completed", 10)]);
        warn_rare_values("status", &vc, 1, 3, 2);

        let msg = r#"property "status" value "complted" appears in 1 file — did you mean "completed" (10 files)?"#;
        assert!(
            crate::warn::was_emitted(msg),
            "expected warning to be emitted for near-duplicate value"
        );
    }

    #[test]
    fn warn_rare_values_no_warning_for_distant_values() {
        let _guard = crate::warn::WARN_TEST_LOCK.lock().unwrap();
        crate::warn::reset_for_test();
        crate::warn::init(false);

        // "done" vs "completed" — Levenshtein 7 > max_distance 2 → no warning
        let vc = counts(&[("done", 1), ("completed", 10)]);
        warn_rare_values("status", &vc, 1, 3, 2);

        let msg = r#"property "status" value "done" appears in 1 file — did you mean "completed" (10 files)?"#;
        assert!(
            !crate::warn::was_emitted(msg),
            "expected no warning for clearly distinct values"
        );
    }

    #[test]
    fn warn_rare_values_no_warning_when_dominant_too_rare() {
        let _guard = crate::warn::WARN_TEST_LOCK.lock().unwrap();
        crate::warn::reset_for_test();
        crate::warn::init(false);

        // Both values appear rarely — dominant_min=3, "completed" only appears 2x
        let vc = counts(&[("complted", 1), ("completed", 2)]);
        warn_rare_values("status", &vc, 1, 3, 2);

        // Neither candidate message should have been emitted.
        let msg = r#"property "status" value "complted" appears in 1 file — did you mean "completed" (2 files)?"#;
        assert!(
            !crate::warn::was_emitted(msg),
            "expected no warning when dominant is below dominant_min"
        );
    }

    #[test]
    fn warn_rare_values_no_warning_when_count_above_threshold() {
        let _guard = crate::warn::WARN_TEST_LOCK.lock().unwrap();
        crate::warn::reset_for_test();
        crate::warn::init(false);

        // rare_count=2 > rare_threshold=1 — "complted" is not rare enough to warn
        let vc = counts(&[("complted", 2), ("completed", 10)]);
        warn_rare_values("status", &vc, 1, 3, 2);

        let msg = r#"property "status" value "complted" appears in 2 files — did you mean "completed" (10 files)?"#;
        assert!(
            !crate::warn::was_emitted(msg),
            "expected no warning when rare_count exceeds rare_threshold"
        );
    }

    #[test]
    fn warn_inconsistent_properties_skips_all_unique() {
        let _guard = crate::warn::WARN_TEST_LOCK.lock().unwrap();
        crate::warn::reset_for_test();
        crate::warn::init(false);

        // Every value appears exactly once — max_count < dominant_min → skip entirely
        let mut map: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
        map.insert("status".to_owned(), counts(&[("a", 1), ("b", 1), ("c", 1)]));
        warn_inconsistent_properties(&map);

        // No warnings should have been emitted — all values are unique so no
        // dominant value exists (count >= dominant_min of 3).
        // Check full warning format (was_emitted uses exact key match).
        for val in ["a", "b", "c"] {
            for other in ["a", "b", "c"] {
                if val == other {
                    continue;
                }
                let msg = format!(
                    r#"property "status" value "{val}" appears in 1 file — did you mean "{other}" (1 files)?"#
                );
                assert!(
                    !crate::warn::was_emitted(&msg),
                    "expected no warning when all values are unique"
                );
            }
        }
    }
}
