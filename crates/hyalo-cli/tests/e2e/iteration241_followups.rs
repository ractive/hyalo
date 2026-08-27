//! Iteration 241 — stale-index detection for `links fix --index` (BUG-2's
//! detection half), `--glob`-not-`--file` hints (UX-1), zero-padded and
//! archived `--iteration` addressing (UX-2), and error-first lint listing
//! (UX-4), from the dogfood v0.20.0 report
//! (`hyalo-knowledgebase/dogfood-results/dogfood-v0200-arch-refactors-and-agent-cli-followups`).

use std::fs;
use std::time::Duration;

use super::common::{hyalo, hyalo_no_hints, md, write_md};
use tempfile::TempDir;

// ===========================================================================
// BUG-2 detection: `links fix --apply --index` must not trust a stale index
// ===========================================================================

#[test]
fn links_fix_index_rescans_externally_edited_file() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();

    write_md(
        dir,
        "target.md",
        md!(r"
---
title: Target
---

# Target
"),
    );
    write_md(
        dir,
        "source.md",
        md!(r"
---
title: Source
---

See [[target]].
"),
    );

    // Build the index while the source file is clean.
    let out = hyalo_no_hints()
        .args(["--dir", dir.to_str().unwrap(), "create-index"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Edit behind the index's back: append a fuzzy-fixable broken link
    // ([[trget]] → target.md at 0.9+ confidence) in place.
    // The sleep guarantees the mtime passes the 1-second staleness tolerance
    // (same-second edits are deliberately treated as current).
    std::thread::sleep(Duration::from_secs(2));
    fs::write(
        dir.join("source.md"),
        md!(r"
---
title: Source
---

See [[target]] and [[trget]].
"),
    )
    .unwrap();

    let out = hyalo_no_hints()
        .args([
            "--dir",
            dir.to_str().unwrap(),
            "links",
            "fix",
            "--apply",
            "--apply-fuzzy",
            "--index",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("index is stale"),
        "expected a stale-index warning on stderr, got: {stderr}"
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let broken = json["results"]["broken"].as_u64().unwrap_or(0);
    assert!(
        broken >= 1,
        "the externally added broken link must be discovered: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    // And it must actually be fixed on disk (fuzzy rewrite → [[target]]).
    let fixed = fs::read_to_string(dir.join("source.md")).unwrap();
    assert!(
        !fixed.contains("[[trget]]"),
        "the broken link should be rewritten by --apply --apply-fuzzy: {fixed}"
    );
}

#[test]
fn links_fix_index_no_warning_when_index_is_current() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    write_md(
        dir,
        "target.md",
        md!(r"
---
title: Target
---

# Target
"),
    );
    write_md(
        dir,
        "source.md",
        md!(r"
---
title: Source
---

See [[target]].
"),
    );
    let out = hyalo_no_hints()
        .args(["--dir", dir.to_str().unwrap(), "create-index"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let out = hyalo_no_hints()
        .args(["--dir", dir.to_str().unwrap(), "links", "fix", "--index"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("index is stale"),
        "a current index must not produce a staleness warning: {stderr}"
    );
}

// ===========================================================================
// UX-1: the file-not-found hint must suggest --glob (which globs), not --file
// ===========================================================================

#[test]
fn file_not_found_hint_says_glob() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "notes/alpha.md",
        md!(r"
---
title: Alpha
---

# Alpha
"),
    );
    let out = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "set",
            "notes/nonexistent.md",
            "--property",
            "status=done",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    // User-error envelopes go to stderr.
    let json: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
    let hint = json["hint"].as_str().unwrap_or_default();
    assert!(
        hint.contains("find --glob"),
        "hint should suggest find --glob: {hint:?}"
    );
    assert!(
        !hint.contains("--file <glob>"),
        "hint must not suggest --file for a glob: {hint:?}"
    );
    // And the suggested flag actually globs.
    let out = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "find",
            "--glob",
            "notes/*.md",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["total"].as_u64().unwrap_or(0), 1);
}

// ===========================================================================
// UX-2: --iteration reaches zero-padded files in subdirectories
// ===========================================================================

fn setup_archive_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join(".hyalo.toml"),
        md!(r#"
dir = "."

[schema.types.iteration]
required = ["title", "type", "status"]
filename-template = "iterations/iteration-{n}-{slug}.md"

[schema.types.iteration.properties.status]
type = "enum"
values = ["planned", "in-progress", "completed", "superseded"]
"#),
    )
    .unwrap();
    write_md(
        tmp.path(),
        "iterations/done/iteration-02-links.md",
        md!(r"
---
title: Iter 2
type: iteration
status: completed
date: 2026-01-05
---

Body of iteration 2.
"),
    );
    write_md(
        tmp.path(),
        "iterations/iteration-206-agent-cli.md",
        md!(r"
---
title: Iter 206
type: iteration
status: planned
date: 2026-02-01
---

Body 206.
"),
    );
    tmp
}

#[test]
fn find_iteration_matches_zero_padded_archived_file() {
    let vault = setup_archive_vault();
    let out = hyalo_no_hints()
        .args([
            "--dir",
            vault.path().to_str().unwrap(),
            "find",
            "--iteration",
            "2",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        json["total"].as_u64().unwrap_or(0),
        1,
        "expected the archived zero-padded iteration: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let file = json["results"][0]["file"].as_str().unwrap_or_default();
    assert_eq!(file, "iterations/done/iteration-02-links.md");
}

#[test]
fn read_iteration_resolves_zero_padded_archived_file() {
    let vault = setup_archive_vault();
    let out = hyalo_no_hints()
        .args([
            "--dir",
            vault.path().to_str().unwrap(),
            "read",
            "--iteration",
            "2",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Body of iteration 2."),
        "read should print the resolved file's body: {stdout}"
    );
}

#[test]
fn find_iteration_206_still_primary_form() {
    // The fallback globs must not widen matching for the canonical form:
    // 206 is already 3 digits wide, so only the exact + recursive glob exist
    // and no unrelated file matches.
    let vault = setup_archive_vault();
    let out = hyalo_no_hints()
        .args([
            "--dir",
            vault.path().to_str().unwrap(),
            "find",
            "--iteration",
            "206",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["total"].as_u64().unwrap_or(0), 1);
    let file = json["results"][0]["file"].as_str().unwrap_or_default();
    assert_eq!(file, "iterations/iteration-206-agent-cli.md");
}

// ===========================================================================
// UX-4: lint lists error-carrying files first; MD011 skips regex prose
// ===========================================================================

#[test]
fn lint_lists_error_file_first_when_warnings_dominate() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    // 55 warning-only files (MD040: fenced block without language).
    for i in 0..55 {
        write_md(
            dir,
            &format!("warn/warn-{i:02}.md"),
            md!(r"
---
title: W
---

```
code
```
"),
        );
    }
    // One error file (MD011: a genuine reversed link) that would sort below
    // the warning files under the old violations-count ordering (1 < 55
    // groups) and be cut by the 50-file cap.
    write_md(
        dir,
        "err/reversed.md",
        md!(r"
---
title: E
---

see (the docs)[https://example.com] now.
"),
    );
    let out = hyalo_no_hints()
        .args(["--dir", dir.to_str().unwrap(), "lint", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("err/reversed.md"),
        "the error file must appear in the capped listing: {stdout}"
    );
    assert!(
        stdout.contains("1 error"),
        "summary should count the error: {stdout}"
    );
}

#[test]
fn lint_md011_ignores_regex_prose() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "regex.md",
        md!(r"
---
title: Regex
---

Matches (3rd|[Tt]hird)[-_] and (2nd|[Ss]econd)[ .,] patterns.
"),
    );
    let out = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "lint",
            "--format",
            "text",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("MD011"),
        "regex prose must not be flagged as reversed links: {stdout}"
    );
    assert!(
        stdout.contains("no issues"),
        "a clean vault must report no issues: {stdout}"
    );
}
