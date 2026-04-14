use super::common::{hyalo_no_hints, write_md};
use tempfile::TempDir;

/// Create a `.hyalo.toml` in `dir` with the given content.
fn write_toml(dir: &std::path::Path, content: &str) {
    std::fs::write(dir.join(".hyalo.toml"), content).unwrap();
}

// ---------------------------------------------------------------------------
// When --dir is provided, the target's .hyalo.toml must be loaded,
// not the CWD's.
// ---------------------------------------------------------------------------

/// Regression test for Bug 3: `--dir` must use the target directory's
/// `.hyalo.toml`, not CWD's config.
///
/// Setup:
///   - CWD directory has a strict schema requiring `title` and `date`.
///   - Target directory has no schema (no `.hyalo.toml`).
///   - Target directory contains a file that only has `title` (no `date`).
///
/// Without the fix: lint reports a false error because CWD's schema is used.
/// With the fix:    lint exits 0 because the target has no schema.
#[test]
fn lint_dir_uses_target_config_not_cwd_config() {
    let cwd_dir = TempDir::new().unwrap();
    let target_dir = TempDir::new().unwrap();

    // CWD has a strict schema that requires both `title` and `date`.
    write_toml(
        cwd_dir.path(),
        r#"dir = "."

[schema.default]
required = ["title", "date"]
"#,
    );

    // Target has no .hyalo.toml — no schema, no constraints.
    // The document only has `title`, which would violate the CWD schema.
    write_md(
        target_dir.path(),
        "doc.md",
        "---\ntitle: Only Title Here\n---\nBody.\n",
    );

    // When --dir points at target, lint must pass (no schema in target).
    hyalo_no_hints()
        .current_dir(cwd_dir.path())
        .args(["lint", "--dir", target_dir.path().to_str().unwrap()])
        .assert()
        .success()
        .code(0);
}

/// Verify the inverse: when CWD has no schema but target has a strict one,
/// violations in the target are still reported.
#[test]
fn lint_dir_applies_target_schema_when_present() {
    let cwd_dir = TempDir::new().unwrap();
    let target_dir = TempDir::new().unwrap();

    // CWD has no .hyalo.toml — no schema.
    // (Just write a doc in CWD to give it some content, but no schema.)
    write_md(
        cwd_dir.path(),
        "cwd_doc.md",
        "---\ntitle: Only Title\n---\nBody.\n",
    );

    // Target has a schema requiring `title` and `date`.
    write_toml(
        target_dir.path(),
        r#"dir = "."

[schema.default]
required = ["title", "date"]
"#,
    );
    // Target doc is missing `date` — should trigger a lint error under target's schema.
    write_md(
        target_dir.path(),
        "missing_date.md",
        "---\ntitle: Missing Date\n---\nBody.\n",
    );

    hyalo_no_hints()
        .current_dir(cwd_dir.path())
        .args(["lint", "--dir", target_dir.path().to_str().unwrap()])
        .assert()
        .code(1);
}

/// When --dir is used, format and hints settings from the target's config
/// are also respected, not those from CWD.
#[test]
fn find_dir_uses_target_config_format() {
    let cwd_dir = TempDir::new().unwrap();
    let target_dir = TempDir::new().unwrap();

    // CWD config sets format to text.
    write_toml(cwd_dir.path(), "dir = \".\"\nformat = \"text\"\n");

    // Target config sets format to json (the default, but explicit here).
    write_toml(target_dir.path(), "dir = \".\"\nformat = \"json\"\n");

    write_md(
        target_dir.path(),
        "note.md",
        "---\ntitle: A Note\n---\nBody.\n",
    );

    // Without any --format flag, the command should use the target's format (json).
    // JSON output from `find` wraps results in an envelope object.
    let output = hyalo_no_hints()
        .current_dir(cwd_dir.path())
        .args(["find", "--dir", target_dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    // JSON output starts with '{'; text output starts with '"'.
    assert!(
        stdout.trim_start().starts_with('{'),
        "expected JSON output from target's config format=json, got: {stdout}"
    );
}
