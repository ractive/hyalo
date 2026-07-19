use super::common::{hyalo_no_hints, md, write_md};
use std::fs;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helper: run `hyalo set` and return (status, parsed JSON, stderr)
// ---------------------------------------------------------------------------

fn set_json(
    tmp: &TempDir,
    extra_args: &[&str],
) -> (std::process::ExitStatus, serde_json::Value, String) {
    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.arg("set");
    cmd.args(extra_args);
    let output = cmd.output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let json: serde_json::Value = if output.status.success() {
        let envelope: serde_json::Value =
            serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
                let stdout = String::from_utf8_lossy(&output.stdout);
                panic!("invalid JSON: {e}\nstdout: {stdout}\nstderr: {stderr}")
            });
        envelope["results"].clone()
    } else {
        serde_json::Value::Null
    };
    (output.status, json, stderr)
}

// ---------------------------------------------------------------------------
// --property K=V: create, overwrite, skip identical
// ---------------------------------------------------------------------------

#[test]
fn set_property_creates_new() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Note
---
"),
    );

    let (status, json, stderr) =
        set_json(&tmp, &["--property", "status=done", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["property"], "status");
    assert_eq!(json["value"], "done");
    assert_eq!(json["modified"].as_array().unwrap().len(), 1);
    assert_eq!(json["skipped"].as_array().unwrap().len(), 0);

    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains("status: done"), "content:\n{content}");
}

#[test]
fn set_property_overwrites_existing() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
status: draft
---
"),
    );

    let (status, json, stderr) = set_json(
        &tmp,
        &["--property", "status=published", "--file", "note.md"],
    );
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["modified"].as_array().unwrap().len(), 1);

    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains("status: published"), "content:\n{content}");
    assert!(
        !content.contains("draft"),
        "old value still present:\n{content}"
    );
}

#[test]
fn set_property_skips_when_identical() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
status: done
---
"),
    );

    let (status, json, stderr) =
        set_json(&tmp, &["--property", "status=done", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["modified"].as_array().unwrap().len(), 0);
    assert_eq!(json["skipped"].as_array().unwrap().len(), 1);
    assert_eq!(
        json["skipped"].as_array().unwrap()[0].as_str().unwrap(),
        "note.md"
    );
}

// ---------------------------------------------------------------------------
// --tag T: add new, idempotent
// ---------------------------------------------------------------------------

#[test]
fn set_tag_adds_new_tag() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Note
---
"),
    );

    let (status, json, stderr) = set_json(&tmp, &["--tag", "rust", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["tag"], "rust");
    assert_eq!(json["modified"].as_array().unwrap().len(), 1);
    assert_eq!(json["skipped"].as_array().unwrap().len(), 0);

    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains("rust"), "tag not written:\n{content}");
}

#[test]
fn set_tag_idempotent_when_already_present() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
tags:
  - rust
---
"),
    );

    let (status, json, stderr) = set_json(&tmp, &["--tag", "rust", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["modified"].as_array().unwrap().len(), 0);
    assert_eq!(json["skipped"].as_array().unwrap().len(), 1);
}

// ---------------------------------------------------------------------------
// Multiple --property and --tag returns an array
// ---------------------------------------------------------------------------

#[test]
fn set_multiple_mutations_returns_array() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Note
---
"),
    );

    let (status, json, stderr) = set_json(
        &tmp,
        &[
            "--property",
            "status=done",
            "--tag",
            "rust",
            "--file",
            "note.md",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");

    assert!(
        json.is_array(),
        "expected array for multiple mutations: {json}"
    );
    assert_eq!(json.as_array().unwrap().len(), 2);
}

// ---------------------------------------------------------------------------
// Guard: --file or --glob required
// ---------------------------------------------------------------------------

#[test]
fn set_requires_file_or_glob() {
    let tmp = TempDir::new().unwrap();
    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["set", "--property", "status=done"]);
    let output = cmd.output().unwrap();
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

// ---------------------------------------------------------------------------
// Guard: at least one --property or --tag required
// ---------------------------------------------------------------------------

#[test]
fn set_requires_at_least_one_mutation() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");
    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["set", "--file", "note.md"]);
    let output = cmd.output().unwrap();
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

