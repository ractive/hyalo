use std::fs;

use super::common::{hyalo, hyalo_no_hints, write_md};
use serde_json::Value;
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

    let output = hyalo_no_hints()
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

    let output = hyalo_no_hints()
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

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        // No --dir flag: relies on config's dir = "vault"
        .args(["summary", "--format", "json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("invalid JSON: {e}\nstdout: {stdout}"));
    // Should find the file inside vault/
    assert_eq!(
        json["results"]["files"]["total"].as_u64().unwrap(),
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
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "--dir",
            vault_path.to_str().unwrap(),
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
        json["results"]["files"]["total"].as_u64().unwrap(),
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

    let output = hyalo_no_hints()
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

    let output = hyalo_no_hints()
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

    // Use plain hyalo() without --no-hints, so the config's hints = true takes effect
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
    // hints = true → output wrapped in {"results": ..., "hints": [...]}
    assert!(
        json.get("hints").is_some(),
        "expected hints envelope in output when config sets hints = true; got: {stdout}"
    );
    assert!(
        json.get("results").is_some(),
        "expected results envelope in output when config sets hints = true; got: {stdout}"
    );
}

#[test]
fn cli_hints_false_overrides_config() {
    let tmp = TempDir::new().unwrap();
    // Config enables hints; explicit --no-hints should disable them
    fs::write(tmp.path().join(".hyalo.toml"), "hints = true\n").unwrap();
    write_note(tmp.path(), "note.md");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["summary", "--format", "json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("invalid JSON: {e}\nstdout: {stdout}"));
    // --no-hints → hints array is empty in envelope; results is always present
    assert!(
        json.get("results").is_some(),
        "expected results envelope in output; got: {stdout}"
    );
    // hints key is always present but should be an empty array with --no-hints
    let hints = json["hints"].as_array().expect("hints should be array");
    assert!(
        hints.is_empty(),
        "hints should be empty when --no-hints overrides config; got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// default_limit config key
// ---------------------------------------------------------------------------

/// Write N minimal markdown files named "file-N.md" with a single tag.
fn write_many_notes(dir: &std::path::Path, count: usize) {
    for i in 0..count {
        write_md(
            dir,
            &format!("file-{i:03}.md"),
            &format!("---\ntitle: Note {i}\ntags:\n  - testtag\n---\n"),
        );
    }
}

#[test]
fn default_limit_applies_to_find() {
    let tmp = TempDir::new().unwrap();
    // Write 60 files — more than the hardcoded default of 50.
    write_many_notes(tmp.path(), 60);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["find"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&stdout).unwrap();
    let results = json["results"].as_array().unwrap();
    // Default limit is 50; only 50 results should be shown.
    assert_eq!(
        results.len(),
        50,
        "expected default limit of 50 results, got {}",
        results.len()
    );
    // Total should report all 60.
    assert_eq!(json["total"], 60);
}

#[test]
fn default_limit_bypassed_with_limit_zero() {
    let tmp = TempDir::new().unwrap();
    write_many_notes(tmp.path(), 60);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["find", "--limit", "0"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&stdout).unwrap();
    let results = json["results"].as_array().unwrap();
    assert_eq!(
        results.len(),
        60,
        "--limit 0 should return all results, got {}",
        results.len()
    );
}

#[test]
fn config_default_limit_overrides_hardcoded_default() {
    let tmp = TempDir::new().unwrap();
    // Set default_limit = 5 in config.
    fs::write(tmp.path().join(".hyalo.toml"), "default_limit = 5\n").unwrap();
    write_many_notes(tmp.path(), 20);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["find"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&stdout).unwrap();
    let results = json["results"].as_array().unwrap();
    assert_eq!(
        results.len(),
        5,
        "config default_limit=5 should cap results to 5, got {}",
        results.len()
    );
    assert_eq!(json["total"], 20);
}

#[test]
fn config_default_limit_zero_means_unlimited() {
    let tmp = TempDir::new().unwrap();
    // default_limit = 0 means unlimited.
    fs::write(tmp.path().join(".hyalo.toml"), "default_limit = 0\n").unwrap();
    write_many_notes(tmp.path(), 60);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["find"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&stdout).unwrap();
    let results = json["results"].as_array().unwrap();
    assert_eq!(
        results.len(),
        60,
        "default_limit=0 in config should return all results, got {}",
        results.len()
    );
}

#[test]
fn cli_limit_overrides_config_default_limit() {
    let tmp = TempDir::new().unwrap();
    // Config sets limit to 5 but CLI passes --limit 3.
    fs::write(tmp.path().join(".hyalo.toml"), "default_limit = 5\n").unwrap();
    write_many_notes(tmp.path(), 20);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["find", "--limit", "3"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&stdout).unwrap();
    let results = json["results"].as_array().unwrap();
    assert_eq!(
        results.len(),
        3,
        "explicit --limit 3 should override config default_limit=5, got {}",
        results.len()
    );
}

#[test]
fn default_limit_applies_to_tags_summary() {
    let tmp = TempDir::new().unwrap();
    // Write files with many unique tags (more than 50).
    for i in 0..60usize {
        write_md(
            tmp.path(),
            &format!("file-{i:03}.md"),
            &format!("---\ntitle: Note {i}\ntags:\n  - tag-{i:03}\n---\n"),
        );
    }

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["tags"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&stdout).unwrap();
    let results = json["results"].as_array().unwrap();
    assert_eq!(
        results.len(),
        50,
        "tags summary should default-limit to 50, got {}",
        results.len()
    );
    assert_eq!(json["total"], 60);
}

#[test]
fn default_limit_applies_to_properties_summary() {
    let tmp = TempDir::new().unwrap();
    // Write files with many unique property keys (more than 50).
    for i in 0..60usize {
        write_md(
            tmp.path(),
            &format!("file-{i:03}.md"),
            &format!("---\ntitle: Note {i}\nprop_{i:03}: value\n---\n"),
        );
    }

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["properties"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&stdout).unwrap();
    let results = json["results"].as_array().unwrap();
    assert_eq!(
        results.len(),
        50,
        "properties summary should default-limit to 50, got {}",
        results.len()
    );
    // 60 unique props + 60 "title" entries = 61 unique property names total
    assert!(
        json["total"].as_u64().unwrap() > 50,
        "total should exceed 50"
    );
}

#[test]
fn default_limit_bypassed_with_jq() {
    let tmp = TempDir::new().unwrap();
    write_many_notes(tmp.path(), 60);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["find", "--jq", ".results | length"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let count: usize = stdout.trim().parse().unwrap();
    assert_eq!(
        count, 60,
        "--jq should bypass default limit and return all 60 results, got {count}"
    );
}

#[test]
fn default_limit_bypassed_with_count() {
    let tmp = TempDir::new().unwrap();
    write_many_notes(tmp.path(), 60);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["find", "--count"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let count: usize = stdout.trim().parse().unwrap();
    assert_eq!(
        count, 60,
        "--count should report true total of 60, got {count}"
    );
}

#[test]
fn default_limit_bypassed_with_jq_tags_summary() {
    let tmp = TempDir::new().unwrap();
    // Write files with 60 unique tags.
    for i in 0..60usize {
        write_md(
            tmp.path(),
            &format!("file-{i:03}.md"),
            &format!("---\ntitle: Note {i}\ntags:\n  - tag-{i:03}\n---\n"),
        );
    }

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["tags", "--jq", ".results | length"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let count: usize = stdout.trim().parse().unwrap();
    assert_eq!(
        count, 60,
        "tags summary --jq should bypass default limit and return all 60 tags, got {count}"
    );
}
