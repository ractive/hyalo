mod common;

use common::{hyalo, md, write_md};

// ---------------------------------------------------------------------------
// Vault fixture
// ---------------------------------------------------------------------------

fn setup_vault() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();

    // alpha.md — has tasks, links, rust+cli tags, priority, status=planned
    write_md(
        tmp.path(),
        "alpha.md",
        md!(r"
---
title: Alpha
status: planned
priority: 3
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
    );

    // beta.md — status=completed, rust tag, content mentioning Rust programming
    write_md(
        tmp.path(),
        "beta.md",
        md!(r"
---
title: Beta
status: completed
tags:
  - rust
---
# Beta Content

Rust programming is great.
"),
    );

    // gamma.md — no frontmatter at all
    write_md(
        tmp.path(),
        "gamma.md",
        md!(r"
# Gamma

Just some body text here.
"),
    );

    // sub/nested.md — status=planned, nested tag
    write_md(
        tmp.path(),
        "sub/nested.md",
        md!(r"
---
title: Nested
status: planned
tags:
  - rust
  - project/backend
---
# Nested Content

Some nested content.
"),
    );

    tmp
}

// ---------------------------------------------------------------------------
// Helper: run find and parse stdout as JSON array
// ---------------------------------------------------------------------------

fn find_json(
    tmp: &tempfile::TempDir,
    extra_args: &[&str],
) -> (std::process::ExitStatus, serde_json::Value, String) {
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.arg("find");
    cmd.args(extra_args);
    let output = cmd.output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let json: serde_json::Value = if output.status.success() {
        serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
            let stdout = String::from_utf8_lossy(&output.stdout);
            panic!("invalid JSON: {e}\nstdout: {stdout}\nstderr: {stderr}")
        })
    } else {
        serde_json::Value::Null
    };
    (output.status, json, stderr)
}

// ---------------------------------------------------------------------------
// Basic: no args, all files
// ---------------------------------------------------------------------------

#[test]
fn find_all_files_returns_sorted_array() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(&tmp, &[]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 4, "expected 4 files, got: {arr:?}");

    // Verify sorted by file path
    let files: Vec<&str> = arr
        .iter()
        .map(|v| v["file"].as_str().expect("field 'file' should be a string"))
        .collect();
    let mut sorted = files.clone();
    sorted.sort();
    assert_eq!(files, sorted, "results not sorted by file path");
}

#[test]
fn find_all_files_have_required_fields() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(&tmp, &[]);
    assert!(status.success(), "stderr: {stderr}");

    for entry in json.as_array().unwrap() {
        assert!(entry["file"].is_string(), "missing file field in {entry}");
        let modified = entry["modified"]
            .as_str()
            .expect("field 'modified' should be a string");
        // ISO 8601: YYYY-MM-DDTHH:MM:SSZ = 20 chars
        assert_eq!(modified.len(), 20, "unexpected modified format: {modified}");
        assert!(modified.ends_with('Z'));
    }
}

// ---------------------------------------------------------------------------
// Basic: --file (still returns array)
// ---------------------------------------------------------------------------

#[test]
fn find_single_file_returns_array_not_object() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(&tmp, &["--file", "alpha.md"]);
    assert!(status.success(), "stderr: {stderr}");

    // Must be an array, not a bare object
    assert!(json.is_array(), "expected array, got: {json}");
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["file"], "alpha.md");
}

// ---------------------------------------------------------------------------
// Basic: --glob
// ---------------------------------------------------------------------------

#[test]
fn find_glob_sub_only() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(&tmp, &["--glob", "sub/*.md"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["file"], "sub/nested.md");
}

// ---------------------------------------------------------------------------
// Basic: --file not found
// ---------------------------------------------------------------------------

#[test]
fn find_file_not_found_exits_1() {
    let tmp = setup_vault();
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["find", "--file", "does_not_exist.md"]);
    let output = cmd.output().unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

// ---------------------------------------------------------------------------
// Property filters
// ---------------------------------------------------------------------------

