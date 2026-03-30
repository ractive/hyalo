#![allow(clippy::missing_errors_doc)]

mod build;
mod filter_index;
mod sort;

pub use filter_index::{filter_index_entries, needs_body};

use anyhow::{Context, Result};
use std::path::Path;

use crate::output::{CommandOutcome, Format};
use hyalo_core::content_search::ContentSearchVisitor;
use hyalo_core::discovery;
use hyalo_core::filter::{self, Fields, FindTaskFilter, PropertyFilter, SortField};
use hyalo_core::heading::{SectionFilter, SectionRange, build_section_scope, in_scope};
use hyalo_core::index::VaultIndex;
use hyalo_core::link_graph::is_self_link;
use hyalo_core::types::{
    BacklinkInfo, ContentMatch, FileObject, FindTaskInfo, LinkInfo, OutlineSection, PropertyInfo,
};

use build::{TitleMatcher, extract_title, matches_task_filter};
use sort::{apply_sort, presort_index_entries};

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

    // Warn if globs matched 0 files and may redundantly include the --dir path
    crate::warn::warn_glob_dir_overlap(dir, globs, scoped_entries.len());

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

    // Pre-check: does any property filter target the "title" key?
    // If so, we may need to inject the derived title (from H1) into the
    // properties map for entries that lack a frontmatter `title`.
    let has_title_property_filter = property_filters.iter().any(|f| f.key() == Some("title"));

    let mut results: Vec<FileObject> = Vec::new();
    let mut total_matching: usize = 0;

    for entry in &scoped_entries {
        // --- Metadata filters using pre-indexed data ---
        // When a property filter targets "title" and the entry has no
        // frontmatter title (or a non-string title), inject the derived
        // title (from H1 heading) so that `--property 'title~=...'`
        // matches derived titles too.  The gate mirrors `extract_title()`
        // which only treats a frontmatter title as authoritative when it
        // is a String.
        let props_with_derived_title;
        let effective_props = if has_title_property_filter
            && !matches!(
                entry.properties.get("title"),
                Some(serde_json::Value::String(_))
            ) {
            let derived = extract_title(&entry.properties, Some(&entry.sections));
            if derived.is_null() {
                &entry.properties
            } else {
                props_with_derived_title = {
                    let mut p = entry.properties.clone();
                    p.insert("title".to_owned(), derived);
                    p
                };
                &props_with_derived_title
            }
        } else {
            &entry.properties
        };

        if !filter::matches_filters_with_tags(
            effective_props,
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
        if has_content_search
            && content_matches
                .as_ref()
                .is_some_and(std::vec::Vec::is_empty)
        {
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

    let json_output = serde_json::Value::Array(json_array);
    Ok(CommandOutcome::success_with_total(
        crate::output::format_success(format, &json_output),
        total as u64,
    ))
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
