use super::common::{hyalo_no_hints, md, write_md};
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
fn lint_multiple_positional_files() {
    // `hyalo lint a.md b.md` — positional FILE is repeatable and both targets
    // are linted, matching --files-from semantics (iter-179 scope item 5).
    let tmp = setup_vault_with_schema();

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "text", "clean.md", "bad_status.md"])
        .output()
        .unwrap();

    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    assert!(
        stdout.contains("2 files checked"),
        "expected both positional files to be linted: {stdout}"
    );
    let exit = output.status.code().unwrap();
    assert_eq!(
        exit, 1,
        "bad_status.md has an invalid enum value, so exit should be non-zero: {stdout}"
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
    // New shape: violations grouped by rule
    assert!(
        first["rule_groups"].is_array(),
        "expected rule_groups array"
    );

    let rule_groups = first["rule_groups"].as_array().unwrap();
    assert!(!rule_groups.is_empty(), "expected at least one rule group");
    let g = &rule_groups[0];
    assert!(g["rule"].is_string(), "expected rule field");
    assert!(g["severity"].is_string(), "expected severity field");
    let violations = g["violations"].as_array().unwrap();
    assert!(!violations.is_empty(), "expected at least one violation");
    let v = &violations[0];
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

#[test]
fn lint_item_pattern_reports_all_violations() {
    // A string-list property with item_pattern should report one violation per
    // failing item — not just the first — so users fix everything in one pass.
    let tmp = TempDir::new().unwrap();
    write_schema_toml(
        tmp.path(),
        r#"dir = "."
[schema.types.doc.properties.tags]
type = "string-list"
item_pattern = "^[a-z][a-z0-9-]*$"
"#,
    );
    write_md(
        tmp.path(),
        "a.md",
        "---\ntitle: A\ntype: doc\ntags:\n  - Foo\n  - 1bad\n  - Bar\n---\nBody\n",
    );

    // Use a large --max-per-rule so all three violations are shown (not truncated).
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--max-per-rule", "100", "a.md"])
        .output()
        .unwrap();

    let exit = output.status.code().unwrap();
    assert_eq!(exit, 1, "item_pattern violations should produce exit 1");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // All three bad items must be reported in a single run.
    assert!(
        stdout.contains("item 0"),
        "expected violation for item 0 (Foo), got:\n{stdout}"
    );
    assert!(
        stdout.contains("item 1"),
        "expected violation for item 1 (1bad), got:\n{stdout}"
    );
    assert!(
        stdout.contains("item 2"),
        "expected violation for item 2 (Bar), got:\n{stdout}"
    );
    // Verify the count: exactly 3 pattern-mismatch violations from one file.
    let pattern_count = stdout.matches("does not match pattern").count();
    assert_eq!(
        pattern_count, 3,
        "expected 3 pattern-mismatch violations, got {pattern_count}:\n{stdout}"
    );
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
// Bug regression: lint JSON total counts violations, not files
// ---------------------------------------------------------------------------

#[test]
fn lint_json_total_counts_violations_not_files() {
    // Use a type-specific schema so we can have a clean file (no warnings) and
    // two files with exactly one error each.  The "no type property" warning is
    // suppressed by giving every file a `type` property.
    let tmp = TempDir::new().unwrap();
    write_schema_toml(
        tmp.path(),
        r#"dir = "."
[schema.default]
required = ["title"]

[schema.types.note]
required = ["title", "date"]

[schema.types.note.properties.date]
type = "date"
"#,
    );
    // Clean file: has both title and date → zero violations
    write_md(
        tmp.path(),
        "clean.md",
        "---\ntitle: OK\ntype: note\ndate: 2026-01-01\ntags:\n  - x\n---\nBody\n",
    );
    // Two files missing required 'date' → 1 error each, 0 warnings (type present)
    write_md(
        tmp.path(),
        "bad1.md",
        "---\ntitle: Bad One\ntype: note\ntags:\n  - x\n---\nBody\n",
    );
    write_md(
        tmp.path(),
        "bad2.md",
        "---\ntitle: Bad Two\ntype: note\ntags:\n  - x\n---\nBody\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "json"])
        .output()
        .unwrap();

    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    let val: serde_json::Value =
        serde_json::from_str(stdout).unwrap_or_else(|e| panic!("JSON parse: {e}\n{stdout}"));

    let results = &val["results"];
    let total = results["total"].as_u64().expect("total should be a number");
    let files_checked = results["files_checked"]
        .as_u64()
        .expect("files_checked should be a number");

    // 2 violations (one error per bad file), 3 files checked
    assert_eq!(
        total, 2,
        "total should count violations, not files: {results}"
    );
    assert_eq!(
        files_checked, 3,
        "files_checked should count all scanned files: {results}"
    );
    // Sanity: they must be different (this was the original bug)
    assert_ne!(
        total, files_checked,
        "total (violations) and files_checked must differ in this fixture"
    );
}

// ---------------------------------------------------------------------------
// Filter and limit tests
// ---------------------------------------------------------------------------

#[test]
fn lint_json_excludes_clean_files() {
    let tmp = setup_vault_with_schema();

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "json"])
        .output()
        .unwrap();

    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    let val: serde_json::Value = serde_json::from_str(stdout)
        .unwrap_or_else(|e| panic!("expected JSON output, got: {stdout}\nerr: {e}"));

    let inner = &val["results"];
    let files = inner["files"].as_array().unwrap();

    // Every file in the output should have at least one rule group (= at least one violation).
    for f in files {
        let rule_groups = f["rule_groups"].as_array().unwrap();
        assert!(
            !rule_groups.is_empty(),
            "clean files should not appear in output: {}",
            f["file"]
        );
    }
}

#[test]
fn lint_limit_caps_output() {
    let tmp = setup_vault_with_schema();
    // setup_vault_with_schema already has missing_date.md and bad_status.md (2 files with violations)
    // Add a third to ensure we have more than 1 violated file.
    write_md(
        tmp.path(),
        "extra_bad.md",
        "---\ntitle: Extra Bad\ntype: note\n---\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "json", "--limit", "1"])
        .output()
        .unwrap();

    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    let val: serde_json::Value = serde_json::from_str(stdout)
        .unwrap_or_else(|e| panic!("expected JSON output, got: {stdout}\nerr: {e}"));

    let inner = &val["results"];
    let files = inner["files"].as_array().unwrap();
    assert!(
        files.len() <= 1,
        "expected at most 1 file in output, got {}",
        files.len()
    );
    // total should still reflect ALL violations (not just the limited output)
    assert!(
        inner["total"].as_u64().unwrap() >= 1,
        "total should reflect all violations"
    );
    // files_truncated flag should be present and true
    assert_eq!(
        inner["files_truncated"].as_bool(),
        Some(true),
        "expected files_truncated=true when output was truncated"
    );
    // errors/warnings/files_with_violations should reflect all files, not just the limited slice
    assert!(
        inner["errors"].as_u64().is_some(),
        "expected errors field in ExtLintOutput"
    );
    assert!(
        inner["warnings"].as_u64().is_some(),
        "expected warnings field in ExtLintOutput"
    );
    let files_with_violations = inner["files_with_violations"].as_u64().unwrap();
    assert!(
        files_with_violations > 1,
        "expected files_with_violations > 1 (full count, not limited), got {files_with_violations}"
    );
}

#[test]
fn lint_limit_text_format_shows_truncation_notice() {
    let tmp = setup_vault_with_schema();
    write_md(
        tmp.path(),
        "extra_bad.md",
        "---\ntitle: Extra Bad\ntype: note\n---\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "text", "--limit", "1"])
        .output()
        .unwrap();

    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    assert!(
        stdout.contains("showing 1 of"),
        "expected truncation notice in text output, got:\n{stdout}"
    );
    // Summary should reflect all files_with_issues, not just the 1 shown
    // e.g. "X files checked, N with issues (..."
    assert!(
        stdout.contains("with issues"),
        "expected 'with issues' summary in text output, got:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Bucket 2: --strict flag
// ---------------------------------------------------------------------------

/// `hyalo lint --strict` exits non-zero when a file has no `type` property.
#[test]
fn lint_strict_exits_nonzero_when_file_missing_type() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(
        tmp.path(),
        r#"dir = "."

[schema.types.note]
required = ["title"]
"#,
    );
    // File with no `type` property — would be a warning in normal mode.
    write_md(
        tmp.path(),
        "no_type.md",
        "---\ntitle: No Type\n---\nBody.\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--strict", "--format", "json"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "--strict: lint should exit non-zero when file has no type; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // The JSON should show errors > 0.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let errors = json["results"]["errors"].as_u64().unwrap_or(0);
    assert!(
        errors > 0,
        "--strict: errors should be > 0 in JSON output; got: {stdout}"
    );
}

/// `hyalo lint --strict` exits zero on a clean vault (all files have `type`).
#[test]
fn lint_strict_exits_zero_on_clean_vault() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(
        tmp.path(),
        r#"dir = "."

[schema.types.note]
required = ["title"]
"#,
    );
    write_md(
        tmp.path(),
        "clean.md",
        "---\ntitle: Clean\ntype: note\ntags:\n  - test\n---\nBody.\n",
    );

    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--strict"])
        .assert()
        .success()
        .code(0);
}

/// `[lint] strict = true` in `.hyalo.toml` has the same effect as `--strict`.
#[test]
fn lint_strict_from_config_exits_nonzero_when_file_missing_type() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(
        tmp.path(),
        r#"dir = "."

[lint]
strict = true

[schema.types.note]
required = ["title"]
"#,
    );
    write_md(
        tmp.path(),
        "no_type.md",
        "---\ntitle: No Type\n---\nBody.\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "json"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "[lint] strict=true: lint should exit non-zero when file has no type"
    );
}

// ---------------------------------------------------------------------------
// BUG-B: HYALO003 — date-format lint rule
// ---------------------------------------------------------------------------

/// A file with `date: 2026-05-10` (valid ISO 8601) should not trigger HYALO003.
#[test]
fn lint_hyalo003_clean_date_no_violation() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(tmp.path(), "dir = \".\"\n");
    write_md(
        tmp.path(),
        "note.md",
        "---\ntitle: Note\ndate: 2026-05-10\n---\nBody.\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--rule", "HYALO003", "--format", "json"])
        .output()
        .unwrap();

    // Should be clean — exit 0
    assert!(output.status.success(), "expected exit 0 for clean date");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    // results.files_with_violations should be 0
    let with_violations = json["results"]["files_with_violations"]
        .as_u64()
        .unwrap_or(0);
    assert_eq!(
        with_violations, 0,
        "expected no violations for valid date, got: {json}"
    );
}

/// A file with `date: not-a-date` should trigger HYALO003 (warn by default).
#[test]
fn lint_hyalo003_bad_date_emits_warning() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(tmp.path(), "dir = \".\"\n");
    write_md(
        tmp.path(),
        "bad.md",
        "---\ntitle: Note\ndate: not-a-date\n---\nBody.\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--rule", "HYALO003", "--format", "json"])
        .output()
        .unwrap();

    // Default severity is warn; exit code 0 (warnings don't fail by default)
    assert!(
        output.status.success(),
        "expected exit 0 for warn-level HYALO003"
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // results.files is the array of file results
    let files = json["results"]["files"]
        .as_array()
        .expect("results.files array");
    assert!(
        !files.is_empty(),
        "expected HYALO003 violation, stdout: {stdout}"
    );

    // Check that HYALO003 appears in the rule_groups of the first file
    let found = files.iter().any(|f| {
        f["rule_groups"]
            .as_array()
            .is_some_and(|rgs| rgs.iter().any(|rg| rg["rule"] == "HYALO003"))
    });
    assert!(found, "expected HYALO003 in rule_groups, stdout: {stdout}");
}

