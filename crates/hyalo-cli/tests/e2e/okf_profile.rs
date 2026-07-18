//! e2e tests for `hyalo lint --profile okf` — the OKF §9 conformance profile.
//!
//! The profile is an *ephemeral overlay*: it merges the same fragment that
//! `hyalo init --profile okf` materializes, so it works with no `.hyalo.toml`
//! present (CI / third-party bundles) and is idempotent on a vault already
//! initialized that way. SPEC §9 errors only on missing frontmatter / missing
//! `type`; everything else (reserved-file structure, broken links, citations)
//! is warn — never a rejection.

use super::common::{hyalo_no_hints, write_md};
use serde_json::Value;
use std::process::Output;
use tempfile::TempDir;

/// Run `hyalo lint --profile okf --format json` in `dir` and return the parsed
/// `results` object plus the process output (for exit-code assertions).
fn lint_okf(dir: &std::path::Path, extra: &[&str]) -> (Value, Output) {
    let mut args = vec!["--dir", ".", "--format", "json", "lint", "--profile", "okf"];
    args.extend_from_slice(extra);
    let output = hyalo_no_hints()
        .current_dir(dir)
        .args(&args)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("JSON parse: {e}\n{stdout}"));
    (json["results"].clone(), output)
}

/// Build a fully-conformant OKF bundle with NO `.hyalo.toml` (exercises the
/// ephemeral overlay).
fn conformant_bundle() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    write_md(
        dir,
        "tables/blocks.md",
        "---\ntype: BigQuery Table\ntitle: Blocks\ndescription: The blocks table.\n---\n# Schema\n\nColumns.\n\n# Citations\n\n- [Wiki](https://en.wikipedia.org/wiki/Bitcoin)\n- [Ref](../references/wiki.md)\n",
    );
    write_md(
        dir,
        "references/wiki.md",
        "---\ntype: Reference\ntitle: Bitcoin Wiki\n---\nOverview.\n",
    );
    write_md(
        dir,
        "index.md",
        "---\nokf_version: \"0.1\"\n---\n\n<!-- okf:index:begin -->\n* [Blocks](tables/blocks.md) - The blocks table.\n<!-- okf:index:end -->\n",
    );
    write_md(
        dir,
        "log.md",
        "# Changelog\n\n## 2026-07-17\n\n- **Added** the blocks table.\n",
    );
    tmp
}

// ---------------------------------------------------------------------------
// AC: all sample bundles report conformant (0 errors) under --profile okf
// ---------------------------------------------------------------------------

#[test]
fn conformant_bundle_has_zero_errors() {
    let tmp = conformant_bundle();
    let (results, output) = lint_okf(tmp.path(), &[]);
    assert_eq!(
        results["errors"].as_u64().unwrap(),
        0,
        "conformant bundle must have 0 errors: {results}"
    );
    assert!(
        output.status.success(),
        "conformant bundle must exit 0: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ---------------------------------------------------------------------------
// AC: missing type -> error; broken link -> warn (not error); unknown type -> clean
// ---------------------------------------------------------------------------

#[test]
fn missing_type_is_an_error() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "tables/notype.md",
        "---\ntitle: No Type\n---\n# Body\n",
    );
    let (results, output) = lint_okf(tmp.path(), &[]);
    assert!(
        results["errors"].as_u64().unwrap() >= 1,
        "missing type must be an error: {results}"
    );
    assert_eq!(output.status.code(), Some(1), "must exit 1 on error");
}

#[test]
fn broken_citation_link_is_a_warning_not_error() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "tables/x.md",
        "---\ntype: Reference\ntitle: X\n---\n# Citations\n\n- [Missing](does-not-exist.md)\n",
    );
    let (results, output) = lint_okf(tmp.path(), &[]);
    assert_eq!(
        results["errors"].as_u64().unwrap(),
        0,
        "broken link must not error (permissive model): {results}"
    );
    assert!(
        results["warnings"].as_u64().unwrap() >= 1,
        "broken citation link must warn: {results}"
    );
    assert!(output.status.success(), "warnings-only must exit 0");
}

#[test]
fn unknown_type_is_clean() {
    let tmp = TempDir::new().unwrap();
    // Unknown `type` + extra key must NOT be rejected (permissive consumption).
    write_md(
        tmp.path(),
        "tables/weird.md",
        "---\ntype: SomethingNobodyDeclared\ntitle: Weird\nextra_key: allowed\n---\n# Schema\n\nx\n\n# Citations\n\n- [a](https://example.com)\n",
    );
    let (results, output) = lint_okf(tmp.path(), &[]);
    assert_eq!(
        results["errors"].as_u64().unwrap(),
        0,
        "unknown type / extra keys must be clean: {results}"
    );
    assert!(output.status.success());
}

