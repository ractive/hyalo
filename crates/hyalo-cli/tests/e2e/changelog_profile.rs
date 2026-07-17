//! e2e tests for the `changelog` profile — init, the path-bound frontmatter-less
//! `changelog` type, the Keep a Changelog 1.1.0 grammar lint rules, and the
//! `hyalo changelog release` / `hyalo changelog add` generators.

use super::common::hyalo_no_hints;
use serde_json::Value;
use std::process::Output;
use tempfile::TempDir;

/// Run `hyalo` in `dir` with `--dir . --format json` and the given args.
fn run(dir: &std::path::Path, args: &[&str]) -> (Value, Output) {
    let mut full = vec!["--dir", ".", "--format", "json"];
    full.extend_from_slice(args);
    let output = hyalo_no_hints()
        .current_dir(dir)
        .args(&full)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    // A user error may be emitted on stderr with empty stdout; callers that
    // expect failure assert on the exit code, not the JSON, so tolerate that.
    let json: Value = if stdout.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("JSON parse: {e}\n{stdout}"))
    };
    (json, output)
}

/// `hyalo init --profile changelog` in a fresh temp dir.
fn init_changelog() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", ".", "init", "--profile", "changelog"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "init --profile changelog failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    tmp
}

/// The Keep a Changelog reference example (a clean changelog).
const REFERENCE: &str = "\
# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added

- New thing.

## [1.1.0] - 2023-03-05

### Added

- Something.

### Fixed

- A bug.

## [1.0.0] - 2017-06-20

### Added

- First release.

[Unreleased]: https://example.com/compare/v1.1.0...HEAD
[1.1.0]: https://example.com/compare/v1.0.0...v1.1.0
[1.0.0]: https://example.com/releases/tag/v1.0.0
";

fn write_changelog(root: &std::path::Path, content: &str) {
    std::fs::write(root.join("CHANGELOG.md"), content).unwrap();
}

/// Whether a rule id appears in the lint results.
fn rule_fired(results: &Value, rule_id: &str) -> bool {
    let Some(files) = results["files"].as_array() else {
        return false;
    };
    files.iter().any(|f| {
        f["rule_groups"]
            .as_array()
            .is_some_and(|gs| gs.iter().any(|g| g["rule"].as_str() == Some(rule_id)))
    })
}

// ---------------------------------------------------------------------------
// init + bind
// ---------------------------------------------------------------------------

#[test]
fn init_writes_changelog_config() {
    let tmp = init_changelog();
    let cfg = std::fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
    assert!(
        cfg.contains("profile = \"changelog\""),
        "records lint profile"
    );
    assert!(cfg.contains("[[schema.bind]]"), "writes a bind entry");
    assert!(
        cfg.contains("\"CHANGELOG.md\""),
        "binds/exempts CHANGELOG.md"
    );
}

// ---------------------------------------------------------------------------
// lint grammar
// ---------------------------------------------------------------------------

#[test]
fn reference_example_lints_clean() {
    let tmp = init_changelog();
    write_changelog(tmp.path(), REFERENCE);
    let (json, output) = run(tmp.path(), &["lint", "--profile", "changelog"]);
    assert!(
        output.status.success(),
        "reference changelog must lint clean: {json}"
    );
    for id in [
        "CHANGELOG-TITLE",
        "CHANGELOG-VERSION-HEADING",
        "CHANGELOG-CATEGORY",
        "CHANGELOG-VERSION-ORDER",
        "CHANGELOG-DATE-ORDER",
        "CHANGELOG-LINK-REF",
    ] {
        assert!(
            !rule_fired(&json["results"], id),
            "{id} must not fire on the clean reference"
        );
    }
}

#[test]
fn wrong_title_fails_lint() {
    let tmp = init_changelog();
    write_changelog(
        tmp.path(),
        "# Change Log\n\n## [1.0.0] - 2020-01-01\n\n### Added\n\n- x.\n\n[1.0.0]: u\n",
    );
    let (json, output) = run(tmp.path(), &["lint", "--profile", "changelog"]);
    assert!(
        rule_fired(&json["results"], "CHANGELOG-TITLE"),
        "wrong title flagged: {json}"
    );
    assert!(
        !output.status.success(),
        "an error-severity grammar violation fails lint"
    );
}

