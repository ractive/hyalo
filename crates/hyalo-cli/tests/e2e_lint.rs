mod common;

use common::{hyalo_no_hints, md, write_md};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Write a `.hyalo.toml` with a `[schema]` block into `dir`.
fn write_schema_toml(dir: &std::path::Path, content: &str) {
    std::fs::write(dir.join(".hyalo.toml"), content).unwrap();
}

/// Set up a minimal vault for lint tests.
fn setup_vault_with_schema() -> TempDir {
    let tmp = TempDir::new().unwrap();

    // Write schema
    write_schema_toml(
        tmp.path(),
        r#"dir = "."

[schema.default]
required = ["title"]

[schema.types.note]
required = ["title", "date"]

[schema.types.note.properties.date]
type = "date"

[schema.types.note.properties.status]
type = "enum"
values = ["draft", "published"]
"#,
    );

    // Clean file
    write_md(
        tmp.path(),
        "clean.md",
        md!(r"
---
title: Clean Note
type: note
date: 2026-04-13
tags:
  - test
---
Body.
"),
    );

    // File missing required property
    write_md(
        tmp.path(),
        "missing_date.md",
        md!(r"
---
title: Missing Date
type: note
tags:
  - test
---
Body.
"),
    );

    // File with invalid enum value
    write_md(
        tmp.path(),
        "bad_status.md",
        md!(r"
---
title: Bad Status
type: note
date: 2026-04-13
status: wip
tags:
  - test
---
Body.
"),
    );

    tmp
}

// ---------------------------------------------------------------------------
// Basic lint tests
// ---------------------------------------------------------------------------

#[test]
fn lint_no_schema_exits_zero() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "---\ntitle: Hello\n---\nBody\n");

    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint"])
        .assert()
        .success()
        .code(0);
}

#[test]
fn lint_clean_vault_exits_zero() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(
        tmp.path(),
        "dir = \".\"\n[schema.default]\nrequired = [\"title\"]\n",
    );
    write_md(
        tmp.path(),
        "clean.md",
        "---\ntitle: Hello\ntype: note\ntags:\n  - test\n---\nBody\n",
    );

    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint"])
        .assert()
        .success()
        .code(0);
}

#[test]
fn lint_exits_one_when_errors_found() {
    let tmp = setup_vault_with_schema();

    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint"])
        .assert()
        .code(1);
}

#[test]
fn lint_text_output_shows_missing_required() {
    let tmp = setup_vault_with_schema();

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "missing_date.md"])
        .output()
        .unwrap();

    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    assert!(
        stdout.contains("missing_date.md"),
        "expected filename in output"
    );
    assert!(
        stdout.contains("missing required property"),
        "expected error message"
    );
    assert!(stdout.contains("date"), "expected property name");
}

#[test]
fn lint_text_output_shows_enum_violation() {
    let tmp = setup_vault_with_schema();

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "bad_status.md"])
        .output()
        .unwrap();

    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    assert!(stdout.contains("bad_status.md"), "expected filename");
    assert!(stdout.contains("wip"), "expected bad value in output");
    assert!(stdout.contains("not in"), "expected enum violation message");
}

#[test]
fn lint_suggests_nearest_enum_value() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(
        tmp.path(),
        r#"dir = "."
[schema.types.note.properties.status]
type = "enum"
values = ["planned", "in-progress", "completed"]
"#,
    );
    write_md(
        tmp.path(),
        "a.md",
        "---\ntitle: A\ntype: note\nstatus: planed\ntags:\n  - test\n---\nBody\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "a.md"])
        .output()
        .unwrap();

    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    assert!(
        stdout.contains("planned"),
        "expected suggestion 'planned' for misspelling 'planed'"
    );
}

#[test]
fn lint_single_file_positional() {
    let tmp = setup_vault_with_schema();

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "text", "clean.md"])
        .output()
        .unwrap();

    let exit = output.status.code().unwrap();
    assert_eq!(exit, 0, "clean file should exit 0");
    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    assert!(
        stdout.contains("no issues"),
        "expected no issues message: {stdout}"
    );
}

#[test]
fn lint_single_file_flag() {
    let tmp = setup_vault_with_schema();

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--file", "clean.md"])
        .output()
        .unwrap();

    let exit = output.status.code().unwrap();
    assert_eq!(exit, 0, "clean file should exit 0");
}

#[test]
fn lint_glob_flag() {
    let tmp = setup_vault_with_schema();

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--glob", "*.md"])
        .output()
        .unwrap();

    // vault has errors so exit 1
    let exit = output.status.code().unwrap();
    assert_eq!(exit, 1, "glob over errored vault should exit 1");
}

