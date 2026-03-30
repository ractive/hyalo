mod common;

use common::{hyalo_no_hints, write_md, write_tagged};
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

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", ".total"])
        .args(["tags", "summary"])
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

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", "[.results[].name] | sort | join(\", \")"])
        .args(["tags", "summary"])
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

    // `properties` returns an array of {count, name, type} objects.
    // Extract just the property names and sort them.
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", "[.results[].name] | sort | join(\", \")"])
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
fn jq_works_on_find_property_filter() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "---\nstatus: draft\n---\n");
    write_md(tmp.path(), "b.md", "---\nstatus: done\n---\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", ".results[].file"])
        .args(["find", "--property", "status=draft"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.trim().contains("a.md"), "got: {stdout}");
    assert!(!stdout.contains("b.md"));
}

// ---------------------------------------------------------------------------
// --jq on find --fields links and find --fields sections
// ---------------------------------------------------------------------------

#[test]
fn jq_works_on_find_links() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "source.md",
        "---\ntitle: Source\n---\nSee [[target]] and [[other]].\n",
    );
    write_md(tmp.path(), "target.md", "# Target\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", ".results[0].links | length"])
        .args(["find", "--file", "source.md", "--fields", "links"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "2", "expected 2 links, got: {stdout}");
}

#[test]
fn jq_works_on_find_sections() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "doc.md",
        "# Introduction\n\nSome text.\n\n## Details\n\nMore text.\n\n### Sub\n\nDeep.\n",
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", ".results[0].sections | length"])
        .args(["find", "--file", "doc.md", "--fields", "sections"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "3", "expected 3 sections, got: {stdout}");
}

// ---------------------------------------------------------------------------
// --jq conflicts with --format text
// ---------------------------------------------------------------------------

#[test]
fn jq_with_format_text_errors() {
    let tmp = setup_vault();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "text"])
        .args(["--jq", ".total"])
        .args(["tags", "summary"])
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

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "json"])
        .args(["--jq", ".total"])
        .args(["tags", "summary"])
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

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", "this is not %%% valid jq"])
        .args(["tags", "summary"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should mention "jq" and give some indication of a syntax/filter problem
    assert!(
        stderr.contains("jq"),
        "expected 'jq' in error message, got: {stderr}"
    );
    assert!(
        stderr.contains("syntax") || stderr.contains("filter") || stderr.contains("parse"),
        "expected description of what went wrong, got: {stderr}"
    );
}

#[test]
fn jq_runtime_error_exits_nonzero() {
    let tmp = setup_vault();

    // Explicit jq runtime error raised via error("deliberate") to verify non-zero exit on runtime failure
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", "error(\"deliberate\")"])
        .args(["tags", "summary"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should mention "jq" and include the error value "deliberate"
    assert!(
        stderr.contains("jq"),
        "expected 'jq' in error message, got: {stderr}"
    );
    assert!(
        stderr.contains("deliberate"),
        "expected the raised error value 'deliberate' in output, got: {stderr}"
    );
}

#[test]
fn jq_filter_error_is_json_when_format_json() {
    let tmp = setup_vault();

    // With --format json (the default), jq errors should be emitted as structured JSON
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "json"])
        .args(["--jq", "error(\"structured-error\")"])
        .args(["tags", "summary"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let parsed: serde_json::Value = serde_json::from_str(&stderr)
        .unwrap_or_else(|_| panic!("expected JSON on stderr for --format json, got: {stderr}"));
    assert!(
        parsed.get("error").is_some(),
        "expected 'error' field in JSON error, got: {stderr}"
    );
    assert!(
        parsed.get("cause").is_some(),
        "expected 'cause' field in JSON error, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// --jq on mutation commands (set, remove, append)
// ---------------------------------------------------------------------------

#[test]
fn jq_works_on_set_command() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: Note\n---\n# Body\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", ".results.modified | length"])
        .args(["set", "--property", "status=done", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        "1",
        "expected 1 modified file, got: {stdout}"
    );
}

#[test]
fn jq_works_on_remove_command() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        "---\ntitle: Note\nstatus: draft\n---\n# Body\n",
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", ".results.modified | length"])
        .args(["remove", "--property", "status", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        "1",
        "expected 1 modified file, got: {stdout}"
    );
}

#[test]
fn jq_works_on_append_command() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        "---\ntitle: Note\naliases:\n  - old\n---\n# Body\n",
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", ".results.modified | length"])
        .args(["append", "--property", "aliases=new", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        "1",
        "expected 1 modified file, got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Filter producing multiple output values
// ---------------------------------------------------------------------------

#[test]
fn jq_multiple_outputs_joined_by_newline() {
    let tmp = setup_vault();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", ".results[].name"])
        .args(["tags", "summary"])
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
