use std::fs;

use super::common::{hyalo_no_hints, md, write_md};
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
    let output = hyalo_no_hints()
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
    let total = json["results"]["links"]["total"]
        .as_u64()
        .expect("links.total should be a number");
    assert!(total > 0, "expected at least one link, got {total}");

    // links.broken >= 1 because b.md has [[nonexistent]] and d.md has [[Authnticaton]]
    let broken = json["results"]["links"]["broken"]
        .as_u64()
        .expect("links.broken should be a number");
    assert_eq!(
        broken, 2,
        "expected 2 broken links: [[nonexistent]] from b.md and [[Authnticaton]] from d.md"
    );

    // broken_links array was removed; summary only reports counts now.
    let links_obj = json["results"]["links"]
        .as_object()
        .expect("results.links should be an object");
    assert!(
        !links_obj.contains_key("broken_links"),
        "broken_links should be removed from summary output"
    );
}

#[test]
fn summary_broken_links_count_includes_nonexistent() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
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

    // b.md has [[nonexistent]] — the broken count must reflect it
    let broken = json["results"]["links"]["broken"]
        .as_u64()
        .expect("links.broken should be a number");
    assert_eq!(
        broken, 2,
        "expected 2 broken links: [[nonexistent]] from b.md and [[Authnticaton]] from d.md"
    );
}

#[test]
fn summary_text_includes_links_line() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
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
    let output = hyalo_no_hints()
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

    let results = json["results"]
        .as_array()
        .expect("find output should have a results array");

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
    let output = hyalo_no_hints()
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

    let results = json["results"]
        .as_array()
        .expect("find output should have a results array");

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
    let output = hyalo_no_hints()
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

    let results = json["results"]
        .as_array()
        .expect("find output should have a results array");
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
    let output = hyalo_no_hints()
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
    let broken_count = json["results"]["broken"]
        .as_u64()
        .expect("'broken' should be a number");
    assert!(
        broken_count >= 1,
        "expected at least 1 broken link, got {broken_count}"
    );

    // fixable >= 1 because [[Authnticaton]] fuzzy-matches authentication.md
    let fixable_count = json["results"]["fixable"]
        .as_u64()
        .expect("'fixable' should be a number");
    assert!(
        fixable_count >= 1,
        "expected at least 1 fixable link, got {fixable_count}"
    );

    // By default (no --apply), applied must be false
    assert!(
        !json["results"]["applied"]
            .as_bool()
            .expect("'applied' should be a bool"),
        "dry-run should report applied=false"
    );

    // fixes is an array with entries having source, line, old_target, new_target
    let fixes = json["results"]["fixes"]
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
    let output = hyalo_no_hints()
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

    let fixes = json["results"]["fixes"]
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
    let apply_output = hyalo_no_hints()
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
        apply_json["results"]["applied"]
            .as_bool()
            .expect("'applied' should be a bool"),
        "links fix --apply should report applied=true"
    );

    let fixed_count = apply_json["results"]["fixable"]
        .as_u64()
        .expect("'fixable' should be a number");
    assert!(fixed_count >= 1, "should have applied at least 1 fix");

    // Capture the broken link count reported by the apply run (before fixes were written).
    let before_broken = apply_json["results"]["broken"]
        .as_u64()
        .expect("'broken' should be a number");

    // Re-run links fix in dry-run mode to measure the remaining broken link count
    // (same unit: number of broken links, not files).
    let after_output = hyalo_no_hints()
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

    let after_broken = after_json["results"]["broken"]
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
    let output = hyalo_no_hints()
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
    let output = hyalo_no_hints()
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

    let fixes = json["results"]["fixes"]
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
    let output = hyalo_no_hints()
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

    let fixable = json["results"]["fixable"]
        .as_u64()
        .expect("'fixable' should be a number");
    let high_threshold_output = hyalo_no_hints()
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
    let high_fixable = high_json["results"]["fixable"]
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

    let output = hyalo_no_hints()
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
    let fixes = json["results"]["fixes"]
        .as_array()
        .expect("'fixes' should be an array");
    assert!(
        fixes.is_empty(),
        "self-link should not appear in fixes: {fixes:?}"
    );
    assert_eq!(
        json["results"]["unfixable"]
            .as_u64()
            .expect("'unfixable' should be a number"),
        1,
        "broken self-link should be counted as unfixable"
    );
}

// ---------------------------------------------------------------------------
// UX-3: --ignore-target
// ---------------------------------------------------------------------------

