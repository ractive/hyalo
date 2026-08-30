//! Cross-command `results` envelope contract (iter-216).
//!
//! The per-command suites each assert their own payload. These tests assert
//! the rules that hold *across* commands, so a new command (or a refactor of
//! an existing one) cannot quietly reintroduce the drift iter-216 removed:
//!
//! - **R1** — the envelope owns `total`. A `total` inside `results` always
//!   means "items the command considered" (a denominator), never a count of
//!   findings. Findings get a name of their own (`violations`, `matched`).
//! - **R2** — top-level `results` keys are always present, including `0`,
//!   `false`, `[]` and `null`.
//! - **R3/R4** — one concept, one key name: every object-shaped mutation
//!   result reports `dry_run`, the bulk-mutation family also reports
//!   `skipped_count`, and `files_with_violations` means the same thing in
//!   `lint` and in `summary`.
//!
//! iter-256 (COH-9) narrowed R3/R4's wording to match reality and widened the
//! `dry_run` coverage to match the wording. Two documented exceptions remain,
//! and both are structural rather than drift:
//!
//! - `task toggle` / `task set` return an *array* of per-task records, so
//!   there is no top-level object to hang `dry_run` on (and a one-element
//!   payload collapses to a bare object, so the shape is not even stable).
//!   Their dry-run records carry `old_status`; their applied records do not — that key is the
//!   discriminator. Adding `dry_run` to every element would repeat one
//!   invocation-level fact N times and would pollute `TaskReadResult`, which
//!   `task read` (a read-only command) shares.
//! - `skipped_count` is bulk-family-only (`set`, `remove`, `append`,
//!   `properties rename`, `tags rename`). A single-target command has no
//!   scanned-but-unchanged set, so a hard-coded `0` would look like a
//!   measurement and be none.
//!
//! The written inventory these rules come from lives at
//! `hyalo-knowledgebase/research/results-json-shape-inventory.md`.

use super::common::{hyalo_no_hints, write_md};
use std::path::Path;
use tempfile::TempDir;

/// Run `hyalo <args>` in `dir` and return the `results` payload.
fn results(dir: &Path, args: &[&str]) -> serde_json::Value {
    let output = hyalo_no_hints()
        .current_dir(dir)
        .args(args)
        .args(["--format", "json"])
        .output()
        .expect("hyalo should run");
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "{args:?} did not print an envelope: {e}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    envelope["results"].clone()
}

/// A two-file vault: `guide.md` mentions `alpha.md`'s title twice in prose,
/// so `links auto` has proposals and `links fix` has nothing broken.
fn vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "alpha.md",
        "---\ntitle: Alpha\n---\n\n# Alpha\n",
    );
    write_md(
        tmp.path(),
        "guide.md",
        "---\ntitle: Guide\n---\n\n# Guide\n\nSee Alpha for details. Alpha again.\n",
    );
    tmp
}

// ---------------------------------------------------------------------------
// R1: `results.total` is a denominator, never a finding count
// ---------------------------------------------------------------------------

#[test]
fn lint_names_its_finding_count_violations_not_total() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(
        tmp.path().join(".hyalo.toml"),
        "[schema.default]\nrequired = [\"title\", \"type\"]\n",
    )
    .unwrap();
    write_md(tmp.path(), "a.md", "---\ntitle: A\n---\nBody\n");

    let r = results(tmp.path(), &["lint"]);
    assert!(
        r.get("total").is_none(),
        "lint must not restate a finding count as `results.total`: {r}"
    );
    let violations = r["violations"]
        .as_u64()
        .unwrap_or_else(|| panic!("lint must report `violations`: {r}"));
    assert!(violations > 0, "the fixture is missing `type`: {r}");
    // The two other run-level counters must reconcile against it.
    assert_eq!(
        violations,
        r["errors"].as_u64().unwrap() + r["warnings"].as_u64().unwrap(),
        "violations must equal errors + warnings: {r}"
    );
}

#[test]
fn links_auto_names_its_proposal_count_matched_not_total() {
    let tmp = vault();

    let r = results(tmp.path(), &["links", "auto"]);
    assert!(
        r.get("total").is_none(),
        "links auto must not restate its proposal count as `results.total`: {r}"
    );
    assert_eq!(
        r["matched"].as_u64(),
        Some(r["matches"].as_array().unwrap().len() as u64),
        "`matched` must be the length of `matches`: {r}"
    );
    // `scanned` is the denominator on this command, and it is a different
    // number from `matched` — which is exactly why one of them cannot be
    // called `total`.
    assert_eq!(r["scanned"].as_u64(), Some(2), "{r}");
}