// ---------------------------------------------------------------------------
// Guard: invalid K=V (empty key) returns error
// ---------------------------------------------------------------------------

#[test]
fn set_empty_property_name_returns_error() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");
    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["set", "--property", "=value", "--file", "note.md"]);
    let output = cmd.output().unwrap();
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

// ---------------------------------------------------------------------------
// Guard: invalid K=V (no `=`) returns error
// ---------------------------------------------------------------------------

#[test]
fn set_invalid_kv_no_equals_returns_error() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");
    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["set", "--property", "no-equals-sign", "--file", "note.md"]);
    let output = cmd.output().unwrap();
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
}

// ---------------------------------------------------------------------------
// Body content is preserved after mutation
// ---------------------------------------------------------------------------

#[test]
fn set_preserves_file_body() {
    let tmp = TempDir::new().unwrap();
    let body = "# Heading\n\nSome body content here.\n";
    write_md(
        tmp.path(),
        "note.md",
        &format!("---\ntitle: Note\n---\n{body}"),
    );

    let (status, _json, stderr) =
        set_json(&tmp, &["--property", "status=done", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");

    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains(body), "body was corrupted:\n{content}");
}

// ---------------------------------------------------------------------------
// --glob modifies multiple files
// ---------------------------------------------------------------------------

#[test]
fn set_with_glob_modifies_multiple_files() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        md!(r"
---
title: A
---
"),
    );
    write_md(
        tmp.path(),
        "b.md",
        md!(r"
---
title: B
---
"),
    );

    let (status, json, stderr) = set_json(&tmp, &["--property", "status=done", "--glob", "*.md"]);
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(json["total"], 2, "expected total=2: {json}");
    assert_eq!(json["modified"].as_array().unwrap().len(), 2);

    let a = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    let b = fs::read_to_string(tmp.path().join("b.md")).unwrap();
    assert!(a.contains("status: done"), "a.md:\n{a}");
    assert!(b.contains("status: done"), "b.md:\n{b}");
}

// ---------------------------------------------------------------------------
// --format text produces non-empty output
// ---------------------------------------------------------------------------

#[test]
fn set_format_text_produces_output() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Note
---
"),
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "text"])
        .args(["set", "--property", "status=done", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.trim().is_empty(), "expected non-empty text output");
}

// ---------------------------------------------------------------------------
// --where-property / --where-tag filter tests
// ---------------------------------------------------------------------------