#[test]
fn missing_frontmatter_is_an_error() {
    let tmp = TempDir::new().unwrap();
    // A non-reserved .md with no frontmatter block at all (SPEC §9 rule 1).
    write_md(
        tmp.path(),
        "tables/raw.md",
        "# Just a body, no frontmatter\n",
    );
    let (results, output) = lint_okf(tmp.path(), &[]);
    assert!(
        results["errors"].as_u64().unwrap() >= 1,
        "missing frontmatter must error: {results}"
    );
    assert_eq!(output.status.code(), Some(1));
}

// ---------------------------------------------------------------------------
// Reserved-file structure (warn) — including CRLF fixtures
// ---------------------------------------------------------------------------

#[test]
fn index_without_link_list_warns() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "index.md",
        "---\nokf_version: \"0.1\"\n---\nThis is prose with no link list at all.\n",
    );
    let (results, output) = lint_okf(tmp.path(), &["--rule", "OKF-INDEX-STRUCTURE"]);
    assert_eq!(results["errors"].as_u64().unwrap(), 0);
    assert!(
        results["warnings"].as_u64().unwrap() >= 1,
        "non-link-list index must warn: {results}"
    );
    assert!(output.status.success());
}

#[test]
fn index_dangling_marker_warns_via_okf_index_markers_rule() {
    // A dangling begin marker (no end) in a reserved index.md should be flagged
    // by OKF-INDEX-MARKERS so CI surfaces the precondition the generator would
    // otherwise skip on (iter-176 BUG-3 companion rule).
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "index.md",
        "---\nokf_version: \"0.1\"\n---\n* [X](x.md)\n\n<!-- okf:index:begin -->\nlist\n",
    );
    let (results, output) = lint_okf(tmp.path(), &["--rule", "OKF-INDEX-MARKERS"]);
    assert_eq!(results["errors"].as_u64().unwrap(), 0, "warn, not error");
    assert!(
        results["warnings"].as_u64().unwrap() >= 1,
        "dangling marker must warn: {results}"
    );
    assert!(output.status.success(), "advisory rule does not fail exit");
}

#[test]
fn index_healthy_markers_pass_okf_index_markers_rule() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "index.md",
        "---\nokf_version: \"0.1\"\n---\n# Index\n\n<!-- okf:index:begin -->\n* [X](x.md)\n<!-- okf:index:end -->\n",
    );
    let (results, _) = lint_okf(tmp.path(), &["--rule", "OKF-INDEX-MARKERS"]);
    assert_eq!(
        results["warnings"].as_u64().unwrap(),
        0,
        "a healthy pair must not warn: {results}"
    );
}

#[test]
fn log_out_of_order_warns() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "log.md",
        "## 2026-07-10\n\n- older\n\n## 2026-07-17\n\n- newer (out of order)\n",
    );
    let (results, _) = lint_okf(tmp.path(), &["--rule", "OKF-LOG-STRUCTURE"]);
    assert!(
        results["warnings"].as_u64().unwrap() >= 1,
        "out-of-order log must warn: {results}"
    );
}

#[test]
fn crlf_reserved_files_are_conformant() {
    // iter-165 retrospective: CRLF is a recurring blind spot in new okf code.
    // A CRLF-terminated log.md/index.md must lint identically to LF.
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "index.md",
        "---\r\nokf_version: \"0.1\"\r\n---\r\n* [X](x.md) - a concept\r\n",
    );
    write_md(
        tmp.path(),
        "log.md",
        "# Changelog\r\n\r\n## 2026-07-17\r\n\r\n- **Added** x.\r\n",
    );
    let (results, output) = lint_okf(tmp.path(), &["--rule-prefix", "OKF"]);
    assert_eq!(
        results["errors"].as_u64().unwrap(),
        0,
        "CRLF reserved files must not error: {results}"
    );
    assert_eq!(
        results["warnings"].as_u64().unwrap(),
        0,
        "CRLF reserved files must have no structure warnings: {results}"
    );
    assert!(output.status.success());
}

// ---------------------------------------------------------------------------
// Citation rules
// ---------------------------------------------------------------------------

#[test]
fn concept_without_citations_warns() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "tables/claim.md",
        "---\ntype: BigQuery Table\ntitle: Claim\n---\n# Schema\n\nA factual claim with no citations.\n",
    );
    let (results, _) = lint_okf(tmp.path(), &["--rule", "OKF-CITATIONS-PRESENT"]);
    assert!(
        results["warnings"].as_u64().unwrap() >= 1,
        "claim-bearing doc without # Citations must warn: {results}"
    );
}

#[test]
fn prose_citation_entry_warns() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "tables/c.md",
        "---\ntype: BigQuery Table\ntitle: C\n---\n# Citations\n\nSee the whitepaper somewhere.\n",
    );
    let (results, _) = lint_okf(tmp.path(), &["--rule", "OKF-CITATIONS-WELL-FORMED"]);
    assert!(
        results["warnings"].as_u64().unwrap() >= 1,
        "free-prose citation must warn: {results}"
    );
}

