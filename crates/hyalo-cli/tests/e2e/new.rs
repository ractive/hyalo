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
// new creates parent directories automatically (iter-140 BUG-4)
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

    // iter-181 task 4: a pattern-constrained string with no valid placeholder is
    // OMITTED (not scaffolded as an invalid `name: TBD`), so the scaffold never
    // ships a value that violates the type's own schema.
    let content = fs::read_to_string(tmp.path().join("branches").join("b.md")).unwrap();
    assert!(
        !content.contains("name:"),
        "expected `name` to be omitted (no invalid TBD placeholder), got:\n{content}"
    );

    // Lint it — should still fail, now because required `name` is missing (the
    // user must fill it in), not because a bogus placeholder violates the pattern.
    let lint_out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--file", "branches/b.md"])
        .output()
        .unwrap();
    // Lint should exit non-zero (errors found).
    assert!(
        !lint_out.status.success(),
        "expected lint to fail on the missing required `name`"
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&lint_out.stdout),
        String::from_utf8_lossy(&lint_out.stderr)
    );
    assert!(
        combined.contains("name"),
        "expected missing-required `name` mentioned in lint output, got:\n{combined}"
    );
}

// ---------------------------------------------------------------------------
// new omits placeholder that would violate the type's schema (iter-181 task 4)
// ---------------------------------------------------------------------------

#[test]
fn new_omits_pattern_violating_placeholder() {
    // A type whose required string carries a regex the generic `TBD` fails.
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
required = ["title", "branch"]

[schema.types.iteration.properties.title]
type = "string"

[schema.types.iteration.properties.branch]
type = "string"
pattern = "^iter-\\d+[a-z]*/"
"#,
    )
    .unwrap();
    fs::create_dir_all(tmp.path().join("iterations")).unwrap();

    let out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["new", "--type", "iteration", "--file", "iterations/i.md"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let content = fs::read_to_string(tmp.path().join("iterations").join("i.md")).unwrap();
    // `branch` would be `TBD`, which violates `^iter-\d+[a-z]*/` — so it is
    // omitted entirely rather than scaffolded with an invalid value.
    assert!(
        !content.contains("branch: TBD"),
        "expected no invalid `branch: TBD` placeholder, got:\n{content}"
    );
    assert!(
        !content.contains("branch:"),
        "expected `branch` key omitted, got:\n{content}"
    );
    // The unconstrained `title` still gets the normal TBD placeholder.
    assert!(
        content.contains("title: TBD"),
        "expected unconstrained title to keep its TBD placeholder, got:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// new keeps a placeholder that satisfies a lax pattern (iter-181 task 4)
// ---------------------------------------------------------------------------

#[test]
fn new_keeps_placeholder_that_matches_pattern() {
    // A pattern the generic `TBD` DOES satisfy must not trigger omission.
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "placeholder.md",
        "---\ntitle: Placeholder\n---\n",
    );
    fs::write(
        tmp.path().join(".hyalo.toml"),
        r#"dir = "."

[schema.types.note]
required = ["slug"]

[schema.types.note.properties.slug]
type = "string"
pattern = "^[A-Za-z]+$"
"#,
    )
    .unwrap();

    let out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["new", "--type", "note", "--file", "n.md"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let content = fs::read_to_string(tmp.path().join("n.md")).unwrap();
    assert!(
        content.contains("slug: TBD"),
        "expected `slug: TBD` kept (TBD matches ^[A-Za-z]+$), got:\n{content}"
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

// ---------------------------------------------------------------------------
// iter-149: `hyalo new` keeps the snapshot index in sync
// ---------------------------------------------------------------------------

/// After `hyalo create-index` + `hyalo new`, the freshly created file must
/// be visible via `hyalo find --index` without a full rebuild.
#[test]
fn new_inserts_into_existing_snapshot_index() {
    let tmp = setup_with_iteration_type();
    fs::create_dir_all(tmp.path().join("iterations")).unwrap();

    // Build a snapshot index that captures only the placeholder.
    let create = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["create-index"])
        .output()
        .unwrap();
    assert!(
        create.status.success(),
        "create-index should succeed; stderr: {}",
        String::from_utf8_lossy(&create.stderr)
    );
    let index_path = tmp.path().join(".hyalo-index");
    assert!(index_path.exists(), "index file should exist");

    // Create a new file — this must also patch the snapshot index in place.
    let new_out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "new",
            "--index",
            "--type",
            "iteration",
            "--file",
            "iterations/iter-001-new.md",
        ])
        .output()
        .unwrap();
    assert!(
        new_out.status.success(),
        "new should succeed; stderr: {}",
        String::from_utf8_lossy(&new_out.stderr)
    );

    // `find --index --file <new>` must locate the file via the index — without
    // a full rebuild, this only succeeds when `new` inserted the entry.
    let find_out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "find",
            "--index",
            "--file",
            "iterations/iter-001-new.md",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        find_out.status.success(),
        "find --index should succeed; stderr: {}",
        String::from_utf8_lossy(&find_out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&find_out.stdout).unwrap();
    let results = json
        .get("results")
        .and_then(|v| v.as_array())
        .cloned()
        .or_else(|| json.get("results").cloned().map(|v| vec![v]))
        .unwrap_or_default();
    assert!(
        results
            .iter()
            .any(|r| r.get("file").and_then(|v| v.as_str()) == Some("iterations/iter-001-new.md")),
        "new file must appear in --index find results: {results:?}"
    );
}

/// When no `.hyalo-index` exists, `hyalo new` succeeds and must not create
/// one — keeping the "indexes are explicit" contract.
#[test]
fn new_does_not_create_index_when_absent() {
    let tmp = setup_with_iteration_type();
    fs::create_dir_all(tmp.path().join("iterations")).unwrap();
    let index_path = tmp.path().join(".hyalo-index");
    assert!(!index_path.exists(), "precondition: no index file");

    let new_out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "new",
            "--type",
            "iteration",
            "--file",
            "iterations/no-index.md",
        ])
        .output()
        .unwrap();
    assert!(
        new_out.status.success(),
        "new should succeed without an index; stderr: {}",
        String::from_utf8_lossy(&new_out.stderr)
    );
    assert!(tmp.path().join("iterations/no-index.md").exists());
    assert!(
        !index_path.exists(),
        "new must not auto-create a snapshot index"
    );
}

