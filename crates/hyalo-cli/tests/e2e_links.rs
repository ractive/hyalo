mod common;

use common::{hyalo, md, write_md};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Vault fixture
// ---------------------------------------------------------------------------

fn setup_vault() -> TempDir {
    let tmp = TempDir::new().expect("tempdir creation should succeed");

    // File with all links working — a links to b which exists at vault root
    write_md(
        tmp.path(),
        "a.md",
        md!(r"
---
title: A
---
See [[b]] for details.
"),
    );

    // File with a broken link and a working link
    write_md(
        tmp.path(),
        "b.md",
        md!(r"
---
title: B
---
See [[nonexistent]] here.
And also [[a]].
"),
    );

    // File with no links (will be excluded by --broken-links)
    write_md(
        tmp.path(),
        "c.md",
        md!(r"
---
title: C
---
No links here.
"),
    );

    // File with a broken link that can be fuzzy-matched to authentication.md
    write_md(
        tmp.path(),
        "d.md",
        md!(r"
---
title: D
---
See [[Authnticaton]] for auth details.
"),
    );

    // The file that the fuzzy match should find
    write_md(
        tmp.path(),
        "authentication.md",
        md!(r"
---
title: Authentication
---
Auth docs.
"),
    );

    tmp
}

// ---------------------------------------------------------------------------
// summary: link health section
// ---------------------------------------------------------------------------

#[test]
fn summary_includes_link_health() {
    let tmp = setup_vault();
    let output = hyalo()
        .args([
            "--dir",
            tmp.path()
                .to_str()
                .expect("temp path should be valid UTF-8"),
            "summary",
            "--format",
            "json",
        ])
        .output()
        .expect("hyalo summary should run");
    assert!(
        output.status.success(),
        "summary exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");

    // links.total counts all links across the vault
    let total = json["links"]["total"]
        .as_u64()
        .expect("links.total should be a number");
    assert!(total > 0, "expected at least one link, got {total}");

    // links.broken >= 1 because b.md has [[nonexistent]] and d.md has [[Authnticaton]]
    let broken = json["links"]["broken"]
        .as_u64()
        .expect("links.broken should be a number");
    assert!(broken >= 1, "expected at least 1 broken link, got {broken}");

    // links.broken_links is an array of broken link entries
    let broken_links = json["links"]["broken_links"]
        .as_array()
        .expect("links.broken_links should be an array");
    assert!(
        !broken_links.is_empty(),
        "broken_links array should not be empty"
    );

    // Each entry must have source, line, and target fields
    let first = &broken_links[0];
    assert!(
        first["source"].is_string(),
        "broken link entry must have 'source' string field"
    );
    assert!(
        first["line"].is_number(),
        "broken link entry must have 'line' number field"
    );
    assert!(
        first["target"].is_string(),
        "broken link entry must have 'target' string field"
    );
}

#[test]
fn summary_broken_links_includes_nonexistent_target() {
    let tmp = setup_vault();
    let output = hyalo()
        .args([
            "--dir",
            tmp.path()
                .to_str()
                .expect("temp path should be valid UTF-8"),
            "summary",
            "--format",
            "json",
        ])
        .output()
        .expect("hyalo summary should run");
    assert!(output.status.success());

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");

    let broken_links = json["links"]["broken_links"]
        .as_array()
        .expect("links.broken_links should be an array");

    // b.md has [[nonexistent]] — that broken link must appear
    let has_nonexistent = broken_links.iter().any(|entry| {
        entry["source"].as_str().unwrap_or("") == "b.md"
            && entry["target"].as_str().unwrap_or("") == "nonexistent"
    });
    assert!(
        has_nonexistent,
        "expected broken link {{source: 'b.md', target: 'nonexistent'}} in: {broken_links:?}"
    );
}

#[test]
fn summary_text_includes_links_line() {
    let tmp = setup_vault();
    let output = hyalo()
        .args([
            "--dir",
            tmp.path()
                .to_str()
                .expect("temp path should be valid UTF-8"),
            "summary",
            "--format",
            "text",
        ])
        .output()
        .expect("hyalo summary --format text should run");
    assert!(
        output.status.success(),
        "summary exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let text = String::from_utf8(output.stdout).expect("stdout should be valid UTF-8");

    // The text output should contain "Links: N total, M broken"
    assert!(
        text.contains("Links:"),
        "expected 'Links:' in summary text output, got:\n{text}"
    );
    assert!(
        text.contains("total"),
        "expected 'total' in Links line, got:\n{text}"
    );
    assert!(
        text.contains("broken"),
        "expected 'broken' in Links line, got:\n{text}"
    );
}

// ---------------------------------------------------------------------------
// find --broken-links
// ---------------------------------------------------------------------------

#[test]
fn find_broken_links_filter() {
    let tmp = setup_vault();
    let output = hyalo()
        .args([
            "--dir",
            tmp.path()
                .to_str()
                .expect("temp path should be valid UTF-8"),
            "find",
            "--broken-links",
            "--format",
            "json",
        ])
        .output()
        .expect("hyalo find --broken-links should run");
    assert!(
        output.status.success(),
        "find --broken-links exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");

    let results = json.as_array().expect("find output should be a JSON array");

    // b.md and d.md have broken links; they must appear
    let files: Vec<&str> = results
        .iter()
        .map(|r| r["file"].as_str().unwrap_or(""))
        .collect();

    assert!(
        files.contains(&"b.md"),
        "b.md (has broken [[nonexistent]]) should appear in --broken-links results: {files:?}"
    );
    assert!(
        files.contains(&"d.md"),
        "d.md (has broken [[Authnticaton]]) should appear in --broken-links results: {files:?}"
    );

    // Files without broken links must NOT appear
    assert!(
        !files.contains(&"a.md"),
        "a.md (no broken links) should NOT appear in --broken-links results: {files:?}"
    );
    assert!(
        !files.contains(&"c.md"),
        "c.md (no links at all) should NOT appear in --broken-links results: {files:?}"
    );
    assert!(
        !files.contains(&"authentication.md"),
        "authentication.md (no broken links) should NOT appear: {files:?}"
    );
}

#[test]
fn find_broken_links_entries_have_null_path() {
    let tmp = setup_vault();
    let output = hyalo()
        .args([
            "--dir",
            tmp.path()
                .to_str()
                .expect("temp path should be valid UTF-8"),
            "find",
            "--broken-links",
            "--format",
            "json",
        ])
        .output()
        .expect("hyalo find --broken-links should run");
    assert!(output.status.success());

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");

    let results = json.as_array().expect("find output should be a JSON array");

    // Each returned file should have at least one link entry with path = null
    for result in results {
        let file = result["file"].as_str().unwrap_or("?");
        let links = result["links"]
            .as_array()
            .unwrap_or_else(|| panic!("file {file} should have a 'links' array"));

        let has_null_path = links.iter().any(|l| l["path"].is_null());
        assert!(
            has_null_path,
            "file {file} returned by --broken-links should have at least one link with path=null"
        );
    }
}

#[test]
fn find_broken_links_combined_with_glob_filter() {
    let tmp = setup_vault();
    // Restrict to b.md only via glob — should return just that file
    let output = hyalo()
        .args([
            "--dir",
            tmp.path()
                .to_str()
                .expect("temp path should be valid UTF-8"),
            "find",
            "--broken-links",
            "--glob",
            "b.md",
            "--format",
            "json",
        ])
        .output()
        .expect("hyalo find --broken-links --glob should run");
    assert!(
        output.status.success(),
        "find --broken-links --glob exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");

    let results = json.as_array().expect("find output should be a JSON array");
    let files: Vec<&str> = results
        .iter()
        .map(|r| r["file"].as_str().unwrap_or(""))
        .collect();

    // Only b.md matches the glob AND has broken links
    assert_eq!(
        files,
        vec!["b.md"],
        "--broken-links AND --glob=b.md should yield only b.md"
    );
}

// ---------------------------------------------------------------------------
// links fix: dry run
// ---------------------------------------------------------------------------

#[test]
fn links_fix_dry_run_reports_broken_and_fixable() {
    let tmp = setup_vault();
    let output = hyalo()
        .args([
            "--dir",
            tmp.path()
                .to_str()
                .expect("temp path should be valid UTF-8"),
            "links",
            "fix",
            "--format",
            "json",
        ])
        .output()
        .expect("hyalo links fix should run");
    assert!(
        output.status.success(),
        "links fix exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");

    // broken >= 1 (at minimum [[nonexistent]] and [[Authnticaton]])
    let broken_count = json["broken"]
        .as_u64()
        .expect("'broken' should be a number");
    assert!(
        broken_count >= 1,
        "expected at least 1 broken link, got {broken_count}"
    );

    // fixable >= 1 because [[Authnticaton]] fuzzy-matches authentication.md
    let fixable_count = json["fixable"]
        .as_u64()
        .expect("'fixable' should be a number");
    assert!(
        fixable_count >= 1,
        "expected at least 1 fixable link, got {fixable_count}"
    );

    // By default (no --apply), applied must be false
    assert!(
        !json["applied"]
            .as_bool()
            .expect("'applied' should be a bool"),
        "dry-run should report applied=false"
    );

    // fixes is an array with entries having source, line, old_target, new_target
    let fixes = json["fixes"]
        .as_array()
        .expect("'fixes' should be an array");
    assert!(
        !fixes.is_empty(),
        "fixes array should not be empty when fixable > 0"
    );

    let first_fix = &fixes[0];
    assert!(
        first_fix["source"].is_string(),
        "fix entry must have 'source'"
    );
    assert!(first_fix["line"].is_number(), "fix entry must have 'line'");
    assert!(
        first_fix["old_target"].is_string(),
        "fix entry must have 'old_target'"
    );
    assert!(
        first_fix["new_target"].is_string(),
        "fix entry must have 'new_target'"
    );
}

#[test]
fn links_fix_dry_run_detects_fuzzy_match() {
    let tmp = setup_vault();
    let output = hyalo()
        .args([
            "--dir",
            tmp.path()
                .to_str()
                .expect("temp path should be valid UTF-8"),
            "links",
            "fix",
            "--format",
            "json",
        ])
        .output()
        .expect("hyalo links fix should run");
    assert!(output.status.success());

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");

    let fixes = json["fixes"]
        .as_array()
        .expect("'fixes' should be an array");

    // [[Authnticaton]] in d.md should be proposed as a fix to authentication.md
    let has_auth_fix = fixes.iter().any(|fix| {
        fix["source"].as_str().unwrap_or("") == "d.md"
            && fix["old_target"]
                .as_str()
                .unwrap_or("")
                .eq_ignore_ascii_case("Authnticaton")
            && fix["new_target"]
                .as_str()
                .unwrap_or("")
                .contains("authentication")
    });
    assert!(
        has_auth_fix,
        "expected a fix for [[Authnticaton]] → authentication.md in d.md, fixes: {fixes:?}"
    );
}

// ---------------------------------------------------------------------------
// links fix: apply
// ---------------------------------------------------------------------------

#[test]
fn links_fix_apply_reduces_broken_links() {
    let tmp = setup_vault();

    // Apply fixes
    let apply_output = hyalo()
        .args([
            "--dir",
            tmp.path()
                .to_str()
                .expect("temp path should be valid UTF-8"),
            "links",
            "fix",
            "--apply",
            "--format",
            "json",
        ])
        .output()
        .expect("hyalo links fix --apply should run");
    assert!(
        apply_output.status.success(),
        "links fix --apply exited non-zero: {}",
        String::from_utf8_lossy(&apply_output.stderr)
    );

    let apply_json: serde_json::Value =
        serde_json::from_slice(&apply_output.stdout).expect("apply stdout should be valid JSON");

    // applied must be true
    assert!(
        apply_json["applied"]
            .as_bool()
            .expect("'applied' should be a bool"),
        "links fix --apply should report applied=true"
    );

    let fixed_count = apply_json["fixable"]
        .as_u64()
        .expect("'fixable' should be a number");
    assert!(fixed_count >= 1, "should have applied at least 1 fix");

    // Capture the broken link count reported by the apply run (before fixes were written).
    let before_broken = apply_json["broken"]
        .as_u64()
        .expect("'broken' should be a number");

    // Re-run links fix in dry-run mode to measure the remaining broken link count
    // (same unit: number of broken links, not files).
    let after_output = hyalo()
        .args([
            "--dir",
            tmp.path()
                .to_str()
                .expect("temp path should be valid UTF-8"),
            "links",
            "fix",
            "--format",
            "json",
        ])
        .output()
        .expect("hyalo links fix (dry-run) should run after apply");
    assert!(
        after_output.status.success(),
        "links fix dry-run after apply exited non-zero: {}",
        String::from_utf8_lossy(&after_output.stderr)
    );

    let after_json: serde_json::Value = serde_json::from_slice(&after_output.stdout)
        .expect("after dry-run stdout should be valid JSON");

    let after_broken = after_json["broken"]
        .as_u64()
        .expect("'broken' should be a number in after dry-run output");

    // After applying fixes, the broken link count must be lower — both values
    // are broken-link counts reported by `links fix`, so the comparison is like-for-like.
    assert!(
        after_broken < before_broken,
        "after applying fixes, broken link count should decrease: before={before_broken}, after={after_broken}"
    );
}

// ---------------------------------------------------------------------------
// links fix: text format
// ---------------------------------------------------------------------------

#[test]
fn links_fix_text_format() {
    let tmp = setup_vault();
    let output = hyalo()
        .args([
            "--dir",
            tmp.path()
                .to_str()
                .expect("temp path should be valid UTF-8"),
            "links",
            "fix",
            "--format",
            "text",
        ])
        .output()
        .expect("hyalo links fix --format text should run");
    assert!(
        output.status.success(),
        "links fix --format text exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let text = String::from_utf8(output.stdout).expect("stdout should be valid UTF-8");

    assert!(
        text.contains("Broken links:"),
        "text output should contain 'Broken links:' — got:\n{text}"
    );
    assert!(
        text.contains("Fixable:"),
        "text output should contain 'Fixable:' — got:\n{text}"
    );
    assert!(
        text.contains("Applied:"),
        "text output should contain 'Applied:' — got:\n{text}"
    );
    // Dry-run default should say "Applied: no"
    assert!(
        text.contains("Applied: no"),
        "default (dry-run) should say 'Applied: no' — got:\n{text}"
    );
}

// ---------------------------------------------------------------------------
// links fix: threshold controls fuzzy matching
// ---------------------------------------------------------------------------

#[test]
fn links_fix_high_threshold_suppresses_fuzzy_fixes() {
    let tmp = setup_vault();
    // With threshold=0.99 the typo "Authnticaton" should not match authentication.md
    let output = hyalo()
        .args([
            "--dir",
            tmp.path()
                .to_str()
                .expect("temp path should be valid UTF-8"),
            "links",
            "fix",
            "--threshold",
            "0.99",
            "--format",
            "json",
        ])
        .output()
        .expect("hyalo links fix --threshold should run");
    assert!(
        output.status.success(),
        "links fix --threshold exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");

    let fixes = json["fixes"]
        .as_array()
        .expect("'fixes' should be an array");

    // At threshold=0.99, the fuzzy match for "Authnticaton" → authentication.md
    // should not fire (score is well below 0.99).
    let has_auth_fix = fixes.iter().any(|fix| {
        fix["source"].as_str().unwrap_or("") == "d.md"
            && fix["old_target"]
                .as_str()
                .unwrap_or("")
                .eq_ignore_ascii_case("Authnticaton")
    });
    assert!(
        !has_auth_fix,
        "at threshold=0.99, [[Authnticaton]] should NOT produce a fix: {fixes:?}"
    );
}

#[test]
fn links_fix_default_threshold_finds_fuzzy_match() {
    let tmp = setup_vault();
    // With the default threshold, "Authnticaton" should fuzzy-match authentication.md
    let output = hyalo()
        .args([
            "--dir",
            tmp.path()
                .to_str()
                .expect("temp path should be valid UTF-8"),
            "links",
            "fix",
            "--format",
            "json",
        ])
        .output()
        .expect("hyalo links fix should run");
    assert!(output.status.success());

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");

    let fixable = json["fixable"]
        .as_u64()
        .expect("'fixable' should be a number");
    let high_threshold_output = hyalo()
        .args([
            "--dir",
            tmp.path()
                .to_str()
                .expect("temp path should be valid UTF-8"),
            "links",
            "fix",
            "--threshold",
            "0.99",
            "--format",
            "json",
        ])
        .output()
        .expect("hyalo links fix --threshold=0.99 should run");
    assert!(high_threshold_output.status.success());

    let high_json: serde_json::Value = serde_json::from_slice(&high_threshold_output.stdout)
        .expect("high threshold stdout should be valid JSON");
    let high_fixable = high_json["fixable"]
        .as_u64()
        .expect("'fixable' should be a number");

    // Default threshold should yield more (or equal) fixes than 0.99 threshold
    assert!(
        fixable >= high_fixable,
        "default threshold should yield >= fixes than threshold=0.99: default={fixable}, high={high_fixable}"
    );
}

// ---------------------------------------------------------------------------
// links fix: self-link guard
// ---------------------------------------------------------------------------

#[test]
fn links_fix_rejects_self_link() {
    let tmp = TempDir::new().expect("tempdir creation should succeed");

    // A file with a broken link whose only fuzzy candidate is itself.
    write_md(
        tmp.path(),
        "sort-by-property-value.md",
        md!(r"
---
title: Sort by property value
---
See [[sort-reverse]] for reverse sorting.
"),
    );

    let output = hyalo()
        .args([
            "--dir",
            tmp.path()
                .to_str()
                .expect("temp path should be valid UTF-8"),
            "links",
            "fix",
            "--format",
            "json",
            "--threshold",
            "0.5",
        ])
        .output()
        .expect("hyalo links fix should run");
    assert!(output.status.success());

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");

    // The broken link should be unfixable, not matched to itself.
    let fixes = json["fixes"]
        .as_array()
        .expect("'fixes' should be an array");
    assert!(
        fixes.is_empty(),
        "self-link should not appear in fixes: {fixes:?}"
    );
    assert_eq!(
        json["unfixable"].as_u64().expect("'unfixable' should be a number"),
        1,
        "broken self-link should be counted as unfixable"
    );
}
