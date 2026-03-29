#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result};
use globset::{GlobBuilder, GlobSetBuilder};
use std::path::Path;

use crate::output::{CommandOutcome, Format};
use hyalo_core::content_search::ContentSearchVisitor;
use hyalo_core::discovery;
use hyalo_core::filter::{self, Fields, FindTaskFilter, PropertyFilter, SortField};
use hyalo_core::heading::{SectionFilter, SectionRange, build_section_scope, in_scope};
use hyalo_core::index::{IndexEntry, VaultIndex};
use hyalo_core::link_graph::{LinkGraph, is_self_link};
use hyalo_core::types::{
    BacklinkInfo, ContentMatch, FileObject, FindTaskInfo, LinkInfo, OutlineSection, PropertyInfo,
};

// ---------------------------------------------------------------------------
// Public command entry point
// ---------------------------------------------------------------------------

/// Filter index entries by optional `--file` and `--glob` scoping arguments.
///
/// - When both are empty, all entries are returned.
/// - `files_arg` entries are matched by exact `rel_path` equality.
/// - `globs` patterns use the same globset semantics as `discovery::match_globs`
///   (positive patterns require a match; negative `!pat` patterns exclude matches).
pub fn filter_index_entries<'a>(
    entries: &'a [IndexEntry],
    files_arg: &[String],
    globs: &[String],
) -> Result<Vec<&'a IndexEntry>> {
    if files_arg.is_empty() && globs.is_empty() {
        return Ok(entries.iter().collect());
    }

    if !files_arg.is_empty() && globs.is_empty() {
        // Exact path matching (same order as `files_arg` for explicit file lists)
        let filtered: Vec<&IndexEntry> = entries
            .iter()
            .filter(|e| files_arg.iter().any(|f| f == &e.rel_path))
            .collect();
        return Ok(filtered);
    }

    // Glob filtering — build positive/negative glob sets (same logic as discovery::match_globs)
    let normalized: Vec<String> = globs
        .iter()
        .map(|p| {
            if let Some(rest) = p.strip_prefix("\\!") {
                format!("!{rest}")
            } else {
                p.clone()
            }
        })
        .collect();

    let mut positive: Vec<&str> = Vec::new();
    let mut negative: Vec<&str> = Vec::new();
    for p in &normalized {
        if let Some(neg) = p.strip_prefix('!') {
            anyhow::ensure!(
                !neg.is_empty(),
                "negation glob pattern must not be empty (got '!')"
            );
            negative.push(neg);
        } else {
            positive.push(p.as_str());
        }
    }

    let positive_set = if positive.is_empty() {
        None
    } else {
        let mut builder = GlobSetBuilder::new();
        for pat in &positive {
            builder.add(
                GlobBuilder::new(pat)
                    .literal_separator(true)
                    .build()
                    .context("invalid glob pattern")?,
            );
        }
        Some(
            builder
                .build()
                .context("failed to build positive globset")?,
        )
    };

    let negative_set = if negative.is_empty() {
        None
    } else {
        let mut builder = GlobSetBuilder::new();
        for pat in &negative {
            builder.add(
                GlobBuilder::new(pat)
                    .literal_separator(true)
                    .build()
                    .context("invalid glob negation pattern")?,
            );
        }
        Some(
            builder
                .build()
                .context("failed to build negative globset")?,
        )
    };

    let filtered: Vec<&IndexEntry> = entries
        .iter()
        .filter(|e| {
            // When files_arg is non-empty, an entry also passes if it is listed
            // explicitly — this handles the case where both files_arg and globs
            // are present (OR semantics between the two inclusion criteria).
            let passes_files = !files_arg.is_empty() && files_arg.iter().any(|f| f == &e.rel_path);
            let passes_positive = positive_set
                .as_ref()
                .is_none_or(|gs| gs.is_match(&e.rel_path));
            let passes_negative = negative_set
                .as_ref()
                .is_none_or(|gs| !gs.is_match(&e.rel_path));
            // An entry is included if it matches files_arg OR (matches positive
            // globs AND is not excluded by a negative glob).
            passes_files || (passes_positive && passes_negative)
        })
        .collect();

    Ok(filtered)
}