#[test]
fn links_fix_ignore_target() {
    let tmp = tempfile::tempdir().unwrap();
    // page.md has two broken links: one normal missing link, one Hugo template target
    write_md(
        tmp.path(),
        "page.md",
        md!(r"
---
title: Page
---
# Page

See [[missing-note]] and [template]({{ .RelPermalink }}).
"),
    );
    write_md(
        tmp.path(),
        "other.md",
        md!(r"
---
title: Other
---
# Other

Some text.
"),
    );

    let out = super::common::hyalo_no_hints()
        .args(["links", "fix", "--ignore-target", "{{", "--format", "json"])
        .arg("--dir")
        .arg(tmp.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    // The Hugo template link should be ignored
    assert_eq!(
        json["results"]["ignored"]
            .as_u64()
            .expect("'ignored' should be a number"),
        1,
        "expected 1 ignored link: {json}"
    );
}

#[test]
fn links_fix_ignore_target_multiple() {
    let tmp = tempfile::tempdir().unwrap();
    write_md(
        tmp.path(),
        "page.md",
        md!(r"
---
title: Page
---
# Page

See [[missing]] and [hugo]({{ .RelPermalink }}) and [hugo2]({{ .Site.BaseURL }}).
"),
    );

    // Two distinct --ignore-target patterns: one matches RelPermalink, the other BaseURL
    let out = super::common::hyalo_no_hints()
        .args([
            "links",
            "fix",
            "--ignore-target",
            "RelPermalink",
            "--ignore-target",
            "BaseURL",
            "--format",
            "json",
        ])
        .arg("--dir")
        .arg(tmp.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        json["results"]["ignored"].as_u64().unwrap_or(0),
        2,
        "expected 2 ignored links (one per pattern): {json}"
    );
}

#[test]
fn links_fix_ignore_target_absent() {
    // With no matching ignore_target, count should be 0
    let tmp = setup_vault();
    let out = super::common::hyalo_no_hints()
        .args([
            "links",
            "fix",
            "--ignore-target",
            "this-will-not-match-anything",
            "--format",
            "json",
        ])
        .arg("--dir")
        .arg(tmp.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        json["results"]["ignored"]
            .as_u64()
            .expect("'ignored' should be a number"),
        0,
        "expected 0 ignored links when pattern doesn't match: {json}"
    );
}

// ---------------------------------------------------------------------------
// Case-insensitive link resolution (iter-117)
// ---------------------------------------------------------------------------

/// Build a fixture vault with case_insensitive = "true".
///
///   iteration_protocols.md  (the file, all lowercase)
///   promise_any.md          (links to it with wrong casing via wikilink)
///   .hyalo.toml             with case_insensitive = "true"
fn setup_mdn_vault() -> TempDir {
    let tmp = TempDir::new().expect("tempdir");

    // Target file — all lowercase
    write_md(
        tmp.path(),
        "iteration_protocols.md",
        md!(r"
---
title: Iteration protocols
---
Content here.
"),
    );

    // Source file — wikilink with different casing from on-disk name
    write_md(
        tmp.path(),
        "promise_any.md",
        md!(r"
---
title: Promise.any
---
See [[Iteration_Protocols]] for details.
"),
    );

    // Config: case_insensitive = "true" forces the fallback regardless of filesystem
    fs::write(
        tmp.path().join(".hyalo.toml"),
        "[links]\ncase_insensitive = \"true\"\n",
    )
    .expect("write .hyalo.toml");

    tmp
}

#[test]
fn case_insensitive_find_links_resolves_to_canonical_path() {
    let tmp = setup_mdn_vault();

    let out = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "find",
            "--file",
            "promise_any.md",
            "--fields",
            "links",
            "--format",
            "json",
        ])
        .output()
        .expect("hyalo find should run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let results = json["results"]
        .as_array()
        .expect("results should be an array");
    assert!(!results.is_empty(), "results should not be empty");
    let links = results[0]["links"]
        .as_array()
        .expect("links should be an array");

    // At least one link should resolve to the canonical lowercase path
    let canonical = "iteration_protocols.md";
    let has_resolved = links.iter().any(|l| l["path"].as_str() == Some(canonical));
    assert!(
        has_resolved,
        "expected link to resolve to canonical path {canonical:?}, got: {links:?}"
    );
}

#[test]
fn case_insensitive_links_fix_dry_run_reports_case_mismatches() {
    let tmp = setup_mdn_vault();

    let out = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "links",
            "fix",
            "--format",
            "json",
        ])
        .output()
        .expect("hyalo links fix should run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let mismatches = json["results"]["case_mismatches"]
        .as_u64()
        .expect("case_mismatches should be a number");
    assert!(
        mismatches >= 1,
        "expected at least 1 case-mismatch fix, got: {json}"
    );

    let mismatch_fixes = json["results"]["case_mismatch_fixes"]
        .as_array()
        .expect("case_mismatch_fixes should be an array");
    assert!(
        !mismatch_fixes.is_empty(),
        "case_mismatch_fixes should list the mismatch entries"
    );

    // The fix should have strategy = "LinkCaseMismatch"
    let strategy = mismatch_fixes[0]["strategy"].as_str().unwrap_or("");
    assert_eq!(
        strategy, "LinkCaseMismatch",
        "strategy should be LinkCaseMismatch"
    );
}

#[test]
fn case_insensitive_links_fix_apply_rewrites_casing() {
    let tmp = setup_mdn_vault();

    // Apply fixes
    let apply = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "links",
            "fix",
            "--apply",
            "--format",
            "json",
        ])
        .output()
        .expect("hyalo links fix --apply should run");
    assert!(
        apply.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&apply.stderr)
    );

    let apply_json: serde_json::Value = serde_json::from_slice(&apply.stdout).unwrap();
    let applied = apply_json["results"]["applied"].as_bool().unwrap_or(false);
    assert!(applied, "applied should be true");

    // After applying, re-run links fix — case_mismatches should drop to 0
    let after = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "links",
            "fix",
            "--format",
            "json",
        ])
        .output()
        .expect("hyalo links fix dry-run after apply should run");
    assert!(after.status.success());

    let after_json: serde_json::Value = serde_json::from_slice(&after.stdout).unwrap();
    let remaining = after_json["results"]["case_mismatches"]
        .as_u64()
        .unwrap_or(1);
    assert_eq!(
        remaining, 0,
        "after apply, case_mismatches should be 0, got: {after_json}"
    );
}

