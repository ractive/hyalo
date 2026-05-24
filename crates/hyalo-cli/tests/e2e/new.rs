use std::fs;

use super::common::{hyalo, hyalo_no_hints, write_md};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a minimal vault with an `iteration` type schema.
fn setup_with_iteration_type() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "placeholder.md",
        "---\ntitle: Placeholder\n---\n",
    );
    fs::write(
        tmp.path().join(".hyalo.toml"),
        r#"dir = "."

[schema.types.iteration]
required = ["title", "date", "status"]

[schema.types.iteration.properties.title]
type = "string"

[schema.types.iteration.properties.date]
type = "date"

[schema.types.iteration.properties.status]
type = "enum"
values = ["planned", "in-progress", "completed"]
"#,
    )
    .unwrap();
    tmp
}

/// Create a vault with a type that has required-sections.
fn setup_with_sectioned_type() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "placeholder.md",
        "---\ntitle: Placeholder\n---\n",
    );
    fs::write(
        tmp.path().join(".hyalo.toml"),
        "[schema.types.note]\nrequired = [\"title\"]\nrequired-sections = [\"## Goal\", \"## Tasks\"]\n",
    )
    .unwrap();
    tmp
}

/// Create a vault with a type that has a pattern on a string property (so `TBD` will fail lint).
fn setup_with_pattern_type() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "placeholder.md",
        "---\ntitle: Placeholder\n---\n",
    );
    fs::write(
        tmp.path().join(".hyalo.toml"),
        r#"dir = "."

[schema.types.branch]
required = ["name"]

[schema.types.branch.properties.name]
type = "string"
pattern = "^iter-\\d+/"
"#,
    )
    .unwrap();
    tmp
}

// ---------------------------------------------------------------------------
// new creates skeleton for type
// ---------------------------------------------------------------------------

#[test]
fn new_creates_skeleton_for_type() {
    let tmp = setup_with_iteration_type();
    fs::create_dir_all(tmp.path().join("iterations")).unwrap();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["new", "--type", "iteration", "--file", "iterations/x.md"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let created = tmp.path().join("iterations").join("x.md");
    assert!(created.exists(), "expected file to be created");

    let content = fs::read_to_string(&created).unwrap();
    assert!(
        content.contains("type: iteration"),
        "expected type in frontmatter"
    );
    assert!(
        content.contains("title: TBD"),
        "expected TBD for string title"
    );
    assert!(
        content.contains("status: planned"),
        "expected first enum value for status, got:\n{content}"
    );
    // Date should be a valid ISO date, not 'TBD'
    let has_date = content.lines().any(|l| {
        l.starts_with("date:")
            && l.len() > 6
            && l[6..].trim().len() == 10
            && l[6..].trim().contains('-')
    });
    assert!(
        has_date,
        "expected date property with ISO date, got:\n{content}"
    );
}

#[test]
fn new_output_json_envelope() {
    let tmp = setup_with_iteration_type();
    fs::create_dir_all(tmp.path().join("iterations")).unwrap();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["new", "--type", "iteration", "--file", "iterations/out.md"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["type"], "iteration");
    assert_eq!(json["results"]["file"], "iterations/out.md");
    assert_eq!(json["results"]["created"], true);
}

// ---------------------------------------------------------------------------
// new emits required sections in body
// ---------------------------------------------------------------------------

#[test]
fn new_emits_required_sections_in_body() {
    let tmp = setup_with_sectioned_type();
    fs::create_dir_all(tmp.path().join("notes")).unwrap();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["new", "--type", "note", "--file", "notes/n.md"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(tmp.path().join("notes").join("n.md")).unwrap();
    assert!(
        content.contains("## Goal"),
        "expected '## Goal' section in body, got:\n{content}"
    );
    assert!(
        content.contains("## Tasks"),
        "expected '## Tasks' section in body, got:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// new refuses if file exists
// ---------------------------------------------------------------------------

#[test]
fn new_rejects_existing_file() {
    let tmp = setup_with_iteration_type();
    // Pre-create the target file.
    fs::create_dir_all(tmp.path().join("iterations")).unwrap();
    write_md(
        tmp.path(),
        "iterations/existing.md",
        "---\ntitle: Existing\n---\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "new",
            "--type",
            "iteration",
            "--file",
            "iterations/existing.md",
        ])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "expected non-zero exit for existing file"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stderr}{stdout}");
    assert!(
        combined.contains("already exists"),
        "expected 'already exists' in output, got:\n{combined}"
    );
}

// ---------------------------------------------------------------------------
// new refuses if parent directory does not exist
// ---------------------------------------------------------------------------

#[test]
fn new_creates_deep_nested_parent_dirs() {
    let tmp = setup_with_iteration_type();

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "new",
            "--type",
            "iteration",
            "--file",
            "deep/nested/notes/foo.md",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "expected success creating deep nested path; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        tmp.path().join("deep/nested/notes/foo.md").exists(),
        "expected file to be created at deep/nested/notes/foo.md"
    );
}

// ---------------------------------------------------------------------------
// new with unknown type lists available types
// ---------------------------------------------------------------------------

#[test]
fn new_unknown_type_lists_available() {
    let tmp = setup_with_iteration_type();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["new", "--type", "ghost", "--file", "out.md"])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "expected non-zero exit for unknown type"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stderr}{stdout}");
    assert!(
        combined.contains("ghost"),
        "expected type name in error, got:\n{combined}"
    );
    assert!(
        combined.contains("iteration"),
        "expected available types listed, got:\n{combined}"
    );
}

