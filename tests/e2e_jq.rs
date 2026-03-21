mod common;

use common::{hyalo, write_md, write_tagged};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn setup_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "a.md", &["rust", "cli"]);
    write_tagged(tmp.path(), "b.md", &["rust", "iteration"]);
    write_md(tmp.path(), "c.md", "No frontmatter.\n");
    tmp
}

// ---------------------------------------------------------------------------
// Basic filter application
// ---------------------------------------------------------------------------

#[test]
fn jq_extracts_total_from_tags_summary() {
    let tmp = setup_vault();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", ".total"])
        .arg("tags")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "3");
}

#[test]
fn jq_maps_tag_names_to_array() {
    let tmp = setup_vault();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", "[.tags[].name] | sort | join(\", \")"])
        .arg("tags")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "cli, iteration, rust");
}

#[test]
fn jq_works_on_properties_command() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        "---\ntitle: Hello\nstatus: draft\n---\n# Body\n",
    );

    // `properties summary` returns an array of {count, name, type} objects.
    // Extract just the property names and sort them.
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", "[.[].name] | sort | join(\", \")"])
        .args(["properties", "summary"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "status, title");
}

#[test]
fn jq_works_on_property_find() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "---\nstatus: draft\n---\n");
    write_md(tmp.path(), "b.md", "---\nstatus: done\n---\n");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", ".files[]"])
        .args(["property", "find", "--name", "status", "--value", "draft"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.trim().contains("a.md"), "got: {stdout}");
    assert!(!stdout.contains("b.md"));
}

// ---------------------------------------------------------------------------
// --jq conflicts with --format text
// ---------------------------------------------------------------------------

#[test]
fn jq_with_format_text_errors() {
    let tmp = setup_vault();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "text"])
        .args(["--jq", ".total"])
        .arg("tags")
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(
        output.status.code(),
        Some(2),
        "expected exit code 2 (usage error)"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--jq cannot be combined with --format text"),
        "expected conflict error, got: {stderr}"
    );
}

#[test]
fn jq_with_format_json_works() {
    let tmp = setup_vault();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "json"])
        .args(["--jq", ".total"])
        .arg("tags")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "3");
}

// ---------------------------------------------------------------------------
// Error cases
// ---------------------------------------------------------------------------

#[test]
fn jq_invalid_filter_exits_nonzero() {
    let tmp = setup_vault();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", "this is not %%% valid jq"])
        .arg("tags")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("jq") || stderr.contains("Error"),
        "expected error message, got: {stderr}"
    );
}

#[test]
fn jq_runtime_error_exits_nonzero() {
    let tmp = setup_vault();

    // .no_such_field on a non-object path that causes a type error (e.g. trying to iterate null)
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", "error(\"deliberate\")"])
        .arg("tags")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("jq") || stderr.contains("Error"),
        "expected error message, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Filter producing multiple output values
// ---------------------------------------------------------------------------

#[test]
fn jq_multiple_outputs_joined_by_newline() {
    let tmp = setup_vault();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", ".tags[].name"])
        .arg("tags")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    // 3 tags, each on its own line
    assert_eq!(lines.len(), 3, "expected 3 lines, got: {stdout}");
    assert!(lines.contains(&"rust"));
    assert!(lines.contains(&"cli"));
    assert!(lines.contains(&"iteration"));
}
