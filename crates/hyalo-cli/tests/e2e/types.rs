use std::fs;

use super::common::{hyalo, hyalo_no_hints, write_md};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a minimal vault with a `.hyalo.toml` that has no `[schema]` section.
fn setup_empty() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: Test\n---\nBody.");
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    tmp
}

/// Create a vault with a pre-existing `[schema.types.note]` entry.
fn setup_with_type() -> TempDir {
    let tmp = setup_empty();
    fs::write(
        tmp.path().join(".hyalo.toml"),
        r#"dir = "."

[schema.default]
required = ["title"]

[schema.types.note]
required = ["title", "date"]
"#,
    )
    .unwrap();
    tmp
}

// ---------------------------------------------------------------------------
// types list
// ---------------------------------------------------------------------------

#[test]
fn types_list_empty() {
    let tmp = setup_empty();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["types", "list"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 0);
    assert!(json["results"].as_array().unwrap().is_empty());
}

#[test]
fn types_list_with_type() {
    let tmp = setup_with_type();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["types", "list"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 1);
    assert_eq!(json["results"][0]["type"], "note");
}

// `hyalo types` (no subcommand) is an alias for `list`
#[test]
fn types_bare_is_alias_for_list() {
    let tmp = setup_with_type();
    let out_bare = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["types"])
        .output()
        .unwrap();
    let out_list = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["types", "list"])
        .output()
        .unwrap();
    assert!(out_bare.status.success());
    assert_eq!(out_bare.stdout, out_list.stdout);
}

// ---------------------------------------------------------------------------
// types show
// ---------------------------------------------------------------------------