#[test]
fn lint_json_output() {
    let tmp = setup_vault_with_schema();

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "json", "missing_date.md"])
        .output()
        .unwrap();

    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    let val: serde_json::Value = serde_json::from_str(stdout)
        .unwrap_or_else(|e| panic!("expected JSON output, got: {stdout}\nerr: {e}"));

    // The pipeline wraps the lint output in the standard envelope:
    // {"results": {"files": [...], "total": N}, "hints": [...]}
    let inner = &val["results"];
    assert!(inner.is_object(), "expected results object in envelope");
    assert!(inner["files"].is_array(), "expected files array");
    assert!(inner["total"].is_number(), "expected total field");

    let files = inner["files"].as_array().unwrap();
    assert!(!files.is_empty());
    let first = &files[0];
    assert!(first["file"].is_string(), "expected file field");
    assert!(first["violations"].is_array(), "expected violations array");

    let violations = first["violations"].as_array().unwrap();
    assert!(!violations.is_empty(), "expected at least one violation");
    let v = &violations[0];
    assert!(v["severity"].is_string(), "expected severity field");
    assert!(v["message"].is_string(), "expected message field");
}

#[test]
fn lint_no_type_property_warn() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(
        tmp.path(),
        r#"dir = "."
[schema.default]
required = ["title"]

[schema.types.note]
required = ["title"]
"#,
    );
    write_md(tmp.path(), "a.md", "---\ntitle: Hello\n---\nBody\n");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "a.md"])
        .output()
        .unwrap();

    // Has warnings but no errors -> exit 0
    let exit = output.status.code().unwrap();
    assert_eq!(exit, 0, "warnings only should exit 0");
    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    assert!(
        stdout.contains("no 'type' property") || stdout.contains("warn"),
        "expected warning about missing type"
    );
}

#[test]
fn lint_unknown_type_uses_default_schema() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(
        tmp.path(),
        r#"dir = "."
[schema.default]
required = ["title"]

[schema.types.note]
required = ["title", "date"]
"#,
    );
    // File with type "unknown" — should only validate against default (title required)
    write_md(
        tmp.path(),
        "a.md",
        "---\ntitle: Hello\ntype: unknown\ntags:\n  - test\n---\nBody\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "a.md"])
        .output()
        .unwrap();

    // "date" is only required for type "note", not for "unknown"
    // So this should pass with exit 0 (title is present)
    let exit = output.status.code().unwrap();
    assert_eq!(exit, 0, "unknown type should validate against default only");
}

#[test]
fn lint_date_format_error() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(
        tmp.path(),
        r#"dir = "."
[schema.types.note.properties.date]
type = "date"
"#,
    );
    write_md(
        tmp.path(),
        "a.md",
        "---\ntitle: A\ntype: note\ndate: April 9\ntags:\n  - test\n---\nBody\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "a.md"])
        .output()
        .unwrap();

    let exit = output.status.code().unwrap();
    assert_eq!(exit, 1, "invalid date format should produce error");
    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    assert!(
        stdout.contains("expected date"),
        "expected date error message in output"
    );
}

#[test]
fn lint_string_pattern_error() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(
        tmp.path(),
        r#"dir = "."
[schema.types.note.properties.branch]
type = "string"
pattern = "^iter-\\d+/"
"#,
    );
    write_md(
        tmp.path(),
        "a.md",
        "---\ntitle: A\ntype: note\nbranch: feature/foo\ntags:\n  - test\n---\nBody\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "a.md"])
        .output()
        .unwrap();

    let exit = output.status.code().unwrap();
    assert_eq!(exit, 1, "pattern mismatch should produce error");
}

// ---------------------------------------------------------------------------
// Summary integration
// ---------------------------------------------------------------------------

#[test]
fn summary_shows_lint_count_when_schema_configured() {
    let tmp = setup_vault_with_schema();

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--format", "json", "summary"])
        .output()
        .unwrap();

    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    let val: serde_json::Value =
        serde_json::from_str(stdout).unwrap_or_else(|e| panic!("JSON parse: {e}\n{stdout}"));

    // When a schema is configured, results.schema should be present
    let schema_field = &val["results"]["schema"];
    assert!(
        !schema_field.is_null(),
        "expected schema field in summary when schema is configured"
    );
    assert!(
        schema_field["errors"].is_number(),
        "expected errors count in schema summary"
    );
    assert!(
        schema_field["warnings"].is_number(),
        "expected warnings count in schema summary"
    );
    assert!(
        schema_field["files_with_issues"].is_number(),
        "expected files_with_issues in schema summary"
    );
}

