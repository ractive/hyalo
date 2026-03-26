#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::SystemTime;

use crate::commands::section_scanner::SectionScanner;
use crate::commands::{FilesOrOutcome, collect_files};
use crate::output::{CommandOutcome, Format};
use hyalo_core::content_search::ContentSearchVisitor;
use hyalo_core::discovery;
use hyalo_core::filter::{self, Fields, FindTaskFilter, PropertyFilter, SortField, extract_tags};
use hyalo_core::frontmatter;
use hyalo_core::heading::{SectionFilter, SectionRange, build_section_scope, in_scope};
use hyalo_core::link_graph::LinkGraph;
use hyalo_core::links::Link;
use hyalo_core::scanner::{self, FileVisitor, FrontmatterCollector, ScanAction};
use hyalo_core::tasks::TaskExtractor;
use hyalo_core::types::{
    BacklinkInfo, ContentMatch, FileObject, FindTaskInfo, LinkInfo, OutlineSection, PropertyInfo,
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
    site_prefix: Option<&str>,
    pattern: Option<&str>,
    regexp: Option<&str>,
    property_filters: &[PropertyFilter],
    tag_filters: &[String],
    task_filter: Option<&FindTaskFilter>,
    section_filters: &[SectionFilter],
    files_arg: &[String],
    glob: Option<&str>,
    fields: &Fields,
    sort: Option<&SortField>,
    limit: Option<usize>,
    format: Format,
) -> Result<CommandOutcome> {
    let files = collect_files(dir, files_arg, glob, format)?;
    let files = match files {
        FilesOrOutcome::Files(f) => f,
        FilesOrOutcome::Outcome(o) => return Ok(o),
    };

    // Normalize empty / whitespace-only pattern to None so that
    // `hyalo find ""` behaves the same as `hyalo find` (no filter).
    let pattern = pattern.and_then(|p| if p.trim().is_empty() { None } else { Some(p) });

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
            match regex::RegexBuilder::new(&effective)
                .size_limit(1 << 20)
                .build()
            {
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

    // Canonicalize the vault directory once so that resolve_target (called
    // per-link inside the loop) can avoid repeated canonicalization.
    let canonical_dir = discovery::canonicalize_vault_dir(dir)?;

    // Build the link graph lazily — only when backlinks field is requested.
    // This requires scanning all files in the vault so it's opt-in.
    // Warnings for skipped files are emitted here; the per-file scan loop
    // below will also skip these files independently, so no duplicates.
    let link_graph = if fields.backlinks {
        let build = LinkGraph::build(dir, site_prefix)?;
        for (path, msg) in &build.warnings {
            eprintln!("warning: skipping {}: {msg}", path.display());
        }
        Some(build.graph)
    } else {
        None
    };

    // Short-circuit: when sort order is by file path (the same order
    // `discover_files` already returns) and backlinks are not requested
    // (which would need a full-vault scan regardless), we can stop as soon
    // as we have accumulated `limit` matching results instead of scanning
    // every file and truncating afterwards.
    //
    // Disabled when `--file` args are given: explicit file lists preserve CLI
    // order (not alphabetical), so stopping early could return the wrong N files.
    let can_short_circuit = limit.is_some()
        && files_arg.is_empty()
        && matches!(sort.unwrap_or(&SortField::File), SortField::File)
        && !fields.backlinks;

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
        let scan_result = {
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
            scanner::scan_file_multi(full_path, &mut visitor_refs)
        };
        match scan_result {
            Ok(()) => {}
            Err(e) if frontmatter::is_parse_error(&e) => {
                eprintln!("warning: skipping {rel_path}: {e}");
                continue;
            }
            Err(e) => return Err(e),
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
            &canonical_dir,
            link_graph.as_ref(),
            site_prefix,
        );

        results.push(obj);

        // Early exit when we have enough results and a full scan is not needed.
        if can_short_circuit && results.len() >= limit.unwrap() {
            break;
        }
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

    // In text mode, an empty result set produces no stdout output, which an
    // LLM (or script) cannot distinguish from a silent failure.  Emit an
    // explicit notice on stderr so the caller knows the query succeeded but
    // matched nothing.  JSON mode keeps the empty array on stdout unchanged.
    if format == crate::output::Format::Text && json_array.is_empty() {
        eprintln!("warning: No files matched.");
    }

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
///
/// `canonical_dir` must be a pre-canonicalized vault path (see
/// `discovery::canonicalize_vault_dir`). It is passed directly to
/// `resolve_target` to avoid per-link re-canonicalization.
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
    canonical_dir: &Path,
    link_graph: Option<&LinkGraph>,
    site_prefix: Option<&str>,
) -> FileObject {
    let properties = if fields.properties {
        let mut map = serde_json::Map::new();
        for (name, value) in props.iter().filter(|(n, _)| n.as_str() != "tags") {
            map.insert(name.clone(), frontmatter::yaml_to_json(value));
        }
        Some(map)
    } else {
        None
    };

    let properties_typed = if fields.properties_typed {
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
                    let path = discovery::resolve_target(canonical_dir, &link.target, site_prefix);
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

    let backlinks = if fields.backlinks {
        let entries = link_graph
            .map(|graph| graph.backlinks(rel_path))
            .unwrap_or_default();
        Some(
            entries
                .into_iter()
                .map(|e| {
                    let source = e.source.to_string_lossy().replace('\\', "/");
                    BacklinkInfo {
                        source,
                        line: e.line,
                        label: e.link.label.clone(),
                    }
                })
                .collect(),
        )
    } else {
        None
    };

    FileObject {
        file: rel_path.to_owned(),
        modified: modified.to_owned(),
        properties,
        properties_typed,
        tags: tags_field,
        sections,
        tasks,
        links,
        backlinks,
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
    fn on_body_line(&mut self, raw: &str, cleaned: &str, _line_num: usize) -> ScanAction {
        // Use `cleaned` for structural parsing (so links inside backtick spans
        // are not extracted) but `raw` for label text (so backtick-wrapped
        // content like [`file.ts`](path) is preserved).
        hyalo_core::links::extract_links_from_text_with_original(cleaned, raw, &mut self.links);
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
                None,
                &[],
                &[],
                None,
                &[],
                &[],
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
                None,
                &[],
                &[],
                None,
                &[],
                &[],
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
                None,
                &[],
                &[],
                None,
                &[],
                &[],
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
        let filter = hyalo_core::filter::parse_property_filter("status=planned").unwrap();
        let fields = Fields::default();
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                None,
                &[filter],
                &[],
                None,
                &[],
                &[],
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
        let filter = hyalo_core::filter::parse_property_filter("title").unwrap();
        let fields = Fields::default();
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                None,
                &[filter],
                &[],
                None,
                &[],
                &[],
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
                None,
                &[],
                &["cli".to_owned()],
                None,
                &[],
                &[],
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
                None,
                &[],
                &["nonexistent-tag".to_owned()],
                None,
                &[],
                &[],
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
                None,
                Some("Rust programming"),
                None,
                &[],
                &[],
                None,
                &[],
                &[],
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
                None,
                Some("Rust"),
                None,
                &[],
                &[],
                None,
                &[],
                &[],
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
                None,
                &[],
                &[],
                None,
                &[],
                &[],
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

    // --- find: empty / whitespace pattern treated as no-filter ---

    /// A vault with two files: one with body content and one with an empty body.
    fn setup_vault_with_empty_body() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();

        fs::write(
            tmp.path().join("has_body.md"),
            md!(r"
---
title: Has Body
---
Some content here.
"),
        )
        .unwrap();

        fs::write(
            tmp.path().join("empty_body.md"),
            md!(r"
---
title: Empty Body
---
"),
        )
        .unwrap();

        tmp
    }

    #[test]
    fn find_empty_string_pattern_matches_all_files() {
        // `hyalo find ""` must return the same count as `hyalo find`.
        // Previously, files with empty bodies were excluded because no body
        // line could match the empty-string pattern, producing an empty
        // content_matches vec that the filter treated as "no match".
        let tmp = setup_vault_with_empty_body();
        let fields = Fields::default();

        let count_no_pattern = {
            let out = unwrap_success(
                find(
                    tmp.path(),
                    None,
                    None,
                    None,
                    &[],
                    &[],
                    None,
                    &[],
                    &[],
                    None,
                    &fields,
                    None,
                    None,
                    Format::Json,
                )
                .unwrap(),
            );
            let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
            parsed.as_array().unwrap().len()
        };

        let count_empty_pattern = {
            let out = unwrap_success(
                find(
                    tmp.path(),
                    None,
                    Some(""),
                    None,
                    &[],
                    &[],
                    None,
                    &[],
                    &[],
                    None,
                    &fields,
                    None,
                    None,
                    Format::Json,
                )
                .unwrap(),
            );
            let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
            parsed.as_array().unwrap().len()
        };

        assert_eq!(
            count_empty_pattern, count_no_pattern,
            "empty pattern should return the same files as no pattern"
        );
        assert_eq!(count_no_pattern, 2, "vault has 2 files");
    }

    #[test]
    fn find_whitespace_only_pattern_matches_all_files() {
        // `hyalo find "   "` should also be treated as no filter.
        let tmp = setup_vault_with_empty_body();
        let fields = Fields::default();

        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                Some("   "),
                None,
                &[],
                &[],
                None,
                &[],
                &[],
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
        assert_eq!(
            arr.len(),
            2,
            "whitespace-only pattern should match all files"
        );
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
                None,
                &[],
                &[],
                Some(&FindTaskFilter::Todo),
                &[],
                &[],
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
                None,
                &[],
                &[],
                Some(&FindTaskFilter::Any),
                &[],
                &[],
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
                None,
                &[],
                &[],
                None,
                &[],
                &[],
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
            assert!(entry["properties"].is_object());
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
                None,
                &[],
                &[],
                None,
                &[],
                &[],
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
                None,
                &[],
                &[],
                None,
                &[],
                &["alpha.md".to_owned()],
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
                None,
                &[],
                &[],
                None,
                &[],
                &[],
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
                None,
                &[],
                &[],
                None,
                &[],
                &[],
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
        let filter = hyalo_core::filter::parse_property_filter("status=nonexistent").unwrap();
        let fields = Fields::default();
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                None,
                &[filter],
                &[],
                None,
                &[],
                &[],
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
            None,
            &[],
            &[],
            None,
            &[],
            &["does-not-exist.md".to_owned()],
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
            properties_typed: false,
            tags: true,
            sections: false,
            tasks: false,
            links: false,
            backlinks: false,
        };
        assert!(!needs_body(&fields, false, false, false));
    }

    #[test]
    fn needs_body_true_when_pattern() {
        let fields = Fields {
            properties: true,
            properties_typed: false,
            tags: true,
            sections: false,
            tasks: false,
            links: false,
            backlinks: false,
        };
        assert!(needs_body(&fields, true, false, false));
    }

    #[test]
    fn needs_body_true_when_sections() {
        let fields = Fields {
            properties: false,
            properties_typed: false,
            tags: false,
            sections: true,
            tasks: false,
            links: false,
            backlinks: false,
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
                None,
                &[],
                &[],
                None,
                &section_filters,
                &["notes.md".to_owned()],
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
                None,
                &[],
                &[],
                None,
                &section_filters,
                &["notes.md".to_owned()],
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
                None,
                Some("background"),
                None,
                &[],
                &[],
                None,
                &section_filters,
                &[],
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
                None,
                &[],
                &[],
                None,
                &section_filters,
                &["notes.md".to_owned()],
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
                None,
                &[],
                &[],
                None,
                &section_filters,
                &["empty.md".to_owned()],
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
            properties_typed: false,
            tags: false,
            sections: false,
            tasks: false,
            links: false,
            backlinks: false,
        };
        assert!(needs_body(&fields, false, false, true));
        assert!(!needs_body(&fields, false, false, false));
    }

    #[test]
    fn find_skips_broken_frontmatter_file() {
        let tmp = setup_vault();
        // Add a file with unclosed frontmatter
        fs::write(
            tmp.path().join("broken.md"),
            "---\ntitle: Broken\nNo closing delimiter.\n",
        )
        .unwrap();
        let fields = Fields::default();
        let result = find(
            tmp.path(),
            None,
            None,
            None,
            &[],
            &[],
            None,
            &[],
            &[],
            None,
            &fields,
            None,
            None,
            Format::Json,
        )
        .unwrap();
        let out = unwrap_success(result);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed.as_array().unwrap();
        // Should return the 3 good files, skipping the broken one
        assert_eq!(arr.len(), 3);
        assert!(
            !arr.iter()
                .any(|e| e["file"].as_str().unwrap().contains("broken"))
        );
    }

    // --- find: properties-typed field ---

    #[test]
    fn find_fields_properties_typed_is_array() {
        let tmp = setup_vault();
        let fields = Fields::parse(&["properties-typed".to_owned()]).unwrap();
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &[],
                None,
                &[],
                &["alpha.md".to_owned()],
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
        let entry = &arr[0];

        // properties_typed must be an array of {name, type, value} objects
        let typed = entry["properties_typed"]
            .as_array()
            .expect("properties_typed should be an array");
        assert!(!typed.is_empty());

        for item in typed {
            assert!(
                item["name"].is_string(),
                "each item must have a string 'name'"
            );
            assert!(
                item["type"].is_string(),
                "each item must have a string 'type'"
            );
            assert!(!item["value"].is_null(), "each item must have a 'value'");
        }

        // tags must be excluded from properties_typed
        assert!(
            typed.iter().all(|p| p["name"] != "tags"),
            "tags should not appear in properties_typed"
        );

        // properties (map) should not be present when not requested
        assert!(
            entry["properties"].is_null(),
            "properties map should be absent"
        );
    }

    #[test]
    fn find_fields_properties_and_properties_typed_together() {
        let tmp = setup_vault();
        let fields = Fields::parse(&["properties,properties-typed".to_owned()]).unwrap();
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &[],
                None,
                &[],
                &["alpha.md".to_owned()],
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
        let entry = &arr[0];

        // Both fields present simultaneously
        assert!(
            entry["properties"].is_object(),
            "properties map should be present"
        );
        assert!(
            entry["properties_typed"].is_array(),
            "properties_typed should be present"
        );
    }

    #[test]
    fn find_fields_properties_typed_type_values() {
        let tmp = setup_vault();
        let fields = Fields::parse(&["properties-typed".to_owned()]).unwrap();
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &[],
                None,
                &[],
                &["alpha.md".to_owned()],
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
        let typed = arr[0]["properties_typed"].as_array().unwrap();

        // alpha.md has status: planned (text) and title: Alpha Note (text)
        let status = typed
            .iter()
            .find(|p| p["name"] == "status")
            .expect("status property missing");
        assert_eq!(status["type"], "text");
        assert_eq!(status["value"], "planned");
    }

    // --- find: backlinks field ---

    #[test]
    fn find_fields_backlinks_shows_incoming_links() {
        let tmp = setup_vault();
        // alpha.md links to [[beta]], so beta should have a backlink from alpha
        let fields = Fields::parse(&["backlinks".to_owned()]).unwrap();
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &[],
                None,
                &[],
                &["beta.md".to_owned()],
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
        let beta = &arr[0];
        let backlinks = beta["backlinks"].as_array().unwrap();
        assert_eq!(backlinks.len(), 1);
        assert_eq!(backlinks[0]["source"], "alpha.md");
        assert!(backlinks[0]["line"].as_u64().unwrap() > 0);
    }

    #[test]
    fn find_fields_backlinks_not_included_by_default() {
        let tmp = setup_vault();
        let fields = Fields::default();
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &[],
                None,
                &[],
                &[],
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
            assert!(
                !entry.as_object().unwrap().contains_key("backlinks"),
                "backlinks key should be absent by default, not just null"
            );
        }
    }

    #[test]
    fn find_fields_backlinks_empty_when_no_incoming() {
        let tmp = setup_vault();
        // gamma has no frontmatter and nobody links to it
        let fields = Fields::parse(&["backlinks".to_owned()]).unwrap();
        let out = unwrap_success(
            find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &[],
                None,
                &[],
                &["gamma.md".to_owned()],
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
        let gamma = &arr[0];
        let backlinks = gamma["backlinks"].as_array().unwrap();
        assert!(backlinks.is_empty());
    }
}