// On macOS (case-insensitive FS) a wrong-cased path resolves via the OS even with CI mode
// disabled, so this test is only meaningful on case-sensitive filesystems.
#[cfg(target_os = "linux")]
#[test]
fn case_insensitive_off_treats_wrong_casing_as_unresolved() {
    let tmp = TempDir::new().expect("tempdir");

    // Target file — all lowercase
    write_md(
        tmp.path(),
        "iteration_protocols.md",
        md!(r"
---
title: Iteration protocols
---
Content.
"),
    );

    // Source — wikilink with different casing from on-disk name
    write_md(
        tmp.path(),
        "promise_any.md",
        md!(r"
---
title: Promise.any
---
See [[Iteration_Protocols]] for details.
"),
    );

    // case_insensitive = "false" — strict mode, no fallback
    fs::write(
        tmp.path().join(".hyalo.toml"),
        "[links]\ncase_insensitive = \"false\"\n",
    )
    .expect("write .hyalo.toml");

    let out = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "find",
            "--file",
            "promise_any.md",
            "--fields",
            "links",
            "--format",
            "json",
        ])
        .output()
        .expect("hyalo find should run");
    assert!(out.status.success());

    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let results = json["results"]
        .as_array()
        .expect("results should be an array");
    assert!(!results.is_empty(), "results should not be empty");
    let links = results[0]["links"]
        .as_array()
        .expect("links should be an array");

    // In strict mode the PascalCase link should NOT resolve (null path)
    let has_null_path = links
        .iter()
        .any(|l| l["path"].is_null() || l["path"] == serde_json::Value::Null);
    assert!(
        has_null_path,
        "strict mode: PascalCase link should be unresolved (null path), got: {links:?}"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn case_insensitive_ambiguous_returns_unresolved_on_case_sensitive_fs() {
    // On a case-sensitive filesystem, Foo.md and foo.md are two distinct files.
    // A link to FOO should be ambiguous and resolve to None.
    let tmp = TempDir::new().expect("tempdir");

    write_md(tmp.path(), "Foo.md", "---\ntitle: Foo\n---\n");
    write_md(tmp.path(), "foo.md", "---\ntitle: foo\n---\n");
    write_md(
        tmp.path(),
        "source.md",
        "---\ntitle: Source\n---\nSee [[FOO]] here.\n",
    );

    fs::write(
        tmp.path().join(".hyalo.toml"),
        "[links]\ncase_insensitive = \"true\"\n",
    )
    .expect("write .hyalo.toml");

    let out = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "find",
            "--file",
            "source.md",
            "--fields",
            "links",
            "--format",
            "json",
        ])
        .output()
        .expect("hyalo find should run");
    assert!(out.status.success());

    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let results = json["results"]
        .as_array()
        .expect("results should be an array");
    assert!(!results.is_empty(), "results should not be empty");
    let links = results[0]["links"]
        .as_array()
        .expect("links should be an array");

    // Ambiguous: both Foo.md and foo.md exist — should be unresolved (null path)
    let all_unresolved = links
        .iter()
        .all(|l| l["path"].is_null() || l["path"] == serde_json::Value::Null);
    assert!(
        all_unresolved,
        "ambiguous case-insensitive match should be unresolved, got: {links:?}"
    );
}