#[test]
fn types_show_unknown_type_exits_nonzero() {
    let tmp = setup_empty();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["types", "show", "ghost"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn types_show_existing_type() {
    let tmp = setup_with_type();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["types", "show", "note"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["type"], "note");
    let required = json["results"]["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("title")));
    assert!(required.contains(&serde_json::json!("date")));
}

// ---------------------------------------------------------------------------
// types remove
// ---------------------------------------------------------------------------

#[test]
fn types_remove_existing_type() {
    let tmp = setup_with_type();
    let output = hyalo()
        .current_dir(tmp.path())
        .args(["types", "remove", "note"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // The type should no longer appear in list
    let list_out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["types", "list"])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&list_out.stdout).unwrap();
    assert_eq!(json["total"], 0);
}

#[test]
fn types_remove_nonexistent_exits_nonzero() {
    let tmp = setup_empty();
    let output = hyalo()
        .current_dir(tmp.path())
        .args(["types", "remove", "ghost"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

// ---------------------------------------------------------------------------
// types set --required
// ---------------------------------------------------------------------------

#[test]
fn types_set_required_adds_field() {
    let tmp = setup_with_type();
    let output = hyalo()
        .current_dir(tmp.path())
        .args(["types", "set", "note", "--required", "status"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // "status" should now be required
    let show = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["types", "show", "note"])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    let required = json["results"]["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("status")));
}

#[test]
fn types_set_required_no_duplicate() {
    let tmp = setup_with_type();
    // "title" is already required — adding it again should not duplicate
    hyalo()
        .current_dir(tmp.path())
        .args(["types", "set", "note", "--required", "title"])
        .output()
        .unwrap();

    let show = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["types", "show", "note"])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    let required = json["results"]["required"].as_array().unwrap();
    let title_count = required
        .iter()
        .filter(|v| v.as_str() == Some("title"))
        .count();
    assert_eq!(title_count, 1);
}

// ---------------------------------------------------------------------------
// types set --property-type
// ---------------------------------------------------------------------------

#[test]
fn types_set_property_type_string() {
    let tmp = setup_with_type();
    let output = hyalo()
        .current_dir(tmp.path())
        .args(["types", "set", "note", "--property-type", "status=string"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let toml_content = fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
    assert!(toml_content.contains("type = \"string\""));
}

#[test]
fn types_set_property_type_date() {
    let tmp = setup_with_type();
    let output = hyalo()
        .current_dir(tmp.path())
        .args(["types", "set", "note", "--property-type", "date=date"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let toml_content = fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
    assert!(toml_content.contains("type = \"date\""));
}

// ---------------------------------------------------------------------------
// types set --property-values (enum)
// ---------------------------------------------------------------------------

#[test]
fn types_set_property_values_creates_enum() {
    let tmp = setup_with_type();
    let output = hyalo()
        .current_dir(tmp.path())
        .args([
            "types",
            "set",
            "note",
            "--property-values",
            "status=draft,published",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let toml_content = fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
    assert!(toml_content.contains("type = \"enum\""));
    assert!(toml_content.contains("draft"));
    assert!(toml_content.contains("published"));
}

// ---------------------------------------------------------------------------
// types set --filename-template
// ---------------------------------------------------------------------------

#[test]
fn types_set_filename_template() {
    let tmp = setup_with_type();
    let output = hyalo()
        .current_dir(tmp.path())
        .args([
            "types",
            "set",
            "note",
            "--filename-template",
            "notes/{slug}.md",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let toml_content = fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
    assert!(toml_content.contains("notes/{slug}.md"));
}

// ---------------------------------------------------------------------------
// types set --dry-run
// ---------------------------------------------------------------------------

#[test]
fn types_set_dry_run_does_not_modify_toml() {
    let tmp = setup_with_type();
    let before = fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["types", "set", "note", "--required", "branch", "--dry-run"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let after = fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
    assert_eq!(
        before, after,
        "--dry-run must not modify the .hyalo.toml file"
    );
}

// ---------------------------------------------------------------------------
// types set --default (auto-apply to vault files)
// ---------------------------------------------------------------------------

#[test]
fn types_set_default_applies_to_matching_files() {
    let tmp = setup_empty();

    // Create two files of type "note" missing "status"
    write_md(tmp.path(), "a.md", "---\ntitle: A\ntype: note\n---\nBody.");
    write_md(tmp.path(), "b.md", "---\ntitle: B\ntype: other\n---\nBody.");

    // Set default (types set auto-creates the type)
    let output = hyalo()
        .current_dir(tmp.path())
        .args(["types", "set", "note", "--default", "status=draft"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // a.md should now have status=draft (it was type note)
    let a_content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(
        a_content.contains("status: draft"),
        "expected status: draft in a.md, got:\n{a_content}"
    );

    // b.md should be unchanged (different type)
    let b_content = fs::read_to_string(tmp.path().join("b.md")).unwrap();
    assert!(
        !b_content.contains("status"),
        "b.md should not be modified, got:\n{b_content}"
    );
}

#[test]
fn types_set_default_dry_run_does_not_modify_files() {
    let tmp = setup_empty();
    write_md(tmp.path(), "a.md", "---\ntitle: A\ntype: note\n---\nBody.");

    let output = hyalo()
        .current_dir(tmp.path())
        .args([
            "types",
            "set",
            "note",
            "--default",
            "status=draft",
            "--dry-run",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());

    let a_content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(
        !a_content.contains("status"),
        "--dry-run must not write to vault files"
    );
}

// ---------------------------------------------------------------------------
// types set error cases
// ---------------------------------------------------------------------------

#[test]
fn types_set_no_flags_exits_nonzero() {
    let tmp = setup_with_type();
    let output = hyalo()
        .current_dir(tmp.path())
        .args(["types", "set", "note"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn types_set_creates_type_when_missing() {
    let tmp = setup_empty();
    let output = hyalo()
        .current_dir(tmp.path())
        .args(["types", "set", "ghost", "--required", "title"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // The type should now exist.
    let list_out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["types", "list"])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&list_out.stdout).unwrap();
    assert_eq!(json["total"], 1);
    assert_eq!(json["results"][0]["type"], "ghost");

    // The action field should indicate creation.
    let set_json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(set_json["results"]["action"], "created_and_updated");
}

#[test]
fn types_set_enables_validate_on_write_when_schema_is_new() {
    let tmp = setup_empty();

    // Before types set, the TOML has no [schema] section.
    let toml_before = fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
    assert!(
        !toml_before.contains("validate_on_write"),
        "precondition: no validate_on_write before types set"
    );

    // Create the first type.
    let output = hyalo()
        .current_dir(tmp.path())
        .args(["types", "set", "article", "--required", "title"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // validate_on_write should now be true in .hyalo.toml.
    let toml_after = fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
    assert!(
        toml_after.contains("validate_on_write = true"),
        "expected validate_on_write = true in .hyalo.toml, got:\n{toml_after}"
    );

    // Adding a second type should NOT duplicate the key.
    let output2 = hyalo()
        .current_dir(tmp.path())
        .args(["types", "set", "note", "--required", "title"])
        .output()
        .unwrap();
    assert!(
        output2.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output2.stderr)
    );
    let toml_final = fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
    assert_eq!(
        toml_final.matches("validate_on_write").count(),
        1,
        "validate_on_write should appear exactly once, got:\n{toml_final}"
    );
}

// ---------------------------------------------------------------------------
// --format text rendering
// ---------------------------------------------------------------------------

/// Create a vault with a rich type definition for text-format tests.
fn setup_with_rich_type() -> TempDir {
    let tmp = setup_empty();
    fs::write(
        tmp.path().join(".hyalo.toml"),
        r#"dir = "."

[schema.types.iteration]
required = ["title", "date", "status"]
filename-template = "iteration-{N}-{slug}.md"

[schema.types.iteration.properties.status]
type = "enum"
values = ["planned", "in-progress", "completed"]

[schema.types.iteration.properties.date]
type = "date"

[schema.types.iteration.defaults]
status = "planned"
"#,
    )
    .unwrap();
    tmp
}

#[test]
fn types_show_format_text_has_indentation() {
    let tmp = setup_with_rich_type();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["types", "show", "iteration", "--format", "text"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    // Type header
    assert!(
        text.contains("Type: iteration"),
        "expected 'Type: iteration' header, got:\n{text}"
    );
    // Required list
    assert!(
        text.contains("Required:"),
        "expected 'Required:' line, got:\n{text}"
    );
    // Properties block with indentation
    assert!(
        text.contains("Properties:"),
        "expected 'Properties:' section, got:\n{text}"
    );
    // Property names indented with two spaces
    assert!(
        text.lines().any(|l| l.starts_with("  status:")),
        "expected '  status:' line with 2-space indent, got:\n{text}"
    );
    assert!(
        text.lines().any(|l| l.starts_with("  date:")),
        "expected '  date:' line with 2-space indent, got:\n{text}"
    );
    // Constraint lines indented with four spaces
    assert!(
        text.lines().any(|l| l.starts_with("    type:")),
        "expected '    type:' line with 4-space indent, got:\n{text}"
    );
    // Defaults block
    assert!(
        text.contains("Defaults:"),
        "expected 'Defaults:' section, got:\n{text}"
    );
    assert!(
        text.lines().any(|l| l.starts_with("  status: planned")),
        "expected '  status: planned' in Defaults block, got:\n{text}"
    );
    // Filename template shown
    assert!(
        text.contains("Filename template: iteration-{N}-{slug}.md"),
        "expected filename template line, got:\n{text}"
    );
}

#[test]
fn types_list_format_text_has_type_headers_and_separation() {
    let tmp = setup_with_rich_type();
    // Add a second type so we can verify blank-line separation.
    hyalo()
        .current_dir(tmp.path())
        .args(["types", "set", "note", "--required", "title"])
        .output()
        .unwrap();

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["types", "list", "--format", "text"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    // Type names appear as headers with counts
    assert!(
        text.lines().any(|l| l.starts_with("iteration (")),
        "expected 'iteration (...)' header line, got:\n{text}"
    );
    assert!(
        text.lines().any(|l| l.starts_with("note (")),
        "expected 'note (...)' header line, got:\n{text}"
    );
    // Required fields listed with indentation
    assert!(
        text.lines().any(|l| l.starts_with("  required:")),
        "expected '  required:' line with 2-space indent, got:\n{text}"
    );
    // Blank line between entries
    assert!(
        text.contains("\n\n"),
        "expected blank line between type entries, got:\n{text}"
    );
}

// ---------------------------------------------------------------------------
// TOML comment preservation
// ---------------------------------------------------------------------------

#[test]
fn types_set_preserves_existing_toml_comments() {
    let tmp = setup_empty();
    // Write a TOML with a comment
    fs::write(
        tmp.path().join(".hyalo.toml"),
        "# My vault config\ndir = \".\"\n",
    )
    .unwrap();

    hyalo()
        .current_dir(tmp.path())
        .args(["types", "set", "note", "--required", "title"])
        .output()
        .unwrap();

    let content = fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
    assert!(
        content.contains("# My vault config"),
        "comment should be preserved:\n{content}"
    );
}

#[test]
fn types_set_auto_creates_string_properties_for_required() {
    let tmp = setup_empty();
    let output = hyalo()
        .current_dir(tmp.path())
        .args(["types", "set", "note", "--required", "status"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // The TOML should have a string property constraint auto-created for "status".
    let toml_content = fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
    assert!(
        toml_content.contains("type = \"string\""),
        "expected auto-created string property for 'status', got:\n{toml_content}"
    );

    // Verify via types show that the property constraint exists.
    let show = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["types", "show", "note"])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    let props = &json["results"]["properties"];
    assert!(
        props.get("status").is_some(),
        "expected 'status' in properties, got:\n{props}"
    );
}

#[test]
fn types_set_upsert_does_not_duplicate_type() {
    let tmp = setup_with_type();
    // "note" already exists — calling types set again should not duplicate it.
    let output = hyalo()
        .current_dir(tmp.path())
        .args(["types", "set", "note", "--required", "status"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let list_out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["types", "list"])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&list_out.stdout).unwrap();
    assert_eq!(json["total"], 1, "type should appear exactly once");

    // Action should be "updated" not "created_and_updated".
    let set_json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(set_json["results"]["action"], "updated");
}