/// HYALO003 is promoted to error under `--strict`.
#[test]
fn lint_hyalo003_strict_promotes_to_error() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(tmp.path(), "dir = \".\"\n");
    write_md(
        tmp.path(),
        "bad.md",
        "---\ntitle: Note\ndate: oops\n---\nBody.\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--strict", "--rule", "HYALO003"])
        .output()
        .unwrap();

    // Under --strict, HYALO003 is an error → exit 1
    assert!(
        !output.status.success(),
        "expected exit 1 for HYALO003 under --strict"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("HYALO003"),
        "expected HYALO003 in output, stdout: {stdout}"
    );
}

/// HYALO003 fires for `created`, `modified`, `updated` as well.
#[test]
fn lint_hyalo003_checks_all_date_keys() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(tmp.path(), "dir = \".\"\n");
    write_md(
        tmp.path(),
        "multi.md",
        "---\ntitle: Note\ncreated: bad\nmodified: 2026-05-10\nupdated: also-bad\n---\nBody.\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--rule", "HYALO003", "--format", "json"])
        .output()
        .unwrap();

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    let stdout = String::from_utf8_lossy(&output.stdout);

    let files = json["results"]["files"]
        .as_array()
        .expect("results.files array");
    assert!(
        !files.is_empty(),
        "expected HYALO003 violations, stdout: {stdout}"
    );

    // Collect all HYALO003 violation messages from rule_groups
    let all_messages: Vec<String> = files
        .iter()
        .flat_map(|f| {
            f["rule_groups"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .filter(|rg| rg["rule"] == "HYALO003")
                .flat_map(|rg| {
                    rg["violations"]
                        .as_array()
                        .unwrap_or(&vec![])
                        .iter()
                        .filter_map(|v| v["message"].as_str().map(str::to_owned))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        all_messages.iter().any(|m| m.contains("created")),
        "expected 'created' violation, messages: {all_messages:?}"
    );
    assert!(
        all_messages.iter().any(|m| m.contains("updated")),
        "expected 'updated' violation, messages: {all_messages:?}"
    );
    // `modified` has a valid date — should not appear
    assert!(
        !all_messages.iter().any(|m| m.contains("modified")),
        "unexpected 'modified' violation (date is valid), messages: {all_messages:?}"
    );
}

// ---------------------------------------------------------------------------
// HYALO004 — datetime-format lint rule
// ---------------------------------------------------------------------------

/// A schema-declared `datetime` property with a valid `YYYY-MM-DDThh:mm:ss`
/// value should not trigger HYALO004.
#[test]
fn lint_hyalo004_valid_datetime_no_violation() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(
        tmp.path(),
        r#"dir = "."

[schema.types.event]
required = ["title"]

[schema.types.event.properties.when]
type = "datetime"
"#,
    );
    write_md(
        tmp.path(),
        "ev.md",
        "---\ntype: event\ntitle: Launch\nwhen: 2026-06-04T14:30:00\n---\nBody.\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--rule", "HYALO004", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "expected clean run");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    let with_violations = json["results"]["files_with_violations"]
        .as_u64()
        .unwrap_or(0);
    assert_eq!(with_violations, 0);
}

/// A date-only value in a schema-declared `datetime` property fires HYALO004.
#[test]
fn lint_hyalo004_date_only_fires() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(
        tmp.path(),
        r#"dir = "."

[schema.types.event]
required = ["title"]

[schema.types.event.properties.when]
type = "datetime"
"#,
    );
    write_md(
        tmp.path(),
        "ev.md",
        "---\ntype: event\ntitle: Launch\nwhen: 2026-06-04\n---\nBody.\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--rule", "HYALO004", "--format", "json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    let files = json["results"]["files"]
        .as_array()
        .expect("results.files array");
    let found = files.iter().any(|f| {
        f["rule_groups"]
            .as_array()
            .is_some_and(|rgs| rgs.iter().any(|rg| rg["rule"] == "HYALO004"))
    });
    assert!(found, "expected HYALO004 in output, stdout: {stdout}");

    // The message should name the offending property.
    let any_msg = files.iter().any(|f| {
        f["rule_groups"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter(|rg| rg["rule"] == "HYALO004")
            .flat_map(|rg| rg["violations"].as_array().unwrap_or(&vec![]).clone())
            .any(|v| v["message"].as_str().is_some_and(|m| m.contains("when")))
    });
    assert!(
        any_msg,
        "expected `when` in violation message, stdout: {stdout}"
    );
}

/// HYALO004 is promoted to error under `--strict`.
#[test]
fn lint_hyalo004_strict_promotes_to_error() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(
        tmp.path(),
        r#"dir = "."

[schema.types.event]
required = ["title"]

[schema.types.event.properties.when]
type = "datetime"
"#,
    );
    write_md(
        tmp.path(),
        "ev.md",
        "---\ntype: event\ntitle: Launch\nwhen: not-a-datetime\n---\nBody.\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--strict", "--rule", "HYALO004"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !output.status.success(),
        "expected exit 1 under --strict, stdout: {stdout}"
    );
    assert!(
        stdout.contains("HYALO004"),
        "expected HYALO004 in output, stdout: {stdout}"
    );
}

/// HYALO003 appears in `lint-rules list`.
#[test]
fn lint_rules_list_includes_hyalo003() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(tmp.path(), "dir = \".\"\n");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint-rules", "list", "--format", "json"])
        .output()
        .unwrap();

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    // lint-rules list wraps rules in results array
    let rules = json["results"].as_array().expect("results array");
    let found = rules.iter().any(|r| r["id"] == "HYALO003");
    assert!(found, "HYALO003 not found in lint-rules list");
}

// ---------------------------------------------------------------------------
// UX-E: lint --strict help text mentions schema dependency
// ---------------------------------------------------------------------------

/// `hyalo lint --help` should mention that --strict requires a schema block.
#[test]
fn lint_strict_help_mentions_schema_dependency() {
    let output = hyalo_no_hints().args(["lint", "--help"]).output().unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("schema") || stdout.contains("[schema"),
        "expected --strict help to mention schema dependency, stdout: {stdout}"
    );
    assert!(
        stdout.contains("strict"),
        "expected --strict flag in help, stdout: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// UX-A: create-index text output shows hint when outside vault
// ---------------------------------------------------------------------------

/// `hyalo create-index -o /tmp/...` text output should include the hint.
#[test]
fn create_index_outside_vault_text_shows_hint() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(tmp.path(), "dir = \".\"\n");
    write_md(tmp.path(), "a.md", "---\ntitle: A\n---\n");

    let out_path = std::env::temp_dir().join("hyalo-test-outside.idx");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "create-index",
            "-o",
            out_path.to_str().unwrap(),
            "--format",
            "text",
        ])
        .output()
        .unwrap();

    // Should fail (outside vault)
    assert!(
        !output.status.success(),
        "expected failure for outside-vault index path"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("hint") || stderr.contains("--allow-outside-vault"),
        "expected hint in text output for outside-vault error, stderr: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// BUG-3: lint exit code regression guard (iter-133)
// Ensures exit code is always 0 for clean vaults and 1 for error violations.
// ---------------------------------------------------------------------------

#[test]
fn lint_exit_code_is_zero_for_clean_vault() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(
        tmp.path(),
        "dir = \".\"\n[schema.default]\nrequired = [\"title\"]\n",
    );
    write_md(
        tmp.path(),
        "clean.md",
        "---\ntitle: Clean Note\ntype: note\n---\nBody text.\n",
    );

    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint"])
        .assert()
        .code(0);
}

