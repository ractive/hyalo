mod common;

use common::{hyalo, md, write_md};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

/// A 3-file vault sufficient for basic hints tests.
fn setup_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();

    write_md(
        tmp.path(),
        "notes/alpha.md",
        md!(r"
---
title: Alpha
status: in-progress
tags:
  - rust
  - cli
---
# Alpha

- [ ] Open task
- [x] Done task
"),
    );

    write_md(
        tmp.path(),
        "notes/beta.md",
        md!(r"
---
title: Beta
status: completed
tags:
  - rust
---
# Beta

- [x] Completed
"),
    );

    write_md(
        tmp.path(),
        "docs/readme.md",
        md!(r"
---
title: Readme
status: planned
tags:
  - docs
---
# Readme

No tasks here.
"),
    );

    tmp
}

// ---------------------------------------------------------------------------
// summary --hints
// ---------------------------------------------------------------------------

#[test]
fn summary_hints_json_has_data_and_hints() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["summary", "--hints", "--format", "json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    // Envelope must have "data" and "hints"
    assert!(parsed.get("data").is_some(), "missing 'data' key");
    assert!(parsed.get("hints").is_some(), "missing 'hints' key");

    // Data must be the vault summary
    assert!(parsed["data"]["files"]["total"].as_u64().unwrap() > 0);

    // Hints must be an array of {description, cmd} objects
    let hints = parsed["hints"].as_array().unwrap();
    assert!(!hints.is_empty());
    for hint in hints {
        assert!(
            hint["cmd"].as_str().unwrap().starts_with("hyalo"),
            "hint cmd should start with hyalo: {hint}"
        );
        assert!(
            hint.get("description").and_then(|d| d.as_str()).is_some(),
            "hint should have description: {hint}"
        );
    }
}

#[test]
fn summary_hints_text_has_arrow_lines() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["summary", "--hints", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(
        stdout.contains("Files:"),
        "should have normal summary output"
    );
    assert!(
        stdout.contains("  -> hyalo"),
        "should have hint lines with arrow prefix"
    );
    assert!(
        stdout.contains("properties"),
        "should suggest properties command: {stdout}"
    );
    assert!(
        stdout.contains("tags"),
        "should suggest tags command: {stdout}"
    );
}

#[test]
fn summary_hints_suggests_tasks_todo_when_open_tasks() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["summary", "--hints", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(
        stdout.contains("find --task todo"),
        "should suggest find --task todo when there are open tasks: {stdout}"
    );
}

#[test]
fn summary_hints_prefers_interesting_status() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["summary", "--hints", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // in-progress should be suggested
    assert!(
        stdout.contains("in-progress"),
        "should suggest in-progress status: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// properties --hints
// ---------------------------------------------------------------------------

#[test]
fn properties_hints_text() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "summary", "--hints", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should suggest find --property for top properties
    assert!(
        stdout.contains("find --property"),
        "should suggest find --property: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// tags --hints
// ---------------------------------------------------------------------------

#[test]
fn tags_hints_text() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "summary", "--hints", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Should suggest find --tag for top tags (rust has 2 files)
    assert!(
        stdout.contains("find --tag rust"),
        "should suggest find --tag for top tag: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Hints active by default (no flag needed)
// ---------------------------------------------------------------------------

#[test]
fn summary_hints_active_by_default_text() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["summary", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("  -> hyalo"),
        "hints should appear by default without --hints flag: {stdout}"
    );
}

#[test]
fn find_hints_active_by_default_json() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--format", "json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(
        parsed.get("data").is_some(),
        "JSON should have hints envelope by default: {stdout}"
    );
    assert!(
        parsed.get("hints").is_some(),
        "JSON should have hints array by default: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Without --hints: output unchanged (regression)
// ---------------------------------------------------------------------------

#[test]
fn summary_without_hints_no_arrows() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["summary", "--no-hints", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(!stdout.contains("  -> "), "should not have hint arrows");
}

#[test]
fn summary_without_hints_json_no_envelope() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["summary", "--no-hints", "--format", "json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    // With --no-hints, should NOT have envelope
    assert!(
        parsed.get("data").is_none(),
        "should not have 'data' envelope with --no-hints"
    );
    // Should have direct summary fields
    assert!(parsed.get("files").is_some());
}

// ---------------------------------------------------------------------------
// --hints + --jq suppresses hints
// ---------------------------------------------------------------------------

#[test]
fn hints_suppressed_with_jq() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["summary", "--hints", "--jq", ".tasks.total"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(
        !stdout.contains("hints"),
        "hints should be suppressed with --jq"
    );
    assert!(!stdout.contains("->"), "no arrow lines with --jq");
    // Should just be a number
    let val: u64 = stdout.trim().parse().unwrap();
    assert!(val > 0);
}

// ---------------------------------------------------------------------------
// --dir propagation
// ---------------------------------------------------------------------------

#[test]
fn hints_propagate_dir_flag() {
    let tmp = setup_vault();
    let dir_str = tmp.path().to_str().unwrap();
    let output = hyalo()
        .args(["--dir", dir_str])
        .args(["summary", "--hints", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // All hints should include --dir
    for line in stdout.lines() {
        if line.starts_with("  -> ") {
            assert!(
                line.contains("--dir"),
                "hint should include --dir flag: {line}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// --glob propagation
// ---------------------------------------------------------------------------

#[test]
fn hints_propagate_glob_for_aggregate_commands() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "properties",
            "summary",
            "--glob",
            "notes/*.md",
            "--hints",
            "--format",
            "text",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Aggregate hints (find --property) should include --glob
    let hint_lines: Vec<&str> = stdout.lines().filter(|l| l.starts_with("  -> ")).collect();
    assert!(!hint_lines.is_empty(), "should have hints");
    for line in &hint_lines {
        assert!(
            line.contains("--glob"),
            "aggregate hint should propagate --glob: {line}"
        );
    }
}

// ---------------------------------------------------------------------------
// find --hints
// ---------------------------------------------------------------------------

#[test]
fn find_hints_with_task_filter() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--task", "todo", "--hints", "--format", "text"])
        .output()
        .unwrap();
    // Should succeed — hints may or may not have drill-down suggestions for find
    assert!(output.status.success());
}

// ---------------------------------------------------------------------------
// Regression: --format text --hints on commands without hint generators
// must still honour the text format instead of emitting raw JSON.
// ---------------------------------------------------------------------------

#[test]
fn find_format_text_with_hints_outputs_text_not_json() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--format", "text", "--hints"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "find --format text --hints failed");
    // Text format does not start with '[' or '{'; JSON would.
    let trimmed = stdout.trim_start();
    assert!(
        !trimmed.starts_with('[') && !trimmed.starts_with('{'),
        "expected text output but got JSON: {}",
        &stdout[..stdout.len().min(200)]
    );
}

#[test]
fn set_format_text_with_hints_outputs_text_not_json() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "set",
            "--property",
            "status=updated",
            "--file",
            "notes/alpha.md",
            "--format",
            "text",
            "--hints",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "set --format text --hints failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let trimmed = stdout.trim_start();
    assert!(
        !trimmed.starts_with('[') && !trimmed.starts_with('{'),
        "expected text output but got JSON: {}",
        &stdout[..stdout.len().min(200)]
    );
}

// ---------------------------------------------------------------------------
// Mutation commands with --hints
// Mutation commands (set, remove, append) accept --hints and generate
// hint suggestions — they produce a hints envelope with verify/read hints.
// These tests verify the flag is accepted and hints are generated.
// ---------------------------------------------------------------------------

#[test]
fn set_hints_accepted_produces_valid_json() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--hints"])
        .args([
            "set",
            "--property",
            "status=updated",
            "--file",
            "notes/alpha.md",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("expected valid JSON, got: {stdout}\nerr: {e}"));
    // Output should be wrapped in a hints envelope
    assert!(
        parsed.get("data").is_some(),
        "should have data envelope: {parsed}"
    );
    assert!(
        parsed["data"].get("modified").is_some(),
        "data should have modified field: {parsed}"
    );
    // Mutation commands generate verify/read hints
    assert!(
        parsed
            .get("hints")
            .and_then(|h| h.as_array())
            .is_some_and(|a| !a.is_empty()),
        "mutation commands should generate hints: {parsed}"
    );
}

#[test]
fn remove_hints_accepted_produces_valid_json() {
    let tmp = setup_vault();
    // notes/alpha.md has status: in-progress — remove it
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--hints"])
        .args(["remove", "--property", "status", "--file", "notes/alpha.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("expected valid JSON, got: {stdout}\nerr: {e}"));
    assert!(
        parsed.get("data").is_some(),
        "should have data envelope: {parsed}"
    );
    assert!(
        parsed["data"].get("modified").is_some(),
        "data should have modified field: {parsed}"
    );
    assert!(
        parsed
            .get("hints")
            .and_then(|h| h.as_array())
            .is_some_and(|a| !a.is_empty()),
        "mutation commands should generate hints: {parsed}"
    );
}

#[test]
fn append_hints_accepted_produces_valid_json() {
    let tmp = setup_vault();
    // Append a new alias to notes/alpha.md
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--hints"])
        .args([
            "append",
            "--property",
            "aliases=alpha-note",
            "--file",
            "notes/alpha.md",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("expected valid JSON, got: {stdout}\nerr: {e}"));
    assert!(
        parsed.get("data").is_some(),
        "should have data envelope: {parsed}"
    );
    assert!(
        parsed["data"].get("modified").is_some(),
        "data should have modified field: {parsed}"
    );
    assert!(
        parsed
            .get("hints")
            .and_then(|h| h.as_array())
            .is_some_and(|a| !a.is_empty()),
        "mutation commands should generate hints: {parsed}"
    );
}

// ---------------------------------------------------------------------------
// find --hints: suggestions based on results
// ---------------------------------------------------------------------------

#[test]
fn find_with_hints_shows_suggestions() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--hints", "--format", "text"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();

    // The vault has files, so hints should be produced.
    assert!(
        stdout.contains("  -> hyalo"),
        "should have hint lines with arrow prefix: {stdout}"
    );
    // Should suggest reading the first result.
    assert!(
        stdout.contains("read --file"),
        "should suggest read --file for first result: {stdout}"
    );
    // Should suggest backlinks for the first result.
    assert!(
        stdout.contains("backlinks --file"),
        "should suggest backlinks --file for first result: {stdout}"
    );
}

#[test]
fn find_with_hints_json_envelope() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--hints", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert!(parsed.get("data").is_some(), "should have 'data' key");
    let hints = parsed["hints"].as_array().unwrap();
    assert!(!hints.is_empty(), "should have at least one hint");
    for hint in hints {
        assert!(
            hint["cmd"].as_str().unwrap().starts_with("hyalo"),
            "hint cmd should start with hyalo: {hint}"
        );
    }
}

#[test]
fn find_with_hints_empty_results_no_hints() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        // Tag that does not exist in the vault.
        .args([
            "find",
            "--tag",
            "nonexistent-tag-xyz",
            "--hints",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Empty results -> no hints generated; output is a plain empty array or
    // a hints envelope with an empty hints array.
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    if let Some(hints) = parsed.get("hints") {
        assert!(
            hints.as_array().is_some_and(std::vec::Vec::is_empty),
            "expected empty hints for empty results: {parsed}"
        );
    }
}

// ---------------------------------------------------------------------------
// Mutation commands with --hints: warning on stderr
// ---------------------------------------------------------------------------

#[test]
fn mutation_with_hints_warns() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "set",
            "--hints",
            "--property",
            "status=updated",
            "--file",
            "notes/alpha.md",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "set --hints should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Mutation commands now generate hints — no warning expected
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        !stderr.contains("--hints has no effect"),
        "should not warn about --hints on mutation commands: {stderr}"
    );
}

#[test]
fn remove_with_hints_warns() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "remove",
            "--hints",
            "--property",
            "status",
            "--file",
            "notes/beta.md",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "remove --hints should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Mutation commands now generate hints — no warning expected
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        !stderr.contains("--hints has no effect"),
        "should not warn about --hints on mutation commands: {stderr}"
    );
}

#[test]
fn append_with_hints_warns() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "append",
            "--hints",
            "--property",
            "aliases=hint-test",
            "--file",
            "notes/alpha.md",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "append --hints should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Mutation commands now generate hints — no warning expected
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        !stderr.contains("--hints has no effect"),
        "should not warn about --hints on mutation commands: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Larger vault fixture for data-driven find --hints tests
// ---------------------------------------------------------------------------

/// A 6-file vault where:
///   - "rust" appears on 4 files (top tag)
///   - status "planned" appears on 4 files (most interesting non-completed status)
///   - status "completed" appears on 2 files
fn setup_large_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();

    for (name, status, tags) in &[
        ("a", "planned", vec!["rust", "cli"]),
        ("b", "planned", vec!["rust"]),
        ("c", "planned", vec!["rust"]),
        ("d", "planned", vec!["rust"]),
        ("e", "completed", vec!["cli"]),
        ("f", "completed", vec!["docs"]),
    ] {
        let tags_yaml: String = tags
            .iter()
            .map(|t| format!("  - {t}"))
            .collect::<Vec<_>>()
            .join("\n");
        write_md(
            tmp.path(),
            &format!("{name}.md"),
            md!(&format!(
                "---\ntitle: {name}\nstatus: {status}\ntags:\n{tags_yaml}\n---\n# {name}\n"
            )),
        );
    }

    tmp
}

// ---------------------------------------------------------------------------
// find --hints: data-driven narrowing suggestions
// ---------------------------------------------------------------------------

#[test]
fn find_hints_suggests_top_tag_from_results() {
    let tmp = setup_large_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--hints", "--format", "text"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();

    // "rust" is the most common tag — hints should suggest it.
    assert!(
        stdout.contains("find --tag rust"),
        "should suggest --tag rust (most common tag): {stdout}"
    );
}

#[test]
fn find_hints_suggests_interesting_status_from_results() {
    let tmp = setup_large_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--hints", "--format", "text"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();

    // "planned" should be preferred over "completed" even though counts are close.
    assert!(
        stdout.contains("status=planned"),
        "should suggest status=planned (interesting status): {stdout}"
    );
    // "completed" should NOT be suggested when "planned" is available.
    assert!(
        !stdout.contains("status=completed"),
        "should not suggest completed when planned is available: {stdout}"
    );
}

#[test]
fn find_hints_no_hardcoded_draft() {
    let tmp = setup_large_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--hints", "--format", "text"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    // The vault has no "draft" items — hints should not hardcode it.
    assert!(
        !stdout.contains("status=draft"),
        "should not suggest hardcoded status=draft: {stdout}"
    );
    assert!(
        !stdout.contains("--tag draft"),
        "should not suggest hardcoded tag draft: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Array-valued status: hints must flatten, not stringify
// ---------------------------------------------------------------------------

/// Vault where some files have array-valued status properties.
/// "deprecated" appears only inside arrays (3 files) — more than "completed" (2 files).
/// "completed" has the lowest priority so "deprecated" wins on both count and priority,
/// proving the hint comes from flattening arrays.
fn setup_array_status_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    // 6 files to trigger narrowing hints (>5 results needed).
    for i in 1..=2 {
        write_md(
            tmp.path(),
            &format!("note-{i}.md"),
            &format!("---\ntitle: Note {i}\nstatus: completed\ntags:\n  - docs\n---\nBody.\n"),
        );
    }
    for (i, extra) in [(3, "experimental"), (4, "legacy"), (5, "wip")] {
        write_md(
            tmp.path(),
            &format!("note-{i}.md"),
            &format!(
                "---\ntitle: Note {i}\nstatus:\n  - deprecated\n  - {extra}\ntags:\n  - docs\n---\nBody.\n"
            ),
        );
    }
    // One more file with no status to pad file count.
    write_md(
        tmp.path(),
        "note-6.md",
        "---\ntitle: Note 6\ntags:\n  - docs\n---\nBody.\n",
    );
    tmp
}

#[test]
fn summary_hints_flatten_array_status() {
    let tmp = setup_array_status_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["summary", "--hints", "--format", "text"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Must NOT suggest a stringified array like status=["deprecated","experimental"].
    assert!(
        !stdout.contains("status=["),
        "hints should not contain stringified array syntax: {stdout}"
    );
}

#[test]
fn find_hints_flatten_array_status() {
    let tmp = setup_array_status_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--hints", "--format", "text"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Must NOT suggest a stringified array.
    assert!(
        !stdout.contains("status=["),
        "find hints should not contain stringified array syntax: {stdout}"
    );

    // "deprecated" appears only inside array-valued status (3 files > "active"'s 2),
    // so the hint must come from flattening arrays — the old code would skip these.
    assert!(
        stdout.contains("status=deprecated"),
        "find hints should suggest status derived from array-valued fields: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Helper: assert a JSON value has a non-empty hints array where every element
// has both "description" and "cmd" string fields.
// ---------------------------------------------------------------------------

fn assert_hints_present(parsed: &serde_json::Value) {
    let hints = parsed["hints"]
        .as_array()
        .unwrap_or_else(|| panic!("expected 'hints' array in: {parsed}"));
    assert!(!hints.is_empty(), "expected at least one hint in: {parsed}");
    for hint in hints {
        assert!(
            hint.get("description").and_then(|d| d.as_str()).is_some(),
            "hint missing 'description': {hint}"
        );
        assert!(
            hint.get("cmd").and_then(|c| c.as_str()).is_some(),
            "hint missing 'cmd': {hint}"
        );
    }
}

// ---------------------------------------------------------------------------
// read --hints
// ---------------------------------------------------------------------------

#[test]
fn read_hints() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Note
---
# Note

Body content here.
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["read", "--file", "note.md", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "invalid JSON: {e}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    assert!(
        parsed.get("data").is_some(),
        "expected 'data' key: {parsed}"
    );
    assert_hints_present(&parsed);
}

// ---------------------------------------------------------------------------
// backlinks --hints
// ---------------------------------------------------------------------------

#[test]
fn backlinks_hints() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "target.md",
        md!(r"
---
title: Target
---
# Target
"),
    );
    write_md(
        tmp.path(),
        "source.md",
        md!(r"
---
title: Source
---
# Source

See [[target]].
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["backlinks", "--file", "target.md", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "invalid JSON: {e}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    assert!(
        parsed.get("data").is_some(),
        "expected 'data' key: {parsed}"
    );
    assert_hints_present(&parsed);
}

// ---------------------------------------------------------------------------
// mv --dry-run --hints
// ---------------------------------------------------------------------------

#[test]
fn mv_dry_run_hints() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "original.md",
        md!(r"
---
title: Original
---
# Original
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "mv",
            "--file",
            "original.md",
            "--to",
            "renamed.md",
            "--dry-run",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "invalid JSON: {e}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    assert!(
        parsed.get("data").is_some(),
        "expected 'data' key: {parsed}"
    );
    // dry-run hint should suggest applying (without --dry-run)
    let hints = parsed["hints"].as_array().unwrap();
    assert!(
        !hints.is_empty(),
        "expected hints for mv --dry-run: {parsed}"
    );
    let apply_hint = hints.iter().find(|h| {
        h.get("cmd")
            .and_then(|c| c.as_str())
            .is_some_and(|s| s.contains("mv") && !s.contains("--dry-run"))
    });
    assert!(
        apply_hint.is_some(),
        "expected a hint suggesting mv without --dry-run: {parsed}"
    );
}

// ---------------------------------------------------------------------------
// task toggle --hints
// ---------------------------------------------------------------------------

#[test]
fn task_toggle_hints() {
    let tmp = TempDir::new().unwrap();
    // Task is on line 6: frontmatter(3) + heading(1) + blank(1) + task(1)
    write_md(
        tmp.path(),
        "tasks.md",
        md!(r"
---
title: Tasks
---
# Tasks

- [ ] task one
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "task", "toggle", "--file", "tasks.md", "--line", "6", "--format", "json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "invalid JSON: {e}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    assert!(
        parsed.get("data").is_some(),
        "expected 'data' key: {parsed}"
    );
    assert_hints_present(&parsed);
}

// ---------------------------------------------------------------------------
// links fix --hints (with a fixable broken link)
// ---------------------------------------------------------------------------

#[test]
fn links_fix_hints() {
    let tmp = TempDir::new().unwrap();
    // "ActualNote" is a wikilink that can be fuzzy-matched to actual-note.md
    write_md(
        tmp.path(),
        "source.md",
        md!(r"
---
title: Source
---
# Source

See [[ActualNote]] for details.
"),
    );
    write_md(
        tmp.path(),
        "actual-note.md",
        md!(r"
---
title: ActualNote
---
# ActualNote
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["links", "fix", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "invalid JSON: {e}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    assert!(
        parsed.get("data").is_some(),
        "expected 'data' key: {parsed}"
    );
    assert_hints_present(&parsed);
}

// ---------------------------------------------------------------------------
// create-index --hints
// ---------------------------------------------------------------------------

#[test]
fn create_index_hints() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Note
---
# Note
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["create-index", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "invalid JSON: {e}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    assert!(
        parsed.get("data").is_some(),
        "expected 'data' key: {parsed}"
    );
    let hints = parsed["hints"].as_array().unwrap();
    assert!(
        !hints.is_empty(),
        "expected hints after create-index: {parsed}"
    );
    // Should suggest using the index and dropping it
    let cmds: Vec<&str> = hints
        .iter()
        .filter_map(|h| h.get("cmd").and_then(|c| c.as_str()))
        .collect();
    assert!(
        cmds.iter().any(|c| c.contains("--index")),
        "expected a hint suggesting --index flag: {parsed}"
    );
    assert!(
        cmds.iter().any(|c| c.contains("drop-index")),
        "expected a hint suggesting drop-index: {parsed}"
    );
}

// ---------------------------------------------------------------------------
// drop-index --hints
// ---------------------------------------------------------------------------

#[test]
fn drop_index_hints() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        md!(r"
---
title: Note
---
# Note
"),
    );

    // First create the index, then drop it and check the hints.
    hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["create-index", "--no-hints"])
        .output()
        .unwrap();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["drop-index", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "invalid JSON: {e}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    assert!(
        parsed.get("data").is_some(),
        "expected 'data' key: {parsed}"
    );
    let hints = parsed["hints"].as_array().unwrap();
    assert!(
        !hints.is_empty(),
        "expected hints after drop-index: {parsed}"
    );
    let cmds: Vec<&str> = hints
        .iter()
        .filter_map(|h| h.get("cmd").and_then(|c| c.as_str()))
        .collect();
    assert!(
        cmds.iter().any(|c| c.contains("create-index")),
        "expected a hint suggesting create-index: {parsed}"
    );
}

// ---------------------------------------------------------------------------
// properties summary --hints: verify data-driven suggestions
// ---------------------------------------------------------------------------

#[test]
fn properties_summary_hints_json_envelope() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "summary", "--hints", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert!(parsed.get("data").is_some(), "should have 'data' key");
    let hints = parsed["hints"].as_array().unwrap();
    assert!(!hints.is_empty(), "should have hints");
    for hint in hints {
        assert!(
            hint["cmd"].as_str().unwrap().starts_with("hyalo"),
            "hint cmd should start with hyalo: {hint}"
        );
    }
}

#[test]
fn properties_summary_hints_suggest_top_properties() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "summary", "--hints", "--format", "text"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    // The vault has "title" and "status" properties on all 3 files.
    assert!(
        stdout.contains("find --property title") || stdout.contains("find --property status"),
        "should suggest find --property for common properties: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// tags summary --hints: verify data-driven suggestions
// ---------------------------------------------------------------------------

#[test]
fn tags_summary_hints_json_envelope() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "summary", "--hints", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert!(parsed.get("data").is_some(), "should have 'data' key");
    let hints = parsed["hints"].as_array().unwrap();
    assert!(!hints.is_empty(), "should have hints");
    for hint in hints {
        assert!(
            hint["cmd"].as_str().unwrap().starts_with("hyalo"),
            "hint cmd should start with hyalo: {hint}"
        );
    }
}

#[test]
fn tags_summary_hints_suggest_top_tag_by_count() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "summary", "--hints", "--format", "text"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    // "rust" tag appears on 2 files (alpha and beta), "docs" on 1 — rust should be top.
    assert!(
        stdout.contains("find --tag rust"),
        "should suggest find --tag for top tag (rust): {stdout}"
    );
}

#[test]
fn tags_summary_hints_empty_vault_no_crash() {
    let tmp = TempDir::new().unwrap();
    // Empty vault — no tags at all.
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "summary", "--hints", "--format", "json"])
        .output()
        .unwrap();
    // Should succeed without crashing.
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    // Either no envelope (no hints) or empty hints array.
    if let Some(hints) = parsed.get("hints") {
        assert!(
            hints.as_array().is_some_and(std::vec::Vec::is_empty),
            "empty vault should produce no hints: {parsed}"
        );
    }
}

// ---------------------------------------------------------------------------
// find --broken-links hints should suggest `links fix`
// ---------------------------------------------------------------------------

#[test]
fn find_broken_links_hints_suggest_links_fix() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "source.md",
        md!(r"
---
title: Source
---
Link to [[nonexistent-page]].
"),
    );
    write_md(
        tmp.path(),
        "other.md",
        md!(r"
---
title: Other
---
No broken links here.
"),
    );

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "find",
            "--broken-links",
            "--fields",
            "links",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "invalid JSON: {e}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    });

    // Should have hints envelope.
    let hints = parsed["hints"]
        .as_array()
        .unwrap_or_else(|| panic!("expected 'hints' array: {parsed}"));

    // At least one hint should suggest `links fix`.
    assert!(
        hints
            .iter()
            .any(|h| h["cmd"].as_str().is_some_and(|c| c.contains("links fix"))),
        "find --broken-links should hint at 'links fix': {hints:?}"
    );
}