// ---------------------------------------------------------------------------
// new then lint flags placeholders
// ---------------------------------------------------------------------------

#[test]
fn new_then_lint_flags_placeholders() {
    let tmp = setup_with_pattern_type();
    // Create the parent dir first.
    fs::create_dir_all(tmp.path().join("branches")).unwrap();

    // Create the file.
    let new_out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["new", "--type", "branch", "--file", "branches/b.md"])
        .output()
        .unwrap();
    assert!(
        new_out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&new_out.stderr)
    );

    // Lint it — should fail because `name: TBD` doesn't match the pattern.
    let lint_out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--file", "branches/b.md"])
        .output()
        .unwrap();
    // Lint should exit non-zero (errors found).
    assert!(
        !lint_out.status.success(),
        "expected lint to fail on TBD placeholder"
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&lint_out.stdout),
        String::from_utf8_lossy(&lint_out.stderr)
    );
    assert!(
        combined.contains("TBD"),
        "expected 'TBD' mentioned in lint output, got:\n{combined}"
    );
}

// ---------------------------------------------------------------------------
// new outputs hint to run lint
// ---------------------------------------------------------------------------

#[test]
fn new_hint_suggests_lint() {
    let tmp = setup_with_iteration_type();
    fs::create_dir_all(tmp.path().join("iterations")).unwrap();
    let output = hyalo()
        .current_dir(tmp.path())
        .args(["new", "--type", "iteration", "--file", "iterations/hint.md"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("lint"),
        "expected lint hint in output, got:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// BUG-5: scaffold should produce exactly one trailing newline (iter-140)
// ---------------------------------------------------------------------------

#[test]
fn new_scaffold_no_md047_trailing_newline_violation() {
    let tmp = setup_with_sectioned_type();
    // Create the file (parent is the vault root, no subdir needed).
    let new_out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["new", "--type", "note", "--file", "scaffold_test.md"])
        .output()
        .unwrap();
    assert!(
        new_out.status.success(),
        "new should succeed; stderr: {}",
        String::from_utf8_lossy(&new_out.stderr)
    );

    // Verify exactly one trailing newline.
    let content = fs::read_to_string(tmp.path().join("scaffold_test.md")).unwrap();
    assert!(
        content.ends_with('\n') && !content.ends_with("\n\n"),
        "expected exactly one trailing newline, got content ending with {:?}",
        &content[content.len().saturating_sub(4)..]
    );

    // Lint should not fire MD047 on the scaffolded file.
    let lint_out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--file", "scaffold_test.md", "--rule", "MD047"])
        .output()
        .unwrap();
    assert!(
        lint_out.status.success(),
        "MD047 should not fire on scaffolded file; stdout: {}",
        String::from_utf8_lossy(&lint_out.stdout)
    );
}