#[test]
fn lint_exit_code_is_one_when_error_violations_found() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(
        tmp.path(),
        "dir = \".\"\n[schema.default]\nrequired = [\"title\", \"date\"]\n",
    );
    // File is missing the required "date" property
    write_md(tmp.path(), "bad.md", "---\ntitle: Missing Date\n---\n");

    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint"])
        .assert()
        .code(1);
}

#[test]
fn lint_exit_code_is_one_for_strict_with_warnings() {
    // --strict promotes missing-type warnings to errors → exit 1.
    let tmp = TempDir::new().unwrap();
    write_schema_toml(tmp.path(), "dir = \".\"\n");
    // File has no "type" property (warning-level without --strict)
    write_md(tmp.path(), "no_type.md", "---\ntitle: No Type\n---\n");

    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--strict"])
        .assert()
        .code(1);
}

// ---------------------------------------------------------------------------
// BUG-5: HYALO001 must detect `- []` and `* []` forms (iter-133)
// ---------------------------------------------------------------------------

#[test]
fn lint_hyalo001_detects_dash_bare_bracket() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(tmp.path(), "dir = \".\"\n");
    write_md(
        tmp.path(),
        "tasks.md",
        "---\ntitle: Tasks\n---\n\n- [] Do something\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--rule", "HYALO001"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("HYALO001"),
        "HYALO001 should fire for `- []`, stdout: {stdout}"
    );
    assert_eq!(output.status.code(), Some(1), "`- []` should cause exit 1");
}

#[test]
fn lint_hyalo001_detects_star_bare_bracket() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(tmp.path(), "dir = \".\"\n");
    write_md(
        tmp.path(),
        "tasks.md",
        "---\ntitle: Tasks\n---\n\n* [] Do something\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--rule", "HYALO001"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("HYALO001"),
        "HYALO001 should fire for `* []`, stdout: {stdout}"
    );
    assert_eq!(output.status.code(), Some(1), "`* []` should cause exit 1");
}

#[test]
fn lint_hyalo001_fix_dash_bare_bracket() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(tmp.path(), "dir = \".\"\n");
    write_md(
        tmp.path(),
        "tasks.md",
        "---\ntitle: Tasks\n---\n\n- [] Do something\n",
    );

    // Apply fix
    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--fix", "--rule", "HYALO001"])
        .assert()
        .success();

    // After fix, no HYALO001 violations remain
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--rule", "HYALO001"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "after --fix, HYALO001 should not fire"
    );
    let content = std::fs::read_to_string(tmp.path().join("tasks.md")).unwrap();
    assert!(
        content.contains("- [ ] Do something"),
        "fix should insert space: `- [ ] Do something`, got: {content}"
    );
}

// ---------------------------------------------------------------------------
// BUG-1: required_sections enforced by lint_one_file_extended (iter-140)
// ---------------------------------------------------------------------------

