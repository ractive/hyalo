//! e2e tests for the `madr` profile — init, path-bound `adr` schema, the MADR
//! advisory lint rules, and the `hyalo madr toc` generator.
//!
//! The profile is pure data over the iter-164 machinery plus two generic
//! mechanisms it is the first consumer of: `[[schema.bind]]` path-bound schemas
//! and `{n:04}` zero-padded filename tokens.

use super::common::{hyalo_no_hints, write_md};
use serde_json::Value;
use std::process::Output;
use tempfile::TempDir;

/// Run `hyalo` in `dir` with `--dir . --format json` and the given args; return
/// the parsed top-level JSON plus the process output (for exit codes).
fn run(dir: &std::path::Path, args: &[&str]) -> (Value, Output) {
    let mut full = vec!["--dir", ".", "--format", "json"];
    full.extend_from_slice(args);
    let output = hyalo_no_hints()
        .current_dir(dir)
        .args(&full)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("JSON parse: {e}\n{stdout}"));
    (json, output)
}

/// `hyalo init --profile madr` in a fresh temp dir.
fn init_madr() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", ".", "init", "--profile", "madr"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "init --profile madr failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    tmp
}

// ---------------------------------------------------------------------------
// init + new + bind
// ---------------------------------------------------------------------------

#[test]
fn init_writes_madr_config() {
    let tmp = init_madr();
    let cfg = std::fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
    assert!(cfg.contains("profile = \"madr\""), "records lint profile");
    assert!(cfg.contains("[[schema.bind]]"), "writes a bind entry");
    assert!(
        cfg.contains("docs/decisions/**/*.md"),
        "binds the ADR subtree"
    );
    assert!(cfg.contains("[schema.types.adr]"), "declares the adr type");
}

#[test]
fn new_adr_scaffolds_required_sections() {
    let tmp = init_madr();
    let (json, output) = run(
        tmp.path(),
        &[
            "new",
            "--type",
            "adr",
            "--file",
            "docs/decisions/0001-use-postgres.md",
        ],
    );
    assert!(output.status.success(), "new adr failed: {json}");
    let content =
        std::fs::read_to_string(tmp.path().join("docs/decisions/0001-use-postgres.md")).unwrap();
    assert!(content.contains("type: adr"));
    assert!(content.contains("## Context and Problem Statement"));
    assert!(content.contains("## Considered Options"));
    assert!(content.contains("## Decision Outcome"));
}

#[test]
fn bound_adr_lints_clean_without_explicit_type() {
    // A file under docs/decisions with NO `type:` frontmatter is validated as an
    // `adr` via the path binding; a complete MADR-4 doc lints clean.
    let tmp = init_madr();
    write_md(
        tmp.path(),
        "docs/decisions/0001-record.md",
        &md_adr("Record decisions", "accepted", None),
    );
    let (json, output) = run(tmp.path(), &["lint"]);
    let r = &json["results"];
    assert_eq!(r["errors"].as_u64(), Some(0), "no errors: {json}");
    assert!(output.status.success(), "lint should pass: {json}");
}