#[test]
fn unknown_category_fails_lint() {
    let tmp = init_changelog();
    write_changelog(
        tmp.path(),
        "# Changelog\n\n## [1.0.0] - 2020-01-01\n\n### Improved\n\n- x.\n\n[1.0.0]: u\n",
    );
    let (json, _) = run(tmp.path(), &["lint", "--profile", "changelog"]);
    assert!(rule_fired(&json["results"], "CHANGELOG-CATEGORY"), "{json}");
}

#[test]
fn out_of_order_versions_fail_lint() {
    let tmp = init_changelog();
    write_changelog(
        tmp.path(),
        "# Changelog\n\n## [1.0.0] - 2020-01-01\n\n### Added\n\n- x.\n\n## [2.0.0] - 2019-01-01\n\n### Added\n\n- y.\n\n[1.0.0]: a\n[2.0.0]: b\n",
    );
    let (json, _) = run(tmp.path(), &["lint", "--profile", "changelog"]);
    assert!(
        rule_fired(&json["results"], "CHANGELOG-VERSION-ORDER"),
        "{json}"
    );
}

#[test]
fn missing_link_ref_warns_but_passes() {
    let tmp = init_changelog();
    // No footer link refs → CHANGELOG-LINK-REF (warn) fires but lint still passes.
    write_changelog(
        tmp.path(),
        "# Changelog\n\n## [1.0.0] - 2020-01-01\n\n### Added\n\n- x.\n",
    );
    let (json, output) = run(tmp.path(), &["lint", "--profile", "changelog"]);
    assert!(rule_fired(&json["results"], "CHANGELOG-LINK-REF"), "{json}");
    assert!(output.status.success(), "a warn does not fail lint");
}

#[test]
fn rule_is_toggleable() {
    let tmp = init_changelog();
    write_changelog(
        tmp.path(),
        "# Change Log\n\n## [1.0.0] - 2020-01-01\n\n### Added\n\n- x.\n\n[1.0.0]: u\n",
    );
    // Disable the title rule; it must no longer fire.
    let (_, out) = run(
        tmp.path(),
        &["lint-rules", "set", "CHANGELOG-TITLE", "--enabled", "false"],
    );
    assert!(out.status.success(), "lint-rules set failed");
    let (json, _) = run(tmp.path(), &["lint", "--profile", "changelog"]);
    assert!(
        !rule_fired(&json["results"], "CHANGELOG-TITLE"),
        "disabled rule must not fire: {json}"
    );
}

#[test]
fn lint_rules_list_includes_changelog() {
    let tmp = init_changelog();
    let (json, output) = run(tmp.path(), &["lint-rules", "list"]);
    assert!(output.status.success(), "lint-rules list failed: {json}");
    let text = json.to_string();
    assert!(
        text.contains("CHANGELOG-TITLE"),
        "catalog lists changelog rules"
    );
    assert!(text.contains("CHANGELOG-VERSION-ORDER"));
}

// ---------------------------------------------------------------------------
// release generator round-trip
// ---------------------------------------------------------------------------