#[test]
fn lint_required_sections_missing_emits_schema_error() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(
        tmp.path(),
        "dir = \".\"\n\n[schema.types.note]\nrequired = [\"title\"]\nrequired_sections = [\"## Tasks\", \"## Notes\"]\n",
    );
    write_md(
        tmp.path(),
        "no_sections.md",
        "---\ntitle: Test\ntype: note\n---\n\nBody without the required sections.\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--file", "no_sections.md"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected exit 1 for missing required sections"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("missing required section"),
        "expected 'missing required section' in output, got:\n{combined}"
    );
}

#[test]
fn lint_required_sections_all_present_exits_zero() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(
        tmp.path(),
        "dir = \".\"\n\n[schema.types.note]\nrequired = [\"title\"]\nrequired_sections = [\"## Tasks\"]\n",
    );
    write_md(
        tmp.path(),
        "with_section.md",
        "---\ntitle: Test\ntype: note\n---\n\n## Tasks\n\nDo things.\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--file", "with_section.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "expected exit 0 when required section is present; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn lint_required_sections_out_of_order_is_violation() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(
        tmp.path(),
        "dir = \".\"\n\n[schema.types.note]\nrequired = [\"title\"]\nrequired_sections = [\"## Tasks\", \"## Notes\"]\n",
    );
    // Sections are reversed compared to schema order.
    write_md(
        tmp.path(),
        "reversed.md",
        "---\ntitle: Test\ntype: note\n---\n\n## Notes\n\nContent.\n\n## Tasks\n\nDo things.\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--file", "reversed.md"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected exit 1 for out-of-order required sections"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("out of order") || combined.contains("missing required section"),
        "expected order violation in output, got:\n{combined}"
    );
}

// ---------------------------------------------------------------------------
// iter-158: lint --fix pipeline fixes (byte/char columns, MD009 blank-line
// injection, MD047 convergence, frontmatter+body combined write, severity
// tiebreak, oversized-file skip, idempotency)
// ---------------------------------------------------------------------------

#[test]
fn lint_fix_md009_does_not_inject_blank_line() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(tmp.path(), "dir = \".\"\n");
    write_md(tmp.path(), "note.md", "---\ntitle: Note\n---\nx   \ny\n");

    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--fix", "--rule", "MD009"])
        .assert()
        .success();

    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(
        content.ends_with("x\ny\n"),
        "MD009 fix must not insert a blank line, got: {content:?}"
    );
}

#[test]
fn lint_fix_md009_preserves_crlf_line_endings() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(tmp.path(), "dir = \".\"\n");
    write_md(
        tmp.path(),
        "note.md",
        "---\r\ntitle: Note\r\n---\r\nx   \r\ny\r\n",
    );

    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--fix", "--rule", "MD009"])
        .assert()
        .success();

    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(
        content.ends_with("x\r\ny\r\n"),
        "MD009 fix must keep CRLF endings uniformly, got: {content:?}"
    );
    assert!(
        !content.contains("\n\r\n"),
        "MD009 fix must not produce mixed/duplicated line endings, got: {content:?}"
    );
}

#[test]
fn lint_fix_hyalo001_non_ascii_line() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(tmp.path(), "dir = \".\"\n");
    write_md(
        tmp.path(),
        "note.md",
        "---\ntitle: Note\n---\n\n[] café task\n",
    );

    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--fix", "--rule", "HYALO001"])
        .assert()
        .success();

    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(
        content.contains("- [ ] café task"),
        "HYALO001 fix must apply on a non-ASCII line, got: {content:?}"
    );
}

#[test]
fn lint_fix_md009_trailing_space_on_cjk_line() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(tmp.path(), "dir = \".\"\n");
    write_md(
        tmp.path(),
        "note.md",
        "---\ntitle: Note\n---\n日本語のテキスト   \n",
    );

    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--fix", "--rule", "MD009"])
        .assert()
        .success();

    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(
        content.ends_with("日本語のテキスト\n"),
        "MD009 fix must strip trailing spaces on a CJK line, got: {content:?}"
    );
}

#[test]
fn lint_fix_md047_converges_in_one_run() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(tmp.path(), "dir = \".\"\n");
    write_md(tmp.path(), "note.md", "---\ntitle: Note\n---\nbody\n\n");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--fix", "--rule", "MD047", "--format", "json"])
        .output()
        .unwrap();
    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    let val: serde_json::Value = serde_json::from_str(stdout).unwrap();
    assert_eq!(val["results"]["total_fixed"], 1);
    assert_eq!(val["results"]["total_remaining"], 0);

    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(
        content.ends_with("body\n") && !content.ends_with("body\n\n"),
        "MD047 must converge to exactly one trailing newline in one run, got: {content:?}"
    );

    // A second run must report zero fixes — no perpetual "fixed=1" loop.
    let output2 = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--fix", "--rule", "MD047", "--format", "json"])
        .output()
        .unwrap();
    let stdout2 = std::str::from_utf8(&output2.stdout).unwrap();
    let val2: serde_json::Value = serde_json::from_str(stdout2).unwrap();
    assert_eq!(val2["results"]["total_fixed"], 0);
    assert_eq!(val2["results"]["files"].as_array().unwrap().len(), 0);
}

#[test]
fn lint_fix_frontmatter_and_body_fixes_both_persist() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(
        tmp.path(),
        "dir = \".\"\n\n[schema.default]\nrequired = [\"title\"]\n\n[schema.default.defaults]\nstatus = \"draft\"\n",
    );
    write_md(
        tmp.path(),
        "note.md",
        "---\ntitle: Note\n---\nline with trailing space   \n",
    );

    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--fix"])
        .assert()
        .success();

    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(
        content.contains("status: draft"),
        "frontmatter default fix must persist, got: {content:?}"
    );
    assert!(
        content.contains("line with trailing space\n"),
        "body fix must persist alongside the frontmatter fix, got: {content:?}"
    );
    assert!(
        !content.contains("space   \n"),
        "trailing spaces must actually be removed, got: {content:?}"
    );
}

#[test]
fn lint_fix_idempotent_second_run_is_a_no_op() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(tmp.path(), "dir = \".\"\n");
    write_md(
        tmp.path(),
        "note.md",
        "---\ntitle: Note\n---\n- [] task with trailing space   \nAnother line.\n\n\n\nToo many blanks above.\n",
    );

    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--fix"])
        .assert()
        .success();
    let after_first = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--fix", "--format", "json"])
        .output()
        .unwrap();
    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    let val: serde_json::Value = serde_json::from_str(stdout).unwrap();
    assert_eq!(
        val["results"]["total_fixed"], 0,
        "second --fix run should find nothing left to fix, got: {stdout}"
    );

    let after_second = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert_eq!(
        after_first, after_second,
        "file bytes must be unchanged by the second --fix run"
    );
}

#[test]
fn lint_fix_error_severity_wins_overlap_with_warn() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(tmp.path(), "dir = \".\"\n");
    // Bare checkbox (HYALO001, Error) with trailing whitespace on the same
    // line (MD009, Warn) — their fix ranges overlap.
    write_md(tmp.path(), "note.md", "---\ntitle: Note\n---\n[] task   \n");

    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--fix"])
        .assert()
        .success();

    let content = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(
        content.contains("- [ ] task"),
        "HYALO001's fix must win the overlap, got: {content:?}"
    );
    assert!(
        !content.contains("task   \n"),
        "trailing spaces should also converge across passes, got: {content:?}"
    );
}