/// Idempotency: even when the snapshot already contains an entry for the
/// to-be-created path (e.g. stale entry from a previous run), `new` should
/// refresh it rather than error out.
#[test]
fn new_with_index_refreshes_stale_entry() {
    let tmp = setup_with_iteration_type();
    fs::create_dir_all(tmp.path().join("iterations")).unwrap();

    // Pre-create the file, build an index that contains it, then delete it
    // so the next `new` call can create a fresh copy.
    let rel = "iterations/iter-002-refresh.md";
    fs::write(
        tmp.path().join(rel),
        "---\ntitle: Old\ndate: 2026-01-01\nstatus: planned\n---\nold body\n",
    )
    .unwrap();
    let create = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["create-index"])
        .output()
        .unwrap();
    assert!(create.status.success());
    fs::remove_file(tmp.path().join(rel)).unwrap();

    let new_out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["new", "--index", "--type", "iteration", "--file", rel])
        .output()
        .unwrap();
    assert!(
        new_out.status.success(),
        "new should refresh the stale entry; stderr: {}",
        String::from_utf8_lossy(&new_out.stderr)
    );

    let find_out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["find", "--index", "--file", rel, "--format", "json"])
        .output()
        .unwrap();
    assert!(find_out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&find_out.stdout).unwrap();
    let body = json
        .get("results")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .or_else(|| json.get("results"))
        .cloned()
        .unwrap_or_default();
    // The scaffolded skeleton uses title "TBD", not the pre-existing "Old".
    let title = body
        .pointer("/properties/title")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(
        title, "TBD",
        "stale entry must be replaced by fresh scaffold"
    );
}
