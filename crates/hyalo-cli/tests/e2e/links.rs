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

// L-A1: angle-bracket destinations `[text](<my dest.md>)` must not be
// flagged as broken, and must resolve correctly for `backlinks`.
#[test]
fn find_broken_links_ignores_angle_bracket_destination_with_spaces() {
    let tmp = TempDir::new().expect("tempdir creation should succeed");
    write_md(tmp.path(), "my dest.md", "# My Dest\n");
    write_md(
        tmp.path(),
        "source.md",
        "See [spaced link](<my dest.md>) for details.\n",
    );

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
    let files: Vec<&str> = results
        .iter()
        .map(|r| r["file"].as_str().unwrap_or(""))
        .collect();
    assert!(
        !files.contains(&"source.md"),
        "source.md's angle-bracket destination resolves to a real file and \
         must NOT be reported as broken: {files:?}"
    );

    // `backlinks` must also resolve the angle-bracket destination.
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["backlinks", "--file", "my dest.md"])
        .output()
        .expect("hyalo backlinks should run");
    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(json["total"], 1, "backlinks JSON: {json}");
    assert_eq!(json["results"]["backlinks"][0]["source"], "source.md");
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

    // The only fixable link in this fixture ([[Authnticaton]] →
    // authentication.md) is a fuzzy (Jaro-Winkler) match, so it is reported
    // in the `fuzzy`/`fuzzy_fixes` bucket, not `fixable`/`fixes` (L-10:
    // `fixable`/`fixes` cover only certain, non-fuzzy fixes).
    let fixable_count = json["results"]["fixable"]
        .as_u64()
        .expect("'fixable' should be a number");
    assert_eq!(
        fixable_count, 0,
        "no certain (non-fuzzy) fixes in this fixture, got {fixable_count}"
    );

    // By default (no --apply), applied must be false
    assert!(
        !json["results"]["applied"]
            .as_bool()
            .expect("'applied' should be a bool"),
        "dry-run should report applied=false"
    );

    // fixes is empty (the only candidate is fuzzy); the fuzzy fix is reported
    // separately with source/line/old_target/new_target/confidence.
    let fixes = json["results"]["fixes"]
        .as_array()
        .expect("'fixes' should be an array");
    assert!(
        fixes.is_empty(),
        "fixes array should be empty when the only candidate is fuzzy: {fixes:?}"
    );

    let fuzzy_fixes = json["results"]["fuzzy_fixes"]
        .as_array()
        .expect("'fuzzy_fixes' should be an array");
    assert!(
        !fuzzy_fixes.is_empty(),
        "fuzzy_fixes should contain the [[Authnticaton]] candidate"
    );

    let first_fix = &fuzzy_fixes[0];
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

/// Classify-verdict lock (iter-189 task 1): pin the `broken` / `case_mismatches`
/// / `ambiguous` buckets for a single vault that exercises every
/// `LinkResolution` variant at once, so the collapse of the Classify-side
/// resolution onto the shared `discovery::classify_link_from_source` entry point
/// cannot silently reshuffle verdicts across buckets. `case_insensitive = "true"`
/// forces the case-insensitive fallback regardless of the host filesystem so the
/// case-mismatch bucket is populated deterministically on every OS.
#[test]
fn links_fix_classify_verdict_buckets_lock() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let dir = tmp.path();

    // Vault files:
    //   Notes/Alpha.md   — target of a valid short-form + a case-mismatch link
    //   dup-a/Twin.md    } two files sharing stem "Twin" → short-form ambiguous
    //   dup-b/Twin.md    }
    fs::create_dir_all(dir.join("Notes")).unwrap();
    fs::create_dir_all(dir.join("dup-a")).unwrap();
    fs::create_dir_all(dir.join("dup-b")).unwrap();
    write_md(dir, "Notes/Alpha.md", md!("---\ntitle: Alpha\n---\n"));
    write_md(dir, "dup-a/Twin.md", md!("---\ntitle: Twin A\n---\n"));
    write_md(dir, "dup-b/Twin.md", md!("---\ntitle: Twin B\n---\n"));

    // Force the case-insensitive fallback so the case-mismatch bucket is
    // deterministic on both case-sensitive (Linux) and case-insensitive (macOS)
    // filesystems.
    fs::write(
        dir.join(".hyalo.toml"),
        "[links]\ncase_insensitive = \"true\"\n",
    )
    .expect("write .hyalo.toml");

    // source.md exercises each variant exactly once:
    //   [[Notes/Alpha]]  — resolved (path-form, correct case)      → no bucket
    //   [[notes/alpha]]  — case-mismatch (path-form, wrong case)   → case_mismatches
    //   [[Alpha]]        — short-form valid (unique stem)          → no bucket
    //   [[Twin]]         — short-form ambiguous (≥2 stems)         → ambiguous
    //   [[DoesNotExist]] — broken                                  → broken
    write_md(
        dir,
        "source.md",
        md!(r"
---
title: Source
---
[[Notes/Alpha]]
[[notes/alpha]]
[[Alpha]]
[[Twin]]
[[DoesNotExist]]
"),
    );

    let output = hyalo_no_hints()
        .args([
            "--dir",
            dir.to_str().unwrap(),
            "links",
            "fix",
            "--format",
            "json",
        ])
        .output()
        .expect("links fix should run");
    assert!(
        output.status.success(),
        "links fix exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    let r = &json["results"];

    // Exactly one broken ([[DoesNotExist]]).
    assert_eq!(
        r["broken"].as_u64().unwrap(),
        1,
        "expected exactly one broken link: {json}"
    );

    // Exactly one ambiguous ([[Twin]]).
    assert_eq!(
        r["ambiguous"].as_u64().unwrap(),
        1,
        "expected exactly one ambiguous short-form link: {json}"
    );

    // Exactly one case-mismatch ([[notes/alpha]]); the correctly-cased and the
    // valid short-form links contribute nothing.
    assert_eq!(
        r["case_mismatches"].as_u64().unwrap(),
        1,
        "expected exactly one case-mismatch: {json}"
    );
    assert_eq!(
        r["case_mismatch_fixes"][0]["old_target"].as_str().unwrap(),
        "notes/alpha",
        "case-mismatch must preserve the written casing: {json}"
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

    // Fuzzy candidates are reported in `fuzzy_fixes`, not `fixes` (L-10).
    let fixes = json["results"]["fuzzy_fixes"]
        .as_array()
        .expect("'fuzzy_fixes' should be an array");

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

    // Apply fixes. The only fixable link in this fixture is the fuzzy match
    // [[Authnticaton]] → authentication.md, so we must opt into fuzzy fixes
    // with --apply-fuzzy (plain --apply excludes fuzzy fixes by default, L-10).
    let apply_output = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path()
                .to_str()
                .expect("temp path should be valid UTF-8"),
            "links",
            "fix",
            "--apply",
            "--apply-fuzzy",
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

    // The only fix in this fixture is the fuzzy match, opted into via
    // --apply-fuzzy; it shows up in `applied_fixes`, not `fixable` (which
    // covers only certain, non-fuzzy fixes — L-10).
    let applied_fixes = apply_json["results"]["applied_fixes"]
        .as_array()
        .expect("'applied_fixes' should be an array");
    assert!(
        !applied_fixes.is_empty(),
        "should have applied at least 1 fix"
    );

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
// links fix: fuzzy confidence tiers (L-10)
// ---------------------------------------------------------------------------

/// Read the on-disk `d.md` and report whether its broken `[[Authnticaton]]`
/// link was rewritten to the authentication target.
fn d_md_was_fuzzy_fixed(tmp: &TempDir) -> bool {
    let body = fs::read_to_string(tmp.path().join("d.md")).expect("d.md should exist");
    !body.contains("Authnticaton") && body.to_lowercase().contains("authentication")
}

#[test]
fn links_fix_apply_excludes_fuzzy_by_default() {
    let tmp = setup_vault();
    // Plain --apply must NOT write the low-confidence fuzzy fix.
    let output = hyalo_no_hints()
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
        .expect("links fix --apply should run");
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    // The fuzzy candidate is reported in its own bucket, not applied.
    assert!(
        json["results"]["fuzzy"].as_u64().unwrap_or(0) >= 1,
        "fuzzy bucket should contain the [[Authnticaton]] candidate: {json}"
    );
    // `fixable` must NOT count the fuzzy candidate: it drives the "Apply N
    // fixes" hint text for plain `--apply`, which does not write fuzzy
    // fixes. Counting it here would make the hint promise a fix that a
    // plain `--apply` run does not actually deliver.
    assert_eq!(
        json["results"]["fixable"].as_u64(),
        Some(0),
        "fixable must exclude the fuzzy-only candidate so the apply hint doesn't overpromise: {json}"
    );
    assert_eq!(
        json["results"]["fuzzy_applied"].as_bool(),
        Some(false),
        "fuzzy_applied should be false without --apply-fuzzy"
    );
    // applied_fixes must not contain the fuzzy fix.
    let applied = json["results"]["applied_fixes"].as_array().unwrap();
    assert!(
        applied.is_empty(),
        "no fixes should be applied (only fuzzy was available): {applied:?}"
    );
    assert!(
        !d_md_was_fuzzy_fixed(&tmp),
        "d.md must be untouched when fuzzy is excluded"
    );
}

#[test]
fn links_fix_apply_fuzzy_opts_in() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "links",
            "fix",
            "--apply",
            "--apply-fuzzy",
            "--format",
            "json",
        ])
        .output()
        .expect("links fix --apply --apply-fuzzy should run");
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["fuzzy_applied"].as_bool(), Some(true));
    assert!(
        d_md_was_fuzzy_fixed(&tmp),
        "d.md should be fixed once fuzzy is opted in"
    );
}