#[test]
fn lint_oversized_file_is_skipped_with_warning() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(tmp.path(), "dir = \".\"\n");
    let path = tmp.path().join("big.md");
    let file = std::fs::File::create(&path).unwrap();
    // Sparse file: exceeds the 100 MiB scanner limit without writing real
    // bytes to disk.
    file.set_len(101 * 1024 * 1024).unwrap();

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "json", "big.md"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("skipping") && stderr.contains("big.md"),
        "expected a skip warning on stderr, got: {stderr}"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "an oversized-file skip is a warning, not an error"
    );

    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    let val: serde_json::Value = serde_json::from_str(stdout).unwrap();
    assert_eq!(
        val["results"]["files_with_violations"], 1,
        "the skipped file must be reported as not-clean, not silently dropped"
    );
}

// ---------------------------------------------------------------------------
// OKF foundations (iter-163): tz timestamps, reserved-file exemption,
// bundle-root okf_version, bundle-absolute links
// ---------------------------------------------------------------------------

/// A `.hyalo.toml` configured like an OKF bundle: a `datetime-tz` timestamp,
/// `[schema] exempt` reserved files, `site_prefix = ""` for bundle-root links.
fn okf_schema_toml() -> &'static str {
    r#"dir = "."
site_prefix = ""

[schema]
exempt = ["**/index.md", "**/log.md"]

[schema.types.concept]
required = ["title"]

[schema.types.concept.properties.timestamp]
type = "datetime-tz"
"#
}

/// A whole OKF-style bundle lints clean: tz-aware timestamps (both YAML
/// spellings), reserved `index.md`/`log.md` skip required-`type`, the root
/// `index.md` carries a lone `okf_version`, and bundle-absolute links resolve.
#[test]
fn lint_okf_bundle_zero_false_positives() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(tmp.path(), okf_schema_toml());

    // Root index.md: only an okf_version key, no `type`. Links to a concept via
    // a bundle-absolute path. Both must lint clean.
    write_md(
        tmp.path(),
        "index.md",
        "---\nokf_version: \"0.1\"\n---\nSee [bitcoin](/concepts/bitcoin.md)\n",
    );
    // Reserved log.md: no `type`, must be exempt.
    write_md(tmp.path(), "log.md", "---\n---\nChangelog body.\n");
    // Concept with quoted offset timestamp (sample-bundle spelling).
    write_md(
        tmp.path(),
        "concepts/bitcoin.md",
        "---\ntype: concept\ntitle: Bitcoin\ntimestamp: '2026-05-28T22:44:47+00:00'\n---\nBody.\n",
    );
    // Concept with unquoted Z timestamp (blog-example spelling).
    write_md(
        tmp.path(),
        "concepts/ledger.md",
        "---\ntype: concept\ntitle: Ledger\ntimestamp: 2026-05-28T14:30:00Z\n---\nBody.\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--strict", "--format", "json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "OKF bundle should lint clean under --strict; stdout: {stdout}, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        json["results"]["files_with_violations"]
            .as_u64()
            .unwrap_or(99),
        0,
        "expected zero violations; stdout: {stdout}"
    );
}

/// A tz-aware value in a `datetime-tz` property that is actually naive (no
/// offset) still fires HYALO004.
#[test]
fn lint_okf_datetime_tz_rejects_naive() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(tmp.path(), okf_schema_toml());
    write_md(
        tmp.path(),
        "concepts/bad.md",
        "---\ntype: concept\ntitle: Bad\ntimestamp: 2026-05-28T14:30:00\n---\nBody.\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--rule", "HYALO004", "--format", "json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let files = json["results"]["files"].as_array().expect("files array");
    let found = files.iter().any(|f| {
        f["rule_groups"]
            .as_array()
            .is_some_and(|rgs| rgs.iter().any(|rg| rg["rule"] == "HYALO004"))
    });
    assert!(
        found,
        "naive value in datetime-tz property should fire HYALO004; stdout: {stdout}"
    );
}

/// A non-reserved file with an `okf_version` key (but no `type`) is still
/// flagged — the root-index allowance is scoped to `index.md`, not arbitrary
/// files.
#[test]
fn lint_okf_version_key_scoped_to_root_index() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(tmp.path(), okf_schema_toml());
    // A non-index, non-exempt file carrying okf_version → undeclared property.
    write_md(
        tmp.path(),
        "concepts/rogue.md",
        "---\ntype: concept\ntitle: Rogue\nokf_version: \"0.1\"\n---\nBody.\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--strict", "--format", "json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !output.status.success(),
        "okf_version in a non-root file should be flagged; stdout: {stdout}"
    );
    assert!(
        stdout.contains("okf_version"),
        "expected undeclared-property message naming okf_version; stdout: {stdout}"
    );
}

/// Bundle-absolute links resolve from bundle root with `site_prefix = ""`,
/// even when a bundle subdir name would collide with an auto-derived prefix.
#[test]
fn lint_okf_bundle_absolute_links_not_broken() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(tmp.path(), okf_schema_toml());
    // The bundle has a `concepts/` subdir; a bundle-absolute link to it must
    // resolve (not be reported broken).
    write_md(
        tmp.path(),
        "index.md",
        "---\nokf_version: \"0.1\"\n---\n[c](/concepts/x.md)\n",
    );
    write_md(
        tmp.path(),
        "concepts/x.md",
        "---\ntype: concept\ntitle: X\n---\nBody.\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["find", "--broken-links", "--format", "json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("/concepts/x.md") && !stdout.contains("\"x.md\""),
        "bundle-absolute link should resolve, not appear broken; stdout: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// --format github (GitHub Actions annotations)
// ---------------------------------------------------------------------------

/// A vault with errors + warnings emits `::error`/`::warning` workflow commands
/// (paths relative to the repo root — here the vault IS the CWD, so no prefix)
/// plus a summary line, and exits 1.
#[test]
fn lint_github_emits_annotations_and_exits_one() {
    let tmp = setup_vault_with_schema();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "github", "missing_date.md"])
        .output()
        .unwrap();
    assert_eq!(output.status.code().unwrap(), 1, "errors -> exit 1");
    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    // A missing required property is an error annotation on the right file.
    assert!(
        stdout.contains("::error file=missing_date.md,"),
        "expected error annotation for missing_date.md; got:\n{stdout}"
    );
    assert!(
        stdout.contains("title=SCHEMA::") && stdout.contains("missing required property"),
        "expected SCHEMA title + message; got:\n{stdout}"
    );
    // Summary line is the last non-empty line.
    let last = stdout.lines().rfind(|l| !l.is_empty()).unwrap();
    assert!(
        last.contains("error") && last.contains("warning") && last.contains(" in "),
        "expected summary line; got: {last}"
    );
}

/// A clean vault under `--format github` prints only the summary line and exits 0.
#[test]
fn lint_github_clean_vault_summary_only_exit_zero() {
    let tmp = setup_vault_with_schema();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "github", "clean.md"])
        .output()
        .unwrap();
    assert_eq!(output.status.code().unwrap(), 0, "clean -> exit 0");
    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    assert!(
        !stdout.contains("::error") && !stdout.contains("::warning"),
        "clean vault should have no annotations; got:\n{stdout}"
    );
    assert_eq!(
        stdout.trim(),
        "0 errors, 0 warnings in 0 files",
        "expected summary-only output"
    );
}

