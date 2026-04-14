use super::common::{hyalo_no_hints, write_md};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Write a markdown file with invalid YAML frontmatter (triggers a parse-error
/// warning when hyalo tries to read it).
fn write_invalid_frontmatter(dir: &std::path::Path, name: &str) {
    write_md(dir, name, "---\nbad: [unclosed bracket\n---\n# Body\n");
}

/// Write a valid markdown file with a known tag so scans produce results.
fn write_valid(dir: &std::path::Path, name: &str) {
    write_md(
        dir,
        name,
        "---\ntitle: Valid\ntags:\n  - test\n---\n# Body\n",
    );
}

// ---------------------------------------------------------------------------
// --quiet / -q: suppress warnings
// ---------------------------------------------------------------------------

/// Without `--quiet`, a parse-error warning is printed to stderr.
#[test]
fn warning_appears_without_quiet() {
    let tmp = tempfile::tempdir().unwrap();
    write_valid(tmp.path(), "good.md");
    write_invalid_frontmatter(tmp.path(), "bad.md");

    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .arg("tags")
        .arg("summary")
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("warning:"),
        "expected warning on stderr without --quiet, got: {stderr:?}"
    );
}

/// With `--quiet`, no warnings are printed to stderr.
#[test]
fn warning_suppressed_with_quiet_long() {
    let tmp = tempfile::tempdir().unwrap();
    write_valid(tmp.path(), "good.md");
    write_invalid_frontmatter(tmp.path(), "bad.md");

    let output = hyalo_no_hints()
        .arg("--quiet")
        .arg("--dir")
        .arg(tmp.path())
        .arg("tags")
        .arg("summary")
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.is_empty(),
        "expected no stderr with --quiet, got: {stderr:?}"
    );
    // stdout should still contain valid JSON
    assert!(output.status.success(), "expected exit 0 with --quiet");
}

/// The short flag `-q` also suppresses warnings.
#[test]
fn warning_suppressed_with_quiet_short() {
    let tmp = tempfile::tempdir().unwrap();
    write_valid(tmp.path(), "good.md");
    write_invalid_frontmatter(tmp.path(), "bad.md");

    let output = hyalo_no_hints()
        .arg("-q")
        .arg("--dir")
        .arg(tmp.path())
        .arg("tags")
        .arg("summary")
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.is_empty(),
        "expected no stderr with -q, got: {stderr:?}"
    );
}

/// The `--quiet` flag suppresses warnings on `find` too.
#[test]
fn warning_suppressed_on_find_with_quiet() {
    let tmp = tempfile::tempdir().unwrap();
    write_valid(tmp.path(), "good.md");
    write_invalid_frontmatter(tmp.path(), "bad.md");

    let output = hyalo_no_hints()
        .arg("-q")
        .arg("--dir")
        .arg(tmp.path())
        .arg("find")
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.is_empty(),
        "expected no stderr from find with -q, got: {stderr:?}"
    );
}

/// The `--quiet` flag suppresses warnings on `properties summary` too.
#[test]
fn warning_suppressed_on_properties_with_quiet() {
    let tmp = tempfile::tempdir().unwrap();
    write_valid(tmp.path(), "good.md");
    write_invalid_frontmatter(tmp.path(), "bad.md");

    let output = hyalo_no_hints()
        .arg("-q")
        .arg("--dir")
        .arg(tmp.path())
        .arg("properties")
        .arg("summary")
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.is_empty(),
        "expected no stderr from properties summary with -q, got: {stderr:?}"
    );
}

// ---------------------------------------------------------------------------
// Warning dedup — verified via e2e + unit tests in warn.rs
// ---------------------------------------------------------------------------

// NOTE: Exact-string dedup of "skipping <file>: <reason>" warnings is not
// trivially triggerable via the CLI because each file produces a unique
// message (the filename is part of the message).  The dedup logic is
// tested exhaustively at the unit level in crates/hyalo-cli/src/warn.rs.
//
// The e2e tests below verify:
//  - Multiple unique warnings appear (no false dedup).
//  - No spurious suppression summary when all messages are distinct.
//  - The `--hints has no effect...` warning (a static message) is not
//    duplicated when --hints is combined with a mutation command.

/// Multiple files with different names each emit a unique warning.
/// The suppression summary must NOT appear when no messages are identical.
#[test]
fn no_dedup_summary_for_unique_warnings() {
    let tmp = tempfile::tempdir().unwrap();
    write_valid(tmp.path(), "good.md");
    for i in 0..3 {
        write_invalid_frontmatter(tmp.path(), &format!("bad{i}.md"));
    }

    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .arg("tags")
        .arg("summary")
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Each bad file emits a unique warning (different filename in message).
    assert_eq!(
        stderr.matches("warning: skipping").count(),
        3,
        "expected 3 distinct warnings, got:\n{stderr}"
    );
    // No suppression summary for distinct messages.
    assert!(
        !stderr.contains("suppressed"),
        "unexpected suppression summary for distinct warnings:\n{stderr}"
    );
}

/// When a static warning message fires and no duplicates occur, no summary appears.
#[test]
fn no_dedup_summary_when_no_warnings_at_all() {
    let tmp = tempfile::tempdir().unwrap();
    write_valid(tmp.path(), "only.md");

    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .arg("tags")
        .arg("summary")
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.is_empty(),
        "expected empty stderr for clean vault, got:\n{stderr}"
    );
}

/// `--hints` on mutation commands (set/remove/append) now generates hints.
/// Verify no spurious warning is emitted and the output contains a hints envelope.
#[test]
fn hints_warning_appears_exactly_once() {
    let tmp = tempfile::tempdir().unwrap();
    write_valid(tmp.path(), "note.md");

    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .arg("--hints")
        .arg("set")
        .arg("--property")
        .arg("status=done")
        .arg("--file")
        .arg("note.md")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "set --hints should succeed; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("--hints has no effect"),
        "should not warn about --hints on mutation commands; got:\n{stderr}"
    );
    // The output should be a valid hints envelope
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("invalid JSON: {e}\nstdout: {stdout}"));
    assert!(
        json.get("hints").is_some(),
        "set --hints should produce hints envelope; got: {stdout}"
    );
}

/// With `--quiet`, no dedup summary appears even when multiple warnings would
/// otherwise be present.
#[test]
fn quiet_suppresses_all_output_including_summary() {
    let tmp = tempfile::tempdir().unwrap();
    write_valid(tmp.path(), "good.md");
    for i in 0..3 {
        write_invalid_frontmatter(tmp.path(), &format!("bad{i}.md"));
    }

    let output = hyalo_no_hints()
        .arg("--quiet")
        .arg("--dir")
        .arg(tmp.path())
        .arg("tags")
        .arg("summary")
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.is_empty(),
        "expected empty stderr with --quiet (all warnings suppressed), got:\n{stderr}"
    );
}
