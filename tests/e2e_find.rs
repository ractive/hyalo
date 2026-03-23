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
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let json: serde_json::Value = if output.status.success() {
        serde_json::from_str(&stdout)
            .unwrap_or_else(|e| panic!("invalid JSON: {e}\nstdout: {stdout}\nstderr: {stderr}"))
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
            entry["properties"].is_array(),
            "properties should be present"
        );
        assert!(entry["tags"].is_array(), "tags should be present");
        assert!(entry["sections"].is_null(), "sections should be absent");
        assert!(entry["tasks"].is_null(), "tasks should be absent");
        assert!(entry["links"].is_null(), "links should be absent");
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
    // Properties with type annotations
    assert!(
        stdout.contains("title (text): Alpha"),
        "property rendering: {stdout}"
    );
    assert!(
        stdout.contains("status (text): planned"),
        "status property: {stdout}"
    );
    // Tags are shown as a dedicated field, not duplicated under properties
    assert!(
        !stdout.contains("tags (list):"),
        "tags should not appear as a property: {stdout}"
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
        stdout.contains("title (text): Alpha"),
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