/// `--strict` flips the missing-`type` annotation from `::warning` to `::error`.
#[test]
fn lint_github_strict_promotes_warning_to_error() {
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
    // A file with `title` but no `type` — triggers the missing-type warning.
    write_md(
        tmp.path(),
        "no_type.md",
        md!(r"
---
title: No Type
---
Body.
"),
    );

    // Without --strict: missing-type is a warning.
    let warn_out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "github", "no_type.md"])
        .output()
        .unwrap();
    let warn_stdout = std::str::from_utf8(&warn_out.stdout).unwrap();
    assert!(
        warn_stdout.contains("::warning file=no_type.md,"),
        "expected warning annotation without --strict; got:\n{warn_stdout}"
    );
    assert_eq!(warn_out.status.code().unwrap(), 0, "warning-only -> exit 0");

    // With --strict: promoted to an error, exit 1.
    let strict_out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--strict", "--format", "github", "no_type.md"])
        .output()
        .unwrap();
    let strict_stdout = std::str::from_utf8(&strict_out.stdout).unwrap();
    assert!(
        strict_stdout.contains("::error file=no_type.md,"),
        "expected error annotation with --strict; got:\n{strict_stdout}"
    );
    assert_eq!(
        strict_out.status.code().unwrap(),
        1,
        "strict error -> exit 1"
    );
}

/// Paths are prefixed with the vault dir relative to CWD when linting from a
/// parent directory (`--dir sub/kb`), so annotations resolve against the repo root.
#[test]
fn lint_github_prefixes_paths_when_dir_below_cwd() {
    let tmp = TempDir::new().unwrap();
    let kb = tmp.path().join("kb");
    std::fs::create_dir_all(&kb).unwrap();
    write_schema_toml(
        &kb,
        r#"[schema.default]
required = ["title"]
"#,
    );
    write_md(
        &kb,
        "bad.md",
        md!(r"
---
title: Bad
status: nope
---
Body.
"),
    );

    // Run from the parent (repo root), pointing --dir at the vault subdir.
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--dir", "kb", "--format", "github", "bad.md"])
        .output()
        .unwrap();
    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    // Every annotation path must be prefixed with the vault dir.
    for line in stdout.lines().filter(|l| l.starts_with("::")) {
        assert!(
            line.contains("file=kb/bad.md"),
            "annotation path should be repo-root-relative (kb/bad.md); got: {line}"
        );
    }
}

/// `--fix --dry-run --format github` uses the fix-mode payload shape
/// (`remaining_groups`, not `rule_groups`) — the renderer must still emit
/// annotations for violations that fix mode can't resolve (a missing required
/// property isn't autofixable), not silently produce zero annotations.
#[test]
fn lint_github_fix_dry_run_emits_remaining_violations() {
    let tmp = setup_vault_with_schema();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "lint",
            "--fix",
            "--dry-run",
            "--format",
            "github",
            "missing_date.md",
        ])
        .output()
        .unwrap();
    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    assert!(
        stdout.contains("::error file=missing_date.md,"),
        "expected a remaining-violation annotation under --fix --dry-run; got:\n{stdout}"
    );
    assert!(
        stdout.contains("missing required property"),
        "expected the SCHEMA message; got:\n{stdout}"
    );
    assert_eq!(output.status.code().unwrap(), 1, "errors -> exit 1");
}

/// `--format github` is rejected for non-lint subcommands with a clear error.
#[test]
fn github_format_rejected_for_non_lint() {
    let tmp = setup_vault_with_schema();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["find", "--format", "github", "note"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code().unwrap(),
        2,
        "non-lint github -> exit 2"
    );
    let stderr = std::str::from_utf8(&output.stderr).unwrap();
    assert!(
        stderr.contains("only supported by `hyalo lint`"),
        "expected lint-only rejection message; got:\n{stderr}"
    );
}

// ---------------------------------------------------------------------------
// iter-174: HYALO005 frontmatter-parse-error + honest caps + skip visibility
// ---------------------------------------------------------------------------

/// Write a temp file containing the given lines and return its handle.
fn write_list_file(lines: &[&str]) -> tempfile::NamedTempFile {
    use std::io::Write as _;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    for line in lines {
        writeln!(f, "{line}").unwrap();
    }
    f
}

/// A vault with a single vault under a `dir = "."` schema and one
/// corrupt-frontmatter file (duplicate YAML key). Returns the tempdir.
fn setup_vault_with_corrupt_file() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(tmp.path(), "dir = \".\"\n[schema.default]\nrequired = []\n");
    // Duplicate mapping key — rejected by the frontmatter parser.
    write_md(
        tmp.path(),
        "corrupt.md",
        "---\ntitle: A\ntitle: B\n---\n# Body\n",
    );
    tmp
}

/// A corrupt-frontmatter file surfaces as an error-severity `HYALO005`
/// violation in text/json/github and exits 1 — never `0 files checked, no
/// issues` (RB-3 / df-own-kb B3).
#[test]
fn lint_corrupt_frontmatter_surfaces_hyalo005_all_formats() {
    let tmp = setup_vault_with_corrupt_file();

    // text
    let text = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "text", "corrupt.md"])
        .output()
        .unwrap();
    assert_eq!(text.status.code().unwrap(), 1, "corrupt file -> exit 1");
    let text_out = std::str::from_utf8(&text.stdout).unwrap();
    assert!(
        text_out.contains("HYALO005") && text_out.contains("could not parse frontmatter"),
        "text output must name HYALO005 + the parse error; got:\n{text_out}"
    );
    assert!(
        !text_out.contains("0 files checked, no issues"),
        "corrupt file must never report a clean run; got:\n{text_out}"
    );

    // json
    let json = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "json", "corrupt.md"])
        .output()
        .unwrap();
    assert_eq!(json.status.code().unwrap(), 1);
    let v: serde_json::Value = serde_json::from_slice(&json.stdout).expect("lint json parses");
    let results = &v["results"];
    assert_eq!(results["errors"].as_u64().unwrap(), 1, "one error counted");
    assert_eq!(
        results["files_checked"].as_u64().unwrap(),
        1,
        "corrupt file still counts in files_checked"
    );
    let rule = results["files"][0]["rule_groups"][0]["rule"]
        .as_str()
        .unwrap();
    assert_eq!(rule, "HYALO005", "rule id is stable HYALO005");

    // github
    let gh = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "github", "corrupt.md"])
        .output()
        .unwrap();
    assert_eq!(gh.status.code().unwrap(), 1);
    let gh_out = std::str::from_utf8(&gh.stdout).unwrap();
    assert!(
        gh_out.contains("::error file=corrupt.md,") && gh_out.contains("title=HYALO005::"),
        "github must emit an ::error annotation titled HYALO005; got:\n{gh_out}"
    );
}

/// A full-vault run includes the corrupt file in its counts and exits 1.
#[test]
fn lint_full_vault_counts_corrupt_file() {
    let tmp = setup_vault_with_corrupt_file();
    // Add a clean file alongside the corrupt one.
    write_md(tmp.path(), "ok.md", "---\ntitle: Fine\n---\n# Body\n");

    let out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code().unwrap(), 1, "corrupt in vault -> exit 1");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["results"]["errors"].as_u64().unwrap(), 1);
    assert_eq!(v["results"]["files_checked"].as_u64().unwrap(), 2);
}

/// HYALO005 is listed by `lint-rules list`, default-on, error by default.
#[test]
fn lint_rules_list_includes_hyalo005() {
    let tmp = setup_vault_with_corrupt_file();
    let out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint-rules", "show", "HYALO005", "--format", "json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let r = &v["results"];
    assert_eq!(r["id"].as_str().unwrap(), "HYALO005");
    assert_eq!(r["default_severity"].as_str().unwrap(), "error");
    assert!(r["default_enabled"].as_bool().unwrap());
}