#[test]
fn set_where_property_scalar_match() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "done.md",
        md!(r"
---
title: Done
status: done
---
"),
    );
    write_md(
        tmp.path(),
        "active.md",
        md!(r"
---
title: Active
status: active
---
"),
    );

    let (status, json, stderr) = set_json(
        &tmp,
        &[
            "--property",
            "status=completed",
            "--where-property",
            "status=done",
            "--glob",
            "**/*.md",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(
        json["modified"].as_array().unwrap().len(),
        1,
        "expected 1 modified: {json}"
    );
    assert_eq!(
        json["skipped"].as_array().unwrap().len(),
        0,
        "expected 0 skipped: {json}"
    );

    let done_content = fs::read_to_string(tmp.path().join("done.md")).unwrap();
    assert!(
        done_content.contains("status: completed"),
        "done.md should be updated:\n{done_content}"
    );

    let active_content = fs::read_to_string(tmp.path().join("active.md")).unwrap();
    assert!(
        active_content.contains("status: active"),
        "active.md should be untouched:\n{active_content}"
    );
    assert!(
        !active_content.contains("completed"),
        "active.md should not have been modified:\n{active_content}"
    );
}

#[test]
fn set_where_property_list_match() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Note
tags:
  - cli
  - rust
---
"),
    );

    let (status, json, stderr) = set_json(
        &tmp,
        &[
            "--property",
            "reviewed=true",
            "--where-property",
            "tags=cli",
            "--glob",
            "**/*.md",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(
        json["modified"].as_array().unwrap().len(),
        1,
        "expected list element match to produce 1 modified: {json}"
    );

    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(
        content.contains("reviewed: true"),
        "file should have been mutated:\n{content}"
    );
}

#[test]
fn set_where_tag_nested_match() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "backend.md",
        md!(r"
---
title: Backend
tags:
  - project/backend
---
"),
    );
    write_md(
        tmp.path(),
        "research.md",
        md!(r"
---
title: Research
tags:
  - research
---
"),
    );

    let (status, json, stderr) = set_json(
        &tmp,
        &[
            "--property",
            "checked=true",
            "--where-tag",
            "project",
            "--glob",
            "**/*.md",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(
        json["modified"].as_array().unwrap().len(),
        1,
        "expected only the nested-tag file to be modified: {json}"
    );

    let backend_content = fs::read_to_string(tmp.path().join("backend.md")).unwrap();
    assert!(
        backend_content.contains("checked: true"),
        "backend.md should be modified:\n{backend_content}"
    );

    let research_content = fs::read_to_string(tmp.path().join("research.md")).unwrap();
    assert!(
        !research_content.contains("checked"),
        "research.md should be untouched:\n{research_content}"
    );
}

#[test]
fn set_where_combined_and() {
    let tmp = TempDir::new().unwrap();
    // Matches both filters: status=active AND tagged with rust
    write_md(
        tmp.path(),
        "both.md",
        md!(r"
---
title: Both
status: active
tags:
  - rust
---
"),
    );
    // Matches property but not tag
    write_md(
        tmp.path(),
        "prop_only.md",
        md!(r"
---
title: PropOnly
status: active
tags:
  - python
---
"),
    );
    // Matches tag but not property
    write_md(
        tmp.path(),
        "tag_only.md",
        md!(r"
---
title: TagOnly
status: draft
tags:
  - rust
---
"),
    );

    let (status, json, stderr) = set_json(
        &tmp,
        &[
            "--property",
            "processed=true",
            "--where-property",
            "status=active",
            "--where-tag",
            "rust",
            "--glob",
            "**/*.md",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(
        json["modified"].as_array().unwrap().len(),
        1,
        "expected only 1 file matching both filters: {json}"
    );

    let both_content = fs::read_to_string(tmp.path().join("both.md")).unwrap();
    assert!(
        both_content.contains("processed: true"),
        "both.md should be modified:\n{both_content}"
    );

    let prop_only = fs::read_to_string(tmp.path().join("prop_only.md")).unwrap();
    assert!(
        !prop_only.contains("processed"),
        "prop_only.md should be untouched:\n{prop_only}"
    );

    let tag_only = fs::read_to_string(tmp.path().join("tag_only.md")).unwrap();
    assert!(
        !tag_only.contains("processed"),
        "tag_only.md should be untouched:\n{tag_only}"
    );
}

#[test]
fn set_where_no_matches() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Note
status: active
---
"),
    );

    let (status, json, stderr) = set_json(
        &tmp,
        &[
            "--property",
            "status=done",
            "--where-property",
            "status=nonexistent",
            "--glob",
            "**/*.md",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(
        json["modified"].as_array().unwrap().len(),
        0,
        "expected 0 modified when no files match filter: {json}"
    );

    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(
        content.contains("status: active"),
        "note.md should be untouched:\n{content}"
    );
}

#[test]
fn set_where_property_operator() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "low.md",
        md!(r"
---
title: Low
priority: 2
---
"),
    );
    write_md(
        tmp.path(),
        "high.md",
        md!(r"
---
title: High
priority: 5
---
"),
    );

    let (status, json, stderr) = set_json(
        &tmp,
        &[
            "--property",
            "urgent=true",
            "--where-property",
            "priority>=3",
            "--glob",
            "**/*.md",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");

    assert_eq!(
        json["modified"].as_array().unwrap().len(),
        1,
        "expected only high-priority file to be modified: {json}"
    );

    let high_content = fs::read_to_string(tmp.path().join("high.md")).unwrap();
    assert!(
        high_content.contains("urgent: true"),
        "high.md should be modified:\n{high_content}"
    );

    let low_content = fs::read_to_string(tmp.path().join("low.md")).unwrap();
    assert!(
        !low_content.contains("urgent"),
        "low.md should be untouched:\n{low_content}"
    );
}

// ---------------------------------------------------------------------------
// Type inference: numbers, booleans, text
// ---------------------------------------------------------------------------

#[test]
fn set_property_infers_number_type() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Note
---
"),
    );

    let (status, _json, stderr) =
        set_json(&tmp, &["--property", "priority=42", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");

    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    // YAML number: no quotes around the value
    assert!(
        content.contains("priority: 42"),
        "expected unquoted number in YAML:\n{content}"
    );
}

#[test]
fn set_property_infers_boolean_type() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Note
---
"),
    );

    let (status, _json, stderr) =
        set_json(&tmp, &["--property", "draft=true", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");

    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    // YAML boolean: no quotes
    assert!(
        content.contains("draft: true"),
        "expected unquoted boolean in YAML:\n{content}"
    );
}

#[test]
fn set_property_infers_text_type() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Note
---
"),
    );

    let (status, _json, stderr) = set_json(
        &tmp,
        &["--property", "status=in-progress", "--file", "note.md"],
    );
    assert!(status.success(), "stderr: {stderr}");

    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(
        content.contains("status: in-progress"),
        "expected text value in YAML:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// List property creation via bracket syntax
// ---------------------------------------------------------------------------

#[test]
fn set_list_property_creates_yaml_sequence() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Note
---
"),
    );
    let (status, json, _) = set_json(
        &tmp,
        &[
            "--property",
            "keywords=[rust, cli, tools]",
            "--file",
            "note.md",
        ],
    );
    assert!(status.success());
    assert_eq!(json["modified"].as_array().unwrap().len(), 1);

    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    // Should be a YAML list, not a string
    assert!(
        content.contains("- rust"),
        "expected YAML list item, got:\n{content}"
    );
    assert!(content.contains("- cli"));
    assert!(content.contains("- tools"));
}

#[test]
fn set_empty_list_property() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: Note\n---\n");
    let (status, _, _) = set_json(&tmp, &["--property", "keywords=[]", "--file", "note.md"]);
    assert!(status.success());

    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(content.contains("keywords: []"));
}

// ---------------------------------------------------------------------------
// Multi-file --file targeting
// ---------------------------------------------------------------------------

#[test]
fn set_multi_file_modifies_all() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        md!(r"
---
title: A
---
"),
    );
    write_md(
        tmp.path(),
        "b.md",
        md!(r"
---
title: B
---
"),
    );
    let (status, json, _) = set_json(
        &tmp,
        &[
            "--property",
            "status=done",
            "--file",
            "a.md",
            "--file",
            "b.md",
        ],
    );
    assert!(status.success());
    assert_eq!(json["modified"].as_array().unwrap().len(), 2);

    let a = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    let b = fs::read_to_string(tmp.path().join("b.md")).unwrap();
    assert!(a.contains("status: done"));
    assert!(b.contains("status: done"));
}

#[test]
fn set_multi_file_partial_failure() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "exists.md",
        md!(r"
---
title: Exists
---
"),
    );
    // "missing.md" does not exist
    let (status, json, stderr) = set_json(
        &tmp,
        &[
            "--property",
            "status=done",
            "--file",
            "exists.md",
            "--file",
            "missing.md",
        ],
    );
    assert!(status.success(), "should succeed with partial failure");
    assert!(stderr.contains("warning"), "should warn about missing file");
    assert_eq!(json["modified"].as_array().unwrap().len(), 1);
}

// ---------------------------------------------------------------------------
// Malformed YAML resilience
// ---------------------------------------------------------------------------

/// `set --glob` skips a file with malformed YAML, modifies valid files, and
/// emits a warning on stderr. The command still exits successfully.
#[test]
fn set_skips_malformed_yaml_file() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "good.md",
        md!(r"
---
title: Good
---
# Good
"),
    );
    // Bare colon key: rejected by serde_yaml_ng.
    write_md(
        tmp.path(),
        "bad.md",
        "---\n: invalid yaml [[[{\n---\n# Bad\n",
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["set", "--property", "status=done", "--glob", "*.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "expected success; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let json = &envelope["results"];
    // Only the valid file should be modified.
    assert_eq!(
        json["modified"].as_array().unwrap().len(),
        1,
        "only one file should be modified; json: {json}"
    );
    assert_eq!(json["modified"][0], "good.md");

    // The valid file was updated on disk.
    let content = fs::read_to_string(tmp.path().join("good.md")).unwrap();
    assert!(content.contains("status: done"), "content:\n{content}");

    // Warning emitted for the bad file.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("warning: skipping"),
        "expected warning on stderr; got: {stderr}"
    );
    assert!(
        stderr.contains("bad.md"),
        "warning should name the bad file; got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Filter guard: reject operator suffixes in --property
// ---------------------------------------------------------------------------

#[test]
fn set_rejects_gte_filter_in_property() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");
    let (status, _, stderr) = set_json(&tmp, &["--property", "priority>=3", "--file", "note.md"]);
    assert!(!status.success(), "should fail; stderr: {stderr}");
    assert!(
        stderr.contains("--where-property"),
        "hint missing; stderr: {stderr}"
    );
}

#[test]
fn set_rejects_lte_filter_in_property() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");
    let (status, _, stderr) = set_json(&tmp, &["--property", "priority<=3", "--file", "note.md"]);
    assert!(!status.success(), "should fail; stderr: {stderr}");
    assert!(
        stderr.contains("--where-property"),
        "hint missing; stderr: {stderr}"
    );
}

#[test]
fn set_rejects_neq_filter_in_property() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");
    let (status, _, stderr) = set_json(&tmp, &["--property", "status!=draft", "--file", "note.md"]);
    assert!(!status.success(), "should fail; stderr: {stderr}");
    assert!(
        stderr.contains("--where-property"),
        "hint missing; stderr: {stderr}"
    );
}

#[test]
fn set_rejects_regex_filter_in_property() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");
    let (status, _, stderr) = set_json(&tmp, &["--property", "name~=pattern", "--file", "note.md"]);
    assert!(!status.success(), "should fail; stderr: {stderr}");
    assert!(
        stderr.contains("--where-property"),
        "hint missing; stderr: {stderr}"
    );
}

#[test]
fn set_accepts_plain_kv_property() {
    // Ensure normal K=V still works after guard is added
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");
    let (status, json, stderr) =
        set_json(&tmp, &["--property", "status=done", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");
    assert_eq!(json["property"], "status");
}

#[test]
fn set_accepts_list_property() {
    // Ensure K=[a,b] still works
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");
    let (status, json, stderr) =
        set_json(&tmp, &["--property", "tags=[a, b, c]", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");
    assert_eq!(json["property"], "tags");
}

// ---------------------------------------------------------------------------
// --dry-run: preview without modifying
// ---------------------------------------------------------------------------

#[test]
fn set_dry_run_does_not_modify() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Note
---
"),
    );

    let (status, json, stderr) = set_json(
        &tmp,
        &[
            "--property",
            "status=done",
            "--file",
            "note.md",
            "--dry-run",
        ],
    );
    assert!(status.success(), "stderr: {stderr}");
    assert_eq!(json["dry_run"], true);
    assert_eq!(json["modified"].as_array().unwrap().len(), 1);

    // File must NOT have been modified
    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(
        !content.contains("status"),
        "file was modified despite --dry-run:\n{content}"
    );
}

#[test]
fn set_dry_run_tag_does_not_modify() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: Note\n---\n");

    let (status, json, stderr) =
        set_json(&tmp, &["--tag", "rust", "--file", "note.md", "--dry-run"]);
    assert!(status.success(), "stderr: {stderr}");
    assert_eq!(json["dry_run"], true);
    assert_eq!(json["modified"].as_array().unwrap().len(), 1);

    // File must NOT have been modified
    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(
        !content.contains("rust"),
        "file was modified despite --dry-run:\n{content}"
    );
}

#[test]
fn set_without_dry_run_has_dry_run_false() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");

    let (status, json, stderr) =
        set_json(&tmp, &["--property", "status=done", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");
    assert_eq!(json["dry_run"], false);

    // File should actually be modified
    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(
        content.contains("status: done"),
        "file was not written:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// BUG-B: date-typed property validation notes
// ---------------------------------------------------------------------------

/// `hyalo set --property date=<garbage>` should emit a `note` field in JSON
/// warning that the value is not a valid ISO 8601 date.
#[test]
fn set_date_property_non_date_emits_note() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");

    let (status, json, stderr) = set_json(
        &tmp,
        &["--property", "date=not-a-date", "--file", "note.md"],
    );
    assert!(status.success(), "stderr: {stderr}");

    // note field must be present and mention the bad value
    let note = json["note"].as_str().expect("expected note field");
    assert!(
        note.contains("not-a-date"),
        "expected bad value in note: {note}"
    );
    assert!(
        note.contains("YYYY-MM-DD") || note.contains("ISO 8601"),
        "expected format hint in note: {note}"
    );
}

/// `hyalo set --property date=2026-05-10` (valid date) must NOT emit a note.
#[test]
fn set_date_property_valid_date_no_note() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");

    let (status, json, stderr) = set_json(
        &tmp,
        &["--property", "date=2026-05-10", "--file", "note.md"],
    );
    assert!(status.success(), "stderr: {stderr}");

    assert!(
        json["note"].is_null(),
        "unexpected note for valid date: {}",
        json["note"]
    );
}

/// Non-date-typed properties (e.g. `status=not-a-date`) must NOT emit a note.
#[test]
fn set_non_date_property_no_note() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");

    let (status, json, stderr) = set_json(
        &tmp,
        &["--property", "status=not-a-date", "--file", "note.md"],
    );
    assert!(status.success(), "stderr: {stderr}");

    assert!(
        json["note"].is_null(),
        "unexpected note for non-date property: {}",
        json["note"]
    );
}

/// The `created` and `modified` keys are also date-typed.
#[test]
fn set_created_property_non_date_emits_note() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");

    let (status, json, stderr) =
        set_json(&tmp, &["--property", "created=oops", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");

    let note = json["note"]
        .as_str()
        .expect("expected note field for created");
    assert!(note.contains("oops"), "expected bad value in note: {note}");
}

#[test]
fn set_modified_property_non_date_emits_note() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");

    let (status, json, stderr) =
        set_json(&tmp, &["--property", "modified=oops", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");

    let note = json["note"]
        .as_str()
        .expect("expected note field for modified");
    assert!(note.contains("oops"), "expected bad value in note: {note}");
}

// ---------------------------------------------------------------------------
// BUG-2: date property validation must reject calendar-invalid dates (iter-133)
// ---------------------------------------------------------------------------

#[test]
fn set_date_rejects_month_13() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");

    let (status, _json, stderr) = set_json(
        &tmp,
        &["--property", "date=2026-13-01", "--file", "note.md"],
    );
    // Should fail — month 13 is invalid
    assert!(
        !status.success(),
        "date=2026-13-01 should be rejected (month 13 is invalid), stderr: {stderr}"
    );
}

#[test]
fn set_date_rejects_day_32() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");

    let (status, _json, stderr) = set_json(
        &tmp,
        &["--property", "date=2026-01-32", "--file", "note.md"],
    );
    assert!(
        !status.success(),
        "date=2026-01-32 should be rejected (day 32 is invalid), stderr: {stderr}"
    );
}

#[test]
fn set_date_rejects_feb_30() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");

    let (status, _json, stderr) = set_json(
        &tmp,
        &["--property", "date=2026-02-30", "--file", "note.md"],
    );
    assert!(
        !status.success(),
        "date=2026-02-30 should be rejected (February has at most 29 days), stderr: {stderr}"
    );
}

#[test]
fn set_date_accepts_valid_date() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");

    let (status, _json, stderr) = set_json(
        &tmp,
        &["--property", "date=2026-05-11", "--file", "note.md"],
    );
    assert!(
        status.success(),
        "date=2026-05-11 should be accepted, stderr: {stderr}"
    );
}

#[test]
fn set_date_accepts_leap_day() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");

    let (status, _json, stderr) = set_json(
        &tmp,
        &["--property", "date=2024-02-29", "--file", "note.md"],
    );
    assert!(
        status.success(),
        "2024-02-29 is a valid leap day, stderr: {stderr}"
    );
}

#[test]
fn set_date_rejects_non_leap_feb_29() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: x\n---\n");

    let (status, _json, stderr) = set_json(
        &tmp,
        &["--property", "date=2023-02-29", "--file", "note.md"],
    );
    assert!(
        !status.success(),
        "2023-02-29 should be rejected (2023 is not a leap year), stderr: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// iter-158 C-1: read/write opening-delimiter predicate drift corrupted files
// on BOM-prefixed, leading-whitespace, and CRLF documents, and rejected
// oversized files unsafely. These are the CLI-level regression tests.
// ---------------------------------------------------------------------------

#[test]
fn set_bom_file_round_trips_without_duplicate_block() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        "\u{feff}---\ntitle: Note\nstatus: draft\n---\nBody.\n",
    );

    let (status, json, stderr) =
        set_json(&tmp, &["--property", "status=done", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");
    assert_eq!(json["modified"].as_array().unwrap().len(), 1);

    let bytes = fs::read(tmp.path().join("note.md")).unwrap();
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        "\u{feff}---\ntitle: Note\nstatus: done\n---\nBody.\n",
        "expected the original BOM-prefixed block updated in place, not duplicated"
    );
}

#[test]
fn set_leading_space_file_prepends_new_block_without_corrupting_pseudo_block() {
    let tmp = TempDir::new().unwrap();
    let original = " ---\ntitle: Note\nstatus: draft\n---\nBody.\n";
    write_md(tmp.path(), "note.md", original);

    let (status, json, stderr) =
        set_json(&tmp, &["--property", "status=done", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");
    assert_eq!(json["modified"].as_array().unwrap().len(), 1);

    let content = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert_eq!(
        content,
        format!("---\nstatus: done\n---\n{original}"),
        "expected exactly one new frontmatter block prepended, with the old \
         ` ---` pseudo-block preserved verbatim as body"
    );

    // The read and write paths must agree that ` ---` never opens frontmatter,
    // so `find --property status=done` sees the newly written property.
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--property", "status=done"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let results = envelope["results"].as_array().unwrap();
    assert_eq!(
        results.len(),
        1,
        "find should see the newly written property: {envelope}"
    );
    assert_eq!(results[0]["file"], "note.md");
}

#[test]
fn set_crlf_file_stays_uniform_crlf_after_mutation() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        "---\r\ntitle: Note\r\nstatus: draft\r\n---\r\nBody.\r\n",
    );

    let (status, json, stderr) =
        set_json(&tmp, &["--property", "status=done", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");
    assert_eq!(json["modified"].as_array().unwrap().len(), 1);

    let bytes = fs::read(tmp.path().join("note.md")).unwrap();
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        "---\r\ntitle: Note\r\nstatus: done\r\n---\r\nBody.\r\n",
        "expected uniform CRLF line endings after mutation"
    );
}

#[test]
fn set_refuses_oversized_file_and_leaves_it_untouched() {
    use std::io::Read as _;

    let tmp = TempDir::new().unwrap();
    let original = b"---\ntitle: Note\n---\nBody.\n";
    write_md(tmp.path(), "big.md", std::str::from_utf8(original).unwrap());
    // Sparse-extend well past the 100 MiB write guard without writing real data.
    let file = fs::OpenOptions::new()
        .write(true)
        .open(tmp.path().join("big.md"))
        .unwrap();
    file.set_len(100 * 1024 * 1024 + 1).unwrap();
    drop(file);

    let (status, _json, stderr) =
        set_json(&tmp, &["--property", "status=done", "--file", "big.md"]);
    assert!(
        !status.success(),
        "oversized file must be refused, not silently rewritten"
    );
    assert!(
        stderr.contains("MiB") && stderr.contains("limit"),
        "expected a size-limit error, got stderr: {stderr}"
    );

    let meta = fs::metadata(tmp.path().join("big.md")).unwrap();
    assert_eq!(
        meta.len(),
        100 * 1024 * 1024 + 1,
        "file size must be unchanged after a refused write"
    );
    let mut prefix = vec![0u8; original.len()];
    fs::File::open(tmp.path().join("big.md"))
        .unwrap()
        .read_exact(&mut prefix)
        .unwrap();
    assert_eq!(prefix, original, "file content must be untouched");
}

// ---------------------------------------------------------------------------
// iter-181 task 3: JSON `value` reflects the coerced value, not raw input
// ---------------------------------------------------------------------------

#[test]
fn set_json_value_echoes_coerced_list() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: Note\n---\n");

    let (status, json, stderr) =
        set_json(&tmp, &["--property", "tags=[a, b, c]", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");

    // The response echoes the parsed YAML list, not the literal "[a, b, c]".
    assert!(
        json["value"].is_array(),
        "expected coerced list value, got: {}",
        json["value"]
    );
    let arr = json["value"].as_array().unwrap();
    assert_eq!(arr.len(), 3, "value: {}", json["value"]);
    assert_eq!(arr[0], "a");
    assert_eq!(arr[2], "c");
}

#[test]
fn set_json_value_echoes_coerced_number() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: Note\n---\n");

    let (status, json, stderr) = set_json(&tmp, &["--property", "priority=3", "--file", "note.md"]);
    assert!(status.success(), "stderr: {stderr}");
    // Coerced to a JSON number, not the string "3".
    assert_eq!(
        json["value"],
        serde_json::json!(3),
        "value: {}",
        json["value"]
    );
    assert!(json["value"].is_number());
}

// ---------------------------------------------------------------------------
// iter-181 task 1: enum/pattern violations emit an advisory note; write proceeds
// ---------------------------------------------------------------------------

/// Write a vault with an `iteration` type whose `status` is an enum and whose
/// `branch` carries a regex pattern.
fn setup_iteration_schema() -> TempDir {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join(".hyalo.toml"),
        r#"dir = "."

[schema.types.iteration]
required = ["title"]

[schema.types.iteration.properties.status]
type = "enum"
values = ["planned", "in-progress", "completed"]

[schema.types.iteration.properties.branch]
type = "string"
pattern = "^iter-\\d+[a-z]*/"
"#,
    )
    .unwrap();
    tmp
}

#[test]
fn set_enum_violation_emits_advisory_note_and_writes() {
    let tmp = setup_iteration_schema();
    write_md(
        tmp.path(),
        "it.md",
        "---\ntitle: It\ntype: iteration\n---\n",
    );

    let (status, json, stderr) = set_json(&tmp, &["--property", "status=bogus", "--file", "it.md"]);
    // Write still proceeds (success), but carries an advisory note.
    assert!(status.success(), "stderr: {stderr}");
    let note = json["note"].as_str().unwrap_or_default();
    assert!(
        !note.is_empty() && note.contains("lint"),
        "expected an advisory note mentioning lint, got: {:?}",
        json["note"]
    );

    // The value was actually written despite the advisory.
    let content = fs::read_to_string(tmp.path().join("it.md")).unwrap();
    assert!(content.contains("status: bogus"), "content:\n{content}");
}

#[test]
fn set_pattern_violation_emits_advisory_note_and_writes() {
    let tmp = setup_iteration_schema();
    write_md(
        tmp.path(),
        "it.md",
        "---\ntitle: It\ntype: iteration\n---\n",
    );

    let (status, json, stderr) = set_json(&tmp, &["--property", "branch=TBD", "--file", "it.md"]);
    assert!(status.success(), "stderr: {stderr}");
    let note = json["note"].as_str().unwrap_or_default();
    assert!(
        !note.is_empty(),
        "expected an advisory note for the pattern violation, got: {:?}",
        json["note"]
    );

    let content = fs::read_to_string(tmp.path().join("it.md")).unwrap();
    assert!(content.contains("branch: TBD"), "content:\n{content}");
}

#[test]
fn set_valid_enum_value_has_no_advisory_note() {
    let tmp = setup_iteration_schema();
    write_md(
        tmp.path(),
        "it.md",
        "---\ntitle: It\ntype: iteration\n---\n",
    );

    let (status, json, stderr) =
        set_json(&tmp, &["--property", "status=planned", "--file", "it.md"]);
    assert!(status.success(), "stderr: {stderr}");
    assert!(
        json["note"].is_null(),
        "expected no advisory note for a valid enum value, got: {:?}",
        json["note"]
    );
}
