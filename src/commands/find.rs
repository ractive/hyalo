#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::SystemTime;

use crate::commands::outline::SectionScanner;
use crate::commands::{FilesOrOutcome, collect_files};
use crate::content_search::ContentSearchVisitor;
use crate::discovery;
use crate::filter::{self, Fields, FindTaskFilter, PropertyFilter, SortField, extract_tags};
use crate::frontmatter;
use crate::heading::{SectionFilter, SectionRange, build_section_scope, in_scope};
use crate::links::Link;
use crate::output::{CommandOutcome, Format};
use crate::scanner::{self, FileVisitor, FrontmatterCollector, ScanAction};
use crate::tasks::TaskExtractor;
use crate::types::{
    ContentMatch, FileObject, FindTaskInfo, LinkInfo, OutlineSection, PropertyInfo,
};

// ---------------------------------------------------------------------------
// Public command entry point
// ---------------------------------------------------------------------------

/// Find files matching the given filters and return them as a JSON array.
///
/// - `pattern`            — case-insensitive content search substring
/// - `regexp`             — regex content search (mutually exclusive with `pattern`)
/// - `property_filters`  — all must match (AND semantics)
/// - `tag_filters`        — all must be present (AND semantics, nested tag rules)
/// - `task_filter`        — file-level task presence/status filter
/// - `section_filters`   — restrict body results to matching sections (OR semantics)
/// - `file` / `glob`      — scope limiting
/// - `fields`             — controls which fields appear in each `FileObject`
/// - `sort`               — sort order; defaults to ascending by file path
/// - `limit`              — maximum number of results
#[allow(clippy::too_many_arguments)]
pub fn find(
    dir: &Path,
    pattern: Option<&str>,
    regexp: Option<&str>,
    property_filters: &[PropertyFilter],
    tag_filters: &[String],
    task_filter: Option<&FindTaskFilter>,
    section_filters: &[SectionFilter],
    file: Option<&str>,
    glob: Option<&str>,
    fields: &Fields,
    sort: Option<&SortField>,
    limit: Option<usize>,
    format: Format,
) -> Result<CommandOutcome> {
    let files = collect_files(dir, file, glob, format)?;
    let files = match files {
        FilesOrOutcome::Files(f) => f,
        FilesOrOutcome::Outcome(o) => return Ok(o),
    };

    let has_content_search = pattern.is_some() || regexp.is_some();
    let has_task_filter = task_filter.is_some();
    let has_section_filter = !section_filters.is_empty();
    let body_needed = needs_body(
        fields,
        has_content_search,
        has_task_filter,
        has_section_filter,
    );

    // Compile the regex once (if any), then clone cheaply per file.
    // Invalid regex is a user error (exit 1), not an internal error (exit 2).
    let compiled_regex = match regexp {
        Some(re) => {
            let effective = format!("(?i){re}");
            match regex::Regex::new(&effective) {
                Ok(r) => Some(r),
                Err(e) => {
                    return Ok(CommandOutcome::UserError(format!(
                        "invalid regular expression: {re}\n{e}"
                    )));
                }
            }
        }
        None => None,
    };

    let mut results: Vec<FileObject> = Vec::new();

    for (full_path, rel_path) in &files {
        // --- Single-pass scan ---
        let mut fm = FrontmatterCollector::new(body_needed);
        let mut section_scanner =
            if body_needed && (fields.sections || fields.links || has_section_filter) {
                Some(SectionScanner::new())
            } else {
                None
            };
        let mut task_extractor = if body_needed && (fields.tasks || has_task_filter) {
            Some(TaskExtractor::new())
        } else {
            None
        };
        let mut link_collector = if body_needed && fields.links {
            Some(LinkCollector::new())
        } else {
            None
        };
        let mut content_visitor = if let Some(ref re) = compiled_regex {
            Some(ContentSearchVisitor::from_compiled(re.clone()))
        } else {
            pattern.map(ContentSearchVisitor::new)
        };

        // Build visitor slice dynamically — only include Some visitors.
        {
            let mut visitor_refs: Vec<&mut dyn FileVisitor> = Vec::new();
            visitor_refs.push(&mut fm);
            if let Some(ref mut ss) = section_scanner {
                visitor_refs.push(ss);
            }
            if let Some(ref mut te) = task_extractor {
                visitor_refs.push(te);
            }
            if let Some(ref mut lc) = link_collector {
                visitor_refs.push(lc);
            }
            if let Some(ref mut cv) = content_visitor {
                visitor_refs.push(cv);
            }
            scanner::scan_file_multi(full_path, &mut visitor_refs)?;
        }

        let props = fm.into_props();

        // --- Extract tags once and apply filters ---
        let tags = extract_tags(&props);
        if !filter::matches_filters_with_tags(&props, property_filters, &tags, tag_filters) {
            continue;
        }

        // --- Consume section scanner to get outline sections ---
        // Must happen before scope building and before build_file_object.
        let outline_sections: Option<Vec<OutlineSection>> =
            section_scanner.map(SectionScanner::into_sections);

        // --- Build section scope ranges (if section filter is active) ---
        let scope_ranges: Vec<SectionRange> = if has_section_filter {
            if let Some(ref sections) = outline_sections {
                build_section_scope(sections, section_filters, usize::MAX)
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // --- Collect tasks (needed for filter and/or output field) ---
        // Consume the extractor now so we can both filter and include in output.
        let mut collected_tasks: Option<Vec<FindTaskInfo>> =
            task_extractor.map(TaskExtractor::into_tasks);

        // --- Collect content matches early so we can apply section scope ---
        let mut content_matches: Option<Vec<ContentMatch>> =
            content_visitor.map(|cv| cv.into_matches());

        // --- Apply section scope filter to body results ---
        if has_section_filter {
            if scope_ranges.is_empty() {
                // No section matched in this file — skip it entirely
                continue;
            }
            if let Some(ref mut tasks) = collected_tasks {
                tasks.retain(|t| in_scope(&scope_ranges, t.line));
            }
            if let Some(ref mut matches) = content_matches {
                matches.retain(|m| in_scope(&scope_ranges, m.line));
            }
        }

        // --- Apply task filter ---
        if let Some(filter) = task_filter {
            let tasks_slice: &[FindTaskInfo] = collected_tasks.as_deref().unwrap_or(&[]);
            if !matches_task_filter(tasks_slice, filter) {
                continue;
            }
        }

        // --- Apply content search filter ---
        if has_content_search && content_matches.as_ref().is_some_and(|m| m.is_empty()) {
            continue;
        }

        // --- Get modified time ---
        let modified = format_modified(full_path)?;

        // --- Build FileObject ---
        let obj = build_file_object(
            rel_path,
            &modified,
            &props,
            &tags,
            fields,
            outline_sections,
            &scope_ranges,
            collected_tasks,
            link_collector,
            content_matches,
            dir,
        );

        results.push(obj);
    }

    // --- Sort ---
    match sort.unwrap_or(&SortField::File) {
        SortField::File => results.sort_by(|a, b| a.file.cmp(&b.file)),
        SortField::Modified => results.sort_by(|a, b| a.modified.cmp(&b.modified)),
    }

    // --- Limit ---
    if let Some(n) = limit {
        results.truncate(n);
    }

    // --- Serialize ---
    let json_array: Vec<serde_json::Value> = results
        .into_iter()
        .map(|obj| serde_json::to_value(obj).expect("derived Serialize impl should not fail"))
        .collect();
    let json_output = serde_json::Value::Array(json_array);

    Ok(CommandOutcome::Success(crate::output::format_success(
        format,
        &json_output,
    )))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Determine whether body scanning is needed at all.
fn needs_body(
    fields: &Fields,
    has_content_search: bool,
    has_task_filter: bool,
    has_section_filter: bool,
) -> bool {
    fields.sections
        || fields.tasks
        || fields.links
        || has_content_search
        || has_task_filter
        || has_section_filter
}

/// Return true if `tasks` satisfy `filter`.
fn matches_task_filter(tasks: &[FindTaskInfo], filter: &FindTaskFilter) -> bool {
    match filter {
        FindTaskFilter::Any => !tasks.is_empty(),
        FindTaskFilter::Todo => tasks.iter().any(|t| !t.done),
        FindTaskFilter::Done => tasks.iter().any(|t| t.done),
        FindTaskFilter::Status(c) => tasks.iter().any(|t| t.status.starts_with(*c)),
    }
}

/// Format a file's last-modified time as ISO 8601 UTC (`YYYY-MM-DDTHH:MM:SSZ`).
fn format_modified(path: &Path) -> Result<String> {
    let meta = std::fs::metadata(path)
        .with_context(|| format!("failed to read metadata for {}", path.display()))?;
    let mtime = meta
        .modified()
        .with_context(|| format!("mtime not available for {}", path.display()))?;
    let secs = mtime
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Ok(format_iso8601(secs))
}

use super::format_iso8601;

/// Build a `FileObject` from the already-scanned data.
#[allow(clippy::too_many_arguments)]
fn build_file_object(
    rel_path: &str,
    modified: &str,
    props: &BTreeMap<String, serde_yaml_ng::Value>,
    tags: &[String],
    fields: &Fields,
    outline_sections: Option<Vec<OutlineSection>>,
    scope_ranges: &[SectionRange],
    collected_tasks: Option<Vec<FindTaskInfo>>,
    link_collector: Option<LinkCollector>,
    content_matches: Option<Vec<ContentMatch>>,
    dir: &Path,
) -> FileObject {
    let properties = if fields.properties {
        Some(
            props
                .iter()
                .filter(|(name, _)| name.as_str() != "tags")
                .map(|(name, value)| PropertyInfo {
                    name: name.clone(),
                    prop_type: frontmatter::infer_type(value).to_owned(),
                    value: frontmatter::yaml_to_json(value),
                })
                .collect(),
        )
    } else {
        None
    };

    let tags_field = if fields.tags {
        Some(tags.to_vec())
    } else {
        None
    };

    let sections = if fields.sections {
        outline_sections.map(|mut secs| {
            if !scope_ranges.is_empty() {
                secs.retain(|s| in_scope(scope_ranges, s.line));
            }
            secs
        })
    } else {
        None
    };

    let tasks = if fields.tasks { collected_tasks } else { None };

    let links = if fields.links {
        link_collector.map(|lc| {
            lc.into_links()
                .into_iter()
                .map(|link| {
                    let path = discovery::resolve_target(dir, &link.target);
                    LinkInfo {
                        target: link.target,
                        path,
                        label: link.label,
                    }
                })
                .collect()
        })
    } else {
        None
    };

    let matches: Option<Vec<ContentMatch>> = content_matches;

    FileObject {
        file: rel_path.to_owned(),
        modified: modified.to_owned(),
        properties,
        tags: tags_field,
        sections,
        tasks,
        links,
        matches,
    }
}

// ---------------------------------------------------------------------------
// LinkCollector visitor
// ---------------------------------------------------------------------------

/// Visitor that collects all `Link` structs from body lines for later resolution.
struct LinkCollector {
    links: Vec<Link>,
}

impl LinkCollector {
    fn new() -> Self {
        Self { links: Vec::new() }
    }

    fn into_links(self) -> Vec<Link> {
        self.links
    }
}

impl FileVisitor for LinkCollector {
    fn on_body_line(&mut self, raw: &str, _line_num: usize) -> ScanAction {
        let cleaned = scanner::strip_inline_code(raw);
        crate::links::extract_links_from_text(cleaned.as_ref(), &mut self.links);
        ScanAction::Continue
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

    fn unwrap_success(outcome: CommandOutcome) -> String {
        match outcome {
            CommandOutcome::Success(s) => s,
            CommandOutcome::UserError(s) => panic!("expected success, got user error: {s}"),
        }
    }

    fn setup_vault() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();

        fs::write(
            tmp.path().join("alpha.md"),
            md!(r"
---
title: Alpha Note
status: planned
tags:
  - rust
  - cli
---
# Introduction

See [[beta]] for context.

## Tasks

- [ ] Write tests
- [x] Write code
"),
        )
        .unwrap();

        fs::write(
            tmp.path().join("beta.md"),
            md!(r"
---
title: Beta Note
status: completed
tags:
  - rust
---
# Beta Content

Some content about Rust programming.
"),
        )
        .unwrap();

        fs::write(
            tmp.path().join("gamma.md"),
            md!(r"
# Gamma (no frontmatter)

Just some text here.
"),
        )
        .unwrap();

        tmp
    }

    // --- find: basic ---

    #[test]
    fn find_all_files_returns_array() {
        let tmp = setup_vault();
        let fields = Fields::default();
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                &[],
                &[],
                None,
                &[],
                None,
                None,
                &fields,
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 3);
    }

    #[test]
    fn find_returns_sorted_by_file_by_default() {
        let tmp = setup_vault();
        let fields = Fields::default();
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                &[],
                &[],
                None,
                &[],
                None,
                None,
                &fields,
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed.as_array().unwrap();
        let files: Vec<&str> = arr.iter().map(|v| v["file"].as_str().unwrap()).collect();
        let mut sorted = files.clone();
        sorted.sort();
        assert_eq!(files, sorted);
    }

    #[test]
    fn find_always_includes_file_and_modified() {
        let tmp = setup_vault();
        let fields = Fields::default();
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                &[],
                &[],
                None,
                &[],
                None,
                None,
                &fields,
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed.as_array().unwrap();
        for entry in arr {
            assert!(entry["file"].as_str().is_some());
            let modified = entry["modified"].as_str().unwrap();
            // ISO 8601 format: YYYY-MM-DDTHH:MM:SSZ (20 chars)
            assert_eq!(modified.len(), 20, "unexpected modified: {modified}");
            assert!(modified.ends_with('Z'));
        }
    }

    // --- find: property filter ---

    #[test]
    fn find_property_filter_eq() {
        let tmp = setup_vault();
        let filter = crate::filter::parse_property_filter("status=planned").unwrap();
        let fields = Fields::default();
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                &[filter],
                &[],
                None,
                &[],
                None,
                None,
                &fields,
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert!(arr[0]["file"].as_str().unwrap().contains("alpha"));
    }

    #[test]
    fn find_property_filter_exists() {
        let tmp = setup_vault();
        let filter = crate::filter::parse_property_filter("title").unwrap();
        let fields = Fields::default();
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                &[filter],
                &[],
                None,
                &[],
                None,
                None,
                &fields,
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed.as_array().unwrap();
        // alpha and beta have title; gamma does not
        assert_eq!(arr.len(), 2);
    }

    // --- find: tag filter ---

    #[test]
    fn find_tag_filter_matches() {
        let tmp = setup_vault();
        let fields = Fields::default();
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                &[],
                &["cli".to_owned()],
                None,
                &[],
                None,
                None,
                &fields,
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert!(arr[0]["file"].as_str().unwrap().contains("alpha"));
    }

    #[test]
    fn find_tag_filter_no_match() {
        let tmp = setup_vault();
        let fields = Fields::default();
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                &[],
                &["nonexistent-tag".to_owned()],
                None,
                &[],
                None,
                None,
                &fields,
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed.as_array().unwrap();
        assert!(arr.is_empty());
    }

    // --- find: content search ---

    #[test]
    fn find_pattern_matches_content() {
        let tmp = setup_vault();
        let fields = Fields::default();
        let out = unwrap_success(
            find(
                tmp.path(),
                Some("Rust programming"),
                None,
                &[],
                &[],
                None,
                &[],
                None,
                None,
                &fields,
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert!(arr[0]["file"].as_str().unwrap().contains("beta"));
    }

    #[test]
    fn find_pattern_includes_matches_field() {
        let tmp = setup_vault();
        let fields = Fields::default();
        let out = unwrap_success(
            find(
                tmp.path(),
                Some("Rust"),
                None,
                &[],
                &[],
                None,
                &[],
                None,
                None,
                &fields,
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed.as_array().unwrap();
        // beta has "Rust programming" in body; alpha has no Rust in body
        // (tags are in frontmatter which is not scanned for content)
        for entry in arr {
            assert!(entry["matches"].is_array(), "matches field missing");
        }
    }

    #[test]
    fn find_no_pattern_no_matches_field() {
        let tmp = setup_vault();
        let fields = Fields::default();
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                &[],
                &[],
                None,
                &[],
                None,
                None,
                &fields,
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed.as_array().unwrap();
        for entry in arr {
            assert!(entry["matches"].is_null(), "matches should be absent");
        }
    }

    // --- find: task filter ---

    #[test]
    fn find_task_filter_todo() {
        let tmp = setup_vault();
        let fields = Fields::default();
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                &[],
                &[],
                Some(&FindTaskFilter::Todo),
                &[],
                None,
                None,
                &fields,
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed.as_array().unwrap();
        // only alpha.md has an open task
        assert_eq!(arr.len(), 1);
        assert!(arr[0]["file"].as_str().unwrap().contains("alpha"));
    }

    #[test]
    fn find_task_filter_any() {
        let tmp = setup_vault();
        let fields = Fields::default();
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                &[],
                &[],
                Some(&FindTaskFilter::Any),
                &[],
                None,
                None,
                &fields,
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert!(arr[0]["file"].as_str().unwrap().contains("alpha"));
    }

    // --- find: fields selection ---

    #[test]
    fn find_fields_subset_properties_only() {
        let tmp = setup_vault();
        let fields = Fields::parse(&["properties".to_owned()]).unwrap();
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                &[],
                &[],
                None,
                &[],
                None,
                None,
                &fields,
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed.as_array().unwrap();
        for entry in arr {
            assert!(entry["properties"].is_array());
            assert!(entry["tags"].is_null());
            assert!(entry["sections"].is_null());
            assert!(entry["tasks"].is_null());
            assert!(entry["links"].is_null());
        }
    }

    #[test]
    fn find_fields_tasks_included() {
        let tmp = setup_vault();
        let fields = Fields::parse(&["tasks".to_owned()]).unwrap();
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                &[],
                &[],
                None,
                &[],
                None,
                None,
                &fields,
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed.as_array().unwrap();
        let alpha = arr
            .iter()
            .find(|e| e["file"].as_str().unwrap().contains("alpha"))
            .unwrap();
        let tasks = alpha["tasks"].as_array().unwrap();
        assert_eq!(tasks.len(), 2);
        let open = tasks.iter().find(|t| t["status"] == " ").unwrap();
        assert!(!open["done"].as_bool().unwrap());
        let done = tasks.iter().find(|t| t["done"].as_bool().unwrap()).unwrap();
        assert!(done["done"].as_bool().unwrap());
    }

    #[test]
    fn find_fields_links_resolved() {
        let tmp = setup_vault();
        // Create beta.md so the wikilink [[beta]] from alpha can resolve
        let fields = Fields::parse(&["links".to_owned()]).unwrap();
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                &[],
                &[],
                None,
                &[],
                Some("alpha.md"),
                None,
                &fields,
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed.as_array().unwrap();
        let alpha = &arr[0];
        let links = alpha["links"].as_array().unwrap();
        assert!(!links.is_empty());
        let beta_link = links.iter().find(|l| l["target"] == "beta").unwrap();
        // beta.md exists in vault, so path should be Some("beta.md")
        assert_eq!(beta_link["path"], "beta.md");
    }

    // --- find: limit ---

    #[test]
    fn find_limit_truncates_results() {
        let tmp = setup_vault();
        let fields = Fields::default();
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                &[],
                &[],
                None,
                &[],
                None,
                None,
                &fields,
                None,
                Some(1),
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 1);
    }

    // --- find: sort ---

    #[test]
    fn find_sort_by_modified() {
        let tmp = setup_vault();
        let fields = Fields::default();
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                &[],
                &[],
                None,
                &[],
                None,
                None,
                &fields,
                Some(&SortField::Modified),
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed.as_array().unwrap();
        let times: Vec<&str> = arr
            .iter()
            .map(|v| v["modified"].as_str().unwrap())
            .collect();
        let mut sorted = times.clone();
        sorted.sort();
        assert_eq!(times, sorted);
    }

    // --- find: empty result ---

    #[test]
    fn find_no_match_returns_empty_array() {
        let tmp = setup_vault();
        let filter = crate::filter::parse_property_filter("status=nonexistent").unwrap();
        let fields = Fields::default();
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                &[filter],
                &[],
                None,
                &[],
                None,
                None,
                &fields,
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(parsed.as_array().unwrap().is_empty());
    }

    // --- find: file not found ---

    #[test]
    fn find_file_not_found_returns_user_error() {
        let tmp = setup_vault();
        let fields = Fields::default();
        let result = find(
            tmp.path(),
            None,
            None,
            &[],
            &[],
            None,
            &[],
            Some("does-not-exist.md"),
            None,
            &fields,
            None,
            None,
            Format::Json,
        )
        .unwrap();
        assert!(matches!(result, CommandOutcome::UserError(_)));
    }

    // --- needs_body ---

    #[test]
    fn needs_body_false_when_only_fm_fields() {
        let fields = Fields {
            properties: true,
            tags: true,
            sections: false,
            tasks: false,
            links: false,
        };
        assert!(!needs_body(&fields, false, false, false));
    }

    #[test]
    fn needs_body_true_when_pattern() {
        let fields = Fields {
            properties: true,
            tags: true,
            sections: false,
            tasks: false,
            links: false,
        };
        assert!(needs_body(&fields, true, false, false));
    }

    #[test]
    fn needs_body_true_when_sections() {
        let fields = Fields {
            properties: false,
            tags: false,
            sections: true,
            tasks: false,
            links: false,
        };
        assert!(needs_body(&fields, false, false, false));
    }

    // --- matches_task_filter ---

    fn make_task(done: bool, status: char) -> FindTaskInfo {
        FindTaskInfo {
            line: 1,
            section: String::new(),
            status: status.to_string(),
            text: "task text".to_owned(),
            done,
        }
    }

    #[test]
    fn task_filter_any_empty() {
        assert!(!matches_task_filter(&[], &FindTaskFilter::Any));
    }

    #[test]
    fn task_filter_any_with_tasks() {
        let tasks = vec![make_task(false, ' ')];
        assert!(matches_task_filter(&tasks, &FindTaskFilter::Any));
    }

    #[test]
    fn task_filter_todo_open_task() {
        let tasks = vec![make_task(false, ' ')];
        assert!(matches_task_filter(&tasks, &FindTaskFilter::Todo));
    }

    #[test]
    fn task_filter_todo_no_open_tasks() {
        let tasks = vec![make_task(true, 'x')];
        assert!(!matches_task_filter(&tasks, &FindTaskFilter::Todo));
    }

    #[test]
    fn task_filter_done_with_done_task() {
        let tasks = vec![make_task(true, 'x')];
        assert!(matches_task_filter(&tasks, &FindTaskFilter::Done));
    }

    #[test]
    fn task_filter_status_char_match() {
        let tasks = vec![make_task(false, '~')];
        assert!(matches_task_filter(&tasks, &FindTaskFilter::Status('~')));
        assert!(!matches_task_filter(&tasks, &FindTaskFilter::Status('!')));
    }

    // --- find: --section filter ---

    fn setup_sectioned_vault() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();

        // File with two top-level sections, tasks only in "Tasks"
        fs::write(
            tmp.path().join("notes.md"),
            md!(r"
---
title: Notes
status: active
---
# Introduction

Some intro text.

## Tasks

- [ ] First task
- [x] Done task

## Background

Background information here.
"),
        )
        .unwrap();

        // File without a Tasks section
        fs::write(
            tmp.path().join("empty.md"),
            md!(r"
---
title: Empty
---
# Introduction

Just intro, no tasks section.
"),
        )
        .unwrap();

        tmp
    }

    #[test]
    fn find_section_filter_restricts_tasks_to_matching_section() {
        let tmp = setup_sectioned_vault();
        let fields = Fields::parse(&["tasks".to_owned()]).unwrap();
        let section_filters = vec![SectionFilter::parse("Tasks").unwrap()];
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                &[],
                &[],
                None,
                &section_filters,
                Some("notes.md"),
                None,
                &fields,
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        let tasks = arr[0]["tasks"].as_array().unwrap();
        assert_eq!(tasks.len(), 2, "should have 2 tasks in Tasks section");
    }

    #[test]
    fn find_section_filter_no_match_excludes_tasks() {
        let tmp = setup_sectioned_vault();
        let fields = Fields::parse(&["tasks".to_owned()]).unwrap();
        // Filter on a section that does not exist
        let section_filters = vec![SectionFilter::parse("Nonexistent").unwrap()];
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                &[],
                &[],
                None,
                &section_filters,
                Some("notes.md"),
                None,
                &fields,
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed.as_array().unwrap();
        // File has no matching section — it is excluded entirely
        assert!(
            arr.is_empty(),
            "file with no matching section should be excluded"
        );
    }

    #[test]
    fn find_section_filter_restricts_content_matches() {
        let tmp = setup_sectioned_vault();
        let fields = Fields::default();
        // "Background" text only appears in ## Background, not ## Tasks
        let section_filters = vec![SectionFilter::parse("Tasks").unwrap()];
        let out = unwrap_success(
            find(
                tmp.path(),
                Some("background"),
                None,
                &[],
                &[],
                None,
                &section_filters,
                None,
                None,
                &fields,
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed.as_array().unwrap();
        // "background" only appears in ## Background section, not in ## Tasks, so no match
        assert!(
            arr.is_empty(),
            "no files should match: 'background' is not in the Tasks section"
        );
    }

    #[test]
    fn find_section_filter_sections_field_filtered() {
        let tmp = setup_sectioned_vault();
        let fields = Fields::parse(&["sections".to_owned()]).unwrap();
        let section_filters = vec![SectionFilter::parse("Tasks").unwrap()];
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                &[],
                &[],
                None,
                &section_filters,
                Some("notes.md"),
                None,
                &fields,
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        let sections = arr[0]["sections"].as_array().unwrap();
        // Only the "Tasks" section (and its heading line) should be included
        assert!(
            sections.iter().all(|s| {
                s["heading"]
                    .as_str()
                    .is_some_and(|h| h.eq_ignore_ascii_case("tasks"))
            }),
            "sections output should only contain the matched section"
        );
    }

    #[test]
    fn find_section_filter_empty_no_section_excludes_file() {
        let tmp = setup_sectioned_vault();
        let fields = Fields::parse(&["tasks".to_owned()]).unwrap();
        let section_filters = vec![SectionFilter::parse("Tasks").unwrap()];
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                &[],
                &[],
                None,
                &section_filters,
                Some("empty.md"),
                None,
                &fields,
                None,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed.as_array().unwrap();
        // empty.md has no Tasks section — it is excluded entirely
        assert!(
            arr.is_empty(),
            "file with no matching section should be excluded"
        );
    }

    #[test]
    fn needs_body_true_when_section_filter() {
        let fields = Fields {
            properties: false,
            tags: false,
            sections: false,
            tasks: false,
            links: false,
        };
        assert!(needs_body(&fields, false, false, true));
        assert!(!needs_body(&fields, false, false, false));
    }
}