/// `--limit 0` means "unlimited" on lint files: it must NOT empty the file list
/// or drop the error count (which would exit 0 on a corrupt vault). ff-rdp B5.
#[test]
fn lint_limit_zero_is_unlimited_not_empty() {
    let tmp = setup_vault_with_corrupt_file();
    // A second corrupt file so a truncation-to-N bug would be observable.
    write_md(tmp.path(), "corrupt2.md", "---\nk: [oops\n---\n# Body\n");

    let out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "json", "--limit", "0"])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code().unwrap(),
        1,
        "--limit 0 must still exit 1 when errors exist"
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v["results"]["errors"].as_u64().unwrap(),
        2,
        "--limit 0 must report all errors, not zero"
    );
    let files = v["results"]["files"].as_array().unwrap();
    assert_eq!(files.len(), 2, "--limit 0 must show all files, not empty");
    assert!(
        !v["results"]["files_truncated"].as_bool().unwrap(),
        "--limit 0 lifts the cap, so files_truncated is false"
    );
}

/// `--limit N` on json output honors the file cap while keeping the error
/// counter and `files_truncated` accurate (mapl BUG-4).
#[test]
fn lint_limit_n_caps_display_but_counts_stay_honest() {
    let tmp = setup_vault_with_corrupt_file();
    write_md(tmp.path(), "corrupt2.md", "---\nk: [oops\n---\n# Body\n");
    write_md(tmp.path(), "corrupt3.md", "---\nx: y\nx: z\n---\n# Body\n");

    let out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "json", "--limit", "1"])
        .output()
        .unwrap();
    assert_eq!(out.status.code().unwrap(), 1);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    // Error count reflects ALL corrupt files, not just the shown one.
    assert_eq!(v["results"]["errors"].as_u64().unwrap(), 3);
    assert_eq!(v["results"]["files_checked"].as_u64().unwrap(), 3);
    assert_eq!(v["results"]["files"].as_array().unwrap().len(), 1);
    assert!(v["results"]["files_truncated"].as_bool().unwrap());
}

/// `--format github` never truncates annotations: with more files than the
/// default 50-file cap, every file still emits its annotation. Regression test
/// for the "caps stay lifted for github" guarantee.
#[test]
fn lint_github_never_truncates_annotations() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(tmp.path(), "dir = \".\"\n[schema.default]\nrequired = []\n");
    // 60 corrupt files > the default max_files cap of 50.
    for i in 0..60 {
        write_md(
            tmp.path(),
            &format!("bad{i:02}.md"),
            "---\na: 1\na: 2\n---\n# Body\n",
        );
    }
    let out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "github"])
        .output()
        .unwrap();
    assert_eq!(out.status.code().unwrap(), 1);
    let stdout = std::str::from_utf8(&out.stdout).unwrap();
    let annotations = stdout
        .lines()
        .filter(|l| l.starts_with("::error file=bad"))
        .count();
    assert_eq!(
        annotations, 60,
        "github must annotate all 60 files, not cap at 50"
    );
}

/// Skip-summary line appears in BOTH text and github when `--files-from` drops
/// input paths, with the correct counters; absent when all inputs resolve (UX-B).
#[test]
fn lint_skip_summary_text_and_github() {
    let tmp = setup_vault_with_corrupt_file();
    write_md(tmp.path(), "ok.md", "---\ntitle: Fine\n---\n# Body\n");
    std::fs::write(tmp.path().join("notes.txt"), "not markdown").unwrap();

    // 1 real .md + 2 missing + 1 non-md.
    let list = write_list_file(&["ok.md", "gone1.md", "gone2.md", "notes.txt"]);
    let list_path = list.path().to_str().unwrap();

    // text: note on stderr.
    let text = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "text", "--files-from", list_path])
        .output()
        .unwrap();
    let text_err = std::str::from_utf8(&text.stderr).unwrap();
    assert!(
        text_err.contains("note: 2 input paths missing")
            && text_err.contains("1 non-markdown skipped"),
        "text must print a skip note with counts; got stderr:\n{text_err}"
    );

    // github: ::notice on stdout.
    let gh = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "github", "--files-from", list_path])
        .output()
        .unwrap();
    let gh_out = std::str::from_utf8(&gh.stdout).unwrap();
    assert!(
        gh_out.contains("::notice::2 input paths missing")
            && gh_out.contains("1 non-markdown skipped"),
        "github must emit a ::notice with counts; got:\n{gh_out}"
    );

    // absent when everything resolves.
    let clean_list = write_list_file(&["ok.md"]);
    let clean = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "lint",
            "--format",
            "text",
            "--files-from",
            clean_list.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let clean_err = std::str::from_utf8(&clean.stderr).unwrap();
    assert!(
        !clean_err.contains("note:"),
        "no skip note when all inputs resolve; got stderr:\n{clean_err}"
    );
}

/// A single missing input path is reported as singular ("1 input path
/// missing"), not "1 input paths missing" (redogfood fix-wave v0.18.0).
#[test]
fn lint_skip_summary_singular_missing_path() {
    let tmp = setup_vault_with_corrupt_file();
    write_md(tmp.path(), "ok.md", "---\ntitle: Fine\n---\n# Body\n");

    // Exactly 1 real .md + 1 missing path.
    let list = write_list_file(&["ok.md", "gone.md"]);
    let list_path = list.path().to_str().unwrap();

    let text = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "text", "--files-from", list_path])
        .output()
        .unwrap();
    let text_err = std::str::from_utf8(&text.stderr).unwrap();
    assert!(
        text_err.contains("note: 1 input path missing"),
        "singular count must say 'path', not 'paths'; got stderr:\n{text_err}"
    );
    assert!(
        !text_err.contains("1 input paths missing"),
        "must not use the plural form for count=1; got stderr:\n{text_err}"
    );

    let gh = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "github", "--files-from", list_path])
        .output()
        .unwrap();
    let gh_out = std::str::from_utf8(&gh.stdout).unwrap();
    assert!(
        gh_out.contains("::notice::1 input path missing"),
        "github format must also use the singular form; got:\n{gh_out}"
    );
}

/// An explicitly named `--file` that is excluded by `[lint] ignore` prints a
/// notice instead of silently reporting `0 files checked`.
#[test]
fn lint_ignored_named_file_prints_notice() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(
        tmp.path(),
        "dir = \".\"\n[schema.default]\nrequired = []\n[lint]\nignore = [\"skip.md\"]\n",
    );
    write_md(tmp.path(), "skip.md", "---\ntitle: Skipped\n---\n# Body\n");

    let out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "text", "skip.md"])
        .output()
        .unwrap();
    let stderr = std::str::from_utf8(&out.stderr).unwrap();
    assert!(
        stderr.contains("excluded by [lint] ignore") && stderr.contains("skip.md"),
        "expected an ignore-exclusion notice naming skip.md; got stderr:\n{stderr}"
    );
}

