#![allow(clippy::missing_errors_doc)]

mod build;
mod filter_index;
mod sort;

pub use filter_index::{filter_index_entries, needs_body};

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;

use crate::output::{CommandOutcome, Format};
use hyalo_core::bm25::{
    Bm25InvertedIndex, DocumentInput, PreTokenizedInput, create_stemmer, parse_language,
    resolve_language, tokenize_document,
};
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
    language: Option<&str>,
    config_language: Option<&str>,
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

    let has_bm25_search = pattern.is_some();
    let has_regex_search = regexp.is_some();
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

    // When BM25 search is active and no explicit sort is given, we will sort by
    // score after scoring. Pre-sorting by file still helps metadata-only filters
    // run in a stable order, but we cannot presort+limit before scoring.
    // Score sort also cannot use the pre-sort optimisation.
    let effective_sort = if has_bm25_search && sort.is_none() {
        Some(SortField::Score)
    } else {
        sort.cloned()
    };
    let effective_sort_ref = effective_sort.as_ref();

    // The index has all metadata, so we can pre-sort by any sort key
    // except BacklinksCount and Score (which need per-result data).
    // presorted=true even without --limit — pre-sorting is no more expensive
    // than post-sorting, and it simplifies the limit/total logic below.
    // BM25 search bypasses pre-sort optimisation entirely (all candidates must
    // be scored before any can be ranked).
    let presorted = !reverse
        && !matches!(
            effective_sort_ref,
            Some(SortField::BacklinksCount | SortField::Score)
        )
        && !has_bm25_search;

    let mut scoped_entries = scoped_entries;
    if presorted {
        presort_index_entries(&mut scoped_entries, effective_sort_ref, index.link_graph());
    }

    // Pre-check: does any property filter target the "title" key?
    // If so, we may need to inject the derived title (from H1) into the
    // properties map for entries that lack a frontmatter `title`.
    let has_title_property_filter = property_filters.iter().any(|f| f.key() == Some("title"));

    // ---------------------------------------------------------------------------
    // Phase 1 (BM25 path): collect metadata-passing candidates, then BM25-score.
    // ---------------------------------------------------------------------------

    // For BM25 search we run metadata filters first (no I/O), then do a single
    // I/O pass to read bodies and build the corpus.
    let bm25_score_map: Option<HashMap<String, f64>> = if has_bm25_search {
        let pat = pattern.unwrap(); // has_bm25_search == pattern.is_some()
        'bm25: {
            // Collect entries that pass all metadata filters.
            let mut candidates: Vec<usize> = Vec::new();
            for (idx, entry) in scoped_entries.iter().enumerate() {
                // Metadata filter (same logic as the main loop below)
                let has_missing_fm_title = has_title_property_filter
                    && !matches!(
                        entry.properties.get("title"),
                        Some(serde_json::Value::String(_))
                    );

                if has_missing_fm_title {
                    let derived = extract_title(&entry.properties, Some(&entry.sections));
                    let title_ok = property_filters
                        .iter()
                        .filter(|f| f.key() == Some("title"))
                        .all(|f| {
                            if derived.is_null() {
                                matches!(f, filter::PropertyFilter::Absent { .. })
                            } else {
                                f.matches_value(&derived)
                            }
                        });
                    if !title_ok {
                        continue;
                    }
                    let non_title_ok = property_filters
                        .iter()
                        .filter(|f| f.key() != Some("title"))
                        .all(|f| f.matches(&entry.properties));
                    if !non_title_ok {
                        continue;
                    }
                    if !tag_filters.is_empty()
                        && !tag_filters
                            .iter()
                            .all(|q| entry.tags.iter().any(|t| filter::tag_matches(t, q)))
                    {
                        continue;
                    }
                } else if !filter::matches_filters_with_tags(
                    &entry.properties,
                    property_filters,
                    &entry.tags,
                    tag_filters,
                ) {
                    continue;
                }

                // Title filter
                if let Some(ref matcher) = title_matcher {
                    let title_val = extract_title(&entry.properties, Some(&entry.sections));
                    if !matcher.matches(&title_val) {
                        continue;
                    }
                }

                // Section filter: check that at least one scope exists
                if has_section_filter {
                    let scope_ranges =
                        build_section_scope(&entry.sections, section_filters, usize::MAX);
                    if scope_ranges.is_empty() {
                        continue;
                    }
                }

                // Task filter
                if let Some(filter) = task_filter
                    && !matches_task_filter(&entry.tasks, filter)
                {
                    continue;
                }

                candidates.push(idx);
            }

            // Resolve query-time stemmer (language from CLI/config, no per-document override for
            // the query itself).
            let query_lang = resolve_language(None, language, config_language);
            let stemmer = create_stemmer(query_lang);

            // Build the candidate set (rel_path strings) for filtering persisted-index results.
            let candidate_paths: std::collections::HashSet<&str> = candidates
                .iter()
                .map(|&idx| scoped_entries[idx].rel_path.as_str())
                .collect();

            // Fastest path: use the persisted BM25 inverted index when available AND no section
            // filter is active. Score all docs, then intersect with metadata-passing candidates.
            if !has_section_filter && let Some(bm25_idx) = index.bm25_index() {
                let all_scored = bm25_idx.score(pat, &stemmer);
                let map: HashMap<String, f64> = all_scored
                    .into_iter()
                    .filter(|m| candidate_paths.contains(m.rel_path.as_str()))
                    .map(|m| (m.rel_path, m.score))
                    .collect();
                break 'bm25 Some(map);
            }

            // Build BM25 index from candidates.
            //
            // Fast path: when the index has pre-tokenized data and no section filter
            // is active (section filters require scoping to specific line ranges, which
            // is only possible with the raw body), use the stored tokens directly —
            // no disk I/O needed.
            //
            // Slow path: read each file from disk for entries that lack stored tokens
            // or when a section filter is active.
            let mut pre_tok_inputs: Vec<PreTokenizedInput> = Vec::new();
            let mut doc_inputs: Vec<DocumentInput> = Vec::new();

            for &idx in &candidates {
                let entry = &scoped_entries[idx];

                // Use pre-tokenized data only when:
                // 1. The index entry has stored tokens, AND
                // 2. No section filter is active (section filters require line-level body slicing), AND
                // 3. The cached language matches the effective language for this entry.
                if !has_section_filter && let Some(ref tokens) = entry.bm25_tokens {
                    let fm_lang = entry.properties.get("language").and_then(|v| v.as_str());
                    let effective_lang = resolve_language(fm_lang, language, config_language);
                    let cached_lang_matches = entry
                        .bm25_language
                        .as_deref()
                        .and_then(|s| parse_language(s).ok())
                        == Some(effective_lang);

                    if cached_lang_matches {
                        pre_tok_inputs.push(PreTokenizedInput {
                            rel_path: entry.rel_path.clone(),
                            tokens: tokens.clone(),
                        });
                        continue;
                    }
                    // Fall through to disk-read tokenization when language mismatches.
                }

                // Slow path: read the file from disk.
                let full_path = dir.join(&entry.rel_path);
                let file_content = match std::fs::read_to_string(&full_path) {
                    Ok(s) => s,
                    Err(e) => {
                        crate::warn::warn(format!("skipping {}: {e}", entry.rel_path));
                        continue;
                    }
                };
                // Feed only the body (after frontmatter) to BM25 so that frontmatter
                // YAML (including tag values) does not influence scoring.
                let raw_body = hyalo_core::frontmatter::body_only(&file_content);

                // When section filters are active, restrict the BM25 body to lines
                // that fall within the matching section scope. This preserves the
                // expectation that "pattern + --section X" only matches files where
                // the pattern appears inside section X, not elsewhere in the document.
                let body = if has_section_filter {
                    let scope_ranges =
                        build_section_scope(&entry.sections, section_filters, usize::MAX);
                    if scope_ranges.is_empty() {
                        // No matching section — this candidate should have been filtered
                        // in Phase 1, but guard here just in case.
                        continue;
                    }
                    // Index line numbers are 1-based relative to the full file (frontmatter + body).
                    // Count lines in the frontmatter prefix to offset body line numbers correctly.
                    let fm_prefix_len = file_content.len() - raw_body.len();
                    let fm_lines = file_content[..fm_prefix_len].lines().count();
                    raw_body
                        .lines()
                        .enumerate()
                        .filter_map(|(i, line)| {
                            // body line i corresponds to file line (fm_lines + i + 1) (1-based)
                            let file_line = fm_lines + i + 1;
                            if in_scope(&scope_ranges, file_line) {
                                Some(line)
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    raw_body.to_owned()
                };

                let title_val = extract_title(&entry.properties, Some(&entry.sections));
                let title_str = title_val.as_str().unwrap_or("").to_owned();
                let fm_lang = entry.properties.get("language").and_then(|v| v.as_str());
                let lang = resolve_language(fm_lang, language, config_language);
                doc_inputs.push(DocumentInput {
                    rel_path: entry.rel_path.clone(),
                    title: title_str,
                    body,
                    language: lang,
                });
            }

            // Score all documents. When the corpus is a mix of pre-tokenized and
            // freshly-read documents, we build two separate corpora and merge scores.
            // This is safe because BM25 scores are relative within a corpus — mixing
            // them directly would be incorrect. However, the common case is that ALL
            // candidates come from the same source (all pre-tokenized or all from disk),
            // so the mixed case only arises during a rolling index upgrade where some
            // entries were indexed before BM25 support was added.
            let scored = if doc_inputs.is_empty() {
                // All candidates were pre-tokenized — fast path.
                let corpus = Bm25InvertedIndex::build_from_tokens(pre_tok_inputs);
                corpus.score(pat, &stemmer)
            } else if pre_tok_inputs.is_empty() {
                // All candidates need file reads — original slow path.
                let corpus = Bm25InvertedIndex::build(doc_inputs);
                corpus.score(pat, &stemmer)
            } else {
                // Mixed: some candidates have pre-tokenized data from the index, others were
                // read from disk. Tokenize the disk-read entries and combine into a single
                // pre-tokenized corpus so all BM25 scores are computed on the same basis.
                let mut all_pre_tok = pre_tok_inputs;
                for doc in doc_inputs {
                    all_pre_tok.push(tokenize_document(doc));
                }
                let corpus = Bm25InvertedIndex::build_from_tokens(all_pre_tok);
                corpus.score(pat, &stemmer)
            };

            // Build a map from rel_path → score (only entries with score > 0).
            let map: HashMap<String, f64> =
                scored.into_iter().map(|m| (m.rel_path, m.score)).collect();
            Some(map)
        } // end 'bm25 block
    } else {
        None
    };

    let mut results: Vec<FileObject> = Vec::new();
    let mut total_matching: usize = 0;

    for entry in &scoped_entries {
        // ---------------------------------------------------------------------------
        // Phase 2 (BM25 path): only build FileObjects for scored entries.
        // ---------------------------------------------------------------------------
        let bm25_score: Option<f64> = if let Some(ref score_map) = bm25_score_map {
            match score_map.get(&entry.rel_path) {
                Some(&s) => Some(s),
                None => continue, // not in score map → no BM25 match
            }
        } else {
            None
        };

        // --- Metadata filters (skipped for BM25 path — already done in Phase 1) ---
        if bm25_score_map.is_none() {
            let has_missing_fm_title = has_title_property_filter
                && !matches!(
                    entry.properties.get("title"),
                    Some(serde_json::Value::String(_))
                );

            if has_missing_fm_title {
                let derived = extract_title(&entry.properties, Some(&entry.sections));
                let title_ok = property_filters
                    .iter()
                    .filter(|f| f.key() == Some("title"))
                    .all(|f| {
                        if derived.is_null() {
                            matches!(f, filter::PropertyFilter::Absent { .. })
                        } else {
                            f.matches_value(&derived)
                        }
                    });
                if !title_ok {
                    continue;
                }
                let non_title_ok = property_filters
                    .iter()
                    .filter(|f| f.key() != Some("title"))
                    .all(|f| f.matches(&entry.properties));
                if !non_title_ok {
                    continue;
                }
                if !tag_filters.is_empty()
                    && !tag_filters
                        .iter()
                        .all(|q| entry.tags.iter().any(|t| filter::tag_matches(t, q)))
                {
                    continue;
                }
            } else if !filter::matches_filters_with_tags(
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

        // --- Task filter using pre-indexed tasks (skipped for BM25 — done in Phase 1) ---
        let mut collected_tasks: Option<Vec<FindTaskInfo>> = if fields.tasks || has_task_filter {
            let mut tasks = entry.tasks.clone();
            if has_section_filter {
                tasks.retain(|t| in_scope(&scope_ranges, t.line));
            }
            Some(tasks)
        } else {
            None
        };

        if bm25_score_map.is_none()
            && let Some(filter) = task_filter
        {
            let tasks_slice: &[FindTaskInfo] = collected_tasks.as_deref().unwrap_or(&[]);
            if !matches_task_filter(tasks_slice, filter) {
                continue;
            }
        }

        // --- Content search: regex path only (BM25 is handled via score_map) ---
        let content_matches: Option<Vec<ContentMatch>> = if has_regex_search {
            let full_path = dir.join(&entry.rel_path);
            let re = compiled_regex.as_ref().unwrap(); // has_regex_search == compiled_regex.is_some()

            let mut content_visitor = ContentSearchVisitor::from_compiled(re.clone());
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

        // Filter: regex content search must have at least one match
        if has_regex_search
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
            score: bm25_score,
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
        apply_sort(&mut results, effective_sort_ref, link_graph_ref);
    }

    if let Some(SortField::Property(key)) = effective_sort_ref
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