/// Find files matching the given filters and return them as a JSON array.
///
/// Uses pre-scanned index data for all metadata (properties, tags, sections,
/// tasks, outbound links). `dir` is still used for:
/// - Content search (disk I/O is required to read file bodies when
///   `pattern` or `regexp` is specified)
/// - Link path resolution (`discovery::resolve_target`)
///
/// Backlinks are resolved via `index.link_graph()` without a fresh vault scan.
#[allow(clippy::too_many_arguments)]
pub fn find(
    index: &dyn VaultIndex,
    dir: &Path,
    site_prefix: Option<&str>,
    pattern: Option<&str>,
    regexp: Option<&str>,
    property_filters: &[PropertyFilter],
    tag_filters: &[String],
    task_filter: Option<&FindTaskFilter>,
    section_filters: &[SectionFilter],
    files_arg: &[String],
    globs: &[String],
    fields: &Fields,
    sort: Option<&SortField>,
    reverse: bool,
    limit: Option<usize>,
    broken_links: bool,
    title_filter: Option<&str>,
    format: Format,
) -> Result<CommandOutcome> {
    if pattern.is_some_and(|p| p.trim().is_empty()) {
        return Ok(CommandOutcome::UserError(
            "body pattern must not be empty; omit the pattern to match all files".to_owned(),
        ));
    }

    let sort_needs_backlinks = matches!(sort, Some(SortField::BacklinksCount));
    let sort_needs_links = matches!(sort, Some(SortField::LinksCount));
    let sort_needs_properties = matches!(sort, Some(SortField::Property(_)));
    let sort_needs_title = matches!(sort, Some(SortField::Title));

    let has_content_search = pattern.is_some() || regexp.is_some();
    let has_task_filter = task_filter.is_some();
    let has_section_filter = !section_filters.is_empty();

    // Compile --title filter once before the loop.
    let title_matcher = match title_filter.map(TitleMatcher::parse) {
        Some(Ok(m)) => Some(m),
        Some(Err(outcome)) => return Ok(outcome),
        None => None,
    };

    // Compile regex once (if any)
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

    // Canonicalize the vault directory for link resolution
    let canonical_dir = discovery::canonicalize_vault_dir(dir)?;

    // Filter entries by --file / --glob scoping
    let scoped_entries = filter_index_entries(index.entries(), files_arg, globs)?;

    // Use the index's pre-built link graph for backlinks
    let link_graph_ref = if fields.backlinks || sort_needs_backlinks {
        Some(index.link_graph())
    } else {
        None
    };

    // When sorting by a frontmatter property or title, or when --broken-links
    // is active, force the relevant fields on even if not requested via --fields.
    let original_fields = fields;
    let effective_fields;
    let fields = if (sort_needs_properties && !fields.properties)
        || (sort_needs_title && !fields.title)
        || (broken_links && !fields.links)
    {
        effective_fields = Fields {
            properties: fields.properties || sort_needs_properties,
            title: fields.title || sort_needs_title,
            links: fields.links || broken_links,
            ..fields.clone()
        };
        &effective_fields
    } else {
        fields
    };

    // The index has all metadata, so we can pre-sort by any sort key
    // except BacklinksCount (which needs the link graph per result).
    // presorted=true even without --limit — pre-sorting is no more expensive
    // than post-sorting, and it simplifies the limit/total logic below.
    let presorted = !reverse && !matches!(sort, Some(SortField::BacklinksCount));

    let mut scoped_entries = scoped_entries;
    if presorted {
        presort_index_entries(&mut scoped_entries, sort, index.link_graph());
    }

    let mut results: Vec<FileObject> = Vec::new();
    let mut total_matching: usize = 0;

    for entry in &scoped_entries {
        // --- Metadata filters using pre-indexed data ---
        if !filter::matches_filters_with_tags(
            &entry.properties,
            property_filters,
            &entry.tags,
            tag_filters,
        ) {
            continue;
        }

        // --- Apply title filter (index path: sections are pre-indexed, no I/O needed) ---
        if let Some(ref matcher) = title_matcher {
            let title_val = extract_title(&entry.properties, Some(&entry.sections));
            if !matcher.matches(&title_val) {
                continue;
            }
        }

        // --- Build section scopes from pre-indexed sections ---
        let scope_ranges: Vec<SectionRange> = if has_section_filter {
            build_section_scope(&entry.sections, section_filters, usize::MAX)
        } else {
            Vec::new()
        };

        if has_section_filter && scope_ranges.is_empty() {
            // No matching section in this file — skip entirely
            continue;
        }

        // --- Task filter using pre-indexed tasks ---
        let mut collected_tasks: Option<Vec<FindTaskInfo>> = if fields.tasks || has_task_filter {
            let mut tasks = entry.tasks.clone();
            if has_section_filter {
                tasks.retain(|t| in_scope(&scope_ranges, t.line));
            }
            Some(tasks)
        } else {
            None
        };

        if let Some(filter) = task_filter {
            let tasks_slice: &[FindTaskInfo] = collected_tasks.as_deref().unwrap_or(&[]);
            if !matches_task_filter(tasks_slice, filter) {
                continue;
            }
        }

        // --- Content search: requires disk I/O ---
        let content_matches: Option<Vec<ContentMatch>> = if has_content_search {
            let full_path = dir.join(&entry.rel_path);
            let mut content_visitor = if let Some(ref re) = compiled_regex {
                ContentSearchVisitor::from_compiled(re.clone())
            } else {
                // pattern is Some at this point since has_content_search is true
                ContentSearchVisitor::new(pattern.unwrap())
            };
            // Re-scan just this file for content (frontmatter already in index)
            let scan_result =
                hyalo_core::scanner::scan_file_multi(&full_path, &mut [&mut content_visitor]);
            match scan_result {
                Ok(()) => {}
                Err(e) if hyalo_core::frontmatter::is_parse_error(&e) => {
                    crate::warn::warn(format!("skipping {}: {e}", entry.rel_path));
                    continue;
                }
                Err(e) => return Err(e),
            }
            let mut matches = content_visitor.into_matches();
            if has_section_filter {
                matches.retain(|m| in_scope(&scope_ranges, m.line));
            }
            Some(matches)
        } else {
            None
        };

        // Drop tasks from collected_tasks if not needed in output (filter already applied)
        if !fields.tasks {
            collected_tasks = None;
        }

        // Filter: content search must have at least one match
        if has_content_search && content_matches.as_ref().is_some_and(|m| m.is_empty()) {
            continue;
        }

        // When pre-sorted with a limit, skip expensive construction once full.
        // (broken_links needs the resolved links, so fall through.)
        if presorted && limit.is_some_and(|n| results.len() >= n) && !broken_links {
            total_matching += 1;
            continue;
        }

        // --- Build sections field ---
        let outline_sections: Option<Vec<OutlineSection>> = if fields.sections {
            let mut secs = entry.sections.clone();
            if !scope_ranges.is_empty() {
                secs.retain(|s| in_scope(&scope_ranges, s.line));
            }
            Some(secs)
        } else {
            None
        };

        // --- Build links field from pre-indexed link data ---
        let links = if fields.links || sort_needs_links {
            Some(
                entry
                    .links
                    .iter()
                    .map(|(_, link)| {
                        let path =
                            discovery::resolve_target(&canonical_dir, &link.target, site_prefix);
                        LinkInfo {
                            target: link.target.clone(),
                            path,
                            label: link.label.clone(),
                        }
                    })
                    .collect::<Vec<_>>(),
            )
        } else {
            None
        };

        // --- Build properties fields ---
        let properties = if fields.properties {
            let mut map = serde_json::Map::new();
            for (name, value) in entry
                .properties
                .iter()
                .filter(|(n, _)| n.as_str() != "tags")
            {
                map.insert(name.clone(), value.clone());
            }
            Some(map)
        } else {
            None
        };

        let properties_typed = if fields.properties_typed {
            Some(
                entry
                    .properties
                    .iter()
                    .filter(|(name, _)| name.as_str() != "tags")
                    .map(|(name, value)| PropertyInfo {
                        name: name.clone(),
                        prop_type: hyalo_core::frontmatter::infer_type(value).to_owned(),
                        value: value.clone(),
                    })
                    .collect(),
            )
        } else {
            None
        };

        let tags_field = if fields.tags {
            Some(entry.tags.clone())
        } else {
            None
        };

        // --- Backlinks from pre-built link graph ---
        let backlinks = if fields.backlinks {
            let entries_bl = link_graph_ref
                .map(|graph| graph.backlinks(&entry.rel_path))
                .unwrap_or_default();
            Some(
                entries_bl
                    .into_iter()
                    .filter(|e| !is_self_link(e, &entry.rel_path))
                    .map(|e| {
                        let source = e.source.to_string_lossy().replace('\\', "/");
                        BacklinkInfo {
                            source,
                            line: e.line,
                            label: e.link.label.clone(),
                        }
                    })
                    .collect::<Vec<_>>(),
            )
        } else {
            None
        };

        // --- Title field (index path) ---
        // entry.sections is always available in the index, so we can look up
        // the first H1 even when fields.sections is false.
        let title = if fields.title {
            Some(extract_title(&entry.properties, Some(&entry.sections)))
        } else {
            None
        };

        let obj = FileObject {
            file: entry.rel_path.clone(),
            modified: entry.modified.clone(),
            title,
            properties,
            properties_typed,
            tags: tags_field,
            sections: outline_sections,
            tasks: collected_tasks,
            links,
            backlinks,
            matches: content_matches,
        };

        // --- Apply broken-links filter ---
        if broken_links {
            let has_broken = obj
                .links
                .as_deref()
                .unwrap_or(&[])
                .iter()
                .any(|l| l.path.is_none());
            if !has_broken {
                continue;
            }
        }

        if presorted {
            total_matching += 1;
        }
        if !presorted || limit.is_none_or(|n| results.len() < n) {
            results.push(obj);
        }
    }

    // --- Sort ---
    if !presorted {
        apply_sort(&mut results, sort, link_graph_ref);
    }

    if let Some(SortField::Property(key)) = sort
        && !results.is_empty()
        && results.iter().all(|r| {
            r.properties
                .as_ref()
                .and_then(|p| p.get(key.as_str()))
                .is_none()
        })
    {
        crate::warn::warn(format!(
            "no files have property '{key}' -- sort has no effect"
        ));
    }

    // Strip internally-computed fields that the user didn't request in --fields.
    if sort_needs_links && !original_fields.links {
        for obj in &mut results {
            obj.links = None;
        }
    }
    if sort_needs_properties && !original_fields.properties {
        for obj in &mut results {
            obj.properties = None;
        }
    }
    if sort_needs_title && !original_fields.title {
        for obj in &mut results {
            obj.title = None;
        }
    }

    // --- Reverse ---
    if reverse {
        results.reverse();
    }

    // --- Limit ---
    // When presorted, total_matching already holds the accurate count and
    // results are already capped — skip truncation.
    let total = if presorted {
        total_matching
    } else {
        let t = results.len();
        if let Some(n) = limit {
            results.truncate(n);
        }
        t
    };

    // --- Serialize ---
    let json_array: Vec<serde_json::Value> = results
        .into_iter()
        .map(|obj| serde_json::to_value(obj).context("failed to serialize find result"))
        .collect::<Result<_>>()?;

    if format == crate::output::Format::Text && json_array.is_empty() {
        crate::warn::warn("No files matched.");
    }

    let json_output = serde_json::json!({
        "total": total,
        "results": json_array,
    });

    Ok(CommandOutcome::Success(crate::output::format_success(
        format,
        &json_output,
    )))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract the title value for `--fields title`.
///
/// Priority:
/// 1. `title` frontmatter property (if it is a string)
/// 2. First H1 heading in the document outline
/// 3. `serde_json::Value::Null` if neither found
fn extract_title(
    props: &indexmap::IndexMap<String, serde_json::Value>,
    outline_sections: Option<&[OutlineSection]>,
) -> serde_json::Value {
    // 1. Frontmatter title property
    if let Some(serde_json::Value::String(s)) = props.get("title") {
        return serde_json::Value::String(s.clone());
    }
    // 2. First H1 heading from outline
    if let Some(sections) = outline_sections {
        for sec in sections {
            if sec.level == 1
                && let Some(ref heading) = sec.heading
            {
                return serde_json::Value::String(heading.clone());
            }
        }
    }
    serde_json::Value::Null
}

/// Pre-compiled title filter — avoids per-file regex compilation and repeated
/// `to_lowercase()` allocation.
enum TitleMatcher {
    /// Case-insensitive substring: stores the lowered pattern.
    Substring(String),
    /// Pre-compiled case-insensitive regex.
    Regex(regex::Regex),
}

impl TitleMatcher {
    /// Parse a `--title` value into a compiled matcher.
    ///
    /// Returns `Err(CommandOutcome::UserError(...))` on invalid regex.
    fn parse(pattern: &str) -> Result<Self, CommandOutcome> {
        if let Some(regex_pat) = pattern.strip_prefix("~=") {
            let effective = format!("(?i){regex_pat}");
            match regex::RegexBuilder::new(&effective)
                .size_limit(1 << 20)
                .build()
            {
                Ok(re) => Ok(Self::Regex(re)),
                Err(e) => Err(CommandOutcome::UserError(format!(
                    "invalid --title regex: {regex_pat}\n{e}"
                ))),
            }
        } else {
            Ok(Self::Substring(pattern.to_lowercase()))
        }
    }

    /// Returns true if the title value matches. `Null` titles never match.
    fn matches(&self, title: &serde_json::Value) -> bool {
        let title_str = match title {
            serde_json::Value::String(s) => s.as_str(),
            _ => return false,
        };
        match self {
            Self::Substring(lowered) => title_str.to_lowercase().contains(lowered.as_str()),
            Self::Regex(re) => re.is_match(title_str),
        }
    }
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

/// Apply the requested sort order to the results.
fn apply_sort(
    results: &mut [FileObject],
    sort: Option<&SortField>,
    link_graph: Option<&LinkGraph>,
) {
    match sort.unwrap_or(&SortField::File) {
        SortField::File => results.sort_by(|a, b| a.file.cmp(&b.file)),
        SortField::Modified => results.sort_by(|a, b| a.modified.cmp(&b.modified)),
        SortField::BacklinksCount => {
            results.sort_by(|a, b| {
                let a_count = a.backlinks.as_ref().map_or_else(
                    || link_graph.map_or(0, |g| g.backlinks(&a.file).len()),
                    Vec::len,
                );
                let b_count = b.backlinks.as_ref().map_or_else(
                    || link_graph.map_or(0, |g| g.backlinks(&b.file).len()),
                    Vec::len,
                );
                b_count.cmp(&a_count)
            });
        }
        SortField::LinksCount => {
            results.sort_by(|a, b| {
                let a_count = a.links.as_ref().map_or(0, Vec::len);
                let b_count = b.links.as_ref().map_or(0, Vec::len);
                b_count.cmp(&a_count)
            });
        }
        SortField::Title => {
            results.sort_by(|a, b| {
                let a_val = a.title.as_ref();
                let b_val = b.title.as_ref();
                filter::compare_property_values(a_val, b_val).then_with(|| a.file.cmp(&b.file))
            });
        }
        SortField::Property(key) => {
            results.sort_by(|a, b| {
                let a_val = a.properties.as_ref().and_then(|p| p.get(key));
                let b_val = b.properties.as_ref().and_then(|p| p.get(key));
                filter::compare_property_values(a_val, b_val).then_with(|| a.file.cmp(&b.file))
            });
        }
    }
}

/// Pre-sort index entries by the requested sort key so that the early-exit
/// optimisation can collect the first N matches in final order.
///
/// This mirrors `apply_sort` but operates on `&IndexEntry` references
/// instead of `FileObject` values, avoiding construction of the full object.
fn presort_index_entries(
    entries: &mut [&IndexEntry],
    sort: Option<&SortField>,
    link_graph: &LinkGraph,
) {
    match sort.unwrap_or(&SortField::File) {
        SortField::File => entries.sort_by(|a, b| a.rel_path.cmp(&b.rel_path)),
        SortField::Modified => entries.sort_by(|a, b| a.modified.cmp(&b.modified)),
        SortField::BacklinksCount => {
            // Descending by backlink count — matches apply_sort.
            entries.sort_by(|a, b| {
                let a_count = link_graph.backlinks(&a.rel_path).len();
                let b_count = link_graph.backlinks(&b.rel_path).len();
                b_count.cmp(&a_count)
            });
        }
        SortField::LinksCount => {
            entries.sort_by(|a, b| {
                let a_count = a.links.len();
                let b_count = b.links.len();
                b_count.cmp(&a_count)
            });
        }
        SortField::Title => {
            entries.sort_by(|a, b| {
                let a_val = extract_title(&a.properties, Some(&a.sections));
                let b_val = extract_title(&b.properties, Some(&b.sections));
                filter::compare_property_values(Some(&a_val), Some(&b_val))
                    .then_with(|| a.rel_path.cmp(&b.rel_path))
            });
        }
        SortField::Property(key) => {
            entries.sort_by(|a, b| {
                let a_val = a.properties.get(key.as_str());
                let b_val = b.properties.get(key.as_str());
                filter::compare_property_values(a_val, b_val)
                    .then_with(|| a.rel_path.cmp(&b.rel_path))
            });
        }
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

    /// Build a `ScannedIndex` from `dir` and call `find`.
    /// Mirrors the old disk-scan helper signature used in pre-Phase-5 tests.
    #[allow(clippy::too_many_arguments)]
    fn run_find(
        dir: &std::path::Path,
        site_prefix: Option<&str>,
        pattern: Option<&str>,
        regexp: Option<&str>,
        property_filters: &[PropertyFilter],
        tag_filters: &[String],
        task_filter: Option<&FindTaskFilter>,
        section_filters: &[SectionFilter],
        files_arg: &[String],
        globs: &[String],
        fields: &Fields,
        sort: Option<&SortField>,
        reverse: bool,
        limit: Option<usize>,
        broken_links: bool,
        title_filter: Option<&str>,
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
        let build =
            ScannedIndex::build(&file_pairs, site_prefix, &ScanOptions { scan_body: true })?;
        find(
            &build.index,
            dir,
            site_prefix,
            pattern,
            regexp,
            property_filters,
            tag_filters,
            task_filter,
            section_filters,
            files_arg,
            globs,
            fields,
            sort,
            reverse,
            limit,
            broken_links,
            title_filter,
            format,
        )
    }

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
            run_find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &[],
                None,
                &[],
                &[],
                &[],
                &fields,
                None,
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
        assert_eq!(arr.len(), 3);
    }

    #[test]
    fn find_returns_sorted_by_file_by_default() {
        let tmp = setup_vault();
        let fields = Fields::default();
        let out = unwrap_success(
            run_find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &[],
                None,
                &[],
                &[],
                &[],
                &fields,
                None,
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
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
            run_find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &[],
                None,
                &[],
                &[],
                &[],
                &fields,
                None,
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
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
            run_find(
                tmp.path(),
                None,
                None,
                None,
                &[filter],
                &[],
                None,
                &[],
                &[],
                &[],
                &fields,
                None,
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert!(arr[0]["file"].as_str().unwrap().contains("alpha"));
    }

    #[test]
    fn find_property_filter_exists() {
        let tmp = setup_vault();
        let filter = hyalo_core::filter::parse_property_filter("title").unwrap();
        let fields = Fields::default();
        let out = unwrap_success(
            run_find(
                tmp.path(),
                None,
                None,
                None,
                &[filter],
                &[],
                None,
                &[],
                &[],
                &[],
                &fields,
                None,
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
        // alpha and beta have title; gamma does not
        assert_eq!(arr.len(), 2);
    }

    // --- find: tag filter ---

    #[test]
    fn find_tag_filter_matches() {
        let tmp = setup_vault();
        let fields = Fields::default();
        let out = unwrap_success(
            run_find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &["cli".to_owned()],
                None,
                &[],
                &[],
                &[],
                &fields,
                None,
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert!(arr[0]["file"].as_str().unwrap().contains("alpha"));
    }

    #[test]
    fn find_tag_filter_no_match() {
        let tmp = setup_vault();
        let fields = Fields::default();
        let out = unwrap_success(
            run_find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &["nonexistent-tag".to_owned()],
                None,
                &[],
                &[],
                &[],
                &fields,
                None,
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
        assert!(arr.is_empty());
    }

    // --- find: content search ---

    #[test]
    fn find_pattern_matches_content() {
        let tmp = setup_vault();
        let fields = Fields::default();
        let out = unwrap_success(
            run_find(
                tmp.path(),
                None,
                Some("Rust programming"),
                None,
                &[],
                &[],
                None,
                &[],
                &[],
                &[],
                &fields,
                None,
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert!(arr[0]["file"].as_str().unwrap().contains("beta"));
    }

    #[test]
    fn find_pattern_includes_matches_field() {
        let tmp = setup_vault();
        let fields = Fields::default();
        let out = unwrap_success(
            run_find(
                tmp.path(),
                None,
                Some("Rust"),
                None,
                &[],
                &[],
                None,
                &[],
                &[],
                &[],
                &fields,
                None,
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
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
            run_find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &[],
                None,
                &[],
                &[],
                &[],
                &fields,
                None,
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
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
    fn find_empty_string_pattern_errors() {
        // `hyalo find ""` must return a user error, not match all files.
        let tmp = setup_vault_with_empty_body();
        let fields = Fields::default();

        let outcome = run_find(
            tmp.path(),
            None,
            Some(""),
            None,
            &[],
            &[],
            None,
            &[],
            &[],
            &[],
            &fields,
            None,
            false,
            None,
            false,
            None,
            Format::Json,
        )
        .unwrap();

        match outcome {
            CommandOutcome::UserError(msg) => {
                assert!(
                    msg.contains("body pattern must not be empty"),
                    "error message should mention empty pattern, got: {msg}"
                );
            }
            CommandOutcome::Success(_) => panic!("expected user error for empty pattern"),
        }
    }

    #[test]
    fn find_whitespace_only_pattern_errors() {
        // `hyalo find "   "` should also return a user error.
        let tmp = setup_vault_with_empty_body();
        let fields = Fields::default();

        let outcome = run_find(
            tmp.path(),
            None,
            Some("   "),
            None,
            &[],
            &[],
            None,
            &[],
            &[],
            &[],
            &fields,
            None,
            false,
            None,
            false,
            None,
            Format::Json,
        )
        .unwrap();

        match outcome {
            CommandOutcome::UserError(msg) => {
                assert!(
                    msg.contains("body pattern must not be empty"),
                    "error message should mention empty pattern, got: {msg}"
                );
            }
            CommandOutcome::Success(_) => panic!("expected user error for whitespace-only pattern"),
        }
    }

    // --- find: task filter ---

    #[test]
    fn find_task_filter_todo() {
        let tmp = setup_vault();
        let fields = Fields::default();
        let out = unwrap_success(
            run_find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &[],
                Some(&FindTaskFilter::Todo),
                &[],
                &[],
                &[],
                &fields,
                None,
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
        // only alpha.md has an open task
        assert_eq!(arr.len(), 1);
        assert!(arr[0]["file"].as_str().unwrap().contains("alpha"));
    }

    #[test]
    fn find_task_filter_any() {
        let tmp = setup_vault();
        let fields = Fields::default();
        let out = unwrap_success(
            run_find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &[],
                Some(&FindTaskFilter::Any),
                &[],
                &[],
                &[],
                &fields,
                None,
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert!(arr[0]["file"].as_str().unwrap().contains("alpha"));
    }

    // --- find: fields selection ---

    #[test]
    fn find_fields_subset_properties_only() {
        let tmp = setup_vault();
        let fields = Fields::parse(&["properties".to_owned()]).unwrap();
        let out = unwrap_success(
            run_find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &[],
                None,
                &[],
                &[],
                &[],
                &fields,
                None,
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
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
            run_find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &[],
                None,
                &[],
                &[],
                &[],
                &fields,
                None,
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
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
            run_find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &[],
                None,
                &[],
                &["alpha.md".to_owned()],
                &[],
                &fields,
                None,
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
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
            run_find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &[],
                None,
                &[],
                &[],
                &[],
                &fields,
                None,
                false,
                Some(1),
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        // When limit truncates results, output is an envelope {total, results}.
        assert!(parsed.is_object(), "expected envelope, got: {parsed}");
        let results = parsed["results"].as_array().unwrap();
        assert_eq!(results.len(), 1);
        // total reflects the full vault size (3 files in unit test vault).
        assert!(
            parsed["total"].as_u64().unwrap() > 1,
            "total should exceed the limit"
        );
    }

    // --- find: sort ---

    #[test]
    fn find_sort_by_modified() {
        let tmp = setup_vault();
        let fields = Fields::default();
        let out = unwrap_success(
            run_find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &[],
                None,
                &[],
                &[],
                &[],
                &fields,
                Some(&SortField::Modified),
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
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
            run_find(
                tmp.path(),
                None,
                None,
                None,
                &[filter],
                &[],
                None,
                &[],
                &[],
                &[],
                &fields,
                None,
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(parsed["results"].as_array().unwrap().is_empty());
    }

    // --- find: file not found ---

    #[test]
    fn find_file_not_found_returns_empty_results() {
        // The index-backed `find` silently returns zero results for a
        // `--file` argument that matches no index entry (no UserError).
        let tmp = setup_vault();
        let fields = Fields::default();
        let result = run_find(
            tmp.path(),
            None,
            None,
            None,
            &[],
            &[],
            None,
            &[],
            &["does-not-exist.md".to_owned()],
            &[],
            &fields,
            None,
            false,
            None,
            false,
            None,
            Format::Json,
        )
        .unwrap();
        let out = unwrap_success(result);
        let json: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(json["total"], 0);
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
            run_find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &[],
                None,
                &section_filters,
                &["notes.md".to_owned()],
                &[],
                &fields,
                None,
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
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
            run_find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &[],
                None,
                &section_filters,
                &["notes.md".to_owned()],
                &[],
                &fields,
                None,
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
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
            run_find(
                tmp.path(),
                None,
                Some("background"),
                None,
                &[],
                &[],
                None,
                &section_filters,
                &[],
                &[],
                &fields,
                None,
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
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
            run_find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &[],
                None,
                &section_filters,
                &["notes.md".to_owned()],
                &[],
                &fields,
                None,
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
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
            run_find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &[],
                None,
                &section_filters,
                &["empty.md".to_owned()],
                &[],
                &fields,
                None,
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
        // empty.md has no Tasks section — it is excluded entirely
        assert!(
            arr.is_empty(),
            "file with no matching section should be excluded"
        );
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
        let result = run_find(
            tmp.path(),
            None,
            None,
            None,
            &[],
            &[],
            None,
            &[],
            &[],
            &[],
            &fields,
            None,
            false,
            None,
            false,
            None,
            Format::Json,
        )
        .unwrap();
        let out = unwrap_success(result);
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
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
            run_find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &[],
                None,
                &[],
                &["alpha.md".to_owned()],
                &[],
                &fields,
                None,
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
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
            run_find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &[],
                None,
                &[],
                &["alpha.md".to_owned()],
                &[],
                &fields,
                None,
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
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
            run_find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &[],
                None,
                &[],
                &["alpha.md".to_owned()],
                &[],
                &fields,
                None,
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
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
            run_find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &[],
                None,
                &[],
                &["beta.md".to_owned()],
                &[],
                &fields,
                None,
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
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
            run_find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &[],
                None,
                &[],
                &[],
                &[],
                &fields,
                None,
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
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
            run_find(
                tmp.path(),
                None,
                None,
                None,
                &[],
                &[],
                None,
                &[],
                &["gamma.md".to_owned()],
                &[],
                &fields,
                None,
                false,
                None,
                false,
                None,
                Format::Json,
            )
            .unwrap(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let arr = parsed["results"].as_array().unwrap();
        let gamma = &arr[0];
        let backlinks = gamma["backlinks"].as_array().unwrap();
        assert!(backlinks.is_empty());
    }
}
