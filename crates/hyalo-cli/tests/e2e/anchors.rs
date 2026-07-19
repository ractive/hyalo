//! L-21 (iter-190) — broken-anchor validation in `find --broken-links`.
//!
//! Covers the anchor e2e matrix: heading anchors that resolve, broken anchors
//! (target resolves, heading missing), broken targets (anchor check skipped),
//! `^block` refs (never reported), markdown fragment variants (bare, angle,
//! percent-encoded), fragment-only same-file links (non-links), index/disk
//! parity, the `links fix` headline-count guard, backlinks preservation, mv
//! fragment preservation, and old-snapshot fallback.

use std::fs;

use super::common::{hyalo_no_hints, md, write_md};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Fixtures & helpers
// ---------------------------------------------------------------------------

/// A vault exercising every anchor case against `Foo.md` (which has a `## Real`
/// heading).
fn setup_anchor_vault() -> TempDir {
    let tmp = TempDir::new().expect("tempdir");

    write_md(
        tmp.path(),
        "Foo.md",
        md!(r"
---
title: Foo
---
# Foo

## Real

Some content.

## my heading

Percent-encoded target.
"),
    );

    // Valid heading anchor.
    write_md(
        tmp.path(),
        "valid_anchor.md",
        md!(r"
---
title: Valid Anchor
---
See [[Foo#Real]] here.
"),
    );

    // Broken heading anchor — target resolves, heading missing.
    write_md(
        tmp.path(),
        "broken_anchor.md",
        md!(r"
---
title: Broken Anchor
---
See [[Foo#nope]] here.
"),
    );

    // Broken target — anchor check must be skipped.
    write_md(
        tmp.path(),
        "broken_target.md",
        md!(r"
---
title: Broken Target
---
See [[Nope#x]] here.
"),
    );

    // Block ref — never reported.
    write_md(
        tmp.path(),
        "block_ref.md",
        md!(r"
---
title: Block Ref
---
See [[Foo#^block]] here.
"),
    );

    // Markdown fragment variants.
    write_md(
        tmp.path(),
        "md_variants.md",
        md!(r"
---
title: Markdown Variants
---
Bare: [t](Foo.md#Real).
Percent: [t](Foo.md#my%20heading).
Broken md anchor: [t](Foo.md#missing).
"),
    );

    // Fragment-only same-file links — must NOT be file links.
    write_md(
        tmp.path(),
        "same_file.md",
        md!(r"
---
title: Same File
---
Wiki: [[#Real]].
Md: [t](#Real).
"),
    );

    tmp
}

/// Run `hyalo find --broken-links --fields links --format json`, optionally with
/// `--index` (index must already exist).
fn run_broken_links(tmp: &TempDir, indexed: bool) -> serde_json::Value {
    let dir = tmp.path().to_str().expect("utf-8 path");
    let mut args = vec![
        "--dir",
        dir,
        "find",
        "--broken-links",
        "--fields",
        "links",
        "--format",
        "json",
    ];
    if indexed {
        args.push("--index");
    }
    let output = hyalo_no_hints()
        .args(&args)
        .output()
        .expect("hyalo find --broken-links should run");
    assert!(
        output.status.success(),
        "find --broken-links exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON")
}

/// Create the default `.hyalo-index` for the vault.
fn create_index(tmp: &TempDir) {
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap(), "create-index"])
        .output()
        .expect("create-index should run");
    assert!(
        output.status.success(),
        "create-index failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Collect the file names present in a find result's `results` array.
fn result_files(json: &serde_json::Value) -> Vec<String> {
    json["results"]
        .as_array()
        .expect("results array")
        .iter()
        .map(|r| r["file"].as_str().unwrap_or("").to_string())
        .collect()
}

/// Find a link entry `{target, path, fragment, broken_anchor}` for a given file
/// and target within a find result.
fn find_link<'a>(
    json: &'a serde_json::Value,
    file: &str,
    target: &str,
) -> Option<&'a serde_json::Value> {
    json["results"]
        .as_array()?
        .iter()
        .find(|r| r["file"].as_str() == Some(file))?
        .get("links")?
        .as_array()?
        .iter()
        .find(|l| l["target"].as_str() == Some(target))
}

// ---------------------------------------------------------------------------
// Core matrix — asserted identically with and without --index (index parity)
// ---------------------------------------------------------------------------

fn assert_matrix(json: &serde_json::Value, label: &str) {
    let files = result_files(json);

    // valid_anchor.md — [[Foo#Real]] resolves fully → NOT surfaced.
    assert!(
        !files.contains(&"valid_anchor.md".to_string()),
        "[{label}] valid anchor must not be broken: {files:?}"
    );

    // block_ref.md — [[Foo#^block]] skipped → NOT surfaced.
    assert!(
        !files.contains(&"block_ref.md".to_string()),
        "[{label}] block ref must never be reported: {files:?}"
    );

    // same_file.md — fragment-only links are not file links → NOT surfaced.
    assert!(
        !files.contains(&"same_file.md".to_string()),
        "[{label}] fragment-only same-file links are not links: {files:?}"
    );

    // broken_anchor.md — [[Foo#nope]] target resolves, anchor broken.
    assert!(
        files.contains(&"broken_anchor.md".to_string()),
        "[{label}] broken anchor must be surfaced: {files:?}"
    );
    let ba = find_link(json, "broken_anchor.md", "Foo").expect("broken_anchor link present");
    assert_eq!(
        ba["path"].as_str(),
        Some("Foo.md"),
        "[{label}] broken-anchor target must resolve"
    );
    assert_eq!(
        ba["broken_anchor"].as_bool(),
        Some(true),
        "[{label}] broken_anchor flag must be true"
    );
    assert_eq!(ba["fragment"].as_str(), Some("nope"));

    // broken_target.md — [[Nope#x]] target broken; anchor check skipped: the
    // same link is never both broken-target and broken-anchor.
    assert!(
        files.contains(&"broken_target.md".to_string()),
        "[{label}] broken target must be surfaced: {files:?}"
    );
    let bt = find_link(json, "broken_target.md", "Nope").expect("broken_target link present");
    assert!(
        bt["path"].is_null(),
        "[{label}] broken target path must be null"
    );
    assert!(
        bt.get("broken_anchor").and_then(serde_json::Value::as_bool) != Some(true),
        "[{label}] broken target must not also be a broken anchor: {bt}"
    );

    // md_variants.md — bare + percent-encoded resolve; #missing is broken.
    assert!(
        files.contains(&"md_variants.md".to_string()),
        "[{label}] md_variants has a broken md anchor: {files:?}"
    );
    let md_links = json["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["file"].as_str() == Some("md_variants.md"))
        .unwrap()["links"]
        .as_array()
        .unwrap();
    // Foo.md#Real and Foo.md#my%20heading resolve (broken_anchor false);
    // Foo.md#missing is broken.
    let mut real_ok = false;
    let mut pct_ok = false;
    let mut missing_broken = false;
    for l in md_links {
        let frag = l["fragment"].as_str().unwrap_or("");
        let broken = l["broken_anchor"].as_bool().unwrap_or(false);
        match frag {
            "Real" => real_ok = !broken && l["path"].as_str() == Some("Foo.md"),
            "my%20heading" => pct_ok = !broken && l["path"].as_str() == Some("Foo.md"),
            "missing" => missing_broken = broken,
            _ => {}
        }
    }
    assert!(real_ok, "[{label}] markdown #Real must resolve");
    assert!(
        pct_ok,
        "[{label}] percent-encoded #my%20heading must resolve to `my heading`"
    );
    assert!(missing_broken, "[{label}] markdown #missing must be broken");
}

#[test]
fn anchor_matrix_disk_scan() {
    let tmp = setup_anchor_vault();
    let json = run_broken_links(&tmp, false);
    assert_matrix(&json, "disk");
}

#[test]
fn anchor_matrix_indexed() {
    let tmp = setup_anchor_vault();
    create_index(&tmp);
    let json = run_broken_links(&tmp, true);
    assert_matrix(&json, "index");
}

// ---------------------------------------------------------------------------
// `links fix` headline counts are NOT inflated by broken anchors
// ---------------------------------------------------------------------------

#[test]
fn links_fix_ignores_broken_anchors() {
    // A vault whose ONLY defect is `[[Foo#nope]]` (target resolves) must report
    // broken: 0, fixable: 0 and no "Apply N fixes" hint.
    let tmp = TempDir::new().expect("tempdir");
    write_md(
        tmp.path(),
        "Foo.md",
        md!(r"
---
title: Foo
---
## Real
"),
    );
    write_md(
        tmp.path(),
        "linker.md",
        md!(r"
---
title: Linker
---
See [[Foo#nope]].
"),
    );

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
        .expect("links fix should run");
    assert!(
        output.status.success(),
        "links fix exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    // `links fix` output is wrapped in a `{hints, results}` envelope.
    let results = &json["results"];
    assert_eq!(
        results["broken"].as_u64(),
        Some(0),
        "broken anchors must not inflate the broken count: {json}"
    );
    assert_eq!(
        results["fixable"].as_u64(),
        Some(0),
        "broken anchors must not inflate the fixable count: {json}"
    );
}

// ---------------------------------------------------------------------------
// backlinks / graph unaffected by fragments
// ---------------------------------------------------------------------------

#[test]
fn backlinks_finds_anchored_linkers() {
    let tmp = setup_anchor_vault();
    let output = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "backlinks",
            "Foo.md",
            "--format",
            "json",
        ])
        .output()
        .expect("backlinks should run");
    assert!(
        output.status.success(),
        "backlinks exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    let sources: Vec<&str> = json["results"]["backlinks"]
        .as_array()
        .expect("backlinks array")
        .iter()
        .map(|b| b["source"].as_str().unwrap_or(""))
        .collect();
    // [[Foo#Real]] from valid_anchor.md must be a backlink of Foo despite the
    // fragment (graph keys are fragment-free).
    assert!(
        sources.contains(&"valid_anchor.md"),
        "backlinks Foo must include the [[Foo#Real]] linker: {sources:?}"
    );
    assert!(
        sources.contains(&"broken_anchor.md"),
        "backlinks Foo must include the [[Foo#nope]] linker: {sources:?}"
    );
}

// ---------------------------------------------------------------------------
// mv preserves fragments (task 6)
// ---------------------------------------------------------------------------

fn setup_mv_vault() -> TempDir {
    let tmp = TempDir::new().expect("tempdir");
    write_md(
        tmp.path(),
        "Foo.md",
        md!(r"
---
title: Foo
aliases:
  - Foo
---
## Real
"),
    );
    // A linker using all three link shapes with #Real fragments, including a
    // frontmatter wikilink value.
    write_md(
        tmp.path(),
        "linker.md",
        md!(r#"
---
title: Linker
ref: "[[Foo#Real]]"
---
Wiki: [[Foo#Real]].
Markdown: [click](Foo.md#Real).
"#),
    );
    tmp
}

#[test]
fn mv_preserves_fragments_dry_run() {
    let tmp = setup_mv_vault();
    let output = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "mv",
            "Foo.md",
            "--to",
            "Bar.md",
            "--dry-run",
            "--format",
            "json",
        ])
        .output()
        .expect("mv dry-run should run");
    assert!(
        output.status.success(),
        "mv dry-run exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    let dump = json["results"].to_string();
    // Planned rewrites keep the `#Real` fragment on every shape.
    assert!(
        dump.contains("[[Bar#Real]]"),
        "dry-run wikilink rewrite must preserve #Real: {dump}"
    );
    assert!(
        dump.contains("(Bar.md#Real)"),
        "dry-run markdown rewrite must preserve #Real: {dump}"
    );
    // Dry-run must not touch disk.
    let linker = fs::read_to_string(tmp.path().join("linker.md")).expect("read linker");
    assert!(
        linker.contains("[[Foo#Real]]"),
        "dry-run must not modify the file: {linker}"
    );
    assert!(
        tmp.path().join("Foo.md").exists(),
        "dry-run must not rename Foo.md"
    );
}

#[test]
fn mv_preserves_fragments_apply() {
    let tmp = setup_mv_vault();
    let output = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "mv",
            "Foo.md",
            "--to",
            "Bar.md",
            "--apply",
        ])
        .output()
        .expect("mv apply should run");
    assert!(
        output.status.success(),
        "mv apply exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let linker = fs::read_to_string(tmp.path().join("linker.md")).expect("read linker");
    assert!(
        linker.contains("[[Bar#Real]]"),
        "wikilink fragment preserved after mv: {linker}"
    );
    assert!(
        linker.contains("(Bar.md#Real)"),
        "markdown fragment preserved after mv: {linker}"
    );
    assert!(
        linker.contains("[[Bar#Real]]") && linker.contains("ref:"),
        "frontmatter wikilink fragment preserved after mv: {linker}"
    );
    assert!(
        !linker.contains("Foo#Real"),
        "no old Foo#Real targets should remain: {linker}"
    );
}

#[test]
fn batch_mv_preserves_fragments() {
    // Batch mv moves Foo.md into a subdir; the linker's #Real fragment must
    // survive the cross-file rewrite.
    let tmp = TempDir::new().expect("tempdir");
    write_md(
        tmp.path(),
        "Foo.md",
        md!(r"
---
title: Foo
status: archived
---
## Real
"),
    );
    write_md(
        tmp.path(),
        "linker.md",
        md!(r"
---
title: Linker
---
Wiki: [[Foo#Real]].
Markdown: [click](Foo.md#Real).
"),
    );
    fs::create_dir_all(tmp.path().join("archive")).unwrap();
    let output = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "mv",
            "--glob",
            "Foo.md",
            "--property",
            "status=archived",
            "--to",
            "archive/",
            "--apply",
        ])
        .output()
        .expect("batch mv should run");
    assert!(
        output.status.success(),
        "batch mv exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let linker = fs::read_to_string(tmp.path().join("linker.md")).expect("read linker");
    // The markdown link is rewritten to the new path with `#Real` intact. The
    // bare-stem wikilink `[[Foo#Real]]` stays as-is (short-form still resolves
    // by basename after the move) — either way the fragment is never dropped.
    assert!(
        linker.contains("(archive/Foo.md#Real)"),
        "batch mv must preserve #Real on the rewritten markdown link: {linker}"
    );
    assert!(
        linker.contains("[[Foo#Real]]"),
        "bare-stem wikilink keeps its #Real fragment: {linker}"
    );
    assert!(
        !linker.contains("Real]].\nMarkdown: [click](Foo.md#Real)"),
        "no un-rewritten old markdown path should remain: {linker}"
    );
}

// ---------------------------------------------------------------------------
// Old-snapshot fallback: a pre-bump index is decoded gracefully.
//
// DEC-060 deviation: under this repo's `to_vec_named` framing, adding the
// backward-compatible `fragment` field does NOT hard-break old snapshots — they
// decode with `fragment: None`, which is fail-safe (stale entries carry no
// fragment, so no false broken-anchor reports). We assert that a rebuilt index
// re-populates fragments and the anchor matrix holds on the index path.
// ---------------------------------------------------------------------------

#[test]
fn rebuilt_index_repopulates_fragments() {
    let tmp = setup_anchor_vault();
    // Build once, then rebuild — the second index carries fragments and the
    // matrix holds on the index path (the graceful-degradation guarantee).
    create_index(&tmp);
    create_index(&tmp);
    let json = run_broken_links(&tmp, true);
    assert_matrix(&json, "rebuilt-index");
}