/// `--fix --dry-run --format github` marks would-be-fixed violations distinctly
/// from remaining ones and uses a `N fixable, M remaining` summary — so the
/// output is not identical to a plain lint run (df-own-kb U6).
#[test]
fn lint_github_fix_dry_run_distinguishable() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(tmp.path(), "dir = \".\"\n[schema.default]\nrequired = []\n");
    // A bare checkbox is an autofixable HYALO001 violation.
    write_md(
        tmp.path(),
        "fixme.md",
        "---\ntitle: Fix Me\n---\n[] bare task\n",
    );

    let out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "lint",
            "--fix",
            "--dry-run",
            "--format",
            "github",
            "fixme.md",
        ])
        .output()
        .unwrap();
    let stdout = std::str::from_utf8(&out.stdout).unwrap();
    assert!(
        stdout.contains("::notice") && stdout.contains("[fixable]"),
        "fixable violations must render as ::notice with a [fixable] title; got:\n{stdout}"
    );
    // Distinct summary shape.
    let last = stdout.lines().rfind(|l| !l.is_empty()).unwrap();
    assert!(
        last.contains("fixable") && last.contains("remaining"),
        "summary must use the fixable/remaining shape; got: {last}"
    );

    // Plain lint of the same file uses the error/warning summary shape — proving
    // the two outputs differ.
    let plain = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "github", "fixme.md"])
        .output()
        .unwrap();
    let plain_out = std::str::from_utf8(&plain.stdout).unwrap();
    assert_ne!(
        stdout, plain_out,
        "fix --dry-run github output must differ from plain lint output"
    );
}

// ---------------------------------------------------------------------------
// Exempt globs honor `[links] case_insensitive` (redogfood fix-wave v0.18.0)
// ---------------------------------------------------------------------------
//
// `hyalo okf index` already treats `INDEX.md` as the reserved index file on a
// case-insensitive filesystem (auto-detected or forced via `[links]
// case_insensitive`); `hyalo lint`'s `[schema] exempt` globs must agree, or a
// literally-named `INDEX.md` spuriously fails the required-`type` check that
// `**/index.md` was supposed to exempt it from.
//
// Both tests below force the mode explicitly via config rather than relying
// on host detection, so they pass identically on Linux, macOS, and Windows.

fn exempt_index_schema_toml() -> &'static str {
    r#"dir = "."

[schema]
exempt = ["**/index.md"]

[schema.default]
required = ["type"]
"#
}

/// `[links] case_insensitive = "true"` forces exempt-glob matching to fold
/// case: a literal `INDEX.md` is treated the same as `index.md` and is
/// exempt from the `required = ["type"]` default schema.
#[test]
fn lint_exempt_glob_case_insensitive_true_exempts_uppercase_index() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(
        tmp.path(),
        &format!(
            "{}\n[links]\ncase_insensitive = \"true\"\n",
            exempt_index_schema_toml()
        ),
    );
    // No `type` property — would fail `required = ["type"]` unless exempt.
    write_md(tmp.path(), "INDEX.md", "---\n---\nBody.\n");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "INDEX.md must be exempt under case_insensitive = true; stdout: {stdout}, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        json["results"]["files_with_violations"]
            .as_u64()
            .unwrap_or(99),
        0,
        "expected zero violations for exempt INDEX.md; stdout: {stdout}"
    );
}

/// `[links] case_insensitive = "false"` keeps exempt-glob matching strict:
/// `INDEX.md` does NOT match `**/index.md` and the missing-`type` violation
/// still fires. This is the inverse of the test above and guards against a
/// fix that makes exempt matching unconditionally case-insensitive.
#[test]
fn lint_exempt_glob_case_insensitive_false_does_not_exempt_uppercase_index() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(
        tmp.path(),
        &format!(
            "{}\n[links]\ncase_insensitive = \"false\"\n",
            exempt_index_schema_toml()
        ),
    );
    write_md(tmp.path(), "INDEX.md", "---\n---\nBody.\n");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !output.status.success(),
        "INDEX.md must NOT be exempt under case_insensitive = false; stdout: {stdout}"
    );
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        json["results"]["files_with_violations"]
            .as_u64()
            .unwrap_or(0),
        1,
        "expected the missing-type violation on INDEX.md; stdout: {stdout}"
    );

    // The genuinely-exempt lowercase `index.md` still lints clean regardless
    // of case_insensitive mode.
    write_md(tmp.path(), "index.md", "---\n---\nBody.\n");
    let output2 = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "json", "index.md"])
        .output()
        .unwrap();
    assert!(
        output2.status.success(),
        "lowercase index.md must remain exempt; stdout: {}",
        String::from_utf8_lossy(&output2.stdout)
    );
}

// ---------------------------------------------------------------------------
// OKF profile: reserved-file predicates (is_index_file/is_log_file) honor
// `[links] case_insensitive` too (same fix-wave, okf_lint.rs half of the bug).
//
// The SCHEMA exempt-glob pass (tested above) is one of two independent
// reserved-file checks; `--profile okf`'s own `is_index_file`/`is_log_file`
// used to be hard-coded case-sensitive, so an adopted `INDEX.md` was exempt
// from SCHEMA but still fell through to the concept-doc rules (spurious
// `OKF-CITATIONS-PRESENT`) instead of being treated as the reserved index.
// ---------------------------------------------------------------------------

/// `[links] case_insensitive = "true"` + `--profile okf`: an `INDEX.md` with
/// no `# Citations` section must NOT get `OKF-CITATIONS-PRESENT` (it is now
/// recognized as the reserved index, not a concept doc) — it may still warn
/// `OKF-INDEX-STRUCTURE` because its body is prose, not a link list.
#[test]
fn lint_okf_profile_case_insensitive_true_index_skips_citations_present() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(
        tmp.path(),
        "dir = \".\"\n\n[links]\ncase_insensitive = \"true\"\n",
    );
    // Prose body, no link list and no `# Citations` — would trip both
    // OKF-INDEX-STRUCTURE (not a link list) and, if misclassified as a
    // concept doc, OKF-CITATIONS-PRESENT.
    write_md(
        tmp.path(),
        "INDEX.md",
        "---\ntype: BigQuery Table\n---\nThis is prose, not a link list.\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "json", "--profile", "okf"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let rule_ids: Vec<&str> = json["results"]["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| f["rule_groups"].as_array())
        .flatten()
        .filter_map(|g| g["rule"].as_str())
        .collect();

    assert!(
        !rule_ids.contains(&"OKF-CITATIONS-PRESENT"),
        "case-folded INDEX.md must not be treated as a concept doc: stdout={stdout}"
    );
    assert!(
        rule_ids.contains(&"OKF-INDEX-STRUCTURE"),
        "case-folded INDEX.md should still get OKF-INDEX-STRUCTURE for its prose body: stdout={stdout}"
    );
}

/// `[links] case_insensitive = "false"` + `--profile okf`: behavior is
/// unchanged from before this whole fix wave — `INDEX.md` is treated as an
/// ordinary concept doc and DOES get `OKF-CITATIONS-PRESENT` when it lacks a
/// `# Citations` section.
#[test]
fn lint_okf_profile_case_insensitive_false_index_keeps_citations_present() {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(
        tmp.path(),
        "dir = \".\"\n\n[links]\ncase_insensitive = \"false\"\n",
    );
    write_md(
        tmp.path(),
        "INDEX.md",
        "---\ntype: BigQuery Table\n---\nThis is prose, not a link list.\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "json", "--profile", "okf"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let rule_ids: Vec<&str> = json["results"]["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| f["rule_groups"].as_array())
        .flatten()
        .filter_map(|g| g["rule"].as_str())
        .collect();

    assert!(
        rule_ids.contains(&"OKF-CITATIONS-PRESENT"),
        "with case_insensitive=false, INDEX.md must remain an ordinary concept doc: stdout={stdout}"
    );
}