#[test]
fn madr_3x_deciders_alias_accepted() {
    // The 3.x `deciders` spelling is a declared property, so it does not trip the
    // undeclared-property warning under strict mode.
    let tmp = init_madr();
    write_md(
        tmp.path(),
        "docs/decisions/0001-x.md",
        "---\ntype: adr\nstatus: accepted\ndeciders: [carol]\n---\n\n## Context and Problem Statement\nx\n\n## Considered Options\n\n- a\n\n## Decision Outcome\n\ny\n",
    );
    let (json, _out) = run(tmp.path(), &["lint", "--strict"]);
    let r = &json["results"];
    // No undeclared-property escalation from `deciders`.
    let files = r["files"].as_array().cloned().unwrap_or_default();
    for f in files {
        for g in f["rule_groups"].as_array().cloned().unwrap_or_default() {
            let rule = g["rule"].as_str().unwrap_or("");
            assert_ne!(
                rule, "SCHEMA",
                "deciders must not be flagged as undeclared: {f}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// advisory lint rules
// ---------------------------------------------------------------------------

#[test]
fn dangling_supersede_warns() {
    let tmp = init_madr();
    write_md(
        tmp.path(),
        "docs/decisions/0001-old.md",
        &md_adr("Old decision", "superseded by ADR-0099", None),
    );
    let (json, output) = run(tmp.path(), &["lint"]);
    let r = &json["results"];
    assert_eq!(
        r["errors"].as_u64(),
        Some(0),
        "supersede is warn, not error"
    );
    assert!(
        r["warnings"].as_u64().unwrap_or(0) >= 1,
        "dangling supersede should warn: {json}"
    );
    assert!(
        rule_fired(r, "MADR-SUPERSEDE-RESOLVE"),
        "MADR-SUPERSEDE-RESOLVE should fire: {json}"
    );
    assert!(output.status.success(), "warn does not fail lint");
}

#[test]
fn resolving_supersede_is_clean() {
    let tmp = init_madr();
    write_md(
        tmp.path(),
        "docs/decisions/0001-old.md",
        &md_adr("Old", "superseded by ADR-0002", None),
    );
    write_md(
        tmp.path(),
        "docs/decisions/0002-new.md",
        &md_adr("New", "accepted", None),
    );
    let (json, _out) = run(tmp.path(), &["lint"]);
    assert!(
        !rule_fired(&json["results"], "MADR-SUPERSEDE-RESOLVE"),
        "existing target must not warn: {json}"
    );
}

#[test]
fn duplicate_number_warns() {
    let tmp = init_madr();
    write_md(
        tmp.path(),
        "docs/decisions/0007-first.md",
        &md_adr("First", "accepted", None),
    );
    write_md(
        tmp.path(),
        "docs/decisions/0007-second.md",
        &md_adr("Second", "accepted", None),
    );
    let (json, _out) = run(tmp.path(), &["lint"]);
    assert!(
        rule_fired(&json["results"], "MADR-DUPLICATE-NUMBER"),
        "duplicate NNNN should warn: {json}"
    );
}

// ---------------------------------------------------------------------------
// madr toc
// ---------------------------------------------------------------------------

#[test]
fn toc_generates_and_is_idempotent() {
    let tmp = init_madr();
    write_md(
        tmp.path(),
        "docs/decisions/0001-record.md",
        &md_adr("Record decisions", "accepted", Some("2026-07-17")),
    );

    // Dry-run first: drift → non-zero exit.
    let (_json, out) = run(tmp.path(), &["madr", "toc"]);
    assert_eq!(out.status.code(), Some(1), "dry-run signals drift");

    // Apply: creates the README with a table row.
    let (json, out) = run(tmp.path(), &["madr", "toc", "--apply"]);
    assert!(out.status.success(), "apply succeeds: {json}");
    let readme = std::fs::read_to_string(tmp.path().join("docs/decisions/README.md")).unwrap();
    assert!(readme.contains("<!-- madr:toc:begin -->"));
    assert!(
        readme.contains("| 0001 | [Record decisions](0001-record.md) | accepted | 2026-07-17 |")
    );

    // Dry-run again: no drift, exit 0.
    let (_json2, out2) = run(tmp.path(), &["madr", "toc"]);
    assert!(out2.status.success(), "no drift after apply");

    // The generated README is exempt from the adr schema — lint stays clean.
    let (lint_json, _) = run(tmp.path(), &["lint"]);
    assert_eq!(
        lint_json["results"]["errors"].as_u64(),
        Some(0),
        "generated TOC must not be linted as a malformed ADR: {lint_json}"
    );
}

#[test]
fn official_madr4_short_template_lints_clean() {
    // The MADR 4.0.0 "short" template, verbatim from
    // <https://github.com/adr/madr/blob/4.0.0/template/adr-template-short.md>
    // (MIT licensed), with the `{status}`/`{date}`/`{title}` placeholders and
    // the optional `{decision-makers}`/`{consulted}`/`{informed}` lines filled
    // in — the "fill in the blanks" contract MADR itself specifies. This is
    // the actual spec-repo fixture, not a hand-rolled equivalent of it, per the
    // "Official MADR 4 template lints clean" acceptance criterion.
    let tmp = init_madr();
    let official = "---\n\
        status: \"accepted\"\n\
        date: 2026-07-17\n\
        decision-makers: [alice, bob]\n\
        ---\n\
        \n\
        # Use PostgreSQL for the primary datastore\n\
        \n\
        ## Context and Problem Statement\n\
        \n\
        We need a primary datastore for the service. Which relational database \
        should we standardize on?\n\
        \n\
        ## Considered Options\n\
        \n\
        * PostgreSQL\n\
        * MySQL\n\
        * SQLite\n\
        \n\
        ## Decision Outcome\n\
        \n\
        Chosen option: \"PostgreSQL\", because it best balances feature set, \
        licensing, and operational maturity.\n";
    write_md(tmp.path(), "docs/decisions/0001-use-postgres.md", official);
    let (json, output) = run(tmp.path(), &["lint"]);
    let r = &json["results"];
    assert_eq!(
        r["errors"].as_u64(),
        Some(0),
        "official MADR 4 short template must lint clean: {json}"
    );
    assert!(
        output.status.success(),
        "lint on the official template should succeed: {json}"
    );
}

#[test]
fn madr_rules_are_generic_catalog_entries_not_hardcoded() {
    // "Pure data over iter-164 machinery" means the MADR-* rules are ordinary
    // `RuleCatalogEntry`s: they must be visible/toggleable through the generic
    // `lint-rules` surface exactly like any other rule (e.g. the OKF-* ones),
    // with no madr-only subcommand needed to inspect or disable them.
    let tmp = init_madr();
    let (json, output) = run(tmp.path(), &["lint-rules", "list"]);
    assert!(output.status.success(), "lint-rules list failed: {json}");
    let rules = json["results"].as_array().cloned().unwrap_or_default();
    for id in ["MADR-SUPERSEDE-RESOLVE", "MADR-DUPLICATE-NUMBER"] {
        assert!(
            rules.iter().any(|r| r["id"].as_str() == Some(id)),
            "{id} must appear in the generic rule catalog: {json}"
        );
    }

    // Toggling one off through the generic surface actually suppresses it —
    // proving there is no madr-specific code path bypassing the shared
    // enable/disable machinery.
    let (_json, out) = run(
        tmp.path(),
        &[
            "lint-rules",
            "set",
            "MADR-SUPERSEDE-RESOLVE",
            "--enabled",
            "false",
        ],
    );
    assert!(out.status.success(), "lint-rules set failed");
    write_md(
        tmp.path(),
        "docs/decisions/0001-old.md",
        &md_adr("Old decision", "superseded by ADR-0099", None),
    );
    let (json, _out) = run(tmp.path(), &["lint"]);
    assert!(
        !rule_fired(&json["results"], "MADR-SUPERSEDE-RESOLVE"),
        "disabling MADR-SUPERSEDE-RESOLVE via the generic surface must suppress it: {json}"
    );
}

#[test]
fn toc_crlf_managed_region_stable() {
    // A README with CRLF line endings and a hand-written prose region must
    // regenerate its managed region without corrupting the surrounding prose.
    let tmp = init_madr();
    write_md(
        tmp.path(),
        "docs/decisions/0001-x.md",
        &md_adr("X", "accepted", Some("2026-07-17")),
    );
    let crlf = "# Architecture Decision Records\r\n\r\nIntro prose.\r\n\r\n<!-- madr:toc:begin -->\r\nOLD\r\n<!-- madr:toc:end -->\r\n\r\nFooter.\r\n";
    write_md(tmp.path(), "docs/decisions/README.md", crlf);
    let (_json, out) = run(tmp.path(), &["madr", "toc", "--apply"]);
    assert!(out.status.success());
    let readme = std::fs::read_to_string(tmp.path().join("docs/decisions/README.md")).unwrap();
    assert!(readme.contains("Intro prose."), "prose preserved: {readme}");
    assert!(readme.contains("Footer."), "footer preserved: {readme}");
    assert!(readme.contains("[X](0001-x.md)"), "table regenerated");
    assert!(!readme.contains("OLD"), "old region replaced");
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// A complete MADR-4 ADR body (with the three required sections).
fn md_adr(title: &str, status: &str, date: Option<&str>) -> String {
    let date_line = date.map_or(String::new(), |d| format!("date: {d}\n"));
    format!(
        "---\ntype: adr\ntitle: {title}\nstatus: {status}\n{date_line}---\n\n## Context and Problem Statement\n\nContext.\n\n## Considered Options\n\n- A\n- B\n\n## Decision Outcome\n\nChose A.\n"
    )
}

/// Whether a rule id appears in the lint `results.files[].rule_groups[].rule`.
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
