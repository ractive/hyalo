//! Iteration 241 — stale-index detection for `links fix --index` (BUG-2's
//! detection half), `--glob`-not-`--file` hints (UX-1), and error-first lint
//! listing (UX-4), from the dogfood v0.20.0 report
//! (`hyalo-knowledgebase/dogfood-results/dogfood-v0200-arch-refactors-and-agent-cli-followups`).
//!
//! The UX-2 `--iteration` zero-padding/archiving tests were removed in
//! iter-242 together with the flag itself.

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

// Review follow-ups (PR #277): links auto stale path, DEC-241 persistence
// contract, hidden-errors hint e2e, and bare-16-vs-16b matching
// ===========================================================================

#[test]
fn links_auto_index_warns_on_stale_snapshot() {
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

Some prose.
"),
    );
    let out = hyalo_no_hints()
        .args(["--dir", dir.to_str().unwrap(), "create-index"])
        .output()
        .unwrap();
    assert!(out.status.success());
    // In-place edit behind the index's back (past the tolerance window).
    std::thread::sleep(Duration::from_secs(2));
    fs::write(
        dir.join("source.md"),
        md!(r"
---
title: Source
---

Some prose mentioning Target.
"),
    )
    .unwrap();
    let out = hyalo_no_hints()
        .args(["--dir", dir.to_str().unwrap(), "links", "auto", "--index"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("index is stale"),
        "links auto must exercise the same staleness check: {stderr}"
    );
}

#[test]
fn stale_refresh_persists_only_under_apply() {
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
    let index_path = dir.join(".hyalo-index");
    let before = fs::read(&index_path).unwrap();
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

    // Dry run: the in-memory refresh must NOT touch the snapshot file
    // (DEC-241's persistence contract).
    let out = hyalo_no_hints()
        .args([
            "--dir",
            dir.to_str().unwrap(),
            "links",
            "fix",
            "--index",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let after_dry = fs::read(&index_path).unwrap();
    assert_eq!(
        before, after_dry,
        "a dry run must not persist the stale refresh"
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        json["results"]["broken"].as_u64().unwrap_or(0) >= 1,
        "dry run still sees the freshly added broken link"
    );

    // Apply: the refresh (plus any fixes) is persisted.
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
    assert!(out.status.success());
    let after_apply = fs::read(&index_path).unwrap();
    assert_ne!(
        before, after_apply,
        "--apply must persist the refreshed entries"
    );
}

#[test]
fn lint_hint_names_errors_hidden_by_file_cap() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    // 51 error-carrying files: one more than the 50-file display cap, so
    // exactly one error is truncated away even under the error-first sort.
    for i in 0..51 {
        write_md(
            dir,
            &format!("err/err-{i:02}.md"),
            md!(r"
---
title: E
---

see (the docs)[https://example.com] now.
"),
        );
    }
    // Hints stay on: the assertion targets the show-all hint text itself.
    let out = hyalo()
        .args(["--dir", dir.to_str().unwrap(), "lint", "--format", "json"])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["results"]["errors"].as_u64().unwrap_or(0), 51);
    assert_eq!(json["results"]["files_truncated"].as_bool(), Some(true));
    let hint_texts: Vec<String> = json["hints"]
        .as_array()
        .map(|h| {
            h.iter()
                .filter_map(|v| v["description"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        hint_texts
            .iter()
            .any(|t| t.contains("1 error hidden by the file cap")),
        "the show-all hint should name the hidden error: {hint_texts:?}"
    );
}
