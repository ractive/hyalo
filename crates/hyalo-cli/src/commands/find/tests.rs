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
    let build = ScannedIndex::build(
        &file_pairs,
        site_prefix,
        &ScanOptions {
            scan_body: true,
            bm25_tokenize: false,
        },
    )?;
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
        None, // language
        None, // config_language
    )
}

macro_rules! md {
    ($s:expr) => {
        $s.strip_prefix('\n').unwrap_or($s)
    };
}

fn unwrap_success(outcome: CommandOutcome) -> String {
    match outcome {
        CommandOutcome::Success { output: s, .. } | CommandOutcome::RawOutput(s) => s,
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
    let arr = parsed.as_array().unwrap();
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
    let arr = parsed.as_array().unwrap();
    let files: Vec<&str> = arr.iter().map(|v| v["file"].as_str().unwrap()).collect();
    let mut sorted = files.clone();
    sorted.sort_unstable();
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
    let arr = parsed.as_array().unwrap();
    // alpha and beta have frontmatter title; gamma has a derived title
    // from its H1 heading — all three should match the existence filter.
    assert_eq!(arr.len(), 3);
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
    let arr = parsed.as_array().unwrap();
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
    let arr = parsed.as_array().unwrap();
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
    let arr = parsed.as_array().unwrap();
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
    let arr = parsed.as_array().unwrap();
    // BM25 search: beta has "Rust programming" in body; alpha has "rust" only in frontmatter
    // (frontmatter is not indexed for BM25 body search).
    // All results should have a relevance score.
    assert!(!arr.is_empty(), "should have at least one result");
    for entry in arr {
        let score = entry["score"].as_f64();
        assert!(score.is_some(), "BM25 result should have a score field");
        assert!(score.unwrap() > 0.0, "score should be positive");
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
        _ => panic!("expected user error for empty pattern"),
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
        _ => panic!("expected user error for whitespace-only pattern"),
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
    // Raw command output is a bare array; total is carried in CommandOutcome.
    let results = parsed.as_array().unwrap();
    assert_eq!(results.len(), 1);
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
    let arr = parsed.as_array().unwrap();
    let times: Vec<&str> = arr
        .iter()
        .map(|v| v["modified"].as_str().unwrap())
        .collect();
    let mut sorted = times.clone();
    sorted.sort_unstable();
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
    assert!(parsed.as_array().unwrap().is_empty());
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
    assert!(json.as_array().unwrap().is_empty());
}

// --- matches_task_filter ---

fn make_task(done: bool, status: char) -> FindTaskInfo {
    FindTaskInfo {
        line: 1,
        section: String::new(),
        status,
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
    let arr = parsed.as_array().unwrap();
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
    let arr = parsed.as_array().unwrap();
    let gamma = &arr[0];
    let backlinks = gamma["backlinks"].as_array().unwrap();
    assert!(backlinks.is_empty());
}

// --- find: content search with frontmatter-only index ---

/// Verifies that content search works correctly when the index was built with
/// `scan_body: false` (frontmatter only). This is the key scenario for the
/// optimisation that decouples content search from the body-scan predicate.
#[test]
fn content_search_works_with_frontmatter_only_index() {
    let tmp = setup_vault();
    let all = hyalo_core::discovery::discover_files(tmp.path()).unwrap();
    let file_pairs: Vec<(std::path::PathBuf, String)> = all
        .into_iter()
        .map(|p| {
            let rel = hyalo_core::discovery::relative_path(tmp.path(), &p);
            (p, rel)
        })
        .collect();
    let build = ScannedIndex::build(
        &file_pairs,
        None,
        &ScanOptions {
            scan_body: false,
            bm25_tokenize: false,
        },
    )
    .unwrap();

    let fields = Fields::default();
    let out = unwrap_success(
        find(
            &build.index,
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
            None,
            None,
        )
        .unwrap(),
    );
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    let arr = parsed.as_array().unwrap();
    assert_eq!(arr.len(), 1, "should find beta.md via BM25 content search");
    assert!(arr[0]["file"].as_str().unwrap().contains("beta"));
    // BM25 search produces a relevance score (no line-level matches)
    let score = arr[0]["score"].as_f64();
    assert!(
        score.is_some(),
        "BM25 result should include a relevance score"
    );
    assert!(score.unwrap() > 0.0, "score should be positive");
}