#[test]
fn summary_no_schema_field_without_config() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "---\ntitle: Hello\n---\nBody\n");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--format", "json", "summary"])
        .output()
        .unwrap();

    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    let val: serde_json::Value =
        serde_json::from_str(stdout).unwrap_or_else(|e| panic!("JSON parse: {e}\n{stdout}"));

    // No schema configured → schema field should be absent (null in JSON)
    assert!(
        val["results"]["schema"].is_null(),
        "schema field should be absent when no schema is configured"
    );
}

// ---------------------------------------------------------------------------
// --fix tests
// ---------------------------------------------------------------------------

/// Schema with defaults, enum, date, and a filename-template on `iteration`.
fn write_schema_with_fixables(dir: &std::path::Path) {
    write_schema_toml(
        dir,
        r#"dir = "."

[schema.default]
required = ["title"]

[schema.types.iteration]
required = ["title", "status", "date"]
filename-template = "iterations/iteration-{n}-{slug}.md"

[schema.types.iteration.defaults]
status = "planned"

[schema.types.iteration.properties.status]
type = "enum"
values = ["planned", "in-progress", "completed"]

[schema.types.iteration.properties.date]
type = "date"
"#,
    );
}

#[test]
fn fix_inserts_default_for_missing_property() {
    let tmp = TempDir::new().unwrap();
    write_schema_with_fixables(tmp.path());
    // Missing status; has date; has type.
    write_md(
        tmp.path(),
        "iterations/iteration-1-a.md",
        "---\ntitle: Iter\ntype: iteration\ndate: 2026-04-13\n---\nBody\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--fix", "iterations/iteration-1-a.md"])
        .output()
        .unwrap();

    assert_eq!(output.status.code().unwrap(), 0, "fix should clean file");
    let updated = std::fs::read_to_string(tmp.path().join("iterations/iteration-1-a.md")).unwrap();
    assert!(
        updated.contains("status: planned"),
        "expected inserted default: {updated}"
    );
}

#[test]
fn fix_corrects_enum_typo() {
    let tmp = TempDir::new().unwrap();
    write_schema_with_fixables(tmp.path());
    write_md(
        tmp.path(),
        "iterations/iteration-2-b.md",
        "---\ntitle: Iter\ntype: iteration\nstatus: planed\ndate: 2026-04-13\n---\nBody\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--fix", "iterations/iteration-2-b.md"])
        .output()
        .unwrap();

    assert_eq!(output.status.code().unwrap(), 0);
    let updated = std::fs::read_to_string(tmp.path().join("iterations/iteration-2-b.md")).unwrap();
    assert!(
        updated.contains("status: planned"),
        "expected typo fixed: {updated}"
    );
    assert!(!updated.contains("planed\n"));
}

#[test]
fn fix_normalizes_date_format() {
    let tmp = TempDir::new().unwrap();
    write_schema_with_fixables(tmp.path());
    write_md(
        tmp.path(),
        "iterations/iteration-3-c.md",
        "---\ntitle: Iter\ntype: iteration\nstatus: planned\ndate: 2026-4-9\n---\nBody\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--fix", "iterations/iteration-3-c.md"])
        .output()
        .unwrap();

    assert_eq!(output.status.code().unwrap(), 0);
    let updated = std::fs::read_to_string(tmp.path().join("iterations/iteration-3-c.md")).unwrap();
    assert!(
        updated.contains("date: 2026-04-09"),
        "expected normalized date: {updated}"
    );
}

#[test]
fn fix_infers_type_from_filename_template() {
    let tmp = TempDir::new().unwrap();
    write_schema_with_fixables(tmp.path());
    // Missing type; filename matches iteration template.
    write_md(
        tmp.path(),
        "iterations/iteration-4-d.md",
        "---\ntitle: Iter\nstatus: planned\ndate: 2026-04-13\n---\nBody\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--fix", "iterations/iteration-4-d.md"])
        .output()
        .unwrap();

    assert_eq!(output.status.code().unwrap(), 0);
    let updated = std::fs::read_to_string(tmp.path().join("iterations/iteration-4-d.md")).unwrap();
    assert!(
        updated.contains("type: iteration"),
        "expected inferred type: {updated}"
    );
}