#[test]
fn mutation_total_stays_the_denominator() {
    let tmp = vault();

    let r = results(
        tmp.path(),
        &["set", "guide.md", "--property", "status=draft"],
    );
    let modified = r["modified"].as_array().unwrap().len() as u64;
    let skipped = r["skipped"].as_array().unwrap().len() as u64;
    assert_eq!(
        r["total"].as_u64(),
        Some(modified + skipped),
        "`results.total` on a mutation is modified + skipped: {r}"
    );
}

// ---------------------------------------------------------------------------
// R2 / R3 / R4: uniform keys across the mutating commands
// ---------------------------------------------------------------------------

/// iter-256 COH-9: the object-shaped mutation results that are *not* part of
/// the `--file/--glob` bulk family. Before iter-256 these reported the
/// write/preview distinction under four different spellings — `applied`
/// (`mv` batch), `apply` (`madr`/`okf`/`changelog`), `dry_run`, or nothing at
/// all (`new`, `types remove`) — so no single jq query answered "did this
/// write?". `apply`/`applied` are retained for back-compat; `dry_run` is now
/// emitted alongside them and is always their exact inverse.
#[test]
fn generators_and_scaffolds_report_dry_run_as_a_bool() {
    let tmp = vault();
    let dir = tmp.path();
    std::fs::create_dir_all(dir.join("docs/decisions")).unwrap();
    write_md(
        dir,
        "docs/decisions/0001-first.md",
        "---\ntitle: First\ntype: decision\nstatus: accepted\n---\n\n# First\n",
    );
    std::fs::write(
        dir.join("CHANGELOG.md"),
        "# Changelog\n\n## [Unreleased]\n\n### Added\n\n- seed\n",
    )
    .unwrap();

    // `new` needs a declared type; `types set` is itself one of the cases.
    let cases: Vec<Vec<&str>> = vec![
        vec!["types", "set", "note", "--required", "title"],
        vec!["new", "--type", "note", "--file", "scratch.md"],
        vec!["types", "remove", "note"],
        vec!["lint-rules", "set", "MD013", "--enabled", "false"],
        vec!["lint-rules", "remove", "MD013"],
        vec!["madr", "toc"],
        vec!["okf", "index"],
        vec!["okf", "log", "--message", "hello"],
        vec!["changelog", "add", "--category", "Added", "--message", "thing"],
        vec!["changelog", "release", "1.0.0"],
        // Batch `mv` runs last: it relocates the files the other cases read.
        vec!["mv", "--glob", "*.md", "--to", "sub"],
    ];

    for args in cases {
        let r = results(dir, &args);
        let dry_run = r
            .get("dry_run")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or_else(|| panic!("{args:?} must report `dry_run` as a bool: {r}"));
        // Where the legacy inverse is still emitted, the two must agree.
        for legacy in ["apply", "applied"] {
            if let Some(v) = r.get(legacy).and_then(serde_json::Value::as_bool) {
                assert_eq!(
                    v, !dry_run,
                    "{args:?}: `{legacy}` must be the exact inverse of `dry_run`: {r}"
                );
            }
        }
    }
}

/// iter-256 COH-9: `task toggle` / `task set` are the documented exception —
/// an array payload with no top-level object. This test pins the discriminator
/// the docs point at (`old_status` present only on a dry run) so it cannot
/// drift without a test failing.
#[test]
fn task_mutations_signal_dry_run_through_old_status() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "todo.md",
        "---\ntitle: Todo\n---\n\n# Todo\n\n- [ ] first\n",
    );

    // `task` collapses a one-element payload to a bare object, so normalise.
    let first = |v: &serde_json::Value| -> serde_json::Value {
        v.as_array().map_or_else(|| v.clone(), |a| a[0].clone())
    };

    let preview = results(tmp.path(), &["task", "toggle", "todo.md", "--all", "--dry-run"]);
    assert!(
        first(&preview).get("old_status").is_some(),
        "a dry-run task record must carry `old_status`: {preview}"
    );

    let applied = results(tmp.path(), &["task", "toggle", "todo.md", "--all"]);
    assert!(
        first(&applied).get("old_status").is_none(),
        "an applied task record must not carry `old_status`: {applied}"
    );
}