#[test]
fn release_rotation_round_trip() {
    let tmp = init_changelog();
    write_changelog(
        tmp.path(),
        "# Changelog\n\n## [Unreleased]\n\n### Added\n\n- Feature A.\n\n## [1.0.0] - 2020-01-01\n\n### Added\n\n- Initial.\n\n[Unreleased]: https://x/compare/v1.0.0...HEAD\n[1.0.0]: https://x/tag/v1.0.0\n",
    );

    // Dry-run first: drift → non-zero exit.
    let (_, dry) = run(
        tmp.path(),
        &["changelog", "release", "1.1.0", "--date", "2026-07-17"],
    );
    assert_eq!(dry.status.code(), Some(1), "dry-run signals drift");

    // Apply the rotation.
    let (json, out) = run(
        tmp.path(),
        &[
            "changelog",
            "release",
            "1.1.0",
            "--date",
            "2026-07-17",
            "--apply",
        ],
    );
    assert!(out.status.success(), "apply succeeds: {json}");
    assert_eq!(json["results"]["version"].as_str(), Some("1.1.0"));

    let content = std::fs::read_to_string(tmp.path().join("CHANGELOG.md")).unwrap();
    assert!(
        content.contains("## [1.1.0] - 2026-07-17"),
        "dated section: {content}"
    );
    assert!(content.contains("## [Unreleased]"), "fresh unreleased kept");
    // Feature A moved into the dated section.
    let dated = content.find("## [1.1.0]").unwrap();
    assert!(content[dated..].contains("Feature A."));
    assert!(
        content.contains("[1.1.0]: TBD"),
        "placeholder link ref added"
    );

    // The rotated changelog still lints clean (once the TBD link is treated as a
    // present ref — the LINK-REF rule only checks presence, not the URL value).
    let (lint_json, lint_out) = run(tmp.path(), &["lint", "--profile", "changelog"]);
    assert!(
        lint_out.status.success(),
        "rotated changelog lints clean: {lint_json}"
    );

    // Releasing the same version again is refused.
    let (dup, dup_out) = run(tmp.path(), &["changelog", "release", "1.1.0", "--apply"]);
    assert!(
        !dup_out.status.success(),
        "duplicate release refused: {dup}"
    );
}

#[test]
fn add_appends_under_unreleased() {
    let tmp = init_changelog();
    write_changelog(
        tmp.path(),
        "# Changelog\n\n## [Unreleased]\n\n## [1.0.0] - 2020-01-01\n\n### Added\n\n- Initial.\n\n[Unreleased]: h\n[1.0.0]: a\n",
    );
    let (json, out) = run(
        tmp.path(),
        &[
            "changelog",
            "add",
            "--category",
            "Fixed",
            "--message",
            "A bug",
            "--apply",
        ],
    );
    assert!(out.status.success(), "add succeeds: {json}");
    let content = std::fs::read_to_string(tmp.path().join("CHANGELOG.md")).unwrap();
    let unrel = content.find("## [Unreleased]").unwrap();
    let v1 = content.find("## [1.0.0]").unwrap();
    let seg = &content[unrel..v1];
    assert!(
        seg.contains("### Fixed"),
        "category created under Unreleased"
    );
    assert!(seg.contains("- A bug"), "entry added");
    assert!(!content[v1..].contains("A bug"), "not added under 1.0.0");
}

#[test]
fn add_rejects_unknown_category() {
    let tmp = init_changelog();
    write_changelog(tmp.path(), "# Changelog\n\n## [Unreleased]\n");
    let (json, out) = run(
        tmp.path(),
        &[
            "changelog",
            "add",
            "--category",
            "Bogus",
            "--message",
            "x",
            "--apply",
        ],
    );
    assert!(!out.status.success(), "unknown category rejected: {json}");
}

#[test]
fn release_rejects_bad_version() {
    let tmp = init_changelog();
    write_changelog(
        tmp.path(),
        "# Changelog\n\n## [Unreleased]\n\n[Unreleased]: h\n",
    );
    let (json, out) = run(tmp.path(), &["changelog", "release", "v1.2", "--apply"]);
    assert!(!out.status.success(), "non-semver version rejected: {json}");
}

// ---------------------------------------------------------------------------
// dogfood: the bundled reference lints clean under the shipped profile
// ---------------------------------------------------------------------------

#[test]
fn reference_lints_clean_via_ephemeral_overlay() {
    // No `.hyalo.toml` at all — the `--profile changelog` overlay must work on a
    // bare directory (CI / third-party changelog scenario).
    let tmp = TempDir::new().unwrap();
    write_changelog(tmp.path(), REFERENCE);
    let (json, output) = run(tmp.path(), &["lint", "--profile", "changelog"]);
    assert!(
        output.status.success(),
        "reference lints clean with ephemeral overlay (no config): {json}"
    );
}
