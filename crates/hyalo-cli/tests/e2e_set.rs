mod common;

use common::{hyalo, md, write_md};
use std::fs;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helper: run `hyalo set` and return (status, parsed JSON, stderr)
// ---------------------------------------------------------------------------

fn set_json(
    tmp: &TempDir,
    extra_args: &[&str],
) -> (std::process::ExitStatus, serde_json::Value, String) {
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.arg("set");
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
    let mut cmd = hyalo();
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
    let mut cmd = hyalo();
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
    let mut cmd = hyalo();
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
    let mut cmd = hyalo();
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

    let output = hyalo()
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

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["set", "--property", "status=done", "--glob", "*.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "expected success; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
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