#[test]
fn every_mutating_command_reports_dry_run_as_a_bool() {
    let tmp = vault();

    // `mv` is run last so it does not move a file the other cases read.
    let cases: Vec<Vec<&str>> = vec![
        vec!["set", "guide.md", "--property", "status=draft"],
        vec!["append", "guide.md", "--property", "related=alpha"],
        vec!["remove", "guide.md", "--property", "status"],
        vec!["properties", "rename", "--from", "related", "--to", "links"],
        vec!["tags", "rename", "--from", "nope", "--to", "still-nope"],
        vec!["links", "auto"],
        vec!["links", "fix"],
        vec!["lint", "--fix", "--dry-run"],
        vec!["lint"],
        vec!["mv", "alpha.md", "beta.md"],
    ];

    for args in cases {
        let r = results(tmp.path(), &args);
        assert!(
            r.get("dry_run")
                .and_then(serde_json::Value::as_bool)
                .is_some(),
            "{args:?} must report `dry_run` as a bool (present even when false): {r}"
        );
    }
}

#[test]
fn every_property_mutation_reports_skipped_count() {
    let tmp = vault();

    let cases: Vec<Vec<&str>> = vec![
        vec!["set", "--glob", "**/*.md", "--property", "status=draft"],
        vec!["append", "--glob", "**/*.md", "--property", "related=alpha"],
        vec!["remove", "--glob", "**/*.md", "--property", "status"],
        vec!["properties", "rename", "--from", "related", "--to", "links"],
        vec!["tags", "rename", "--from", "nope", "--to", "still-nope"],
    ];

    for args in cases {
        let r = results(tmp.path(), &args);
        let count = r["skipped_count"]
            .as_u64()
            .unwrap_or_else(|| panic!("{args:?} must report `skipped_count`: {r}"));
        // Where the list is emitted too, the two must agree.
        if let Some(list) = r["skipped"].as_array() {
            assert_eq!(
                count,
                list.len() as u64,
                "{args:?}: skipped_count must equal skipped.len(): {r}"
            );
        }
    }
}

#[test]
fn links_dry_run_distinguishes_preview_from_an_apply_with_no_work() {
    // With every title below --min-length there is no title inventory to scan
    // against, so `links auto` short-circuits and reports `applied: false`
    // even under `--apply`. `dry_run` is what tells the two runs apart
    // (iter-216 D-4).
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "solo.md",
        "---\ntitle: Ab\n---\n\nNo mentions.\n",
    );

    let preview = results(tmp.path(), &["links", "auto", "--min-length", "9"]);
    let applied = results(
        tmp.path(),
        &["links", "auto", "--apply", "--min-length", "9"],
    );

    assert_eq!(preview["applied"].as_bool(), Some(false), "{preview}");
    assert_eq!(
        applied["applied"].as_bool(),
        Some(false),
        "no title inventory, so `applied` stays false even under --apply: {applied}"
    );
    assert_eq!(preview["dry_run"].as_bool(), Some(true), "{preview}");
    assert_eq!(applied["dry_run"].as_bool(), Some(false), "{applied}");
}

#[test]
fn files_with_violations_means_the_same_in_lint_and_summary() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(
        tmp.path().join(".hyalo.toml"),
        "[schema.default]\nrequired = [\"title\", \"type\"]\n",
    )
    .unwrap();
    write_md(tmp.path(), "a.md", "---\ntitle: A\n---\nBody\n");
    write_md(tmp.path(), "b.md", "---\ntitle: B\n---\nBody\n");

    let lint = results(tmp.path(), &["lint"]);
    let summary = results(tmp.path(), &["summary"]);

    let from_lint = lint["files_with_violations"].as_u64().unwrap();
    let from_summary = summary["schema"]["files_with_violations"]
        .as_u64()
        .unwrap_or_else(|| panic!("summary must name it `files_with_violations`: {summary}"));
    assert_eq!(from_lint, 2, "{lint}");
    assert_eq!(
        from_lint, from_summary,
        "the digest and the full run must agree: {lint} / {summary}"
    );
    assert!(
        summary["schema"].get("files_with_issues").is_none(),
        "the pre-iter-216 spelling must be gone: {summary}"
    );
}