#[test]
fn find_property_eq_status_planned() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(&tmp, &["--property", "status=planned"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    // alpha (planned) + nested (planned) = 2
    assert_eq!(
        arr.len(),
        2,
        "expected 2 files with status=planned: {arr:?}"
    );
    let files: Vec<&str> = arr
        .iter()
        .map(|v| v["file"].as_str().expect("field 'file' should be a string"))
        .collect();
    assert!(files.contains(&"alpha.md"));
    assert!(files.contains(&"sub/nested.md"));
}

#[test]
fn find_property_neq_status_completed() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(&tmp, &["--property", "status!=completed"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    // gamma has no status → filter returns false for !=; alpha+nested have status!=completed → 2 files
    assert_eq!(arr.len(), 2, "expected 2 files (alpha, nested): {arr:?}");
    let files: Vec<&str> = arr
        .iter()
        .map(|v| v["file"].as_str().expect("field 'file' should be a string"))
        .collect();
    assert!(files.contains(&"alpha.md"));
    assert!(files.contains(&"sub/nested.md"));
}

#[test]
fn find_property_existence_status() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(&tmp, &["--property", "status"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    // alpha, beta, nested have status; gamma does not
    assert_eq!(arr.len(), 3, "expected 3 files with status: {arr:?}");
    let files: Vec<&str> = arr
        .iter()
        .map(|v| v["file"].as_str().expect("field 'file' should be a string"))
        .collect();
    assert!(files.contains(&"alpha.md"));
    assert!(files.contains(&"beta.md"));
    assert!(files.contains(&"sub/nested.md"));
}

#[test]
fn find_property_gte_priority() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(&tmp, &["--property", "priority>=3"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    // Only alpha has priority: 3
    assert_eq!(arr.len(), 1, "expected 1 file with priority>=3: {arr:?}");
    assert_eq!(arr[0]["file"], "alpha.md");
}

#[test]
fn find_property_and_semantics() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(
        &tmp,
        &["--property", "status=planned", "--property", "priority>=3"],
    );
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    // Only alpha satisfies both: status=planned AND priority>=3
    assert_eq!(arr.len(), 1, "expected 1 file: {arr:?}");
    assert_eq!(arr[0]["file"], "alpha.md");
}

// ---------------------------------------------------------------------------
// Tag filters
// ---------------------------------------------------------------------------

#[test]
fn find_tag_rust_matches_three_files() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(&tmp, &["--tag", "rust"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    // alpha, beta, nested all have rust tag
    assert_eq!(arr.len(), 3, "expected 3 files with rust tag: {arr:?}");
}

#[test]
fn find_tag_cli_matches_only_alpha() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(&tmp, &["--tag", "cli"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1, "expected 1 file with cli tag: {arr:?}");
    assert_eq!(arr[0]["file"], "alpha.md");
}

#[test]
fn find_tag_project_matches_nested_tag() {
    let tmp = setup_vault();
    // "project" should match "project/backend" in nested.md
    let (status, json, stderr) = find_json(&tmp, &["--tag", "project"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(
        arr.len(),
        1,
        "expected 1 file matching tag 'project': {arr:?}"
    );
    assert_eq!(arr[0]["file"], "sub/nested.md");
}

#[test]
fn find_tag_and_semantics() {
    let tmp = setup_vault();
    // rust AND cli → only alpha has both
    let (status, json, stderr) = find_json(&tmp, &["--tag", "rust", "--tag", "cli"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1, "expected 1 file with rust+cli tags: {arr:?}");
    assert_eq!(arr[0]["file"], "alpha.md");
}

// ---------------------------------------------------------------------------
// Content search (pattern)
// ---------------------------------------------------------------------------

#[test]
fn find_pattern_rust_programming_matches_beta() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(&tmp, &["Rust programming"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1, "expected 1 file: {arr:?}");
    assert_eq!(arr[0]["file"], "beta.md");
}

#[test]
fn find_pattern_write_matches_alpha() {
    let tmp = setup_vault();
    // alpha body has "Write tests" and "Write code"
    let (status, json, stderr) = find_json(&tmp, &["Write"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1, "expected 1 file: {arr:?}");
    assert_eq!(arr[0]["file"], "alpha.md");
}

#[test]
fn find_pattern_no_match_returns_empty_array() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(&tmp, &["nonexistent_phrase_xyz"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert!(arr.is_empty(), "expected empty array: {arr:?}");
}

#[test]
fn find_pattern_includes_matches_field() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(&tmp, &["Rust programming"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert!(
        arr[0]["matches"].is_array(),
        "matches field should be present when pattern given"
    );
}

#[test]
fn find_no_pattern_no_matches_field() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(&tmp, &[]);
    assert!(status.success(), "stderr: {stderr}");

    for entry in json.as_array().unwrap() {
        assert!(
            entry["matches"].is_null(),
            "matches field should be absent without pattern, got: {}",
            entry["matches"]
        );
    }
}

// ---------------------------------------------------------------------------
// Task filter
// ---------------------------------------------------------------------------

#[test]
fn find_task_todo_matches_alpha() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(&tmp, &["--task", "todo"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    // Only alpha has an open task
    assert_eq!(arr.len(), 1, "expected 1 file with todo task: {arr:?}");
    assert_eq!(arr[0]["file"], "alpha.md");
}

#[test]
fn find_task_done_matches_alpha() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(&tmp, &["--task", "done"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    // Only alpha has a completed task
    assert_eq!(arr.len(), 1, "expected 1 file with done task: {arr:?}");
    assert_eq!(arr[0]["file"], "alpha.md");
}

#[test]
fn find_task_any_matches_only_files_with_tasks() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(&tmp, &["--task", "any"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    // Only alpha has tasks
    assert_eq!(arr.len(), 1, "expected 1 file with any tasks: {arr:?}");
    assert_eq!(arr[0]["file"], "alpha.md");
}

// ---------------------------------------------------------------------------
// Combined filters
// ---------------------------------------------------------------------------

#[test]
fn find_tag_and_property_combined() {
    let tmp = setup_vault();
    // rust tag AND status=planned → alpha and nested
    let (status, json, stderr) =
        find_json(&tmp, &["--tag", "rust", "--property", "status=planned"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 2, "expected 2 files: {arr:?}");
    let files: Vec<&str> = arr
        .iter()
        .map(|v| v["file"].as_str().expect("field 'file' should be a string"))
        .collect();
    assert!(files.contains(&"alpha.md"));
    assert!(files.contains(&"sub/nested.md"));
}

#[test]
fn find_pattern_and_tag_combined() {
    let tmp = setup_vault();
    // "Write" in body AND rust tag → only alpha
    let (status, json, stderr) = find_json(&tmp, &["Write", "--tag", "rust"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1, "expected 1 file: {arr:?}");
    assert_eq!(arr[0]["file"], "alpha.md");
}

#[test]
fn find_all_four_filters_combined() {
    let tmp = setup_vault();
    // All four filter types AND'd together:
    //   --property status=planned  → alpha, sub/nested
    //   --tag rust                 → alpha, beta, sub/nested
    //   --task todo                → alpha only (only file with open tasks)
    //   -e "Write"                 → alpha only (body contains "Write tests" / "Write code")
    // Only alpha satisfies all four simultaneously.
    let (status, json, stderr) = find_json(
        &tmp,
        &[
            "--property",
            "status=planned",
            "--tag",
            "rust",
            "--task",
            "todo",
            "-e",
            "Write",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(
        arr.len(),
        1,
        "expected exactly 1 file matching all four filters: {arr:?}"
    );
    assert_eq!(arr[0]["file"], "alpha.md");
}

// ---------------------------------------------------------------------------
// Fields
// ---------------------------------------------------------------------------

#[test]
fn find_fields_properties_and_tags_only() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(&tmp, &["--fields", "properties,tags"]);
    assert!(status.success(), "stderr: {stderr}");

    for entry in json.as_array().unwrap() {
        assert!(
            entry["properties"].is_object(),
            "properties should be present"
        );
        assert!(entry["tags"].is_array(), "tags should be present");
        assert!(entry["sections"].is_null(), "sections should be absent");
        assert!(entry["tasks"].is_null(), "tasks should be absent");
        assert!(entry["links"].is_null(), "links should be absent");
        assert!(entry["backlinks"].is_null(), "backlinks should be absent");
    }
}

#[test]
fn find_fields_tasks_only() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(&tmp, &["--fields", "tasks"]);
    assert!(status.success(), "stderr: {stderr}");

    for entry in json.as_array().unwrap() {
        // tasks field present (may be null for files without tasks, but the key exists)
        assert!(entry["properties"].is_null(), "properties should be absent");
        assert!(entry["tags"].is_null(), "tags should be absent");
        assert!(entry["sections"].is_null(), "sections should be absent");
        assert!(entry["links"].is_null(), "links should be absent");
    }

    // alpha specifically should have tasks populated
    let arr = json.as_array().unwrap();
    let alpha = arr
        .iter()
        .find(|e| e["file"].as_str().expect("field 'file' should be a string") == "alpha.md")
        .expect("alpha.md should be present in results");
    assert!(
        alpha["tasks"].is_array(),
        "alpha should have tasks array when tasks field requested"
    );
}

#[test]
fn find_fields_backlinks_shows_incoming_links() {
    let tmp = setup_vault();
    // alpha.md links to [[beta]], so beta should have a backlink from alpha
    let (status, json, stderr) = find_json(&tmp, &["--fields", "backlinks", "--file", "beta.md"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    let beta = &arr[0];
    let backlinks = beta["backlinks"]
        .as_array()
        .expect("backlinks should be an array");
    assert_eq!(backlinks.len(), 1, "beta should have 1 backlink from alpha");
    assert_eq!(backlinks[0]["source"], "alpha.md");
    assert!(backlinks[0]["line"].as_u64().unwrap() > 0);
}

#[test]
fn find_fields_backlinks_not_included_by_default() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(&tmp, &[]);
    assert!(status.success(), "stderr: {stderr}");

    for entry in json.as_array().unwrap() {
        assert!(
            !entry.as_object().unwrap().contains_key("backlinks"),
            "backlinks key should be absent by default, not just null"
        );
    }
}

// ---------------------------------------------------------------------------
// Sort
// ---------------------------------------------------------------------------

#[test]
fn find_sort_modified() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(&tmp, &["--sort", "modified"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 4);

    let times: Vec<&str> = arr
        .iter()
        .map(|v| {
            v["modified"]
                .as_str()
                .expect("field 'modified' should be a string")
        })
        .collect();
    let mut sorted = times.clone();
    sorted.sort();
    assert_eq!(times, sorted, "results not sorted by modified time");
}

#[test]
fn find_sort_file_default() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(&tmp, &["--sort", "file"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    let files: Vec<&str> = arr
        .iter()
        .map(|v| v["file"].as_str().expect("field 'file' should be a string"))
        .collect();
    let mut sorted = files.clone();
    sorted.sort();
    assert_eq!(files, sorted);
}

// ---------------------------------------------------------------------------
// Limit
// ---------------------------------------------------------------------------

#[test]
fn find_limit_2_returns_2_results() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(&tmp, &["--limit", "2"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 2, "expected exactly 2 results with --limit 2");
}

#[test]
fn find_limit_larger_than_results() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(&tmp, &["--limit", "100"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    // All 4 files returned; limit is a ceiling not a floor
    assert_eq!(arr.len(), 4);
}

// ---------------------------------------------------------------------------
// Text format
// ---------------------------------------------------------------------------

#[test]
fn find_text_format_produces_output() {
    let tmp = setup_vault();
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["--format", "text"]);
    cmd.arg("find");
    let output = cmd.output().unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.trim().is_empty(), "expected non-empty text output");
}

#[test]
fn find_text_format_with_pattern() {
    let tmp = setup_vault();
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["--format", "text"]);
    cmd.args(["find", "Rust programming"]);
    let output = cmd.output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("beta.md"),
        "expected beta.md in text output: {stdout}"
    );
}

// Text format: FileObject renders structured output with properties, tags, sections
#[test]
fn find_text_format_file_object_structure() {
    let tmp = setup_vault();
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["--format", "text"]);
    cmd.args(["find", "--file", "alpha.md"]);
    let output = cmd.output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    // File path as header
    assert!(stdout.contains("alpha.md"), "file path header: {stdout}");
    // Group labels
    assert!(
        stdout.contains("properties:"),
        "properties group label: {stdout}"
    );
    assert!(
        stdout.contains("sections:"),
        "sections group label: {stdout}"
    );
    // Properties as key: value (no type annotation in map format)
    assert!(
        stdout.contains("title: Alpha"),
        "property rendering: {stdout}"
    );
    assert!(
        stdout.contains("status: planned"),
        "status property: {stdout}"
    );
    // Tags are shown as a dedicated field, not duplicated under properties
    // (tags key exists in the map but should be excluded from properties section)
    assert!(
        !stdout.contains("    tags:"),
        "tags should not appear under properties: {stdout}"
    );
    assert!(stdout.contains("tags: [rust, cli]"), "tags line: {stdout}");
    // Section headings
    assert!(stdout.contains("# Introduction"), "h1 section: {stdout}");
    assert!(stdout.contains("## Tasks"), "h2 section: {stdout}");
    // Tasks with checkbox notation
    assert!(
        stdout.contains("[ ] Write tests"),
        "todo task checkbox: {stdout}"
    );
    assert!(
        stdout.contains("[x] Write code"),
        "done task checkbox: {stdout}"
    );
}

// Text format: content search shows matches with line numbers
#[test]
fn find_text_format_content_matches() {
    let tmp = setup_vault();
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["--format", "text"]);
    cmd.args(["find", "Rust programming"]);
    let output = cmd.output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(stdout.contains("beta.md"), "file header: {stdout}");
    assert!(stdout.contains("matches:"), "matches header: {stdout}");
    assert!(
        stdout.contains("Rust programming is great"),
        "match text: {stdout}"
    );
}

// Text format: multiple FileObjects separated by blank lines
#[test]
fn find_text_format_multi_file_separation() {
    let tmp = setup_vault();
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["--format", "text"]);
    cmd.arg("find");
    let output = cmd.output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Multiple files should be separated by blank lines
    assert!(
        stdout.contains("\n\n"),
        "expected blank line between file entries: {stdout}"
    );
}

// Text format: find with --fields properties only shows properties
#[test]
fn find_text_format_fields_properties_only() {
    let tmp = setup_vault();
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["--format", "text"]);
    cmd.args(["find", "--file", "alpha.md", "--fields", "properties"]);
    let output = cmd.output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should have properties with group label
    assert!(
        stdout.contains("properties:"),
        "properties group label: {stdout}"
    );
    assert!(
        stdout.contains("title: Alpha"),
        "property present: {stdout}"
    );
    // Should NOT have sections (not requested)
    assert!(
        !stdout.contains("# Introduction"),
        "sections should be absent: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Regexp search (--regexp / -e)
// ---------------------------------------------------------------------------

#[test]
fn find_regexp_alternation_matches_multiple_files() {
    let tmp = setup_vault();
    // "programming|body" should match beta (has "programming") and gamma (has "body")
    let (status, json, stderr) = find_json(&tmp, &["--regexp", "programming|body"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 2, "expected 2 files: {arr:?}");
    let files: Vec<&str> = arr
        .iter()
        .map(|v| v["file"].as_str().expect("field 'file' should be a string"))
        .collect();
    assert!(files.contains(&"beta.md"));
    assert!(files.contains(&"gamma.md"));
}

#[test]
fn find_regexp_short_flag_e_works() {
    let tmp = setup_vault();
    // Use lowercase to verify -e applies case-insensitive matching by default
    let (status, json, stderr) = find_json(&tmp, &["-e", "rust.*great"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1, "expected 1 file: {arr:?}");
    assert_eq!(arr[0]["file"], "beta.md");
}

#[test]
fn find_regexp_case_insensitive_by_default() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(&tmp, &["--regexp", "rust PROGRAMMING"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1, "expected 1 file: {arr:?}");
    assert_eq!(arr[0]["file"], "beta.md");
}

#[test]
fn find_regexp_includes_matches_field() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(&tmp, &["-e", "programming"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert!(
        arr[0]["matches"].is_array(),
        "matches field should be present with --regexp"
    );
    let matches = arr[0]["matches"]
        .as_array()
        .expect("field 'matches' should be an array");
    assert!(!matches.is_empty(), "should have at least one match");
}

#[test]
fn find_regexp_no_match_returns_empty_array() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(&tmp, &["--regexp", r"\d{4}-\d{2}-\d{2}"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert!(arr.is_empty(), "expected empty array: {arr:?}");
}

#[test]
fn find_regexp_combined_with_tag() {
    let tmp = setup_vault();
    // regex match + tag filter
    let (status, json, stderr) = find_json(&tmp, &["-e", "content", "--tag", "rust"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    // beta has "Content" in heading + rust tag; nested has "content" in body text + rust tag
    assert_eq!(arr.len(), 2, "expected 2 files: {arr:?}");
}

#[test]
fn find_regexp_invalid_exits_1() {
    let tmp = setup_vault();
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["find", "--regexp", "[invalid"]);
    let output = cmd.output().unwrap();

    assert!(!output.status.success(), "should fail on invalid regex");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid regular expression"),
        "expected error message about invalid regex, got: {stderr}"
    );
}

#[test]
fn find_regexp_conflicts_with_positional_pattern() {
    let tmp = setup_vault();
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["find", "substring", "--regexp", "regex"]);
    let output = cmd.output().unwrap();

    assert!(
        !output.status.success(),
        "should fail when both positional and --regexp are given"
    );
}

// ---------------------------------------------------------------------------
// Path traversal: dotdot in filename
// ---------------------------------------------------------------------------

#[test]
fn find_file_with_dotdot_in_name_succeeds() {
    let tmp = tempfile::tempdir().unwrap();
    write_md(
        tmp.path(),
        "etc..md",
        md!(r"
---
title: Dotdot
---
# Dotdot file
"),
    );

    let (status, json, stderr) = find_json(&tmp, &["--file", "etc..md"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1, "expected 1 result: {arr:?}");
    assert_eq!(arr[0]["file"], "etc..md");
}

// ---------------------------------------------------------------------------
// Error cases
// ---------------------------------------------------------------------------

#[test]
fn find_task_invalid_multi_char_exits_1() {
    let tmp = setup_vault();
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["find", "--task", "invalid_multi_char"]);
    let output = cmd.output().unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn find_fields_bogus_exits_1() {
    let tmp = setup_vault();
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["find", "--fields", "bogus"]);
    let output = cmd.output().unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn find_sort_bogus_exits_1() {
    let tmp = setup_vault();
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["find", "--sort", "bogus"]);
    let output = cmd.output().unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn find_glob_no_match_exits_1() {
    let tmp = setup_vault();
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["find", "--glob", "nonexistent/**/*.md"]);
    let output = cmd.output().unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

// ---------------------------------------------------------------------------
// --section filter tests
// ---------------------------------------------------------------------------

fn setup_section_vault() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();

    // doc.md — multiple sections, nested subsections, tasks in different sections
    write_md(
        tmp.path(),
        "doc.md",
        md!(r"
---
title: Doc
tags:
  - test
---
# Introduction

Intro text here.

## Tasks

- [ ] First task
- [x] Second task

### Subtasks

- [ ] Nested task

## Notes

Some notes with a TODO marker.

- [ ] Note task
"),
    );

    // other.md — has a Tasks section too (tests multi-file matching)
    // Also has a ## Introduction (level-2) to test level-pinning against doc.md's # Introduction (level-1)
    write_md(
        tmp.path(),
        "other.md",
        md!(r"
---
title: Other
---
# Overview

Overview text.

## Introduction

A level-2 introduction section.

## Tasks

- [ ] Other task
- [x] Done task

## Design

Design details with TODO items.
"),
    );

    tmp
}

#[test]
fn section_filter_scopes_tasks() {
    let tmp = setup_section_vault();
    let (status, json, _) = find_json(
        &tmp,
        &["--section", "Tasks", "--task", "todo", "--fields", "tasks"],
    );
    assert!(status.success());
    let arr = json.as_array().unwrap();
    // Both files have ## Tasks sections with open tasks
    assert_eq!(arr.len(), 2);
    for entry in arr {
        let tasks = entry["tasks"]
            .as_array()
            .expect("field 'tasks' should be an array");
        for task in tasks {
            // All returned tasks must be in a section that starts with Tasks-related headings
            let section = task["section"]
                .as_str()
                .expect("field 'section' should be a string");
            assert!(
                section.contains("Tasks") || section.contains("Subtasks"),
                "unexpected section: {section}"
            );
        }
    }
}

#[test]
fn section_filter_includes_nested_children() {
    let tmp = setup_section_vault();
    let (status, json, _) = find_json(
        &tmp,
        &[
            "--section",
            "Tasks",
            "--task",
            "any",
            "--fields",
            "tasks",
            "--file",
            "doc.md",
        ],
    );
    assert!(status.success());
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    let tasks = arr[0]["tasks"]
        .as_array()
        .expect("field 'tasks' should be an array");
    // Should include: First task, Second task (## Tasks), Nested task (### Subtasks)
    // Should NOT include: Note task (## Notes)
    assert_eq!(tasks.len(), 3);
    let texts: Vec<&str> = tasks
        .iter()
        .map(|t| t["text"].as_str().expect("field 'text' should be a string"))
        .collect();
    assert!(texts.contains(&"First task"));
    assert!(texts.contains(&"Second task"));
    assert!(texts.contains(&"Nested task"));
}

#[test]
fn section_filter_nearest_heading_in_output() {
    let tmp = setup_section_vault();
    let (status, json, _) = find_json(
        &tmp,
        &[
            "--section",
            "Tasks",
            "--task",
            "any",
            "--fields",
            "tasks",
            "--file",
            "doc.md",
        ],
    );
    assert!(status.success());
    let tasks = json.as_array().unwrap()[0]["tasks"]
        .as_array()
        .expect("field 'tasks' should be an array");
    // The nested task should show "### Subtasks" as its section, not "## Tasks"
    let nested = tasks
        .iter()
        .find(|t| t["text"].as_str().expect("field 'text' should be a string") == "Nested task")
        .expect("task 'Nested task' should be present");
    assert_eq!(
        nested["section"]
            .as_str()
            .expect("field 'section' should be a string"),
        "### Subtasks"
    );
}

#[test]
fn section_filter_case_insensitive() {
    let tmp = setup_section_vault();
    let (status, json, _) = find_json(
        &tmp,
        &["--section", "tasks", "--task", "any", "--fields", "tasks"],
    );
    assert!(status.success());
    let arr = json.as_array().unwrap();
    // Should still match ## Tasks sections
    assert_eq!(arr.len(), 2);
}

#[test]
fn section_filter_level_pinned() {
    let tmp = setup_section_vault();
    // doc.md has "# Introduction" (level 1); other.md has "## Introduction" (level 2).
    // Using "# Introduction" should match only doc.md (level-pinned to 1) and exclude other.md.
    let (status, json, _) = find_json(
        &tmp,
        &["--section", "# Introduction", "--fields", "sections"],
    );
    assert!(status.success());
    let arr = json.as_array().unwrap();
    // Only doc.md should be returned — other.md's ## Introduction is level 2, not level 1
    assert_eq!(
        arr.len(),
        1,
        "only doc.md should match a level-1 Introduction filter"
    );
    assert_eq!(
        arr[0]["file"]
            .as_str()
            .expect("field 'file' should be a string"),
        "doc.md"
    );
}

#[test]
fn section_filter_or_semantics() {
    let tmp = setup_section_vault();
    let (status, json, _) = find_json(
        &tmp,
        &[
            "--section",
            "Tasks",
            "--section",
            "Notes",
            "--task",
            "any",
            "--fields",
            "tasks",
            "--file",
            "doc.md",
        ],
    );
    assert!(status.success());
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    let tasks = arr[0]["tasks"]
        .as_array()
        .expect("field 'tasks' should be an array");
    // Should include tasks from both Tasks and Notes sections
    let texts: Vec<&str> = tasks
        .iter()
        .map(|t| t["text"].as_str().expect("field 'text' should be a string"))
        .collect();
    assert!(texts.contains(&"First task"));
    assert!(texts.contains(&"Note task"));
}

#[test]
fn section_filter_no_match_excludes_file() {
    let tmp = setup_section_vault();
    let (status, json, _) = find_json(&tmp, &["--section", "Nonexistent", "--task", "any"]);
    assert!(status.success());
    let arr = json.as_array().unwrap();
    // No files should match since no section named "Nonexistent" exists
    assert!(arr.is_empty());
}

#[test]
fn section_filter_content_search_scoped() {
    let tmp = setup_section_vault();
    let (status, json, _) = find_json(&tmp, &["--section", "Notes", "TODO", "--file", "doc.md"]);
    assert!(status.success());
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    let matches = arr[0]["matches"]
        .as_array()
        .expect("field 'matches' should be an array");
    // The TODO in Notes section should be found
    assert_eq!(matches.len(), 1);
    assert_eq!(
        matches[0]["section"]
            .as_str()
            .expect("field 'section' should be a string"),
        "## Notes"
    );
}

#[test]
fn section_filter_content_search_excludes_other_sections() {
    let tmp = setup_section_vault();
    // "TODO" appears in both Notes and Design, but --section "Notes" should only find it in Notes
    let (status, json, _) = find_json(&tmp, &["--section", "Notes", "TODO", "--file", "other.md"]);
    assert!(status.success());
    let arr = json.as_array().unwrap();
    // other.md has no ## Notes section, so it shouldn't match
    assert!(arr.is_empty());
}

#[test]
fn section_filter_invalid_exits_1() {
    let tmp = setup_section_vault();
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["find", "--section", "####### Too deep"]);
    let output = cmd.output().unwrap();
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn empty_text_result_prints_notice_on_stderr() {
    let tmp = setup_vault();
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args([
        "find",
        "--property",
        "status=nonexistent",
        "--format",
        "text",
    ]);
    let output = cmd.output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stdout.trim().is_empty(),
        "stdout should be empty for zero results, got: {stdout:?}"
    );
    assert!(
        stderr.contains("No files matched"),
        "stderr should contain 'No files matched' notice, got: {stderr}"
    );
}

#[test]
fn empty_json_result_returns_empty_array_no_stderr() {
    let tmp = setup_vault();
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args([
        "find",
        "--property",
        "status=nonexistent",
        "--format",
        "json",
    ]);
    let output = cmd.output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(stdout.trim(), "[]");
    assert!(
        !stderr.contains("No files matched"),
        "JSON mode should not print 'No files matched' notice"
    );
}

// ---------------------------------------------------------------------------
// --fields properties-typed
// ---------------------------------------------------------------------------

#[test]
fn find_fields_properties_typed_json() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(
        &tmp,
        &["--file", "alpha.md", "--fields", "properties-typed"],
    );
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    let entry = &arr[0];

    // properties_typed must be an array
    let typed = entry["properties_typed"]
        .as_array()
        .expect("properties_typed should be an array");
    assert!(!typed.is_empty());

    // Each element has name, type, value keys
    for item in typed {
        assert!(item["name"].is_string());
        assert!(item["type"].is_string());
        assert!(!item["value"].is_null());
    }

    // tags should not appear in properties_typed
    assert!(
        typed.iter().all(|p| p["name"] != "tags"),
        "tags must not appear in properties_typed"
    );

    // properties (map) should not be present when not requested
    assert!(
        entry["properties"].is_null(),
        "properties map should be absent when only properties-typed was requested"
    );
}

#[test]
fn find_fields_properties_typed_type_and_value() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(
        &tmp,
        &["--file", "alpha.md", "--fields", "properties-typed"],
    );
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    let typed = arr[0]["properties_typed"].as_array().unwrap();

    // alpha.md has: title: Alpha, status: planned, priority: 3
    let status_prop = typed
        .iter()
        .find(|p| p["name"] == "status")
        .expect("status property missing");
    assert_eq!(status_prop["type"], "text");
    assert_eq!(status_prop["value"], "planned");

    let priority_prop = typed
        .iter()
        .find(|p| p["name"] == "priority")
        .expect("priority property missing");
    assert_eq!(priority_prop["type"], "number");
    assert_eq!(priority_prop["value"], 3);
}

#[test]
fn find_fields_properties_and_properties_typed_together_e2e() {
    let tmp = setup_vault();
    let (status, json, stderr) = find_json(
        &tmp,
        &[
            "--file",
            "alpha.md",
            "--fields",
            "properties,properties-typed",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    let entry = &arr[0];

    // Both fields present
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
fn find_fields_properties_typed_text_format() {
    let tmp = setup_vault();
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args([
        "find",
        "--file",
        "alpha.md",
        "--fields",
        "properties-typed",
        "--format",
        "text",
    ]);
    let output = cmd.output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(
        stdout.contains("properties_typed:"),
        "text output should contain properties_typed header, got: {stdout}"
    );
    // Expect entries formatted as "name (type): value"
    assert!(
        stdout.contains("(text):") || stdout.contains("(number):"),
        "text output should contain typed property entries, got: {stdout}"
    );
}

#[test]
fn find_fields_properties_typed_unknown_field_error() {
    let tmp = setup_vault();
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["find", "--fields", "properties-badtypo"]);
    let output = cmd.output().unwrap();
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

// ---------------------------------------------------------------------------
// Absence filter: !K
// ---------------------------------------------------------------------------

#[test]
fn find_property_absence_no_priority() {
    let tmp = setup_vault();
    // alpha has priority=3; beta, gamma, nested do not → 3 matches
    let (status, json, stderr) = find_json(&tmp, &["--property", "!priority"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 3, "expected 3 files without priority: {arr:?}");
    let files: Vec<&str> = arr.iter().map(|v| v["file"].as_str().unwrap()).collect();
    assert!(
        !files.contains(&"alpha.md"),
        "alpha has priority — should be excluded"
    );
    assert!(files.contains(&"beta.md"));
    assert!(files.contains(&"gamma.md"));
    assert!(files.contains(&"sub/nested.md"));
}

#[test]
fn find_property_absence_no_status_only_gamma() {
    let tmp = setup_vault();
    // Only gamma.md has no frontmatter (and therefore no status)
    let (status, json, stderr) = find_json(&tmp, &["--property", "!status"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1, "expected 1 file without status: {arr:?}");
    assert_eq!(arr[0]["file"], "gamma.md");
}

#[test]
fn find_property_absence_combined_with_other_filters() {
    let tmp = setup_vault();
    // Files without priority AND with status=planned → nested only (alpha has priority)
    let (status, json, stderr) = find_json(
        &tmp,
        &["--property", "!priority", "--property", "status=planned"],
    );
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1, "expected 1 match: {arr:?}");
    assert_eq!(arr[0]["file"], "sub/nested.md");
}

// ---------------------------------------------------------------------------
// Regex filter: K~=pattern and K~=/pattern/flags
// ---------------------------------------------------------------------------

#[test]
fn find_property_regex_bare_substring() {
    let tmp = setup_vault();
    // status~=compl matches "completed" (beta only)
    let (status, json, stderr) = find_json(&tmp, &["--property", "status~=compl"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1, "expected 1 file: {arr:?}");
    assert_eq!(arr[0]["file"], "beta.md");
}

#[test]
fn find_property_regex_delimited_anchored_exact() {
    let tmp = setup_vault();
    // status~=/^planned$/ — only exact "planned" values
    let (status, json, stderr) = find_json(&tmp, &["--property", r"status~=/^planned$/"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 2, "expected alpha + nested: {arr:?}");
    let files: Vec<&str> = arr.iter().map(|v| v["file"].as_str().unwrap()).collect();
    assert!(files.contains(&"alpha.md"));
    assert!(files.contains(&"sub/nested.md"));
}

#[test]
fn find_property_regex_case_insensitive_flag() {
    let tmp = setup_vault();
    // title~=/ALPHA/i — case-insensitive match against "Alpha"
    let (status, json, stderr) = find_json(&tmp, &["--property", "title~=/ALPHA/i"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1, "expected alpha.md: {arr:?}");
    assert_eq!(arr[0]["file"], "alpha.md");
}

#[test]
fn find_property_regex_list_property_any_element() {
    let tmp = setup_vault();
    // tags~=cli — matches "cli" element in alpha's tags list (alpha only)
    let (status, json, stderr) = find_json(&tmp, &["--property", "tags~=cli"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1, "expected alpha.md only: {arr:?}");
    assert_eq!(arr[0]["file"], "alpha.md");
}

#[test]
fn find_property_regex_list_property_nested_tag() {
    let tmp = setup_vault();
    // tags~=backend — matches "project/backend" element in nested's tags list
    let (status, json, stderr) = find_json(&tmp, &["--property", "tags~=backend"]);
    assert!(status.success(), "stderr: {stderr}");

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1, "expected sub/nested.md: {arr:?}");
    assert_eq!(arr[0]["file"], "sub/nested.md");
}

#[test]
fn find_property_regex_invalid_pattern_returns_user_error() {
    let tmp = setup_vault();
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["find", "--property", "status~=[invalid"]);
    let output = cmd.output().unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1), "invalid regex should exit 1");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid") || stderr.contains("regex") || stderr.contains("property"),
        "expected error message about invalid regex, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Section substring matching
// ---------------------------------------------------------------------------

fn setup_substring_section_vault() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();

    write_md(
        tmp.path(),
        "tasks.md",
        md!(r"
---
title: Task File
---
# Overview

Some text.

## Tasks [4/4]

- [x] Done item one
- [x] Done item two
- [x] Done item three
- [x] Done item four
"),
    );

    write_md(
        tmp.path(),
        "decision.md",
        md!(r"
---
title: Decisions
---
# DEC-031: Discoverable Drill-Down Hints Architecture (2026-03-22)

Decision text.

- [ ] Implement hints

# DEC-032: Another Decision (2026-03-23)

Another decision.

- [ ] Implement feature
"),
    );

    tmp
}

#[test]
fn section_filter_substring_matches_heading_with_count_suffix() {
    // "Tasks" should match "Tasks [4/4]" via substring
    let tmp = setup_substring_section_vault();
    let (status, json, stderr) = find_json(
        &tmp,
        &[
            "--file",
            "tasks.md",
            "--section",
            "Tasks",
            "--fields",
            "sections",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1, "expected tasks.md to match");
    let sections = arr[0]["sections"].as_array().unwrap();
    let headings: Vec<&str> = sections
        .iter()
        .map(|s| s["heading"].as_str().unwrap_or(""))
        .collect();
    assert!(
        headings.iter().any(|h| h.contains("Tasks")),
        "expected a section containing 'Tasks', got: {headings:?}"
    );
}

#[test]
fn section_filter_substring_matches_ticket_heading() {
    // "DEC-031" should match "DEC-031: Discoverable Drill-Down Hints Architecture (2026-03-22)"
    let tmp = setup_substring_section_vault();
    let (status, json, stderr) = find_json(
        &tmp,
        &[
            "--file",
            "decision.md",
            "--section",
            "DEC-031",
            "--task",
            "any",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1, "expected decision.md to match");
    let tasks = arr[0]["tasks"].as_array().unwrap();
    assert!(!tasks.is_empty(), "expected tasks in DEC-031 section");
}

#[test]
fn section_filter_substring_exact_heading_still_matches() {
    // Backwards compatible: exact heading text still works
    let tmp = setup_section_vault();
    let (status, json, stderr) = find_json(&tmp, &["--section", "Tasks", "--task", "any"]);
    assert!(status.success(), "stderr: {stderr}");
    let arr = json.as_array().unwrap();
    assert!(!arr.is_empty(), "expected files with ## Tasks section");
}

#[test]
fn section_filter_level_pinned_substring() {
    // "## Task" (level-pinned + substring) should match "## Tasks [4/4]" at level 2
    let tmp = setup_substring_section_vault();
    let (status, json, stderr) = find_json(
        &tmp,
        &[
            "--file",
            "tasks.md",
            "--section",
            "## Task",
            "--fields",
            "sections",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");
    let arr = json.as_array().unwrap();
    assert_eq!(
        arr.len(),
        1,
        "level-pinned substring should match: stderr={stderr}"
    );
}

#[test]
fn section_filter_regex_matches() {
    // ~=/DEC-03[12]/ should match both DEC-031 and DEC-032
    let tmp = setup_substring_section_vault();
    let (status, json, stderr) = find_json(
        &tmp,
        &[
            "--file",
            "decision.md",
            "--section",
            "~=/DEC-03[12]/",
            "--task",
            "any",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1, "expected decision.md to match");
    let tasks = arr[0]["tasks"].as_array().unwrap();
    // Both DEC-031 and DEC-032 have tasks, so we should get 2 tasks
    assert_eq!(tasks.len(), 2, "expected tasks from both matching sections");
}

#[test]
fn section_filter_regex_anchored() {
    // ~=/^Tasks$/ should match heading "Tasks" exactly but NOT "Tasks [4/4]"
    let tmp = setup_substring_section_vault();
    let (status, json, _) = find_json(
        &tmp,
        &[
            "--file",
            "tasks.md",
            "--section",
            "~=/^Tasks$/",
            "--task",
            "any",
        ],
    );
    assert!(status.success());
    let arr = json.as_array().unwrap();
    // tasks.md only has "Tasks [4/4]" — anchored regex should NOT match
    assert!(
        arr.is_empty(),
        "anchored regex should not match 'Tasks [4/4]'"
    );
}

#[test]
fn section_filter_regex_invalid_exits_1() {
    let tmp = setup_section_vault();
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["find", "--section", "~=/[invalid/"]);
    let output = cmd.output().unwrap();
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("regex") || stderr.contains("invalid"),
        "expected regex error, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Glob negation
// ---------------------------------------------------------------------------

fn setup_negation_vault() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    write_md(tmp.path(), "index.md", "---\ntitle: Index\n---\n# Index\n");
    write_md(
        tmp.path(),
        "notes/draft.md",
        "---\ntitle: Draft\n---\n# Draft\n",
    );
    write_md(
        tmp.path(),
        "notes/final.md",
        "---\ntitle: Final\n---\n# Final\n",
    );
    write_md(
        tmp.path(),
        "notes/index.md",
        "---\ntitle: Notes Index\n---\n# Notes Index\n",
    );
    tmp
}

#[test]
fn find_glob_negation_excludes_specific_file() {
    let tmp = setup_negation_vault();
    let (status, json, stderr) = find_json(&tmp, &["--glob", "!notes/draft.md"]);
    assert!(status.success(), "stderr: {stderr}");
    let arr = json.as_array().unwrap();
    let files: Vec<&str> = arr.iter().map(|v| v["file"].as_str().unwrap()).collect();
    assert!(
        !files.contains(&"notes/draft.md"),
        "draft.md should be excluded"
    );
    assert!(files.contains(&"notes/final.md"));
    assert!(files.contains(&"index.md"));
    assert_eq!(arr.len(), 3);
}

#[test]
fn find_glob_negation_wildcard_pattern() {
    let tmp = tempfile::tempdir().unwrap();
    write_md(tmp.path(), "a.md", "---\ntitle: A\n---\n");
    write_md(tmp.path(), "draft-b.md", "---\ntitle: Draft B\n---\n");
    write_md(tmp.path(), "draft-c.md", "---\ntitle: Draft C\n---\n");
    write_md(tmp.path(), "final.md", "---\ntitle: Final\n---\n");

    let (status, json, stderr) = find_json(&tmp, &["--glob", "!draft-*"]);
    assert!(status.success(), "stderr: {stderr}");
    let arr = json.as_array().unwrap();
    let files: Vec<&str> = arr.iter().map(|v| v["file"].as_str().unwrap()).collect();
    assert!(
        !files.iter().any(|f| f.starts_with("draft-")),
        "draft files should be excluded"
    );
    assert!(files.contains(&"a.md"));
    assert!(files.contains(&"final.md"));
    assert_eq!(arr.len(), 2);
}

#[test]
fn find_glob_negation_double_star() {
    // !**/index.md should exclude all index.md files recursively
    let tmp = setup_negation_vault();
    let (status, json, stderr) = find_json(&tmp, &["--glob", "!**/index.md"]);
    assert!(status.success(), "stderr: {stderr}");
    let arr = json.as_array().unwrap();
    let files: Vec<&str> = arr.iter().map(|v| v["file"].as_str().unwrap()).collect();
    assert!(
        !files.iter().any(|f| f.ends_with("index.md")),
        "all index.md files should be excluded, got: {files:?}"
    );
    assert!(files.contains(&"notes/draft.md"));
    assert!(files.contains(&"notes/final.md"));
    assert_eq!(arr.len(), 2);
}

#[test]
fn find_glob_positive_still_works() {
    let tmp = setup_negation_vault();
    let (status, json, stderr) = find_json(&tmp, &["--glob", "notes/*.md"]);
    assert!(status.success(), "stderr: {stderr}");
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 3, "expected 3 files in notes/");
    let files: Vec<&str> = arr.iter().map(|v| v["file"].as_str().unwrap()).collect();
    assert!(files.iter().all(|f| f.starts_with("notes/")));
}

// ---------------------------------------------------------------------------
// Multi-file --file targeting
// ---------------------------------------------------------------------------

#[test]
fn find_multi_file_returns_array() {
    let tmp = tempfile::tempdir().unwrap();
    write_md(tmp.path(), "a.md", "---\ntitle: A\n---\n");
    write_md(tmp.path(), "b.md", "---\ntitle: B\n---\n");
    write_md(tmp.path(), "c.md", "---\ntitle: C\n---\n");

    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["find", "--file", "a.md", "--file", "b.md"]);
    let output = cmd.output().unwrap();
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json.is_array());
    assert_eq!(json.as_array().unwrap().len(), 2);
}

// ---------------------------------------------------------------------------
// Content search inside code blocks
// ---------------------------------------------------------------------------

#[test]
fn find_pattern_matches_inside_code_block() {
    let tmp = tempfile::tempdir().unwrap();
    write_md(
        tmp.path(),
        "code.md",
        md!(r"
---
title: Code Example
---
# Code

```rust
let typescript = 42;
```
"),
    );

    let (status, json, stderr) = find_json(&tmp, &["typescript"]);
    assert!(status.success(), "stderr: {stderr}");
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1, "should find match inside code block: {arr:?}");
    assert_eq!(arr[0]["file"], "code.md");
}

#[test]
fn find_regex_matches_inside_code_block() {
    let tmp = tempfile::tempdir().unwrap();
    write_md(
        tmp.path(),
        "code.md",
        md!(r"
---
title: Code Example
---
# Code

```python
def hello_world():
    pass
```
"),
    );

    let (status, json, stderr) = find_json(&tmp, &["-e", "hello.*world"]);
    assert!(status.success(), "stderr: {stderr}");
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1, "should find regex match inside code block");
}

#[test]
fn find_pattern_only_inside_code_block_still_found() {
    // Term appears ONLY inside a code block, not in body text
    let tmp = tempfile::tempdir().unwrap();
    write_md(
        tmp.path(),
        "only_code.md",
        md!(r"
---
title: Only Code
---
Nothing special here.

```
unique_code_term_xyz
```
"),
    );

    let (status, json, stderr) = find_json(&tmp, &["unique_code_term_xyz"]);
    assert!(status.success(), "stderr: {stderr}");
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1, "term only inside code block should be found");
}

// ---------------------------------------------------------------------------
// Heading code spans preserved
// ---------------------------------------------------------------------------

#[test]
fn find_heading_with_code_span_preserved_in_section() {
    let tmp = tempfile::tempdir().unwrap();
    write_md(
        tmp.path(),
        "heading.md",
        md!(r"
---
title: Heading Test
---
## The `versions` field

Some content about versions.
"),
    );

    let (status, json, stderr) = find_json(&tmp, &["content about"]);
    assert!(status.success(), "stderr: {stderr}");
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    let matches = arr[0]["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1);
    // Section should preserve the backtick content
    let section = matches[0]["section"].as_str().unwrap();
    assert!(
        section.contains("versions"),
        "heading code span should be preserved, got: {section}"
    );
}

// ---------------------------------------------------------------------------
// Regex case sensitivity
// ---------------------------------------------------------------------------

#[test]
fn find_regex_case_sensitive_override() {
    let tmp = tempfile::tempdir().unwrap();
    write_md(
        tmp.path(),
        "case.md",
        md!(r"
---
title: Case Test
---
TypeScript is great
typescript is lowercase
TYPESCRIPT is uppercase
"),
    );

    // Default: case-insensitive, all 3 lines match
    let (status, json, stderr) = find_json(&tmp, &["-e", "typescript"]);
    assert!(status.success(), "stderr: {stderr}");
    let matches = json.as_array().unwrap()[0]["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 3, "default should be case-insensitive");

    // (?-i) override: only exact case matches
    let (status, json, stderr) = find_json(&tmp, &["-e", "(?-i)TypeScript"]);
    assert!(status.success(), "stderr: {stderr}");
    let matches = json.as_array().unwrap()[0]["matches"].as_array().unwrap();
    assert_eq!(
        matches.len(),
        1,
        "(?-i) should make search case-sensitive: {matches:?}"
    );
    assert_eq!(matches[0]["text"], "TypeScript is great");
}