#[test]
fn links_fix_min_confidence_gates_fuzzy() {
    let tmp = setup_vault();
    // A confidence bar of 1.0 excludes the imperfect fuzzy match, so nothing
    // is applied even though --min-confidence implies --apply-fuzzy.
    let output = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "links",
            "fix",
            "--apply",
            "--min-confidence",
            "1.0",
            "--format",
            "json",
        ])
        .output()
        .expect("links fix --min-confidence 1.0 should run");
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json["results"]["fuzzy_applied"].as_bool(),
        Some(true),
        "min-confidence implies fuzzy is enabled"
    );
    assert!(
        !d_md_was_fuzzy_fixed(&tmp),
        "a <1.0 fuzzy match must be gated out by --min-confidence 1.0"
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

    // Target file — all lowercase, inside a subdirectory so the link below
    // must use a path-form (with `/`) and cannot resolve via the Obsidian
    // short-form stem fallback. That fallback is intentionally NOT gated on
    // `case_insensitive` (it's an Obsidian convention).
    write_md(
        tmp.path(),
        "docs/iteration_protocols.md",
        md!(r"
---
title: Iteration protocols
---
Content.
"),
    );

    // Source — path-form wikilink with different casing from on-disk path.
    write_md(
        tmp.path(),
        "promise_any.md",
        md!(r"
---
title: Promise.any
---
See [[Docs/Iteration_Protocols]] for details.
"),
    );

    // case_insensitive = "false" — strict mode, no path fallback
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
fn links_auto_first_only_respects_existing_link_in_same_sentence() {
    // Regression test: a file that already contains [[fake-login]] plus a
    // plain-text mention of the same title later in the same sentence must
    // NOT gain a second link with --first-only — the existing wikilink IS
    // the first mention. Previously this produced
    // "the [[fake-login]] envVars block from [[fake-login]]".
    let tmp = TempDir::new().expect("tempdir creation should succeed");

    write_md(
        tmp.path(),
        "fake-login.md",
        md!(r"
---
title: Fake Login
---
Fake login fixture page.
"),
    );
    write_md(
        tmp.path(),
        "notes.md",
        md!(r"
---
title: Notes
---
Read the [[fake-login]] envVars block from fake-login before continuing.
"),
    );

    let results = run_links_auto(tmp.path(), &["--first-only", "--format", "json"]);
    let matches = results["matches"]
        .as_array()
        .expect("results.matches should be an array");
    let fake_login_matches: Vec<_> = matches
        .iter()
        .filter(|m| {
            m["file"].as_str() == Some("notes.md")
                && m["link_target"].as_str() == Some("fake-login")
        })
        .collect();
    assert!(
        fake_login_matches.is_empty(),
        "existing [[fake-login]] link should suppress the plain-text mention, got: {fake_login_matches:?}"
    );

    // --apply should leave the file with exactly one link and the plain
    // mention untouched (not converted, not duplicated).
    let apply_results =
        run_links_auto(tmp.path(), &["--first-only", "--apply", "--format", "json"]);
    assert_eq!(apply_results["applied"].as_bool(), Some(true));
    let content = fs::read_to_string(tmp.path().join("notes.md")).unwrap();
    let link_count = content.matches("[[fake-login").count();
    assert_eq!(
        link_count, 1,
        "should still have exactly one [[fake-login]] link after apply, content: {content}"
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

// ---------------------------------------------------------------------------
// Finding 1 regression: bare-basename intra-folder links must not be rewritten
// ---------------------------------------------------------------------------

/// After `hyalo mv`, `links fix` must NOT rewrite a bare-basename markdown link
/// whose target already exists in the source file's own directory.
///
/// Scenario: `a/foo.md` contains `[bar](bar.md)`. `a/bar.md` exists. The link
/// resolves correctly via source-relative lookup, so `links fix` must leave it
/// untouched (no case-mismatch rewrite, no broken-link entry).
#[test]
fn links_fix_does_not_rewrite_intra_folder_bare_basename() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let dir = tmp.path();

    // Create a/foo.md linking to bar.md (same folder) and a/bar.md
    fs::create_dir_all(dir.join("a")).unwrap();
    write_md(
        dir,
        "a/foo.md",
        md!(r"
---
title: Foo
---
See [bar](bar.md) for details.
"),
    );
    write_md(
        dir,
        "a/bar.md",
        md!(r"
---
title: Bar
---
# Bar
"),
    );

    // Run links fix (dry-run) — should report 0 changes.
    let output = hyalo_no_hints()
        .args([
            "--dir",
            dir.to_str().unwrap(),
            "links",
            "fix",
            "--format",
            "json",
        ])
        .output()
        .expect("links fix should run");

    assert!(
        output.status.success(),
        "links fix exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    // broken count must be 0
    let broken = json["results"]["broken"].as_u64().unwrap_or(0);
    assert_eq!(broken, 0, "bare-basename link should not be broken");

    // case_mismatches must be 0
    let case_mismatches = json["results"]["case_mismatches"].as_u64().unwrap_or(0);
    assert_eq!(
        case_mismatches, 0,
        "bare-basename link should not be flagged as case-mismatch"
    );

    // The source file must be unchanged.
    let content = fs::read_to_string(dir.join("a/foo.md")).unwrap();
    assert!(
        content.contains("[bar](bar.md)"),
        "bare-basename link must not be rewritten; content: {content}"
    );
}

// ---------------------------------------------------------------------------
// UX-B: `hyalo links` (no subcommand) runs `links fix --dry-run`
// ---------------------------------------------------------------------------

/// `hyalo links` with no subcommand should behave identically to `hyalo links fix --dry-run`.
#[test]
fn links_no_subcommand_same_as_fix_dry_run() {
    let tmp = setup_vault();

    let out_default = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["links", "--format", "json"])
        .output()
        .unwrap();

    let out_explicit = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["links", "fix", "--dry-run", "--format", "json"])
        .output()
        .unwrap();

    // Both should succeed
    assert!(
        out_default.status.success(),
        "hyalo links should succeed, stderr: {}",
        String::from_utf8_lossy(&out_default.stderr)
    );
    assert!(out_explicit.status.success());

    let json_default: serde_json::Value =
        serde_json::from_slice(&out_default.stdout).expect("valid JSON");
    let json_explicit: serde_json::Value =
        serde_json::from_slice(&out_explicit.stdout).expect("valid JSON");

    // The "applied" flag must be false (dry-run mode)
    assert_eq!(
        json_default["results"]["applied"], false,
        "default should be dry-run (applied=false)"
    );
    assert_eq!(json_explicit["results"]["applied"], false);

    // broken count should match
    assert_eq!(
        json_default["results"]["broken"], json_explicit["results"]["broken"],
        "broken count should be the same"
    );
}

// ---------------------------------------------------------------------------
// Iteration 134: short-form wikilink handling (Obsidian compatibility)
// ---------------------------------------------------------------------------

/// A bare `[[Corina]]` that resolves (stem-match) to `sub/Corina.md` must NOT
/// be reported as broken, case-mismatch, or ambiguous, and must not be rewritten.
#[test]
fn short_form_wikilink_valid_stem_not_flagged() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let dir = tmp.path();

    fs::create_dir_all(dir.join("sub")).unwrap();
    write_md(
        dir,
        "sub/Corina.md",
        md!(r"
---
title: Corina
---
# Corina
"),
    );
    write_md(
        dir,
        "index.md",
        md!(r"
---
title: Index
---
See [[Corina]] for details.
"),
    );

    let out = hyalo_no_hints()
        .args([
            "--dir",
            dir.to_str().unwrap(),
            "links",
            "fix",
            "--format",
            "json",
        ])
        .output()
        .expect("links fix should run");
    assert!(
        out.status.success(),
        "links fix exited non-zero: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert_eq!(
        json["results"]["broken"].as_u64().unwrap_or(1),
        0,
        "[[Corina]] resolving to sub/Corina.md must not be broken: {json}"
    );
    assert_eq!(
        json["results"]["case_mismatches"].as_u64().unwrap_or(1),
        0,
        "[[Corina]] resolving to sub/Corina.md must not be a case-mismatch: {json}"
    );
    assert_eq!(
        json["results"]["ambiguous"].as_u64().unwrap_or(1),
        0,
        "[[Corina]] with one stem match must not be ambiguous: {json}"
    );

    // --apply must not rewrite the file
    let apply_out = hyalo_no_hints()
        .args([
            "--dir",
            dir.to_str().unwrap(),
            "links",
            "fix",
            "--apply",
            "--format",
            "json",
        ])
        .output()
        .expect("links fix --apply should run");
    assert!(apply_out.status.success());

    let content = fs::read_to_string(dir.join("index.md")).unwrap();
    assert!(
        content.contains("[[Corina]]"),
        "--apply must not rewrite valid short-form link; content: {content}"
    );
    assert!(
        !content.contains("[[sub/Corina]]"),
        "--apply must not expand short-form to full path; content: {content}"
    );
}

/// A stem-case mismatch (`[[corina]]` for `Corina.md`) is rewritten to
/// `[[Corina]]` — the short form is preserved, never expanded to a full path.
#[test]
fn short_form_stem_case_mismatch_rewrites_stem_only() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let dir = tmp.path();

    fs::create_dir_all(dir.join("sub")).unwrap();
    write_md(
        dir,
        "sub/Corina.md",
        md!(r"
---
title: Corina
---
# Corina
"),
    );
    write_md(
        dir,
        "index.md",
        md!(r"
---
title: Index
---
See [[corina]] for details.
"),
    );

    // Dry-run: should report 1 case-mismatch (stem casing differs)
    let dry_out = hyalo_no_hints()
        .args([
            "--dir",
            dir.to_str().unwrap(),
            "links",
            "fix",
            "--format",
            "json",
        ])
        .output()
        .expect("links fix should run");
    assert!(dry_out.status.success());

    let dry_json: serde_json::Value = serde_json::from_slice(&dry_out.stdout).expect("valid JSON");
    assert_eq!(
        dry_json["results"]["broken"].as_u64().unwrap_or(1),
        0,
        "stem-case-mismatch must not be reported as broken: {dry_json}"
    );
    assert_eq!(
        dry_json["results"]["case_mismatches"].as_u64().unwrap_or(0),
        1,
        "stem-case-mismatch must appear in case_mismatches: {dry_json}"
    );

    // --apply: must rewrite [[corina]] to [[Corina]], not to [[sub/Corina]]
    let apply_out = hyalo_no_hints()
        .args([
            "--dir",
            dir.to_str().unwrap(),
            "links",
            "fix",
            "--apply",
            "--format",
            "json",
        ])
        .output()
        .expect("links fix --apply should run");
    assert!(apply_out.status.success());

    let content = fs::read_to_string(dir.join("index.md")).unwrap();
    assert!(
        content.contains("[[Corina]]"),
        "--apply must rewrite [[corina]] to [[Corina]]; content: {content}"
    );
    assert!(
        !content.contains("[[sub/Corina]]"),
        "--apply must never expand short-form to full path; content: {content}"
    );
    assert!(
        !content.contains("[[corina]]"),
        "--apply must have fixed the casing; content: {content}"
    );
}

/// Two files sharing a stem produce an `ambiguous` report; `--apply` leaves
/// both links untouched.
#[test]
fn short_form_ambiguous_not_auto_fixed() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let dir = tmp.path();

    fs::create_dir_all(dir.join("a")).unwrap();
    fs::create_dir_all(dir.join("b")).unwrap();
    write_md(
        dir,
        "a/Corina.md",
        md!(r"---
title: Corina A
---
"),
    );
    write_md(
        dir,
        "b/Corina.md",
        md!(r"---
title: Corina B
---
"),
    );
    write_md(
        dir,
        "index.md",
        md!(r"
---
title: Index
---
See [[Corina]] here.
"),
    );

    let dry_out = hyalo_no_hints()
        .args([
            "--dir",
            dir.to_str().unwrap(),
            "links",
            "fix",
            "--format",
            "json",
        ])
        .output()
        .expect("links fix should run");
    assert!(dry_out.status.success());

    let json: serde_json::Value = serde_json::from_slice(&dry_out.stdout).expect("valid JSON");
    assert_eq!(
        json["results"]["broken"].as_u64().unwrap_or(1),
        0,
        "ambiguous link must not be reported as broken: {json}"
    );
    assert_eq!(
        json["results"]["ambiguous"].as_u64().unwrap_or(0),
        1,
        "expected 1 ambiguous link: {json}"
    );

    // --apply must not rewrite the ambiguous link
    let apply_out = hyalo_no_hints()
        .args([
            "--dir",
            dir.to_str().unwrap(),
            "links",
            "fix",
            "--apply",
            "--format",
            "json",
        ])
        .output()
        .expect("links fix --apply should run");
    assert!(apply_out.status.success());

    let content = fs::read_to_string(dir.join("index.md")).unwrap();
    assert!(
        content.contains("[[Corina]]"),
        "--apply must leave ambiguous link untouched; content: {content}"
    );
}

/// Path-form case mismatches (target has `/`) are still detected and rewritten
/// even in Obsidian-compatible mode.
#[test]
fn path_form_case_mismatch_still_rewritten() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let dir = tmp.path();

    fs::create_dir_all(dir.join("sub")).unwrap();
    write_md(
        dir,
        "sub/corina.md",
        md!(r"---
title: corina
---
"),
    );
    write_md(
        dir,
        "index.md",
        md!(r"
---
title: Index
---
See [[sub/Corina]] for details.
"),
    );

    // Enable case_insensitive mode so path-form mismatches are detected.
    fs::write(
        dir.join(".hyalo.toml"),
        "[links]\ncase_insensitive = \"true\"\n",
    )
    .unwrap();

    let dry_out = hyalo_no_hints()
        .args([
            "--dir",
            dir.to_str().unwrap(),
            "links",
            "fix",
            "--format",
            "json",
        ])
        .output()
        .expect("links fix should run");
    assert!(dry_out.status.success());

    let json: serde_json::Value = serde_json::from_slice(&dry_out.stdout).expect("valid JSON");
    assert_eq!(
        json["results"]["broken"].as_u64().unwrap_or(1),
        0,
        "path-form link should not be broken: {json}"
    );
    assert_eq!(
        json["results"]["case_mismatches"].as_u64().unwrap_or(0),
        1,
        "path-form case mismatch should be detected: {json}"
    );

    // --apply must rewrite the path-form casing to match the on-disk file.
    let apply_out = hyalo_no_hints()
        .args([
            "--dir",
            dir.to_str().unwrap(),
            "links",
            "fix",
            "--apply",
            "--format",
            "json",
        ])
        .output()
        .expect("links fix --apply should run");
    assert!(apply_out.status.success());

    let content = fs::read_to_string(dir.join("index.md")).unwrap();
    assert!(
        content.contains("[[sub/corina]]"),
        "--apply must rewrite [[sub/Corina]] to [[sub/corina]]; content: {content}"
    );
    assert!(
        !content.contains("[[sub/Corina]]"),
        "--apply must have fixed the casing; content: {content}"
    );
}

/// `--expand-short-form` opts into path expansion of short-form wikilinks.
/// With it, [[Corina]] → sub/Corina.md is treated as broken/fixable and
/// --apply rewrites it to [[sub/Corina]].
#[test]
fn expand_short_form_flag_opts_into_path_expansion() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let dir = tmp.path();

    fs::create_dir_all(dir.join("sub")).unwrap();
    write_md(
        dir,
        "sub/Corina.md",
        md!(r"
---
title: Corina
---
# Corina
"),
    );
    write_md(
        dir,
        "index.md",
        md!(r"
---
title: Index
---
See [[Corina]] for details.
"),
    );

    // Without --expand-short-form: 0 broken
    let no_expand = hyalo_no_hints()
        .args([
            "--dir",
            dir.to_str().unwrap(),
            "links",
            "fix",
            "--format",
            "json",
        ])
        .output()
        .expect("links fix should run");
    assert!(no_expand.status.success());
    let no_expand_json: serde_json::Value =
        serde_json::from_slice(&no_expand.stdout).expect("valid JSON");
    assert_eq!(
        no_expand_json["results"]["broken"].as_u64().unwrap_or(1),
        0,
        "without --expand-short-form, [[Corina]] must be valid: {no_expand_json}"
    );

    // With --expand-short-form: [[Corina]] is treated as broken (stem not found at vault root)
    // and fixable via strategy 3 (shortest-path) → links fix finds it
    let with_expand = hyalo_no_hints()
        .args([
            "--dir",
            dir.to_str().unwrap(),
            "links",
            "fix",
            "--expand-short-form",
            "--format",
            "json",
        ])
        .output()
        .expect("links fix --expand-short-form should run");
    assert!(with_expand.status.success());
    let with_expand_json: serde_json::Value =
        serde_json::from_slice(&with_expand.stdout).expect("valid JSON");
    // With expansion enabled, [[Corina]] is not found at vault root → broken or fixable
    let broken = with_expand_json["results"]["broken"].as_u64().unwrap_or(0);
    let fixable = with_expand_json["results"]["fixable"].as_u64().unwrap_or(0);
    assert!(
        broken + fixable >= 1,
        "--expand-short-form must expose [[Corina]] as broken or fixable: {with_expand_json}"
    );

    // --apply with --expand-short-form must rewrite [[Corina]] to [[sub/Corina]].
    let apply_out = hyalo_no_hints()
        .args([
            "--dir",
            dir.to_str().unwrap(),
            "links",
            "fix",
            "--expand-short-form",
            "--apply",
            "--format",
            "json",
        ])
        .output()
        .expect("links fix --apply --expand-short-form should run");
    assert!(apply_out.status.success());

    let content = fs::read_to_string(dir.join("index.md")).unwrap();
    assert!(
        content.contains("[[sub/Corina]]"),
        "--apply --expand-short-form must rewrite [[Corina]] to [[sub/Corina]]; content: {content}"
    );
}

// ---------------------------------------------------------------------------
// links fix: frontmatter wikilinks (H-bug — frontmatter fixes were reported
// as applied but never written to disk; see iteration-160)
// ---------------------------------------------------------------------------

#[test]
fn links_fix_apply_rewrites_frontmatter_and_body_wikilinks() {
    // Minimal repro from the bug report: a broken target referenced both in
    // a frontmatter `related:` list and in the body of the same file.
    let tmp = TempDir::new().expect("tempdir creation should succeed");

    write_md(
        tmp.path(),
        "sub/real-target.md",
        md!(r"
---
title: Real Target
---
Body.
"),
    );

    write_md(
        tmp.path(),
        "a.md",
        md!(r#"
---
title: A
related: ["[[wrong/real-target]]"]
---
Body also links [[wrong/real-target]].
"#),
    );

    let apply_output = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path()
                .to_str()
                .expect("temp path should be valid UTF-8"),
            "links",
            "fix",
            "--apply",
            // DEC-076 (iter-211): `wrong/real-target` writes a directory, so
            // repairing it by basename is a guess and needs the fuzzy opt-in.
            // iter-212 adds a confidence floor on top, and `wrong/` shares
            // nothing with `sub/`, so the guess scores exactly 0.7. This test
            // is about frontmatter+body rewriting, not gating — open the floor.
            "--apply-fuzzy",
            "--min-confidence",
            "0",
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

    assert_eq!(
        apply_json["results"]["broken"].as_u64(),
        Some(2),
        "expected 2 broken links (frontmatter + body): {apply_json}"
    );
    assert_eq!(
        apply_json["results"]["fuzzy"].as_u64(),
        Some(2),
        "expected both occurrences to be fixable: {apply_json}"
    );
    assert_eq!(
        apply_json["results"]["unapplied"].as_u64(),
        Some(0),
        "no fix should be reported unapplied: {apply_json}"
    );
    let applied_fixes = apply_json["results"]["applied_fixes"]
        .as_array()
        .expect("'applied_fixes' should be an array");
    assert_eq!(
        applied_fixes.len(),
        2,
        "both the frontmatter and body fix must be reported as applied: {apply_json}"
    );

    // The actual assertion that matters: both occurrences must be rewritten
    // on disk, not just reported as applied.
    let written = fs::read_to_string(tmp.path().join("a.md")).expect("a.md should be readable");
    assert!(
        !written.contains("wrong/real-target"),
        "broken target must not remain anywhere in the file, got:\n{written}"
    );
    assert_eq!(
        written.matches("[[sub/real-target]]").count(),
        2,
        "both frontmatter and body wikilinks must be rewritten, got:\n{written}"
    );

    // Re-running must report the fix-loop has converged: 0 broken, 0 fixable.
    // Before the fix, the frontmatter occurrence was reported as applied but
    // never written, so a re-run kept reporting it as fixable forever.
    let rerun_output = hyalo_no_hints()
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
    assert!(rerun_output.status.success());

    let rerun_json: serde_json::Value =
        serde_json::from_slice(&rerun_output.stdout).expect("rerun stdout should be valid JSON");
    assert_eq!(
        rerun_json["results"]["broken"].as_u64(),
        Some(0),
        "fix-loop must converge — no broken links should remain: {rerun_json}"
    );
    assert_eq!(
        rerun_json["results"]["fixable"].as_u64(),
        Some(0),
        "fix-loop must converge — no fixable links should remain: {rerun_json}"
    );
}

#[test]
fn links_fix_dry_run_does_not_write_frontmatter() {
    // Dry-run must remain plan-only: no file should be touched, and the
    // 'applied' flag must be false with no unapplied/applied_fixes noise.
    let tmp = TempDir::new().expect("tempdir creation should succeed");

    write_md(
        tmp.path(),
        "sub/real-target.md",
        md!(r"
---
title: Real Target
---
Body.
"),
    );

    write_md(
        tmp.path(),
        "a.md",
        md!(r#"
---
title: A
related: ["[[wrong/real-target]]"]
---
Body also links [[wrong/real-target]].
"#),
    );

    let before = fs::read_to_string(tmp.path().join("a.md")).expect("a.md should be readable");

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
        .expect("hyalo links fix (dry-run) should run");
    assert!(output.status.success());

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(json["results"]["applied"].as_bool(), Some(false));
    assert_eq!(
        json["results"]["unapplied"].as_u64(),
        Some(0),
        "dry-run has attempted nothing, so nothing can be unapplied: {json}"
    );
    assert_eq!(
        json["results"]["applied_fixes"].as_array().map(Vec::len),
        Some(0),
        "dry-run must not report any fix as applied: {json}"
    );

    let after = fs::read_to_string(tmp.path().join("a.md")).expect("a.md should be readable");
    assert_eq!(before, after, "dry-run must not modify the file");
}

// ---------------------------------------------------------------------------
// iter-183 Phase B: cross-line suppression regressions (L-3, L-15)
//
// These exercise the shared `LineScanner` end-to-end through `find
// --broken-links`: a broken `[[link]]` hidden inside a MULTI-LINE inline code
// span or a MULTI-LINE HTML comment must NOT be reported, because the scanner
// now carries the open code-span / comment across lines. Before Phase B each
// body-scan loop stripped only per-line, so the interior link leaked out.
// ---------------------------------------------------------------------------

/// Run `find --broken-links --format json` against `dir` and return the set of
/// files reported as having broken links.
fn broken_link_files(dir: &std::path::Path) -> Vec<String> {
    let output = hyalo_no_hints()
        .args([
            "--dir",
            dir.to_str().expect("temp path should be valid UTF-8"),
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
    json["results"]
        .as_array()
        .map(|rs| {
            rs.iter()
                .filter_map(|r| r["file"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn find_broken_links_ignores_multiline_code_span() {
    // L-3: a `[[gone]]` sitting inside a code span opened two lines earlier
    // (```` ``code ... code`` ````) must be treated as literal, not a link.
    let tmp = TempDir::new().expect("tempdir creation should succeed");
    write_md(
        tmp.path(),
        "span.md",
        md!(r"
---
title: Span
---
Intro ``open code
this [[gone]] is inside the span
still code`` and here is [[alsomissing]] outside.
"),
    );
    let files = broken_link_files(tmp.path());
    // The file DOES have a genuinely broken link outside the span, so it still
    // appears — but the assertion that matters is the interior link is not the
    // reason. Verify by removing the outside link too.
    assert!(
        files.contains(&"span.md".to_owned()),
        "span.md has a real broken link outside the span: {files:?}"
    );

    // Now a file whose ONLY `[[...]]` is inside the multi-line span must have
    // NO broken links reported at all.
    let tmp2 = TempDir::new().expect("tempdir creation should succeed");
    write_md(
        tmp2.path(),
        "only.md",
        md!(r"
---
title: Only
---
Intro ``open code
this [[gone]] is inside the span
still code`` done.
"),
    );
    let files2 = broken_link_files(tmp2.path());
    assert!(
        !files2.contains(&"only.md".to_owned()),
        "only.md's sole link is inside a multi-line code span and must not be reported broken: {files2:?}"
    );
}

#[test]
fn find_broken_links_ignores_multiline_html_comment() {
    // L-15: a `[[gone]]` inside a multi-line HTML comment must be suppressed.
    let tmp = TempDir::new().expect("tempdir creation should succeed");
    write_md(
        tmp.path(),
        "html.md",
        md!(r"
---
title: Html
---
Before <!-- start comment
this [[gone]] is commented out
end comment --> done.
"),
    );
    let files = broken_link_files(tmp.path());
    assert!(
        !files.contains(&"html.md".to_owned()),
        "the only link is inside a multi-line HTML comment and must not be reported broken: {files:?}"
    );
}

#[test]
fn find_broken_links_still_reports_after_multiline_span_closes() {
    // Guard against over-suppression: a real broken link AFTER a multi-line
    // code span closes must still be reported.
    let tmp = TempDir::new().expect("tempdir creation should succeed");
    write_md(
        tmp.path(),
        "after.md",
        md!(r"
---
title: After
---
Intro ``open
inside [[ignored]]
close`` then a real broken [[reallymissing]].
"),
    );
    let files = broken_link_files(tmp.path());
    assert!(
        files.contains(&"after.md".to_owned()),
        "the broken link after the span closes must still be reported: {files:?}"
    );
}

#[test]
fn find_broken_links_ignores_backslash_escaped_link() {
    // L-16: a backslash-escaped link (`\[[not-a-link]]`) is literal text per
    // CommonMark / Obsidian and must NOT be extracted — so it can never be
    // reported as a broken link, even though the target does not exist.
    let tmp = TempDir::new().expect("tempdir creation should succeed");
    write_md(
        tmp.path(),
        "escaped.md",
        md!(r"
---
title: Escaped
---
This \[[escaped-missing]] is literal, but this [[real-missing]] is broken.
Also \[label](escaped-md-missing.md) is literal too.
"),
    );
    let files = broken_link_files(tmp.path());
    // The file appears because of the unescaped `[[real-missing]]`.
    assert!(
        files.contains(&"escaped.md".to_owned()),
        "the unescaped broken link must be reported: {files:?}"
    );

    // Inspect the per-file broken-link targets to confirm the escaped ones are
    // absent and only the real one is present.
    let output = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "find",
            "--broken-links",
            "--format",
            "json",
        ])
        .output()
        .expect("hyalo find --broken-links should run");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let blob = json.to_string();
    assert!(
        blob.contains("real-missing"),
        "unescaped broken target should be listed: {blob}"
    );
    assert!(
        !blob.contains("escaped-missing"),
        "escaped wikilink target must not appear anywhere: {blob}"
    );
    assert!(
        !blob.contains("escaped-md-missing"),
        "escaped markdown-link target must not appear anywhere: {blob}"
    );
}

// ---------------------------------------------------------------------------
// L-11: honest partial-failure envelopes (iter-187)
// ---------------------------------------------------------------------------

/// Build a vault with one fixable link in a subdirectory we can make
/// read-only to induce a mid-batch write failure.
///
/// The markdown link `[bar](wrong/place/bar.md)` in `docs/src.md` points at a
/// non-existent path (broken), but the stem `bar` uniquely matches
/// `sub/bar.md` → a `BasenameFallback` fix. Using a path-form markdown link
/// avoids Obsidian short-form resolution, which would otherwise make a bare
/// `[[bar]]` resolve as valid — and under DEC-076 (iter-211) that written
/// directory is exactly what puts the repair behind `--apply-fuzzy`, which the
/// failure test therefore passes.
#[cfg(unix)]
fn setup_readonly_fix_vault() -> TempDir {
    let tmp = TempDir::new().expect("tempdir creation should succeed");
    write_md(
        tmp.path(),
        "docs/src.md",
        md!(r"
---
title: Src
---
See [bar](wrong/place/bar.md) here.
"),
    );
    write_md(
        tmp.path(),
        "sub/bar.md",
        md!(r"
---
title: Bar
---
Body.
"),
    );
    tmp
}

#[cfg(unix)]
#[test]
fn links_fix_apply_partial_failure_reports_failed_and_exits_nonzero() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = setup_readonly_fix_vault();
    // Make only `docs/` (src.md's parent) read-only so the atomic write cannot
    // create its temp file there — the write fails but the vault root stays
    // writable so index/snapshot operations still work and the envelope is
    // still emitted.
    let docs = tmp.path().join("docs");
    let mut perms = fs::metadata(&docs).unwrap().permissions();
    perms.set_mode(0o555);
    fs::set_permissions(&docs, perms).unwrap();

    let output = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "links",
            "fix",
            "--apply",
            "--apply-fuzzy",
            // iter-212: `--apply-fuzzy` now gates on a confidence floor. This
            // test is about the write-failure path, not about scoring, so the
            // floor is opened right up.
            "--min-confidence",
            "0",
            "--format",
            "json",
        ])
        .output()
        .expect("hyalo links fix --apply should run");

    // Restore perms for cleanup.
    let mut restore = fs::metadata(&docs).unwrap().permissions();
    restore.set_mode(0o755);
    let _ = fs::set_permissions(&docs, restore);

    // Partial failure ⇒ non-zero exit code.
    assert!(
        !output.status.success(),
        "read-only write failure must exit non-zero; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("stdout should be valid JSON even on failure");
    let results = &json["results"];
    let failed = results["failed"]
        .as_u64()
        .expect("'failed' should be a number");
    assert!(failed >= 1, "expected at least one failed fix: {results}");
    let failed_fixes = results["failed_fixes"]
        .as_array()
        .expect("'failed_fixes' should be an array");
    assert!(!failed_fixes.is_empty(), "failed_fixes must list the file");
    assert_eq!(
        failed_fixes[0]["source"].as_str(),
        Some("docs/src.md"),
        "failed fix should name the read-only source"
    );
    assert!(
        failed_fixes[0]["error"].as_str().is_some(),
        "failed fix must carry an error string"
    );
    // The file whose write failed must NOT appear as applied.
    let applied = results["applied_fixes"].as_array().unwrap();
    assert!(
        applied
            .iter()
            .all(|f| f["source"].as_str() != Some("docs/src.md")),
        "a failed fix must not be reported as applied"
    );
}

#[cfg(unix)]
#[test]
fn links_auto_apply_read_only_reports_failed_in_envelope() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = TempDir::new().expect("tempdir creation should succeed");
    write_md(
        tmp.path(),
        "sprint-review.md",
        md!(r"
---
title: Sprint Review
---
Body.
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

    // Make the vault dir read-only so the write to meetings.md fails.
    let mut perms = fs::metadata(tmp.path()).unwrap().permissions();
    perms.set_mode(0o555);
    fs::set_permissions(tmp.path(), perms).unwrap();

    let output = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "links",
            "auto",
            "--apply",
            "--format",
            "json",
        ])
        .output()
        .expect("hyalo links auto --apply should run");

    let mut restore = fs::metadata(tmp.path()).unwrap().permissions();
    restore.set_mode(0o755);
    let _ = fs::set_permissions(tmp.path(), restore);

    assert!(
        !output.status.success(),
        "read-only auto write failure must exit non-zero"
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("stdout should be valid JSON even on failure");
    let results = &json["results"];
    assert!(
        results["files_failed"].as_u64().unwrap_or(0) >= 1,
        "expected files_failed >= 1: {results}"
    );
    let outcomes = results["apply_outcomes"].as_array().unwrap();
    assert!(
        outcomes
            .iter()
            .any(|o| o["status"].as_str() == Some("failed")
                && o["file"].as_str() == Some("meetings.md")),
        "meetings.md must appear as a failed apply outcome: {outcomes:?}"
    );
}

#[test]
fn links_fix_dry_run_reports_stale_fix_as_unapplied() {
    // L-25: with an on-disk edit after detection would-be-see the link, dry-run
    // must report the stale fix under `unapplied_fixes` (parity with --apply),
    // and must not mutate the file.
    let tmp = TempDir::new().expect("tempdir creation should succeed");
    write_md(
        tmp.path(),
        "src.md",
        md!(r"
---
title: Src
---
Nothing links here anymore.
"),
    );

    // A dry-run over a fresh vault with no broken links simply reports zero
    // fixable — the important guarantee (proven in the unit test parity test)
    // is that dry-run runs the same plan-building phase. Here we assert the
    // envelope now always carries the `unapplied`/`failed` fields so downstream
    // tooling can rely on them.
    let output = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "links",
            "fix",
            "--format",
            "json",
        ])
        .output()
        .expect("hyalo links fix dry-run should run");
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let results = &json["results"];
    assert!(
        results.get("unapplied").is_some(),
        "envelope must carry 'unapplied'"
    );
    assert!(
        results.get("failed").is_some(),
        "envelope must carry 'failed'"
    );
}

// ---------------------------------------------------------------------------
// `[links.auto]` config exclusions (iter-195a)
// ---------------------------------------------------------------------------

/// Vault used by the `[links.auto]` tests.
///
/// `guide.md` mentions "Permissions" twice, "Daily" twice, and "Widget" once,
/// so one fixture covers exclusion, first-only, and target-glob behaviour.
fn setup_auto_config_vault(config: &str) -> TempDir {
    let tmp = TempDir::new().expect("tempdir creation should succeed");
    fs::write(tmp.path().join(".hyalo.toml"), config).expect("config should be writable");

    write_md(
        tmp.path(),
        "permissions.md",
        md!(r"
---
title: Permissions
---
How the permission model works.
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
        "templates/widget.md",
        md!(r"
---
title: Widget
---
Template for widgets.
"),
    );
    write_md(
        tmp.path(),
        "guide.md",
        md!(r"
---
title: Guide
---
Permissions are checked first.
Daily runs happen nightly, and Permissions apply there too.
A Widget is rendered, and Daily wraps up.
"),
    );
    tmp
}

/// Run `hyalo links auto` from *inside* the vault so the vault's own
/// `.hyalo.toml` is the config that gets loaded — config resolution is
/// CWD-based, `--dir` only moves the vault.
fn run_links_auto_in_vault(dir: &std::path::Path, extra_args: &[&str]) -> serde_json::Value {
    let output = hyalo_no_hints()
        .current_dir(dir)
        .args(["links", "auto", "--format", "json"])
        .args(extra_args)
        .output()
        .expect("hyalo links auto should run");
    assert!(
        output.status.success(),
        "links auto exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    json["results"].clone()
}

/// The distinct `link_target`s proposed in a `links auto` result.
fn proposed_targets(results: &serde_json::Value) -> Vec<String> {
    let mut targets: Vec<String> = results["matches"]
        .as_array()
        .expect("results.matches should be an array")
        .iter()
        .filter_map(|m| m["link_target"].as_str().map(str::to_owned))
        .collect();
    targets.sort();
    targets.dedup();
    targets
}

#[test]
fn links_auto_config_exclude_titles_applies_without_flags() {
    let tmp = setup_auto_config_vault("[links.auto]\nexclude_titles = [\"permissions\"]\n");

    let results = run_links_auto_in_vault(tmp.path(), &[]);

    assert_eq!(
        proposed_targets(&results),
        vec!["daily".to_owned(), "widget".to_owned()],
        "config exclude_titles should suppress permissions with no CLI flags: {results}"
    );
    assert_eq!(
        results["config_excluded"], 1,
        "one candidate title was removed by config: {results}"
    );
}

#[test]
fn links_auto_config_exclude_titles_is_case_insensitive() {
    // The config spelling need not match the page's casing, matching
    // `--exclude-title`'s own case-insensitive comparison.
    let tmp = setup_auto_config_vault("[links.auto]\nexclude_titles = [\"PERMISSIONS\"]\n");

    let results = run_links_auto_in_vault(tmp.path(), &[]);

    assert!(
        !proposed_targets(&results).contains(&"permissions".to_owned()),
        "differently-cased config entry should still exclude: {results}"
    );
}

#[test]
fn links_auto_cli_exclude_title_extends_config_list() {
    let tmp = setup_auto_config_vault("[links.auto]\nexclude_titles = [\"permissions\"]\n");

    let results = run_links_auto_in_vault(tmp.path(), &["--exclude-title", "Daily"]);

    assert_eq!(
        proposed_targets(&results),
        vec!["widget".to_owned()],
        "the flag must extend (not replace) the config list: {results}"
    );
    assert_eq!(
        results["config_excluded"], 1,
        "config_excluded counts only what the config took away: {results}"
    );
}

#[test]
fn links_auto_cli_exclude_target_glob_extends_config_list() {
    let tmp = setup_auto_config_vault("[links.auto]\nexclude_target_globs = [\"templates/*\"]\n");

    let results = run_links_auto_in_vault(tmp.path(), &["--exclude-target-glob", "daily.md"]);

    assert_eq!(
        proposed_targets(&results),
        vec!["permissions".to_owned()],
        "config glob and flag glob should both apply: {results}"
    );
    assert_eq!(
        results["config_excluded"], 1,
        "the glob-excluded template page contributed one candidate title: {results}"
    );
}

#[test]
fn links_auto_config_first_only_behaves_like_the_flag() {
    let tmp = setup_auto_config_vault("[links.auto]\nfirst_only = true\n");

    let results = run_links_auto_in_vault(tmp.path(), &[]);

    // guide.md mentions Permissions twice and Daily twice; first-only keeps one each.
    assert_eq!(
        results["total"], 3,
        "first_only from config should keep one mention per target: {results}"
    );
    assert!(
        results.get("config_excluded").is_none(),
        "first_only alone removes no candidate titles: {results}"
    );
}

#[test]
fn links_auto_flag_first_only_wins_over_config_false() {
    let tmp = setup_auto_config_vault("[links.auto]\nfirst_only = false\n");

    let without_flag = run_links_auto_in_vault(tmp.path(), &[]);
    assert_eq!(
        without_flag["total"], 5,
        "first_only = false should link every mention: {without_flag}"
    );

    let with_flag = run_links_auto_in_vault(tmp.path(), &["--first-only"]);
    assert_eq!(
        with_flag["total"], 3,
        "an explicit --first-only wins for this run: {with_flag}"
    );
}

#[test]
fn links_auto_config_and_flag_first_only_compose() {
    let tmp = setup_auto_config_vault("[links.auto]\nfirst_only = true\n");

    let results = run_links_auto_in_vault(tmp.path(), &["--first-only"]);

    assert_eq!(
        results["total"], 3,
        "flag plus config is still first-only: {results}"
    );
}

// ---------------------------------------------------------------------------
// `--no-first-only` counter-flag (iter-198)
// ---------------------------------------------------------------------------

#[test]
fn links_auto_no_first_only_overrides_config_first_only() {
    let tmp = setup_auto_config_vault("[links.auto]\nfirst_only = true\n");

    // Baseline: the persisted key collapses the duplicate mentions.
    let persisted = run_links_auto_in_vault(tmp.path(), &[]);
    assert_eq!(
        persisted["total"], 3,
        "config first_only should keep one mention per target: {persisted}"
    );

    // The counter-flag gets every mention back for this one run, without
    // touching .hyalo.toml.
    let overridden = run_links_auto_in_vault(tmp.path(), &["--no-first-only"]);
    assert_eq!(
        overridden["total"], 5,
        "--no-first-only should link every mention despite the config: {overridden}"
    );
}

#[test]
fn links_auto_no_first_only_applies_every_mention() {
    let tmp = setup_auto_config_vault("[links.auto]\nfirst_only = true\n");

    let results = run_links_auto_in_vault(tmp.path(), &["--no-first-only", "--apply"]);
    assert_eq!(
        results["applied"].as_bool(),
        Some(true),
        "write path honours it too: {results}"
    );
    assert_eq!(
        results["total"], 5,
        "every mention is written, not just the first: {results}"
    );

    let content = fs::read_to_string(tmp.path().join("guide.md")).expect("guide.md should be read");
    assert_eq!(
        content.matches("[[permissions").count(),
        2,
        "both Permissions mentions should be linked: {content}"
    );
    assert_eq!(
        content.matches("[[daily").count(),
        2,
        "both Daily mentions should be linked: {content}"
    );
}

#[test]
fn links_auto_no_first_only_without_config_is_a_no_op() {
    // Nothing to turn off: the run already links every mention.
    let tmp = setup_auto_config_vault("[links.auto]\nfirst_only = false\n");

    let plain = run_links_auto_in_vault(tmp.path(), &[]);
    let flagged = run_links_auto_in_vault(tmp.path(), &["--no-first-only"]);

    assert_eq!(plain["total"], 5, "baseline links every mention: {plain}");
    assert_eq!(
        flagged["total"], plain["total"],
        "--no-first-only changes nothing when first_only is already off: {flagged}"
    );
}

#[test]
fn links_auto_first_only_and_no_first_only_conflict() {
    let tmp = setup_auto_config_vault("[links.auto]\nfirst_only = true\n");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["links", "auto", "--first-only", "--no-first-only"])
        .output()
        .expect("hyalo links auto should run");

    assert!(
        !output.status.success(),
        "the two flags contradict each other and must be rejected"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot be used with"),
        "expected a clap conflict error, got: {stderr}"
    );
}

#[test]
fn links_auto_omits_config_excluded_when_config_removed_nothing() {
    // A config exclusion naming a title no page has removes no candidates, so
    // the key stays out of the envelope (the `links.out_of_vault` precedent).
    let tmp = setup_auto_config_vault("[links.auto]\nexclude_titles = [\"nonexistent\"]\n");

    let results = run_links_auto_in_vault(tmp.path(), &[]);

    assert!(
        results.get("config_excluded").is_none(),
        "config_excluded should be omitted when zero: {results}"
    );
}

#[test]
fn links_auto_without_config_omits_config_excluded() {
    let tmp = setup_auto_config_vault("");

    let results = run_links_auto_in_vault(tmp.path(), &["--exclude-title", "Permissions"]);

    assert!(
        results.get("config_excluded").is_none(),
        "CLI-only exclusions are not config exclusions: {results}"
    );
}

#[test]
fn links_auto_text_output_reports_config_excluded() {
    let tmp = setup_auto_config_vault("[links.auto]\nexclude_titles = [\"permissions\"]\n");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["links", "auto", "--format", "text"])
        .output()
        .expect("hyalo links auto should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(
        stdout.contains("Excluded by [links.auto] config: 1 title"),
        "text output should explain the config exclusions; got: {stdout}"
    );
}

#[test]
fn links_auto_config_exclusions_survive_apply() {
    let tmp = setup_auto_config_vault("[links.auto]\nexclude_titles = [\"permissions\"]\n");

    let results = run_links_auto_in_vault(tmp.path(), &["--apply"]);
    assert_eq!(
        results["applied"], true,
        "apply should report success: {results}"
    );

    let guide = fs::read_to_string(tmp.path().join("guide.md")).expect("guide.md should exist");
    assert!(
        !guide.contains("[[permissions]]"),
        "config-excluded title must not be written on --apply: {guide}"
    );
    assert!(
        guide.contains("[[daily]]"),
        "non-excluded titles should still be linked: {guide}"
    );
}

// ---------------------------------------------------------------------------
// iter-197: advisory note for common-English-word candidate titles
// ---------------------------------------------------------------------------

/// Run `links auto` in `dir` and return `(stdout, stderr)` verbatim.
///
/// The common-title note is a stderr-only signal, so these tests need both
/// streams — `run_links_auto_in_vault` above discards stderr.
fn run_links_auto_capturing(dir: &std::path::Path, extra_args: &[&str]) -> (String, String) {
    let output = hyalo_no_hints()
        .current_dir(dir)
        .args(["links", "auto", "--format", "json"])
        .args(extra_args)
        .output()
        .expect("hyalo links auto should run");
    assert!(
        output.status.success(),
        "links auto exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn links_auto_notes_common_word_titles_on_stderr() {
    // `permissions.md` is the canonical noise case: an ordinary English word as
    // a page title, mentioned twice in prose in guide.md.
    let tmp = setup_auto_config_vault("");

    let (_stdout, stderr) = run_links_auto_capturing(tmp.path(), &[]);

    assert!(
        stderr.contains("note:"),
        "the advisory should be a note, not a warning: {stderr}"
    );
    assert!(
        stderr.contains("common English word"),
        "note should explain why the titles are flagged: {stderr}"
    );
    assert!(
        stderr.contains("\"Permissions\" (2×)"),
        "note should name the offending title, in the vault's own spelling \
         (L-13), with its match count: {stderr}"
    );
    assert!(
        stderr.contains("--exclude-title Permissions"),
        "note should hand the user a ready-to-paste flag: {stderr}"
    );
    assert!(
        stderr.contains("[links.auto] exclude_titles"),
        "note should point at the persistent fix too: {stderr}"
    );
}

#[test]
fn links_auto_common_title_note_never_touches_the_stdout_report() {
    // The report shape is the contract; the note lives on stderr only. A run
    // with the note enabled must produce byte-identical stdout to one with it
    // suppressed.
    let tmp = setup_auto_config_vault("");

    let (with_note, stderr) = run_links_auto_capturing(tmp.path(), &[]);
    let (without_note, silent_stderr) =
        run_links_auto_capturing(tmp.path(), &["--no-warn-common-titles"]);

    assert!(
        !stderr.is_empty(),
        "the control run should actually have emitted the note"
    );
    assert!(
        silent_stderr.is_empty(),
        "--no-warn-common-titles should leave stderr empty: {silent_stderr}"
    );
    assert_eq!(
        with_note, without_note,
        "the note must not change the stdout envelope"
    );
    assert!(
        !with_note.contains("common English word"),
        "the advisory text must never leak into stdout: {with_note}"
    );
}

#[test]
fn links_auto_config_warn_common_titles_false_silences_the_note() {
    let tmp = setup_auto_config_vault("[links.auto]\nwarn_common_titles = false\n");

    let (_stdout, stderr) = run_links_auto_capturing(tmp.path(), &[]);

    assert!(
        stderr.is_empty(),
        "warn_common_titles = false should silence the note for every run: {stderr}"
    );
}

#[test]
fn links_auto_quiet_suppresses_the_common_title_note() {
    let tmp = setup_auto_config_vault("");

    let (_stdout, stderr) = run_links_auto_capturing(tmp.path(), &["-q"]);

    assert!(
        stderr.is_empty(),
        "-q suppresses every note, including this one: {stderr}"
    );
}

#[test]
fn links_auto_common_title_note_disappears_once_the_title_is_excluded() {
    // Acting on the note removes it: the heuristic reports emitted matches, so
    // an excluded title has nothing left to report.
    let tmp = setup_auto_config_vault("");

    let (_stdout, stderr) =
        run_links_auto_capturing(tmp.path(), &["--exclude-title", "permissions"]);

    assert!(
        !stderr.contains("common English word"),
        "excluding the title should extinguish the note: {stderr}"
    );
}

#[test]
fn links_auto_config_excluded_title_also_extinguishes_the_note() {
    let tmp = setup_auto_config_vault("[links.auto]\nexclude_titles = [\"permissions\"]\n");

    let (_stdout, stderr) = run_links_auto_capturing(tmp.path(), &[]);

    assert!(
        !stderr.contains("common English word"),
        "a config exclusion should extinguish the note as well: {stderr}"
    );
}

#[test]
fn links_auto_stays_silent_for_domain_specific_titles() {
    let tmp = TempDir::new().expect("tempdir creation should succeed");
    write_md(
        tmp.path(),
        "kubernetes.md",
        md!(r"
---
title: Kubernetes
---
Container orchestration.
"),
    );
    write_md(
        tmp.path(),
        "guide.md",
        md!(r"
---
title: Guide
---
We deploy on Kubernetes twice a week.
"),
    );

    let (stdout, stderr) = run_links_auto_capturing(tmp.path(), &[]);

    assert!(
        stdout.contains("kubernetes"),
        "the domain title should still be proposed: {stdout}"
    );
    assert!(
        stderr.is_empty(),
        "no common-word title means no note at all: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// iter-205: frequency trigger for the common-title note (dogfood UX-1)
// ---------------------------------------------------------------------------

/// Run `links auto` in `dir` with an explicit output format and return
/// `(stdout, stderr)` verbatim.
fn run_links_auto_capturing_format(
    dir: &std::path::Path,
    format: &str,
    extra_args: &[&str],
) -> (String, String) {
    let output = hyalo_no_hints()
        .current_dir(dir)
        .args(["links", "auto", "--format", format])
        .args(extra_args)
        .output()
        .expect("hyalo links auto should run");
    assert!(
        output.status.success(),
        "links auto exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// A GitHub-Docs-shaped vault: one page whose title dominates the run without
/// being an English word (`mentions` prose mentions), plus a quiet
/// domain-specific title. This is the shape UX-1 was invisible on.
fn setup_frequent_title_vault(config: &str, title: &str, mentions: usize) -> TempDir {
    use std::fmt::Write as _;

    let tmp = TempDir::new().expect("tempdir creation should succeed");
    fs::write(tmp.path().join(".hyalo.toml"), config).expect("config should be writable");

    write_md(
        tmp.path(),
        "dominant.md",
        &format!("---\ntitle: {title}\n---\nThe page everything mentions.\n"),
    );
    write_md(
        tmp.path(),
        "kubernetes.md",
        md!(r"
---
title: Kubernetes
---
Container orchestration.
"),
    );

    let mut guide = String::from("---\ntitle: Guide\n---\n");
    for i in 0..mentions {
        let _ = writeln!(guide, "Step {i}: {title} is configured in the usual place.");
    }
    for i in 0..10 {
        let _ = writeln!(guide, "Note {i}: Kubernetes schedules the run.");
    }
    write_md(tmp.path(), "guide.md", &guide);

    tmp
}

#[test]
fn links_auto_notes_a_dominant_title_that_is_not_an_english_word() {
    // Dogfood UX-1: on GitHub Docs a page titled "Workflows" produced 43% of
    // all proposed links and the wordlist-only trigger never mentioned it.
    let tmp = setup_frequent_title_vault("", "Workflows", 30);

    let (_stdout, stderr) = run_links_auto_capturing(tmp.path(), &[]);

    assert!(
        stderr.contains("note:"),
        "the advisory should be a note, not a warning: {stderr}"
    );
    assert!(
        stderr.contains("unusually frequent"),
        "the note should name frequency as the reason: {stderr}"
    );
    assert!(
        !stderr.contains("common English word"),
        "\"Workflows\" is not a wordlist hit — the reason must not claim it is: {stderr}"
    );
    assert!(
        stderr.contains("\"Workflows\" (30×, 75%)"),
        "the note should quote the count and the share of the run: {stderr}"
    );
    assert!(
        stderr.contains("--exclude-title Workflows"),
        "the note should hand the user a ready-to-paste flag: {stderr}"
    );
    assert!(
        !stderr.contains("Kubernetes"),
        "the quiet title should not be mentioned: {stderr}"
    );
}

#[test]
fn links_auto_frequency_note_disappears_after_one_paste_back() {
    // The whole point of the suggestion: one round, not two.
    let tmp = setup_frequent_title_vault("", "Workflows", 30);

    let (_stdout, stderr) = run_links_auto_capturing(tmp.path(), &["--exclude-title", "Workflows"]);

    assert!(
        stderr.is_empty(),
        "excluding the dominant title should extinguish the note: {stderr}"
    );
}

#[test]
fn links_auto_frequency_note_never_touches_the_stdout_report() {
    // Same contract as the wordlist path: the note is stderr-only, in every
    // output format.
    let tmp = setup_frequent_title_vault("", "Workflows", 30);

    for format in ["json", "text"] {
        let (with_note, stderr) = run_links_auto_capturing_format(tmp.path(), format, &[]);
        let (without_note, silent_stderr) =
            run_links_auto_capturing_format(tmp.path(), format, &["--no-warn-common-titles"]);

        assert!(
            stderr.contains("unusually frequent"),
            "the control run should actually have emitted the note ({format}): {stderr}"
        );
        assert!(
            silent_stderr.is_empty(),
            "--no-warn-common-titles should leave stderr empty ({format}): {silent_stderr}"
        );
        assert_eq!(
            with_note, without_note,
            "the note must not change the stdout envelope ({format})"
        );
        assert!(
            !with_note.contains("unusually frequent"),
            "the advisory text must never leak into stdout ({format}): {with_note}"
        );
    }
}

#[test]
fn links_auto_config_opt_out_also_silences_the_frequency_note() {
    let tmp = setup_frequent_title_vault(
        "[links.auto]\nwarn_common_titles = false\n",
        "Workflows",
        30,
    );

    let (_stdout, stderr) = run_links_auto_capturing(tmp.path(), &[]);

    assert!(
        stderr.is_empty(),
        "warn_common_titles = false governs both triggers: {stderr}"
    );
}

#[test]
fn links_auto_multiword_frequent_title_round_trips_through_the_suggested_flag() {
    // "runner groups" (45 links on the dogfood's GitHub Docs slice) is the
    // real-data case for the shell-quoting path: paste the flag back verbatim
    // and the note has to go away.
    let tmp = setup_frequent_title_vault("", "runner groups", 30);

    let (_stdout, stderr) = run_links_auto_capturing(tmp.path(), &[]);
    assert!(
        stderr.contains("--exclude-title 'runner groups'"),
        "a title with a space must be shell-quoted in the suggestion: {stderr}"
    );

    // What the shell hands back after stripping the quotes.
    let (_stdout, quiet) =
        run_links_auto_capturing(tmp.path(), &["--exclude-title", "runner groups"]);
    assert!(
        quiet.is_empty(),
        "pasting the quoted flag should extinguish the note: {quiet}"
    );
}

#[test]
fn links_auto_notes_a_dominant_non_ascii_title() {
    // The wordlist is ASCII-gated, so before iter-205 a German vault never saw
    // the note at all however dominant its titles were.
    let tmp = setup_frequent_title_vault("", "Übersicht", 30);

    let (_stdout, stderr) = run_links_auto_capturing(tmp.path(), &[]);

    assert!(
        stderr.contains("\"Übersicht\" (30×, 75%)"),
        "a non-ASCII title should reach the frequency trigger: {stderr}"
    );
    assert!(
        stderr.contains("--exclude-title 'Übersicht'"),
        "the suggested flag should be shell-safe: {stderr}"
    );
}

#[test]
fn links_auto_stays_silent_when_no_title_clears_the_frequency_floor() {
    // The knowledgebase-shaped case: a handful of domain-specific titles, none
    // of them an English word and none anywhere near 25 matches.
    let tmp = setup_frequent_title_vault("", "Workflows", 20);

    let (stdout, stderr) = run_links_auto_capturing(tmp.path(), &[]);

    assert!(
        stdout.contains("Workflows"),
        "the links themselves should still be proposed: {stdout}"
    );
    assert!(
        stderr.is_empty(),
        "20 matches is under the floor — no note: {stderr}"
    );
}

#[test]
fn links_auto_note_truncates_the_prose_list_but_not_the_flags() {
    // Dogfood L-12: with more offenders than the note lists, the flag list
    // still has to cover all of them, and the note has to admit it truncated.
    let tmp = TempDir::new().expect("tempdir creation should succeed");
    let words = [
        "access", "account", "action", "active", "address", "agree", "answer",
    ];
    let mut guide = String::from("---\ntitle: Guide\n---\n");
    for word in words {
        use std::fmt::Write as _;
        write_md(
            tmp.path(),
            &format!("{word}.md"),
            &format!("---\ntitle: {word}\n---\nA page.\n"),
        );
        let _ = writeln!(guide, "The {word} page is over there.");
    }
    write_md(tmp.path(), "guide.md", &guide);

    let (_stdout, stderr) = run_links_auto_capturing(tmp.path(), &[]);

    assert!(
        stderr.contains("showing the 5 noisiest of 7"),
        "the note should admit that its prose list is capped: {stderr}"
    );
    assert_eq!(
        stderr.matches("--exclude-title ").count(),
        7,
        "every offender needs a flag so one paste-back extinguishes the note: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// iter-200: links apply-path integrity (dogfood H-1, H-2, M-1)
// ---------------------------------------------------------------------------

/// Run `hyalo links fix` in `dir` with the given extra args and return the
/// parsed `results` object.
fn links_fix_results(dir: &std::path::Path, extra: &[&str]) -> serde_json::Value {
    let mut cmd = hyalo_no_hints();
    cmd.args([
        "--dir",
        dir.to_str().expect("temp path should be valid UTF-8"),
        "links",
        "fix",
        "--format",
        "json",
    ]);
    cmd.args(extra);
    let output = cmd.output().expect("hyalo links fix should run");
    assert!(
        output.status.success(),
        "links fix exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    json["results"].clone()
}

#[test]
fn links_fix_apply_keeps_site_absolute_links_site_absolute() {
    // Dogfood H-1: the writer emitted the vault-relative path, which the
    // resolver then read as relative to the *source file's* directory — so
    // every rewrite was permanently broken and stayed "fixable" forever.
    let tmp = TempDir::new().expect("tempdir creation should succeed");
    write_md(
        tmp.path(),
        "docs/page.md",
        md!(r"
See [AUTOTITLE](/how-tos/old-home/moved-page) for details.
"),
    );
    write_md(tmp.path(), "how-tos/new-home/moved-page.md", "# Moved\n");

    // The target moved directories, so the only evidence is the basename —
    // a guess, hence behind the fuzzy gate (M-1).
    let before = links_fix_results(tmp.path(), &[]);
    assert_eq!(before["broken"].as_u64(), Some(1), "{before}");

    let applied = links_fix_results(tmp.path(), &["--apply", "--apply-fuzzy"]);
    assert_eq!(applied["applied_fixes"].as_array().map(Vec::len), Some(1));

    let written = fs::read_to_string(tmp.path().join("docs").join("page.md"))
        .expect("page.md should be readable");
    assert!(
        written.contains("[AUTOTITLE](/how-tos/new-home/moved-page)"),
        "site-absolute form must survive the rewrite, got: {written}"
    );

    // The whole point: a re-run sees nothing left to do.
    let after = links_fix_results(tmp.path(), &[]);
    assert_eq!(after["broken"].as_u64(), Some(0), "{after}");
    assert_eq!(after["fixable"].as_u64(), Some(0), "{after}");
}

#[test]
fn links_fix_relative_basename_guess_is_gated_like_the_site_absolute_one() {
    // DEC-076 (iter-211 / BUG-12): the gate keys on whether the author wrote a
    // *directory*, not on a leading slash. `../c/target.md` asserts a location
    // just as firmly as `/c/target.md` does, so throwing that location away
    // and substituting `z/target.md` — matched on the basename alone — is the
    // same guess and lands behind the same `--apply-fuzzy` gate. Before this
    // change the two spellings had opposite gates, which is what the
    // 2026-08-23 dogfood called indefensible.
    let tmp = TempDir::new().expect("tempdir creation should succeed");
    write_md(
        tmp.path(),
        "a/b/page.md",
        md!(r"
See [x](../c/target.md) here.
"),
    );
    write_md(tmp.path(), "z/target.md", "# Target\n");

    let plain = links_fix_results(tmp.path(), &["--apply"]);
    assert_eq!(
        plain["applied_fixes"].as_array().map(Vec::len),
        Some(0),
        "a written directory makes this a guess, not a certain fix: {plain}"
    );
    assert_eq!(
        plain["fuzzy_fixes"][0]["strategy"].as_str(),
        Some("BasenameFallback"),
        "and it must be reported under the honest strategy: {plain}"
    );

    // iter-212 adds a second gate on top of the opt-in: `a/c/` (what
    // `../c/target.md` resolves to) shares nothing with `z/`, so the guess
    // scores the bare-basename floor of 0.7 and a default `--apply-fuzzy`
    // still refuses it.
    let default_floor = links_fix_results(tmp.path(), &["--apply", "--apply-fuzzy"]);
    assert_eq!(
        default_floor["applied_fixes"].as_array().map(Vec::len),
        Some(0),
        "a cross-tree basename guess is below the default confidence floor: {default_floor}"
    );

    // Opening the floor writes it, still in the author's source-relative style.
    let applied = links_fix_results(
        tmp.path(),
        &["--apply", "--apply-fuzzy", "--min-confidence", "0"],
    );
    assert_eq!(applied["applied_fixes"].as_array().map(Vec::len), Some(1));

    let written =
        fs::read_to_string(tmp.path().join("a").join("b").join("page.md")).expect("readable");
    assert!(
        written.contains("[x](../../z/target.md)"),
        "expected a source-relative target, got: {written}"
    );

    let after = links_fix_results(tmp.path(), &[]);
    assert_eq!(after["broken"].as_u64(), Some(0), "{after}");
}

#[test]
fn links_fix_bare_stem_repair_stays_a_plain_apply_fix() {
    // The other half of DEC-076: a target with no directory component asserts
    // no location, so resolving it by stem is the documented Obsidian
    // short-form rule — a resolution, not a guess — and plain `--apply`
    // writes it.
    let tmp = TempDir::new().expect("tempdir creation should succeed");
    write_md(
        tmp.path(),
        "a/b/page.md",
        md!(r"
See [x](target.md) here.
"),
    );
    write_md(tmp.path(), "z/target.md", "# Target\n");

    let applied = links_fix_results(tmp.path(), &["--apply"]);
    assert_eq!(
        applied["applied_fixes"].as_array().map(Vec::len),
        Some(1),
        "a bare-stem repair stays a certain fix: {applied}"
    );

    let after = links_fix_results(tmp.path(), &[]);
    assert_eq!(after["broken"].as_u64(), Some(0), "{after}");
}

#[test]
fn links_fix_site_absolute_directory_target_resolves_to_index_md() {
    // iter-203: the fixture behind dogfood M-1 — `/actions` alongside
    // `actions/index.md` — now simply *resolves*. There is nothing broken to
    // guess about, which is the whole point of directory-index resolution.
    let tmp = TempDir::new().expect("tempdir creation should succeed");
    write_md(
        tmp.path(),
        "index.md",
        md!(r"
See [GitHub Actions](/actions) for details.
"),
    );
    write_md(tmp.path(), "actions/index.md", "# Actions\n");
    write_md(tmp.path(), "graphql/reference/actions.md", "# GraphQL\n");

    let report = links_fix_results(tmp.path(), &[]);
    assert_eq!(report["broken"].as_u64(), Some(0), "{report}");
    assert_eq!(report["fixable"].as_u64(), Some(0), "{report}");
    assert_eq!(
        report["fuzzy_fixes"].as_array().map(Vec::len),
        Some(0),
        "a resolving link must not appear as a fuzzy candidate: {report}"
    );

    // A plain --apply leaves the file alone.
    let before = fs::read_to_string(tmp.path().join("index.md")).expect("readable");
    let _ = links_fix_results(tmp.path(), &["--apply"]);
    let after = fs::read_to_string(tmp.path().join("index.md")).expect("readable");
    assert_eq!(before, after, "plain --apply must not rewrite a valid link");
}

#[test]
fn links_fix_site_absolute_basename_guess_is_behind_the_fuzzy_gate() {
    // Dogfood M-1: `/actions` "resolved" to `graphql/reference/actions.md`
    // labelled LinkCaseMismatch at confidence 1.0, in the default apply
    // bucket. With no `actions/index.md` to resolve to (see the test above),
    // the link is genuinely broken and the cross-directory basename guess must
    // stay behind the fuzzy gate.
    let tmp = TempDir::new().expect("tempdir creation should succeed");
    write_md(
        tmp.path(),
        "index.md",
        md!(r"
See [GitHub Actions](/actions) for details.
"),
    );
    write_md(tmp.path(), "graphql/reference/actions.md", "# GraphQL\n");

    let report = links_fix_results(tmp.path(), &[]);
    assert_eq!(
        report["case_mismatches"].as_u64(),
        Some(0),
        "a cross-directory basename guess is not a case mismatch: {report}"
    );
    assert_eq!(
        report["fixable"].as_u64(),
        Some(0),
        "the guess must not sit in the default apply bucket: {report}"
    );
    let fuzzy = report["fuzzy_fixes"]
        .as_array()
        .expect("fuzzy_fixes array")
        .clone();
    assert_eq!(fuzzy.len(), 1, "{report}");
    assert_eq!(fuzzy[0]["strategy"].as_str(), Some("BasenameFallback"));
    assert!(
        fuzzy[0]["confidence"].as_f64().unwrap_or(1.0) < 1.0,
        "a guess must not claim confidence 1.0: {report}"
    );

    // A plain --apply leaves the file alone.
    let before = fs::read_to_string(tmp.path().join("index.md")).expect("readable");
    let _ = links_fix_results(tmp.path(), &["--apply"]);
    let after = fs::read_to_string(tmp.path().join("index.md")).expect("readable");
    assert_eq!(before, after, "plain --apply must not write a guess");
}

#[test]
fn links_auto_apply_never_writes_inside_urls_or_link_labels() {
    // Dogfood H-2: a page titled `net` turned
    // `[x](https://pkg.go.dev/x/actions.summerwind.net/v1)` into
    // `…summerwind.[[net]]/v1`, destroying two working URLs per line.
    let tmp = TempDir::new().expect("tempdir creation should succeed");
    write_md(
        tmp.path(),
        "net.md",
        md!(r"
---
title: net
---
# net
"),
    );
    write_md(tmp.path(), "other.md", "# Other\n");
    let body = md!(r"
Link: [x](https://pkg.go.dev/x/actions.summerwind.net/v1)

Bare: https://example.net/path is a URL.

Label: [the net thing](https://example.com/a)

Internal: [read about net](other.md)

Prose mention of net should be linked.
");
    write_md(tmp.path(), "page.md", body);

    let output = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().expect("valid UTF-8"),
            "links",
            "auto",
            "--apply",
            "--format",
            "json",
        ])
        .output()
        .expect("hyalo links auto --apply should run");
    assert!(
        output.status.success(),
        "links auto --apply exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let written = fs::read_to_string(tmp.path().join("page.md")).expect("readable");
    assert!(
        written.contains("[x](https://pkg.go.dev/x/actions.summerwind.net/v1)"),
        "the URL destination must be untouched: {written}"
    );
    assert!(
        written.contains("Bare: https://example.net/path is a URL."),
        "the bare URL must be untouched: {written}"
    );
    assert!(
        written.contains("[the net thing](https://example.com/a)"),
        "an external link's label must be untouched: {written}"
    );
    assert!(
        written.contains("[read about net](other.md)"),
        "an internal link's label must be untouched: {written}"
    );
    assert!(
        written.contains("Prose mention of [[net]] should be linked."),
        "the real prose mention must be linked: {written}"
    );
}

// ---------------------------------------------------------------------------
// iter-207: inert-zone completion (dogfood BUG-1 / BUG-2 / BUG-3 / BUG-4)
// ---------------------------------------------------------------------------

/// Run `links auto --apply` over `tmp` and return the rewritten `page.md`.
fn auto_apply_page(tmp: &TempDir) -> String {
    let output = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().expect("valid UTF-8"),
            "links",
            "auto",
            "--apply",
            "--format",
            "json",
        ])
        .output()
        .expect("hyalo links auto --apply should run");
    assert!(
        output.status.success(),
        "links auto --apply exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::read_to_string(tmp.path().join("page.md")).expect("readable")
}

/// Vault with a single linkable page titled `git` plus `page.md` holding `body`.
fn vault_with_page(title: &str, body: &str) -> TempDir {
    let tmp = TempDir::new().expect("tempdir creation should succeed");
    write_md(
        tmp.path(),
        &format!("{title}.md"),
        &format!("---\ntitle: {title}\n---\n# {title}\n"),
    );
    write_md(tmp.path(), "page.md", body);
    tmp
}

/// BUG-1 minimal repro: one unmatched backtick (`` press <kbd>`</kbd> ``) used
/// to pair with the *opening* backtick of a later code span, leaving the real
/// code unblanked so `` `git blame` `` became `` `[[git]] blame` ``.
#[test]
fn links_auto_apply_leaves_code_spans_after_an_unmatched_backtick_alone() {
    let tmp = vault_with_page(
        "git",
        md!(r"
Before: `git blame` stays code.

Press <kbd>`</kbd> to open a terminal.

After: `git blame` should still be code.
"),
    );
    let written = auto_apply_page(&tmp);
    assert!(
        !written.contains("[[git]]"),
        "no wikilink may be injected into a code span: {written}"
    );
    assert_eq!(
        written.matches("`git blame`").count(),
        2,
        "both code spans must survive verbatim: {written}"
    );
}

/// The same shapes the dogfood measured on GitHub Docs and vscode-docs: a
/// stray backtick followed, paragraphs later, by real code spans.
#[test]
fn links_auto_apply_survives_real_corpus_stray_backtick_shapes() {
    let tmp = TempDir::new().expect("tempdir creation should succeed");
    for title in ["README", "settings", "json"] {
        write_md(
            tmp.path(),
            &format!("{title}.md"),
            &format!("---\ntitle: {title}\n---\n# {title}\n"),
        );
    }
    write_md(
        tmp.path(),
        "page.md",
        md!(r"
The ` character starts a code span.

| key | ` | note |
| --- | - | ---- |

Open `README.md` and `settings.json` to continue.
"),
    );
    let written = auto_apply_page(&tmp);
    assert!(
        written.contains("`README.md`") && written.contains("`settings.json`"),
        "code spans must survive verbatim: {written}"
    );
    assert!(
        !written.contains("[["),
        "no wikilink may be injected at all: {written}"
    );
}

/// BUG-2: 3,328 of 11,141 GitHub Docs insertions landed inside `{% … %}` /
/// `{{ … }}`, destroying variable references.
#[test]
fn links_auto_apply_never_writes_inside_liquid_expressions() {
    let tmp = vault_with_page(
        "copilot",
        md!(r"
Tag: {% data variables.product.prodname_copilot %} end.

Output: {{ site.copilot.baseurl }}/x end.

Unterminated: {% ifversion copilot

A plain copilot mention should be linked.
"),
    );
    let written = auto_apply_page(&tmp);
    assert!(
        written.contains("{% data variables.product.prodname_copilot %}"),
        "a Liquid tag must be untouched: {written}"
    );
    assert!(
        written.contains("{{ site.copilot.baseurl }}"),
        "a Liquid output expression must be untouched: {written}"
    );
    assert!(
        written.contains("Unterminated: {% ifversion copilot"),
        "an unterminated Liquid marker makes the rest of the line inert: {written}"
    );
    assert!(
        written.contains("A plain [[copilot]] mention should be linked."),
        "the real prose mention must still be linked: {written}"
    );
}

/// BUG-3: 128 vscode-docs insertions landed inside HTML tags, breaking image
/// paths, anchor names and class hooks.
#[test]
fn links_auto_apply_never_writes_inside_html_tags() {
    let tmp = vault_with_page(
        "net",
        md!(r#"
Image: <img src="net.png" alt="net diagram">

Anchor: <a name="net" class="net-hook">see</a>

Scheme: <a href="vscode://net/open">open</a>

Between tags: <div>net prose</div> here.
"#),
    );
    let written = auto_apply_page(&tmp);
    assert!(
        written.contains(r#"<img src="net.png" alt="net diagram">"#),
        "tag attributes must be untouched: {written}"
    );
    assert!(
        written.contains(r#"<a name="net" class="net-hook">"#),
        "anchor names and class hooks must be untouched: {written}"
    );
    assert!(
        written.contains(r#"<a href="vscode://net/open">"#),
        "a non-http scheme in an attribute must be untouched: {written}"
    );
    assert!(
        written.contains("<div>[[net]] prose</div>"),
        "text between tags stays linkable: {written}"
    );
}

/// BUG-4: `{% ifversion … %}/path{% endif %}/…` fuzzy-matched a real file at
/// 0.95 and was rewritten, silently dropping the conditional. 25 such offers
/// on the GitHub Docs corpus; the round-trip guard cannot catch it because the
/// rewritten target genuinely resolves.
#[test]
fn links_fix_never_rewrites_templated_destinations() {
    let tmp = TempDir::new().expect("tempdir creation should succeed");
    write_md(tmp.path(), "guides.md", "# Guides\n");
    let body = md!(r"
Conditional: [a]({% ifversion ghes %}/admin{% endif %}/guides)
Variable: [b]({{ site.baseurl }}/guides)
Shell: [c](${BASE}/guides)
Real typo: [d](guidez)
");
    write_md(tmp.path(), "src.md", body);

    let report = links_fix_results(tmp.path(), &["--dry-run"]);
    assert_eq!(
        report["templated"].as_u64(),
        Some(3),
        "all three template forms land in the named bucket: {report}"
    );
    let templated = report["templated_links"]
        .as_array()
        .expect("templated_links array");
    assert_eq!(templated.len(), 3, "{report}");
    assert!(
        templated
            .iter()
            .all(|t| t["target"].as_str().unwrap_or_default().contains("guides")),
        "templated links keep their original target text: {report}"
    );
    assert_eq!(
        report["unfixable"].as_u64(),
        Some(0),
        "templated links are not silently folded into unfixable: {report}"
    );

    // Neither plain --apply nor --apply-fuzzy may touch them.
    let before = fs::read_to_string(tmp.path().join("src.md")).expect("readable");
    let _ = links_fix_results(tmp.path(), &["--apply", "--apply-fuzzy"]);
    let after = fs::read_to_string(tmp.path().join("src.md")).expect("readable");
    for templated_target in [
        "{% ifversion ghes %}/admin{% endif %}/guides",
        "{{ site.baseurl }}/guides",
        "${BASE}/guides",
    ] {
        assert!(
            after.contains(templated_target),
            "templated destination {templated_target} was rewritten: {after}"
        );
    }
    assert_ne!(before, after, "the real typo should still have been fixed");
    assert!(
        after.contains("[d](guides)") && !after.contains("[d](guidez)"),
        "{after}"
    );
}

/// BUG-7 (iter-202 regression): an in-vault symlink that sorts before its
/// target used to become the canonical representative, dropping the real file
/// from the fuzzy candidate set — `[fuzzy 0.966]` turned into `Unfixable: 1`.
#[cfg(unix)]
#[test]
fn links_fix_symlink_does_not_shadow_the_real_file_in_fuzzy_candidates() {
    let tmp = TempDir::new().expect("tempdir creation should succeed");
    write_md(
        tmp.path(),
        "notes/source.md",
        md!(r"
---
title: Source
---
See [[targt]] here.
"),
    );
    write_md(
        tmp.path(),
        "notes/target.md",
        md!(r"
---
title: Target
---
x
"),
    );
    std::os::unix::fs::symlink("target.md", tmp.path().join("notes/alias-target.md"))
        .expect("symlink creation should succeed");

    let report = links_fix_results(tmp.path(), &["--dry-run"]);
    assert_eq!(
        report["unfixable"].as_u64(),
        Some(0),
        "the alias must not shadow the real file: {report}"
    );
    let fuzzy = report["fuzzy_fixes"].as_array().expect("fuzzy_fixes array");
    assert_eq!(fuzzy.len(), 1, "{report}");
    assert_eq!(
        fuzzy[0]["new_target"].as_str(),
        Some("notes/target.md"),
        "the fix must be attributed to the real filename: {report}"
    );
    assert!(
        fuzzy[0]["confidence"].as_f64().unwrap_or_default() > 0.96,
        "the 0.966 offer must survive: {report}"
    );
}

#[test]
fn links_fix_apply_conformance_broken_count_decreases_and_valid_links_untouched() {
    // Regression gate for the whole apply-path class (iter-200): a corpus
    // mixing site-absolute, relative, `../`, and URL-adjacent links. Applying
    // every proposed fix must make the broken count strictly decrease and must
    // not touch a single link that already resolved.
    let tmp = TempDir::new().expect("tempdir creation should succeed");

    // Targets.
    write_md(tmp.path(), "how-tos/new-home/moved-page.md", "# Moved\n");
    write_md(tmp.path(), "how-tos/Case-Page.md", "# Case\n");
    write_md(tmp.path(), "z/target.md", "# Target\n");
    write_md(tmp.path(), "index.md", "# Index\n");

    // Sources.
    write_md(
        tmp.path(),
        "docs/page.md",
        md!(r"
Site-absolute, moved: [a](/how-tos/old-home/moved-page)
Site-absolute, case only: [b](/how-tos/case-page)
"),
    );
    write_md(
        tmp.path(),
        "a/b/page.md",
        md!(r"
Relative, wrong dir: [c](../c/target.md)
Parent-relative, valid: [d](../../index.md)
"),
    );
    let untouched_body = md!(r"
Valid wikilink: [[z/target]]
Valid relative: [e](../index.md)
External: [f](https://example.com/how-tos/case-page)
Bare URL: https://example.com/z/target.md
");
    write_md(tmp.path(), "docs/valid.md", untouched_body);

    let before = links_fix_results(tmp.path(), &[]);
    let broken_before = before["broken"].as_u64().expect("broken count");
    assert!(broken_before > 0, "fixture must start broken: {before}");

    let applied = links_fix_results(tmp.path(), &["--apply", "--apply-fuzzy"]);
    assert_eq!(
        applied["failed"].as_u64(),
        Some(0),
        "no write may fail: {applied}"
    );

    let after = links_fix_results(tmp.path(), &[]);
    let broken_after = after["broken"].as_u64().expect("broken count");
    assert!(
        broken_after < broken_before,
        "broken count must strictly decrease ({broken_before} → {broken_after}): {after}"
    );
    assert_eq!(
        after["case_mismatches"].as_u64(),
        Some(0),
        "case mismatches must be repaired too: {after}"
    );

    // iter-212: exactly one fixture link survives a default `--apply-fuzzy` —
    // `../c/target.md` → `z/target.md` discards the author's directory for an
    // unrelated tree and scores the bare-basename floor of 0.7, under the
    // default 0.8. It is reported, not written.
    assert_eq!(broken_after, 1, "{after}");
    assert_eq!(after["fuzzy_below_floor"].as_u64(), Some(1), "{after}");
    assert_eq!(
        after["fuzzy_fixes"][0]["confidence"].as_f64(),
        Some(0.7),
        "{after}"
    );

    // Opening the floor is the documented escape hatch and clears the rest.
    let forced = links_fix_results(
        tmp.path(),
        &["--apply", "--apply-fuzzy", "--min-confidence", "0"],
    );
    assert_eq!(forced["failed"].as_u64(), Some(0), "{forced}");
    let final_state = links_fix_results(tmp.path(), &[]);
    assert_eq!(
        final_state["broken"].as_u64(),
        Some(0),
        "every fixture link has a real target: {final_state}"
    );

    // The file whose links all resolved must be byte-identical.
    let still = fs::read_to_string(tmp.path().join("docs").join("valid.md")).expect("readable");
    assert_eq!(
        still, untouched_body,
        "a file with no broken links must not be rewritten"
    );

    // The valid `../../index.md` link in a rewritten file must survive verbatim.
    let nested = fs::read_to_string(tmp.path().join("a").join("b").join("page.md")).expect("ok");
    assert!(
        nested.contains("[d](../../index.md)"),
        "a resolving link in a rewritten file must not change: {nested}"
    );
}

// ---------------------------------------------------------------------------
// L-15 (iter-204): JSON match positions are 1-based in both axes
// ---------------------------------------------------------------------------

/// `links auto` reported a 1-based `line` next to a 0-based `col`, so a
/// consumer that trusted one was off by one on the other.
#[test]
fn links_auto_match_col_is_one_based() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "target.md",
        "---\ntitle: Target Note\n---\nBody.\n",
    );
    // "Target Note" starts at byte offset 4 of line 4 → column 5.
    write_md(
        tmp.path(),
        "src.md",
        "---\ntitle: Src\n---\nSee Target Note here.\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["links", "auto", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "links auto failed: {output:?}");
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).unwrap();
    let m = &json["results"]["matches"][0];
    assert_eq!(m["line"], 4, "line is 1-based: {json}");
    assert_eq!(m["col"], 5, "col must be 1-based too: {json}");
    assert_eq!(m["matched_text"], "Target Note");
}

/// iter-210: `col` counts Unicode scalars, not bytes, so it agrees with what an
/// editor shows and with lint's `column`. It used to be a byte index, which
/// drifted on any line containing a multibyte character.
#[test]
fn links_auto_match_col_counts_characters_not_bytes() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "target.md",
        "---\ntitle: Target Note\n---\nBody.\n",
    );
    // Line 4 is "Café — Target Note here." — "Target Note" starts at character
    // index 7 (column 8) but byte offset 11 (é is 2 bytes, — is 3).
    write_md(
        tmp.path(),
        "src.md",
        "---\ntitle: Src\n---\nCafé — Target Note here.\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["links", "auto", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "links auto failed: {output:?}");
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).unwrap();
    let m = &json["results"]["matches"][0];
    assert_eq!(m["matched_text"], "Target Note", "{json}");
    assert_eq!(
        m["col"], 8,
        "col must count characters (8), not bytes (12): {json}"
    );
}

/// The character column must still point at the right byte when the fix is
/// written: applying the auto-link on a multibyte line produces valid text.
#[test]
fn links_auto_apply_is_correct_on_multibyte_lines() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "target.md",
        "---\ntitle: Target Note\n---\nBody.\n",
    );
    write_md(
        tmp.path(),
        "src.md",
        "---\ntitle: Src\n---\nCafé — Target Note here.\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["links", "auto", "--apply", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "links auto --apply failed: {output:?}"
    );
    let body = std::fs::read_to_string(tmp.path().join("src.md")).unwrap();
    assert!(
        body.starts_with("---\ntitle: Src\n---\nCafé — [[")
            && body.trim_end().ends_with("]] here."),
        "the rewrite must land on the right bytes: {body:?}"
    );
}

// ---------------------------------------------------------------------------
// UX-4 / BUG-11 (iter-210): links output truth
// ---------------------------------------------------------------------------

/// A vault carrying one link of every interesting kind: certain-fixable
/// (short-form, via `--expand-short-form`), fuzzy, unfixable, and
/// case-mismatched.
///
/// Note the case-mismatch line behaves differently per filesystem: on a
/// case-insensitive volume `[[Sub/Target]]` resolves and lands in the
/// `case_mismatches` bucket, on a case-sensitive one it is broken and gets a
/// certain `CaseInsensitive` fix. Every assertion below is written to hold
/// either way.
fn setup_bucket_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "sub/target.md",
        "---\ntitle: Target\n---\nBody.\n",
    );
    write_md(
        tmp.path(),
        "src.md",
        "---\ntitle: Src\n---\n\
         Certain: [[target]]\n\
         Fuzzy: [[targett]]\n\
         Unfixable: [[completely-unrelated-xyzzy]]\n\
         Case: [[Sub/Target]]\n",
    );
    tmp
}

fn links_fix_json(dir: &std::path::Path, extra: &[&str]) -> serde_json::Value {
    let mut args = vec!["links", "fix", "--format", "json"];
    args.extend_from_slice(extra);
    let out = hyalo_no_hints()
        .current_dir(dir)
        .args(&args)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let val: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("links fix {extra:?} did not emit JSON: {stdout} ({e})"));
    val["results"].clone()
}

fn links_fix_text(dir: &std::path::Path, extra: &[&str]) -> String {
    let mut args = vec!["links", "fix", "--format", "text"];
    args.extend_from_slice(extra);
    let out = hyalo_no_hints()
        .current_dir(dir)
        .args(&args)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// UX-4: the JSON buckets partition `broken` exactly, and the text report now
/// prints all of them — the `fuzzy` count used to be JSON-only, so text readers
/// could not reconcile "6098 broken" with "25 fixable + 1400 unfixable".
#[test]
fn links_text_and_json_buckets_sum_to_broken() {
    let tmp = setup_bucket_vault();

    let r = links_fix_json(tmp.path(), &["--expand-short-form", "--dry-run"]);
    let n = |k: &str| r[k].as_u64().unwrap_or_else(|| panic!("missing {k}: {r}"));
    assert!(n("broken") > 0, "fixture must produce broken links: {r}");
    assert_eq!(
        n("broken"),
        n("fixable") + n("fuzzy") + n("unfixable") + n("templated"),
        "JSON buckets must partition `broken`: {r}"
    );

    let text = links_fix_text(tmp.path(), &["--expand-short-form", "--dry-run"]);
    let count_line = |label: &str| -> u64 {
        text.lines()
            .find_map(|l| l.strip_prefix(label))
            .unwrap_or_else(|| panic!("text output has no `{label}` line:\n{text}"))
            .trim()
            .parse()
            .unwrap_or_else(|e| panic!("`{label}` line is not a number in:\n{text} ({e})"))
    };
    let broken = count_line("Broken links:");
    let fixable = count_line("Fixable:");
    let fuzzy = count_line("Low-confidence matches (excluded from plain --apply):");
    let unfixable = count_line("Unfixable:");
    assert_eq!(
        broken,
        fixable + fuzzy + unfixable,
        "text buckets must sum to the broken count:\n{text}"
    );
    assert_eq!(broken, n("broken"), "text and JSON must agree:\n{text}");
}

/// UX-4: unfixable links used to be JSON-only. They now appear in text too.
#[test]
fn links_text_lists_unfixable_links() {
    let tmp = setup_bucket_vault();

    let text = links_fix_text(tmp.path(), &["--dry-run"]);
    assert!(
        text.contains("Unfixable links (no candidate in the vault):"),
        "text must carry an unfixable section:\n{text}"
    );
    assert!(
        text.contains("completely-unrelated-xyzzy"),
        "text must name the unfixable target:\n{text}"
    );
}

/// The fuzzy proposal listing is the longest section, so it must come last —
/// otherwise the actionable buckets are buried under thousands of lines.
#[test]
fn links_text_puts_fuzzy_listing_after_actionable_buckets() {
    let tmp = setup_bucket_vault();

    let text = links_fix_text(tmp.path(), &["--dry-run"]);
    let unfixable_at = text
        .find("Unfixable links (no candidate in the vault):")
        .unwrap_or_else(|| panic!("no unfixable section:\n{text}"));
    let fuzzy_at = text
        .find("Low-confidence matches (not applied")
        .unwrap_or_else(|| panic!("no fuzzy listing:\n{text}"));
    assert!(
        unfixable_at < fuzzy_at,
        "the fuzzy listing must come last:\n{text}"
    );
}

/// The text lists are capped so a vault with thousands of unfixable links stays
/// readable; the footer says how many were withheld and where to get them.
#[test]
fn links_text_caps_long_bucket_listings() {
    let tmp = TempDir::new().unwrap();
    let mut body = String::from("---\ntitle: Src\n---\n");
    for i in 0..30 {
        use std::fmt::Write as _;
        let _ = writeln!(body, "Line {i}: [[totally-absent-target-{i:03}-zzz]]");
    }
    write_md(tmp.path(), "src.md", &body);

    let text = links_fix_text(tmp.path(), &["--dry-run"]);
    let listed = text
        .lines()
        .filter(|l| l.contains("totally-absent-target-"))
        .count();
    assert_eq!(listed, 20, "the text listing is capped at 20:\n{text}");
    assert!(
        text.contains("and 10 more (use --format json for the full list)"),
        "the cap footer must say how many were withheld:\n{text}"
    );

    // JSON is never capped — a script still sees everything.
    let r = links_fix_json(tmp.path(), &["--dry-run"]);
    assert_eq!(
        r["unfixable_links"].as_array().map(Vec::len),
        Some(30),
        "JSON must list every unfixable link: {r}"
    );
}

/// BUG-11: a script must be able to audit every proposed fix from dry-run JSON
/// — file, both targets, strategy and confidence — without parsing text output,
/// and `--apply` must report the same detail.
#[test]
fn links_json_carries_per_fix_detail_in_dry_run_and_apply() {
    fn assert_detail(arr: &serde_json::Value, label: &str, require_non_empty: bool) {
        let items = arr
            .as_array()
            .unwrap_or_else(|| panic!("{label} is not an array: {arr}"));
        if require_non_empty {
            assert!(!items.is_empty(), "{label} must not be empty: {arr}");
        }
        for f in items {
            for key in [
                "source",
                "line",
                "old_target",
                "new_target",
                "strategy",
                "confidence",
            ] {
                assert!(!f[key].is_null(), "{label} entry lacks {key}: {f}");
            }
            assert!(
                f["confidence"]
                    .as_f64()
                    .is_some_and(|c| (0.0..=1.0).contains(&c)),
                "{label} confidence must be in [0,1]: {f}"
            );
        }
    }

    let tmp = setup_bucket_vault();

    let dry = links_fix_json(tmp.path(), &["--expand-short-form", "--dry-run"]);
    assert_detail(&dry["fixes"], "fixes", true);
    assert_detail(&dry["fuzzy_fixes"], "fuzzy_fixes", true);
    // Only populated on a case-insensitive filesystem — see setup_bucket_vault.
    assert_detail(&dry["case_mismatch_fixes"], "case_mismatch_fixes", false);
    assert_eq!(
        dry["fuzzy_fixes"][0]["strategy"], "FuzzyMatch",
        "the fuzzy proposal must name its strategy: {dry}"
    );
    assert_eq!(
        dry["fixes"][0]["strategy"], "ShortestPath",
        "the certain proposal must name its strategy: {dry}"
    );

    let applied = links_fix_json(tmp.path(), &["--expand-short-form", "--apply"]);
    assert_detail(&applied["fixes"], "fixes (apply)", true);
    assert_detail(&applied["fuzzy_fixes"], "fuzzy_fixes (apply)", true);
    assert_detail(&applied["applied_fixes"], "applied_fixes", true);
}

// ---------------------------------------------------------------------------
// iter-212: fuzzy confidence trust — scoring, default floor, honest labels
// ---------------------------------------------------------------------------

/// Build the BUG-11 shape: one relocation *inside* a section (correct) and two
/// same-prefix substitutions *across* sections (wrong). Before iter-212 the
/// two wrong proposals scored 0.9 / 0.889 and the correct one 0.6.
fn setup_confidence_corpus() -> TempDir {
    let tmp = TempDir::new().expect("tempdir creation should succeed");
    // Real destinations.
    write_md(tmp.path(), "graphql/reference/actions.md", "# GraphQL actions\n");
    write_md(
        tmp.path(),
        "code-security/code-scanning/actions-built-in-queries.md",
        "# Built-in queries\n",
    );
    write_md(
        tmp.path(),
        "code-security/how-tos/find-and-fix/configuring-larger-runners-for-default-setup.md",
        "# Larger runners\n",
    );
    // One source holding all three broken links.
    write_md(
        tmp.path(),
        "src.md",
        md!(r"
Wrong A: [a](/actions/reference/actions-limits)
Wrong B: [b](/billing/reference/actions-minute-multipliers)
Correct: [c](/code-security/how-tos/scan-code/configuring-larger-runners-for-default-setup)
"),
    );
    tmp
}

fn confidence_of(results: &serde_json::Value, needle: &str) -> f64 {
    results["fuzzy_fixes"]
        .as_array()
        .expect("fuzzy_fixes array")
        .iter()
        .find(|f| {
            f["old_target"]
                .as_str()
                .is_some_and(|t| t.ends_with(needle))
        })
        .unwrap_or_else(|| panic!("no proposal for {needle}: {results}"))["confidence"]
        .as_f64()
        .expect("confidence is a number")
}

#[test]
fn links_fix_confidence_orders_the_correct_relocation_highest() {
    let tmp = setup_confidence_corpus();
    let r = links_fix_results(tmp.path(), &[]);

    let wrong_a = confidence_of(&r, "actions-limits");
    let wrong_b = confidence_of(&r, "actions-minute-multipliers");
    let correct = confidence_of(&r, "configuring-larger-runners-for-default-setup");

    assert!(
        correct > wrong_a && correct > wrong_b,
        "the only correct proposal must score highest \
         (correct={correct} wrong_a={wrong_a} wrong_b={wrong_b}): {r}"
    );
    // And the ordering has to translate into the apply decision.
    assert!(correct >= 0.8, "correct={correct}: {r}");
    assert!(wrong_a < 0.8 && wrong_b < 0.8, "{r}");
}

#[test]
fn links_fix_bare_apply_fuzzy_respects_the_default_floor() {
    let tmp = setup_confidence_corpus();
    let applied = links_fix_results(tmp.path(), &["--apply", "--apply-fuzzy"]);

    assert_eq!(
        applied["fuzzy_min_confidence"].as_f64(),
        Some(0.8),
        "the effective floor must be reported, not left null: {applied}"
    );
    assert_eq!(
        applied["applied_fixes"].as_array().map(Vec::len),
        Some(1),
        "only the in-section relocation may be written: {applied}"
    );
    assert_eq!(
        applied["fuzzy_below_floor"].as_u64(),
        Some(2),
        "the two cross-section guesses stay reported-but-unapplied: {applied}"
    );

    // Broken count decreases monotonically and the survivors keep candidates.
    let after = links_fix_results(tmp.path(), &[]);
    assert_eq!(after["broken"].as_u64(), Some(2), "{after}");
    assert_eq!(after["unfixable"].as_u64(), Some(0), "{after}");
}

#[test]
fn links_fix_min_confidence_zero_restores_accept_everything() {
    let tmp = setup_confidence_corpus();
    let applied = links_fix_results(
        tmp.path(),
        &["--apply", "--apply-fuzzy", "--min-confidence", "0"],
    );
    assert_eq!(applied["fuzzy_min_confidence"].as_f64(), Some(0.0));
    assert_eq!(
        applied["fuzzy_below_floor"].as_u64(),
        Some(0),
        "nothing is below a zero floor: {applied}"
    );
    assert_eq!(
        applied["applied_fixes"].as_array().map(Vec::len),
        Some(3),
        "the escape hatch must write every proposal, garbage included: {applied}"
    );
}

#[test]
fn links_fix_min_confidence_near_one_applies_nothing() {
    let tmp = setup_confidence_corpus();
    let applied = links_fix_results(
        tmp.path(),
        &["--apply", "--apply-fuzzy", "--min-confidence", "0.99"],
    );
    assert_eq!(
        applied["applied_fixes"].as_array().map(Vec::len),
        Some(0),
        "{applied}"
    );
    let after = links_fix_results(tmp.path(), &[]);
    assert_eq!(after["broken"].as_u64(), Some(3), "{after}");
}

#[test]
fn links_fix_config_fuzzy_min_confidence_moves_the_floor() {
    let tmp = setup_confidence_corpus();
    fs::write(
        tmp.path().join(".hyalo.toml"),
        "[links]\nfuzzy_min_confidence = 0.99\n",
    )
    .expect("config write should succeed");

    let dry = links_fix_results(tmp.path(), &[]);
    assert_eq!(
        dry["fuzzy_min_confidence"].as_f64(),
        Some(0.99),
        "the config key must reach the report: {dry}"
    );
    // The config key moves the bar but must never opt *in* to applying.
    assert_eq!(dry["fuzzy_applied"].as_bool(), Some(false), "{dry}");

    let applied = links_fix_results(tmp.path(), &["--apply", "--apply-fuzzy"]);
    assert_eq!(
        applied["applied_fixes"].as_array().map(Vec::len),
        Some(0),
        "config floor of 0.99 rejects everything: {applied}"
    );

    // An explicit flag still wins over the config value.
    let overridden = links_fix_results(
        tmp.path(),
        &["--apply", "--apply-fuzzy", "--min-confidence", "0.8"],
    );
    assert_eq!(overridden["fuzzy_min_confidence"].as_f64(), Some(0.8));
    assert_eq!(
        overridden["applied_fixes"].as_array().map(Vec::len),
        Some(1),
        "{overridden}"
    );
}

#[test]
fn links_fix_text_distinguishes_basename_fallback_from_fuzzy() {
    let tmp = TempDir::new().expect("tempdir creation should succeed");
    write_md(tmp.path(), "notes/target.md", "# Target\n");
    write_md(
        tmp.path(),
        "src.md",
        md!(r"
Typo: [[targt]]
Relocation: [x](/wrong/place/target.md)
"),
    );

    let output = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().expect("temp path should be valid UTF-8"),
            "links",
            "fix",
            "--format",
            "text",
        ])
        .output()
        .expect("hyalo links fix should run");
    let text = String::from_utf8_lossy(&output.stdout);

    assert!(
        text.contains("[basename-fallback "),
        "a basename-fallback guess must be labelled as one: {text}"
    );
    assert!(
        text.contains("[fuzzy-match "),
        "a path-similarity guess must be labelled as one: {text}"
    );
    assert!(
        !text.contains("[fuzzy 0."),
        "the old strategy-blind `[fuzzy N]` label must be gone: {text}"
    );
    assert!(
        text.contains("below the confidence floor 0.8"),
        "the report must name the floor that suppressed a proposal: {text}"
    );
}