#[test]
fn numbered_and_bullet_citations_both_accepted() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "tables/c.md",
        "---\ntype: BigQuery Table\ntitle: C\n---\n# Citations\n\n1. [Spec](https://example.com/spec)\n- [Wiki](https://example.com/wiki)\n",
    );
    let (results, _) = lint_okf(tmp.path(), &["--rule", "OKF-CITATIONS-WELL-FORMED"]);
    assert_eq!(
        results["warnings"].as_u64().unwrap(),
        0,
        "numbered AND bullet citation lists must both be accepted: {results}"
    );
}

// ---------------------------------------------------------------------------
// Idempotence: init'd okf vault -> plain lint == lint --profile okf
// ---------------------------------------------------------------------------

#[test]
fn overlay_is_idempotent_on_initialized_vault() {
    let tmp = TempDir::new().unwrap();
    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["init", "--profile", "okf", "--dir", "."])
        .output()
        .unwrap();
    // A bundle with a mix of clean + warn-worthy content.
    write_md(
        tmp.path(),
        "tables/blocks.md",
        "---\ntype: BigQuery Table\ntitle: Blocks\n---\n# Schema\n\nx\n",
    );
    write_md(
        tmp.path(),
        "tables/notype.md",
        "---\ntitle: No Type\n---\nBody\n",
    );

    let plain = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--format", "json", "lint"])
        .output()
        .unwrap();
    let with_profile = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--format", "json", "lint", "--profile", "okf"])
        .output()
        .unwrap();

    let p: Value = serde_json::from_str(&String::from_utf8_lossy(&plain.stdout)).unwrap();
    let w: Value = serde_json::from_str(&String::from_utf8_lossy(&with_profile.stdout)).unwrap();
    assert_eq!(
        p["results"]["errors"], w["results"]["errors"],
        "errors must match: plain={p}\nprofile={w}"
    );
    assert_eq!(
        p["results"]["warnings"], w["results"]["warnings"],
        "warnings must match"
    );
    assert_eq!(
        p["results"]["rules_fired"], w["results"]["rules_fired"],
        "rules_fired must match"
    );
    assert_eq!(
        plain.status.code(),
        with_profile.status.code(),
        "exit codes must match"
    );
}

#[test]
fn okf_rule_is_individually_toggleable() {
    let tmp = TempDir::new().unwrap();
    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["init", "--profile", "okf", "--dir", "."])
        .output()
        .unwrap();
    write_md(
        tmp.path(),
        "claim.md",
        "---\ntype: BigQuery Table\ntitle: Claim\n---\n# Schema\n\nA claim, no citations.\n",
    );

    let warns = |dir: &std::path::Path| -> u64 {
        let out = hyalo_no_hints()
            .current_dir(dir)
            .args(["--format", "json", "lint"])
            .output()
            .unwrap();
        let json: Value = serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
        json["results"]["warnings"].as_u64().unwrap()
    };

    let before = warns(tmp.path());
    assert!(before >= 1, "expected the citations-present warning");

    // Disable the rule via `lint-rules set` — a real override must be written and
    // honored by the profile runtime.
    hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "lint-rules",
            "set",
            "OKF-CITATIONS-PRESENT",
            "--enabled",
            "false",
        ])
        .output()
        .unwrap();

    let after = warns(tmp.path());
    assert!(
        after < before,
        "disabling OKF-CITATIONS-PRESENT must reduce warnings: before={before} after={after}"
    );
}

// ---------------------------------------------------------------------------
// Unknown profile is a clean user error
// ---------------------------------------------------------------------------

#[test]
fn unknown_profile_errors_cleanly() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "---\ntype: x\n---\nBody\n");
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", ".", "lint", "--profile", "nonesuch"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("unknown profile 'nonesuch'") && combined.contains("okf"),
        "must name the unknown profile and list available ones: {combined}"
    );
}

// ---------------------------------------------------------------------------
// Composition: --profile okf works with --files-from
// ---------------------------------------------------------------------------

#[test]
fn profile_composes_with_files_from_stdin() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "tables/bad.md",
        "---\ntitle: no type\n---\nBody\n",
    );
    write_md(
        tmp.path(),
        "tables/ok.md",
        "---\ntype: Reference\ntitle: OK\n---\nBody\n",
    );
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "--dir",
            ".",
            "--format",
            "json",
            "lint",
            "--profile",
            "okf",
            "--files-from",
            "-",
        ])
        .write_stdin("tables/bad.md\n")
        .output()
        .unwrap();
    let json: Value = serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).unwrap();
    assert!(
        json["results"]["errors"].as_u64().unwrap() >= 1,
        "the bad file scoped via --files-from must error: {json}"
    );
    // ok.md was not in the list, so it should not be checked.
    assert_eq!(json["results"]["files_checked"].as_u64().unwrap(), 1);
}