#[test]
fn fix_dry_run_does_not_modify_files() {
    let tmp = TempDir::new().unwrap();
    write_schema_with_fixables(tmp.path());
    let body = "---\ntitle: Iter\ntype: iteration\nstatus: planed\ndate: 2026-04-13\n---\nBody\n";
    write_md(tmp.path(), "iterations/iteration-5-e.md", body);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--fix", "--dry-run", "iterations/iteration-5-e.md"])
        .output()
        .unwrap();

    // dry-run should succeed (exit 0) because the enum typo is fixable.
    assert_eq!(
        output.status.code().unwrap(),
        0,
        "dry-run with fixable issues should exit 0"
    );
    let content = std::fs::read_to_string(tmp.path().join("iterations/iteration-5-e.md")).unwrap();
    assert_eq!(content, body, "dry-run must not modify the file");
}

#[test]
fn fix_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    write_schema_with_fixables(tmp.path());
    write_md(
        tmp.path(),
        "iterations/iteration-6-f.md",
        "---\ntitle: Iter\ntype: iteration\nstatus: planed\ndate: 2026-4-3\n---\nBody\n",
    );

    // First run
    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--fix", "iterations/iteration-6-f.md"])
        .output()
        .unwrap();
    let first = std::fs::read_to_string(tmp.path().join("iterations/iteration-6-f.md")).unwrap();

    // Second run must be a no-op on disk.
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--fix", "iterations/iteration-6-f.md"])
        .output()
        .unwrap();
    assert_eq!(output.status.code().unwrap(), 0);
    let second = std::fs::read_to_string(tmp.path().join("iterations/iteration-6-f.md")).unwrap();
    assert_eq!(first, second, "second --fix run must be idempotent");
}

#[test]
fn fix_preserves_frontmatter_key_order() {
    let tmp = TempDir::new().unwrap();
    write_schema_with_fixables(tmp.path());
    // Ordering: title, type, date — then status will be inserted.
    let original =
        "---\ntitle: Iter\ntype: iteration\ndate: 2026-04-13\n---\nBody bytes preserved.\n";
    write_md(tmp.path(), "iterations/iteration-7-g.md", original);

    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--fix", "iterations/iteration-7-g.md"])
        .output()
        .unwrap();

    let updated = std::fs::read_to_string(tmp.path().join("iterations/iteration-7-g.md")).unwrap();
    // Original keys appear in their original relative order.
    let title_idx = updated.find("title:").unwrap();
    let type_idx = updated.find("type:").unwrap();
    let date_idx = updated.find("date:").unwrap();
    assert!(title_idx < type_idx);
    assert!(type_idx < date_idx);
    // Body is preserved verbatim.
    assert!(updated.ends_with("Body bytes preserved.\n"));
}

#[test]
fn fix_dry_run_requires_fix() {
    // --dry-run without --fix must be rejected by clap
    let tmp = TempDir::new().unwrap();
    write_schema_with_fixables(tmp.path());

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--dry-run"])
        .output()
        .unwrap();
    assert!(!output.status.success(), "--dry-run alone must fail");
}

#[test]
fn fix_reports_missing_required_without_default() {
    // With no default, a missing required field is reported, never fabricated.
    let tmp = TempDir::new().unwrap();
    write_schema_with_fixables(tmp.path());
    // Missing `title` — no default available.
    write_md(
        tmp.path(),
        "iterations/iteration-8-h.md",
        "---\ntype: iteration\nstatus: planned\ndate: 2026-04-13\n---\nBody\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--fix", "iterations/iteration-8-h.md"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code().unwrap(),
        1,
        "unfixable violation must keep exit 1"
    );
    let updated = std::fs::read_to_string(tmp.path().join("iterations/iteration-8-h.md")).unwrap();
    assert!(
        !updated.contains("title:"),
        "title must not be fabricated: {updated}"
    );
}

#[test]
fn fix_json_output_includes_fixes_array() {
    let tmp = TempDir::new().unwrap();
    write_schema_with_fixables(tmp.path());
    write_md(
        tmp.path(),
        "iterations/iteration-9-i.md",
        "---\ntitle: Iter\ntype: iteration\nstatus: planed\ndate: 2026-04-13\n---\nBody\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "--format",
            "json",
            "lint",
            "--fix",
            "iterations/iteration-9-i.md",
        ])
        .output()
        .unwrap();

    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    let val: serde_json::Value = serde_json::from_str(stdout)
        .unwrap_or_else(|e| panic!("expected JSON output, got: {stdout}\nerr: {e}"));
    let fixes = &val["results"]["fixes"];
    assert!(fixes.is_array(), "expected fixes array: {stdout}");
    let arr = fixes.as_array().unwrap();
    assert!(!arr.is_empty(), "expected at least one fix entry");
    let actions = &arr[0]["actions"];
    assert!(actions.is_array());
    assert!(
        actions
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["kind"] == "fix-enum-typo"),
        "expected fix-enum-typo action"
    );
}