// ---------------------------------------------------------------------------
// links auto
// ---------------------------------------------------------------------------

/// Run `hyalo links auto` against `dir` with the given extra args, parse the
/// JSON envelope and return the `results` object.
fn run_links_auto(dir: &std::path::Path, extra_args: &[&str]) -> serde_json::Value {
    let mut cmd = hyalo_no_hints();
    cmd.args([
        "--dir",
        dir.to_str().expect("temp path should be valid UTF-8"),
    ])
    .args(["links", "auto"])
    .args(extra_args);
    let output = cmd.output().expect("hyalo links auto should run");
    assert!(
        output.status.success(),
        "links auto exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    json["results"].clone()
}

#[test]
fn links_auto_dry_run_finds_mentions() {
    let tmp = TempDir::new().expect("tempdir creation should succeed");

    write_md(
        tmp.path(),
        "sprint-review.md",
        md!(r"
---
title: Sprint Review
---
Sprint review process description.
"),
    );
    write_md(
        tmp.path(),
        "meetings.md",
        md!(r"
---
title: Meetings
---
We held a Sprint Review last week.
"),
    );

    let results = run_links_auto(tmp.path(), &["--format", "json"]);

    let total = results["total"]
        .as_u64()
        .expect("results.total should be a number");
    assert!(
        total >= 1,
        "expected at least 1 unlinked mention, got {total}"
    );

    let applied = results["applied"]
        .as_bool()
        .expect("results.applied should be a bool");
    assert!(!applied, "dry-run should report applied=false");

    let matches = results["matches"]
        .as_array()
        .expect("results.matches should be an array");
    let has_meetings_match = matches.iter().any(|m| {
        m["file"].as_str() == Some("meetings.md")
            && m["link_target"].as_str() == Some("sprint-review")
    });
    assert!(
        has_meetings_match,
        "expected a match in meetings.md with link_target=sprint-review, matches: {matches:?}"
    );
}

#[test]
fn links_auto_apply_writes_wikilinks() {
    let tmp = TempDir::new().expect("tempdir creation should succeed");

    write_md(
        tmp.path(),
        "sprint-review.md",
        md!(r"
---
title: Sprint Review
---
Sprint review process description.
"),
    );
    write_md(
        tmp.path(),
        "meetings.md",
        md!(r"
---
title: Meetings
---
We held a Sprint Review last week.
"),
    );

    let results = run_links_auto(tmp.path(), &["--apply", "--format", "json"]);

    let applied = results["applied"]
        .as_bool()
        .expect("results.applied should be a bool");
    assert!(applied, "links auto --apply should report applied=true");

    let total = results["total"]
        .as_u64()
        .expect("results.total should be a number");
    assert!(total >= 1, "expected at least 1 applied replacement");

    let meetings_content = fs::read_to_string(tmp.path().join("meetings.md"))
        .expect("meetings.md should be readable after apply");
    assert!(
        meetings_content.contains("[[sprint-review]]"),
        "meetings.md should contain [[sprint-review]] after apply, got:\n{meetings_content}"
    );
    // The bare mention on that line should have been replaced — it must not
    // appear as plain text followed by a non-bracket character.
    let bare_mention_still_present = meetings_content
        .lines()
        .any(|l| l.contains("Sprint Review") && !l.contains("[[sprint-review]]"));
    assert!(
        !bare_mention_still_present,
        "bare 'Sprint Review' (outside brackets) should be gone after apply, got:\n{meetings_content}"
    );
}

#[test]
fn links_auto_skips_existing_links() {
    let tmp = TempDir::new().expect("tempdir creation should succeed");

    write_md(
        tmp.path(),
        "sprint-review.md",
        md!(r"
---
title: Sprint Review
---
Sprint review process description.
"),
    );
    // One already-linked mention and one bare mention on a different line.
    write_md(
        tmp.path(),
        "notes.md",
        md!(r"
---
title: Notes
---
See [[sprint-review]] here.
Sprint Review on Friday.
"),
    );

    let results = run_links_auto(tmp.path(), &["--format", "json"]);

    let matches = results["matches"]
        .as_array()
        .expect("results.matches should be an array");
    let notes_matches: Vec<_> = matches
        .iter()
        .filter(|m| m["file"].as_str() == Some("notes.md"))
        .collect();
    assert_eq!(
        notes_matches.len(),
        1,
        "only the unlinked mention on the second line should match, got: {notes_matches:?}"
    );
}

#[test]
fn links_auto_skips_code_blocks() {
    let tmp = TempDir::new().expect("tempdir creation should succeed");

    write_md(
        tmp.path(),
        "config.md",
        md!(r"
---
title: Config
---
Configuration reference.
"),
    );
    write_md(
        tmp.path(),
        "docs.md",
        md!(r"
---
title: Docs
---
```
Config details go here
```
See Config for more information.
"),
    );

    let results = run_links_auto(tmp.path(), &["--format", "json"]);

    let matches = results["matches"]
        .as_array()
        .expect("results.matches should be an array");

    // Only the mention outside the code block should match.
    let docs_matches: Vec<_> = matches
        .iter()
        .filter(|m| m["file"].as_str() == Some("docs.md"))
        .collect();

    // Line numbers inside the fenced block should not appear.
    let has_code_block_match = docs_matches.iter().any(|m| {
        // Lines 4 and 5 (1-based) are inside the fence; line 7 is outside.
        m["line"].as_u64().is_some_and(|l| (4..=5).contains(&l))
    });
    assert!(
        !has_code_block_match,
        "matches inside code block should be skipped, docs matches: {docs_matches:?}"
    );

    // There should be exactly one match: the outside mention.
    assert_eq!(
        docs_matches.len(),
        1,
        "only the mention outside the code block should match, got: {docs_matches:?}"
    );
}

#[test]
fn links_auto_skips_headings() {
    let tmp = TempDir::new().expect("tempdir creation should succeed");

    write_md(
        tmp.path(),
        "alpha.md",
        md!(r"
---
title: Alpha
---
Alpha documentation.
"),
    );
    write_md(
        tmp.path(),
        "page.md",
        md!(r"
---
title: Page
---
# Alpha Section
Alpha is great.
"),
    );

    let results = run_links_auto(tmp.path(), &["--format", "json"]);

    let matches = results["matches"]
        .as_array()
        .expect("results.matches should be an array");

    let page_matches: Vec<_> = matches
        .iter()
        .filter(|m| m["file"].as_str() == Some("page.md"))
        .collect();

    // The heading line (# Alpha Section) should be skipped; only the body
    // line "Alpha is great." should produce a match.
    assert_eq!(
        page_matches.len(),
        1,
        "only the body-text mention should match (not the heading), got: {page_matches:?}"
    );
    // The match must be on the body line (line 5, "Alpha is great.") and NOT
    // on the heading line (line 4, "# Alpha Section").
    let match_line = page_matches[0]["line"]
        .as_u64()
        .expect("match.line should be a number");
    assert!(
        match_line > 4,
        "match should be on the body line after the heading, got line {match_line}"
    );
}

#[test]
fn links_auto_skips_self_links() {
    let tmp = TempDir::new().expect("tempdir creation should succeed");

    write_md(
        tmp.path(),
        "sprint-review.md",
        md!(r"
---
title: Sprint Review
---
This Sprint Review process is important.
"),
    );

    let results = run_links_auto(tmp.path(), &["--format", "json"]);

    let total = results["total"]
        .as_u64()
        .expect("results.total should be a number");
    assert_eq!(
        total, 0,
        "a file should not generate a self-link, got total={total}"
    );
}

#[test]
fn links_auto_min_length_filter() {
    let tmp = TempDir::new().expect("tempdir creation should succeed");

    write_md(
        tmp.path(),
        "a.md",
        md!(r"
---
title: A
---
Single character title.
"),
    );
    write_md(
        tmp.path(),
        "beta.md",
        md!(r"
---
title: Beta
---
Beta documentation.
"),
    );
    write_md(
        tmp.path(),
        "page.md",
        md!(r"
---
title: Page
---
A and Beta are both mentioned here.
"),
    );

    // With default --min-length 3, only "Beta" (len 4) should match.
    let results_default = run_links_auto(tmp.path(), &["--format", "json"]);
    let matches_default = results_default["matches"]
        .as_array()
        .expect("results.matches should be an array");
    let page_default: Vec<_> = matches_default
        .iter()
        .filter(|m| m["file"].as_str() == Some("page.md"))
        .collect();
    let has_beta_default = page_default
        .iter()
        .any(|m| m["link_target"].as_str() == Some("beta"));
    let has_a_default = page_default
        .iter()
        .any(|m| m["link_target"].as_str() == Some("a"));
    assert!(
        has_beta_default,
        "Beta should match with default min-length, matches: {page_default:?}"
    );
    assert!(
        !has_a_default,
        "single-char title 'A' should be filtered by default min-length=3, matches: {page_default:?}"
    );

    // With --min-length 1, "A" should also match.
    let results_min1 = run_links_auto(tmp.path(), &["--min-length", "1", "--format", "json"]);
    let matches_min1 = results_min1["matches"]
        .as_array()
        .expect("results.matches should be an array");
    let page_min1: Vec<_> = matches_min1
        .iter()
        .filter(|m| m["file"].as_str() == Some("page.md"))
        .collect();
    let has_a_min1 = page_min1
        .iter()
        .any(|m| m["link_target"].as_str() == Some("a"));
    assert!(
        has_a_min1,
        "single-char title 'A' should match with --min-length 1, matches: {page_min1:?}"
    );
}

#[test]
fn links_auto_exclude_title() {
    let tmp = TempDir::new().expect("tempdir creation should succeed");

    write_md(
        tmp.path(),
        "sprint-review.md",
        md!(r"
---
title: Sprint Review
---
Sprint review process.
"),
    );
    write_md(
        tmp.path(),
        "daily.md",
        md!(r"
---
title: Daily
---
Daily standup notes.
"),
    );
    write_md(
        tmp.path(),
        "page.md",
        md!(r"
---
title: Page
---
Sprint Review and Daily are both mentioned.
"),
    );

    let results = run_links_auto(
        tmp.path(),
        &["--exclude-title", "Sprint Review", "--format", "json"],
    );

    let matches = results["matches"]
        .as_array()
        .expect("results.matches should be an array");
    let has_sprint_review = matches
        .iter()
        .any(|m| m["link_target"].as_str() == Some("sprint-review"));
    let has_daily = matches
        .iter()
        .any(|m| m["link_target"].as_str() == Some("daily"));

    assert!(
        !has_sprint_review,
        "Sprint Review should be excluded via --exclude-title, matches: {matches:?}"
    );
    assert!(
        has_daily,
        "Daily should still match (not excluded), matches: {matches:?}"
    );
}

#[test]
fn links_auto_text_format() {
    let tmp = TempDir::new().expect("tempdir creation should succeed");

    write_md(
        tmp.path(),
        "sprint-review.md",
        md!(r"
---
title: Sprint Review
---
Sprint review process.
"),
    );
    write_md(
        tmp.path(),
        "notes.md",
        md!(r"
---
title: Notes
---
Sprint Review happened last week.
"),
    );

    let mut cmd = hyalo_no_hints();
    cmd.args([
        "--dir",
        tmp.path()
            .to_str()
            .expect("temp path should be valid UTF-8"),
        "links",
        "auto",
        "--format",
        "text",
    ]);
    let output = cmd
        .output()
        .expect("hyalo links auto --format text should run");
    assert!(
        output.status.success(),
        "links auto --format text exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let text = String::from_utf8(output.stdout).expect("stdout should be valid UTF-8");

    assert!(
        text.to_lowercase().contains("unlinked mention"),
        "text output should contain 'unlinked mention', got:\n{text}"
    );
    assert!(
        text.contains("Applied: no"),
        "dry-run text output should contain 'Applied: no', got:\n{text}"
    );
    assert!(
        text.contains('\u{2192}'),
        "text output should contain the → arrow in match lines, got:\n{text}"
    );
}

#[test]
fn links_auto_ambiguous_titles_skipped() {
    let tmp = TempDir::new().expect("tempdir creation should succeed");

    // Two files with the same title — the title is ambiguous.
    write_md(
        tmp.path(),
        "alpha.md",
        md!(r"
---
title: Common Title
---
First file.
"),
    );
    write_md(
        tmp.path(),
        "beta.md",
        md!(r"
---
title: Common Title
---
Second file.
"),
    );
    write_md(
        tmp.path(),
        "page.md",
        md!(r"
---
title: Page
---
See Common Title here.
"),
    );

    let results = run_links_auto(tmp.path(), &["--format", "json"]);

    let total = results["total"]
        .as_u64()
        .expect("results.total should be a number");
    assert_eq!(
        total, 0,
        "ambiguous title should produce no matches, got total={total}"
    );

    let ambiguous = results["ambiguous_titles"]
        .as_array()
        .expect("results.ambiguous_titles should be an array");
    assert!(
        !ambiguous.is_empty(),
        "ambiguous_titles should be non-empty when two files share the same title"
    );
}

#[test]
fn links_auto_word_boundaries() {
    let tmp = TempDir::new().expect("tempdir creation should succeed");

    write_md(
        tmp.path(),
        "sprint.md",
        md!(r"
---
title: Sprint
---
Sprint documentation.
"),
    );
    write_md(
        tmp.path(),
        "page.md",
        md!(r"
---
title: Page
---
Sprinting fast. Sprint starts Monday.
"),
    );

    let results = run_links_auto(tmp.path(), &["--format", "json"]);

    let matches = results["matches"]
        .as_array()
        .expect("results.matches should be an array");
    let page_matches: Vec<_> = matches
        .iter()
        .filter(|m| m["file"].as_str() == Some("page.md"))
        .collect();

    // Only the standalone "Sprint" word should match, not "Sprint" inside "Sprinting".
    assert_eq!(
        page_matches.len(),
        1,
        "only standalone 'Sprint' should match (not inside 'Sprinting'), got: {page_matches:?}"
    );
    let matched_text = page_matches[0]["matched_text"]
        .as_str()
        .expect("match.matched_text should be a string");
    assert_eq!(
        matched_text.to_ascii_lowercase(),
        "sprint",
        "matched text should be 'sprint', got: {matched_text}"
    );
}

#[test]
fn links_auto_glob_filter() {
    let tmp = TempDir::new().expect("tempdir creation should succeed");

    write_md(
        tmp.path(),
        "sprint-review.md",
        md!(r"
---
title: Sprint Review
---
Sprint review process.
"),
    );
    write_md(
        tmp.path(),
        "meetings/weekly.md",
        md!(r"
---
title: Weekly Meeting
---
We covered Sprint Review in this session.
"),
    );
    write_md(
        tmp.path(),
        "other.md",
        md!(r"
---
title: Other
---
Sprint Review was also mentioned here.
"),
    );

    let results = run_links_auto(tmp.path(), &["--glob", "meetings/*", "--format", "json"]);

    let matches = results["matches"]
        .as_array()
        .expect("results.matches should be an array");

    // Only meetings/weekly.md should appear in the matches.
    let has_weekly = matches
        .iter()
        .any(|m| m["file"].as_str() == Some("meetings/weekly.md"));
    let has_other = matches
        .iter()
        .any(|m| m["file"].as_str() == Some("other.md"));

    assert!(
        has_weekly,
        "meetings/weekly.md should have matches when glob=meetings/*, matches: {matches:?}"
    );
    assert!(
        !has_other,
        "other.md should be excluded by the glob filter, matches: {matches:?}"
    );
}

#[test]
fn links_auto_first_only() {
    let tmp = TempDir::new().expect("tempdir creation should succeed");

    write_md(
        tmp.path(),
        "alice.md",
        md!(r"
---
title: Alice
---
Alice bio.
"),
    );
    write_md(
        tmp.path(),
        "notes.md",
        md!(r"
---
title: Notes
---
Alice went to the park. Later Alice came back. Then Alice left again.
"),
    );

    // Without --first-only: multiple Alice matches in notes.md
    let results = run_links_auto(tmp.path(), &["--format", "json"]);
    let matches = results["matches"]
        .as_array()
        .expect("results.matches should be an array");
    let alice_count = matches
        .iter()
        .filter(|m| {
            m["file"].as_str() == Some("notes.md") && m["link_target"].as_str() == Some("alice")
        })
        .count();
    assert!(
        alice_count >= 2,
        "without --first-only, expected multiple Alice matches, got {alice_count}"
    );

    // With --first-only: at most 1 Alice match per file
    let results = run_links_auto(tmp.path(), &["--first-only", "--format", "json"]);
    let matches = results["matches"]
        .as_array()
        .expect("results.matches should be an array");
    let alice_count = matches
        .iter()
        .filter(|m| {
            m["file"].as_str() == Some("notes.md") && m["link_target"].as_str() == Some("alice")
        })
        .count();
    assert_eq!(
        alice_count, 1,
        "with --first-only, expected exactly 1 Alice match, got {alice_count}"
    );
}

#[test]
fn links_auto_first_only_with_apply() {
    let tmp = TempDir::new().expect("tempdir creation should succeed");

    write_md(
        tmp.path(),
        "alice.md",
        md!(r"
---
title: Alice
---
Alice bio.
"),
    );
    write_md(
        tmp.path(),
        "notes.md",
        md!(r"
---
title: Notes
---
Alice went to the park. Later Alice came back.
"),
    );

    let results = run_links_auto(tmp.path(), &["--first-only", "--apply", "--format", "json"]);
    assert_eq!(
        results["applied"].as_bool(),
        Some(true),
        "should report applied=true"
    );

    let content = std::fs::read_to_string(tmp.path().join("notes.md")).unwrap();
    let link_count = content.matches("[[alice").count();
    assert_eq!(
        link_count, 1,
        "with --first-only --apply, only first mention should be linked, content: {content}"
    );
}

#[test]
fn links_auto_exclude_target_glob() {
    let tmp = TempDir::new().expect("tempdir creation should succeed");

    write_md(
        tmp.path(),
        "templates/start.md",
        md!(r"
---
title: Start
---
Start template.
"),
    );
    write_md(
        tmp.path(),
        "people/alice.md",
        md!(r"
---
title: Alice
---
Alice bio.
"),
    );
    write_md(
        tmp.path(),
        "notes.md",
        md!(r"
---
title: Notes
---
We Start with Alice today.
"),
    );

    // Without exclusion: both match
    let results = run_links_auto(tmp.path(), &["--format", "json"]);
    let matches = results["matches"]
        .as_array()
        .expect("results.matches should be an array");
    let has_start = matches
        .iter()
        .any(|m| m["link_target"].as_str() == Some("start"));
    let has_alice = matches
        .iter()
        .any(|m| m["link_target"].as_str() == Some("alice"));
    assert!(has_start, "without exclusion, Start should match");
    assert!(has_alice, "without exclusion, Alice should match");

    // With --exclude-target-glob: Start should be excluded
    let results = run_links_auto(
        tmp.path(),
        &["--exclude-target-glob", "templates/*", "--format", "json"],
    );
    let matches = results["matches"]
        .as_array()
        .expect("results.matches should be an array");
    let has_start = matches
        .iter()
        .any(|m| m["link_target"].as_str() == Some("start"));
    let has_alice = matches
        .iter()
        .any(|m| m["link_target"].as_str() == Some("alice"));
    assert!(
        !has_start,
        "Start should be excluded by --exclude-target-glob, matches: {matches:?}"
    );
    assert!(has_alice, "Alice should still match, matches: {matches:?}");
}

#[test]
fn links_auto_exclude_target_glob_multiple() {
    let tmp = TempDir::new().expect("tempdir creation should succeed");

    write_md(
        tmp.path(),
        "templates/start.md",
        md!(r"
---
title: Start
---
Start template.
"),
    );
    write_md(
        tmp.path(),
        "archive/old-note.md",
        md!(r"
---
title: Old Note
---
Old note content.
"),
    );
    write_md(
        tmp.path(),
        "people/alice.md",
        md!(r"
---
title: Alice
---
Alice bio.
"),
    );
    write_md(
        tmp.path(),
        "notes.md",
        md!(r"
---
title: Notes
---
We Start with Alice and review the Old Note.
"),
    );

    let results = run_links_auto(
        tmp.path(),
        &[
            "--exclude-target-glob",
            "templates/*",
            "--exclude-target-glob",
            "archive/*",
            "--format",
            "json",
        ],
    );
    let matches = results["matches"]
        .as_array()
        .expect("results.matches should be an array");
    let targets: Vec<&str> = matches
        .iter()
        .filter_map(|m| m["link_target"].as_str())
        .collect();
    assert!(
        !targets.contains(&"start"),
        "templates/* should be excluded, targets: {targets:?}"
    );
    assert!(
        !targets.contains(&"old-note"),
        "archive/* should be excluded, targets: {targets:?}"
    );
    assert!(
        targets.contains(&"alice"),
        "Alice should NOT be excluded, targets: {targets:?}"
    );
}

#[test]
fn links_auto_first_only_and_exclude_target_glob_combined() {
    let tmp = TempDir::new().expect("tempdir creation should succeed");

    write_md(
        tmp.path(),
        "templates/start.md",
        md!(r"
---
title: Start
---
Start template.
"),
    );
    write_md(
        tmp.path(),
        "alice.md",
        md!(r"
---
title: Alice
---
Alice bio.
"),
    );
    write_md(
        tmp.path(),
        "notes.md",
        md!(r"
---
title: Notes
---
Alice went to Start the project. Then Alice returned. We Start again.
"),
    );

    let results = run_links_auto(
        tmp.path(),
        &[
            "--first-only",
            "--exclude-target-glob",
            "templates/*",
            "--format",
            "json",
        ],
    );
    let matches = results["matches"]
        .as_array()
        .expect("results.matches should be an array");

    // Start should be fully excluded (--exclude-target-glob)
    let has_start = matches
        .iter()
        .any(|m| m["link_target"].as_str() == Some("start"));
    assert!(!has_start, "Start should be excluded by glob");

    // Alice should appear exactly once (--first-only)
    let alice_count = matches
        .iter()
        .filter(|m| m["link_target"].as_str() == Some("alice"))
        .count();
    assert_eq!(
        alice_count, 1,
        "Alice should appear exactly once with --first-only"
    );
}
