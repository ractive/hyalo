mod common;

use std::fs;

use common::{hyalo, write_md};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Fixture helper
// ---------------------------------------------------------------------------

/// Write a minimal valid markdown file with frontmatter.
fn write_note(dir: &std::path::Path, name: &str) {
    write_md(
        dir,
        name,
        "---\ntitle: Test Note\n---\n# Hello\n\nSome content.\n",
    );
}

// ---------------------------------------------------------------------------
// format config key
// ---------------------------------------------------------------------------

#[test]
fn config_sets_default_format() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".hyalo.toml"), "format = \"text\"\n").unwrap();
    write_note(tmp.path(), "note.md");

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["summary"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    // Text format uses "Files:" label, JSON does not
    assert!(
        stdout.contains("Files:"),
        "expected text-format output; got: {stdout}"
    );
}

#[test]
fn cli_format_overrides_config() {
    let tmp = TempDir::new().unwrap();
    // Config says text, but CLI arg forces json
    fs::write(tmp.path().join(".hyalo.toml"), "format = \"text\"\n").unwrap();
    write_note(tmp.path(), "note.md");

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["summary", "--format", "json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    // JSON output starts with '{'
    assert!(
        stdout.trim_start().starts_with('{'),
        "expected JSON output; got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// dir config key
// ---------------------------------------------------------------------------

#[test]
fn config_sets_default_dir() {
    let tmp = TempDir::new().unwrap();
    // Note is inside vault/, config points dir at vault/
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \"vault\"\n").unwrap();
    write_note(tmp.path(), "vault/note.md");

    let output = hyalo()
        .current_dir(tmp.path())
        // No --dir flag: relies on config's dir = "vault"
        .args(["summary", "--format", "json", "--no-hints"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("invalid JSON: {e}\nstdout: {stdout}"));
    // Should find the file inside vault/
    assert_eq!(
        json["files"]["total"].as_u64().unwrap(),
        1,
        "config dir should point at vault/ which contains one file"
    );
}

#[test]
fn cli_dir_overrides_config() {
    let tmp = TempDir::new().unwrap();
    // Config points at "wrong/" which is empty; CLI --dir points at actual vault
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \"wrong\"\n").unwrap();
    fs::create_dir_all(tmp.path().join("wrong")).unwrap();
    write_note(tmp.path(), "vault/note.md");

    let vault_path = tmp.path().join("vault");
    let output = hyalo()
        .current_dir(tmp.path())
        .args([
            "--dir",
            vault_path.to_str().unwrap(),
            "--no-hints",
            "summary",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("invalid JSON: {e}\nstdout: {stdout}"));
    // CLI --dir wins; should scan vault/, not wrong/
    assert_eq!(
        json["files"]["total"].as_u64().unwrap(),
        1,
        "CLI --dir should override config dir, finding file in vault/"
    );
}

// ---------------------------------------------------------------------------
// Missing config → hardcoded defaults
// ---------------------------------------------------------------------------

#[test]
fn missing_config_uses_defaults() {
    let tmp = TempDir::new().unwrap();
    // No .hyalo.toml written — hardcoded defaults apply (format=json, dir=., hints=true)
    write_note(tmp.path(), "note.md");

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["summary"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    // Default format is JSON
    assert!(
        stdout.trim_start().starts_with('{'),
        "expected JSON output (default format); got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Malformed config → warning on stderr, hardcoded defaults
// ---------------------------------------------------------------------------

#[test]
fn malformed_config_warns_on_stderr() {
    let tmp = TempDir::new().unwrap();
    // Invalid TOML — not a valid key = value pair
    fs::write(tmp.path().join(".hyalo.toml"), "{{invalid\n").unwrap();
    write_note(tmp.path(), "note.md");

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["summary", "--format", "json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Command must still succeed (falls back to defaults)
    assert!(output.status.success(), "stderr: {stderr}");
    // Hardcoded defaults still produce valid JSON
    assert!(
        stdout.trim_start().starts_with('{'),
        "expected JSON fallback output; got: {stdout}"
    );
    // A warning must be emitted
    assert!(
        stderr.to_lowercase().contains("warning"),
        "expected warning on stderr for malformed config; got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// hints config key
// ---------------------------------------------------------------------------

#[test]
fn config_sets_hints_true() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".hyalo.toml"), "hints = true\n").unwrap();
    write_note(tmp.path(), "note.md");

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["summary", "--format", "json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("invalid JSON: {e}\nstdout: {stdout}"));
    // hints = true → output wrapped in {"data": ..., "hints": [...]}
    assert!(
        json.get("hints").is_some(),
        "expected hints envelope in output when config sets hints = true; got: {stdout}"
    );
    assert!(
        json.get("data").is_some(),
        "expected data envelope in output when config sets hints = true; got: {stdout}"
    );
}

#[test]
fn cli_hints_false_overrides_config() {
    let tmp = TempDir::new().unwrap();
    // Config enables hints; explicit --no-hints should disable them
    fs::write(tmp.path().join(".hyalo.toml"), "hints = true\n").unwrap();
    write_note(tmp.path(), "note.md");

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["summary", "--format", "json", "--no-hints"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("invalid JSON: {e}\nstdout: {stdout}"));
    // --no-hints → no envelope; direct summary fields
    assert!(
        json.get("hints").is_none(),
        "hints envelope should be absent when --no-hints overrides config; got: {stdout}"
    );
    assert!(
        json.get("files").is_some(),
        "expected direct summary fields (no envelope); got: {stdout}"
    );
}
