/// E2E tests for iter-136:
/// - Defect 1: wikilink resolver must accept `.md` suffix
/// - Defect 2: `hyalo mv` rewriter prefers short-form for unique basenames
use std::fs;

use super::common::{hyalo_no_hints, write_md};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// T1 — Resolver: [[foo.md]] resolves like [[foo]]
// ---------------------------------------------------------------------------

#[test]
fn t1_wikilink_md_suffix_resolves_like_bare() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "notes/foo.md", "Content.\n");
    write_md(
        tmp.path(),
        "index.md",
        "Plain [[foo]] vs suffix [[foo.md]] vs path [[notes/foo]] vs path+suffix [[notes/foo.md]].\n",
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["links", "fix"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // All four wikilinks should resolve — zero broken, zero case_mismatches, zero ambiguous
    assert_eq!(
        json["broken"].as_array().map_or(0, Vec::len),
        0,
        "broken: {json}"
    );
    assert_eq!(
        json["case_mismatches"].as_array().map_or(0, Vec::len),
        0,
        "case_mismatches: {json}"
    );
    assert_eq!(
        json["ambiguous"].as_array().map_or(0, Vec::len),
        0,
        "ambiguous: {json}"
    );
}

// ---------------------------------------------------------------------------
// T3 — Heading wikilink with .md suffix
// ---------------------------------------------------------------------------

#[test]
fn t3_heading_wikilink_with_md_suffix() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "notes/foo.md", "# Heading\n## Bar\nContent.\n");
    write_md(tmp.path(), "index.md", "See [[foo.md#Bar]].\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["links", "fix"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json["broken"].as_array().map_or(0, Vec::len),
        0,
        "broken: {json}"
    );
}

// ---------------------------------------------------------------------------
// T4 — Alias wikilink with .md suffix
// ---------------------------------------------------------------------------

#[test]
fn t4_alias_wikilink_with_md_suffix() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "notes/foo.md", "Content.\n");
    write_md(tmp.path(), "index.md", "See [[foo.md|the foo note]].\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["links", "fix"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json["broken"].as_array().map_or(0, Vec::len),
        0,
        "broken: {json}"
    );

    // Alias text preserved (link is valid, no rewrite needed)
    let content = fs::read_to_string(tmp.path().join("index.md")).unwrap();
    assert!(
        content.contains("the foo note"),
        "alias should be preserved: {content}"
    );
}

// ---------------------------------------------------------------------------
// T5 — mv prefers short-form for unique basenames
// ---------------------------------------------------------------------------

#[test]
fn t5_mv_prefers_short_form_unique_basename() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "iterations/iteration-42.md", "Content.\n");
    write_md(
        tmp.path(),
        "notes/index.md",
        "See [[iteration-42]] and [[iterations/iteration-42]].\n",
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "mv",
            "--file",
            "iterations/iteration-42.md",
            "--to",
            "iterations/done/iteration-42.md",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(tmp.path().join("notes/index.md")).unwrap();
    // Both links should be in short-form (stem is unique)
    assert!(
        content.contains("[[iteration-42]]"),
        "notes/index.md should use short-form: {content}"
    );
    // The path-form [[iterations/iteration-42]] should be rewritten
    assert!(
        !content.contains("[[iterations/iteration-42]]"),
        "old path-form link should be gone: {content}"
    );
    // No path-expanded form should appear either
    assert!(
        !content.contains("[[iterations/done/iteration-42]]"),
        "path-expanded form should not appear when stem is unique: {content}"
    );
}

// ---------------------------------------------------------------------------
// T6 — mv falls back to path-form for ambiguous basenames
// ---------------------------------------------------------------------------

#[test]
fn t6_mv_path_form_for_ambiguous_basename() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a/dup.md", "A dup.\n");
    write_md(tmp.path(), "b/dup.md", "B dup.\n");
    write_md(tmp.path(), "index.md", "See [[a/dup]].\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "a/dup.md", "--to", "archive/dup.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(tmp.path().join("index.md")).unwrap();
    // stem "dup" is ambiguous (archive/dup.md and b/dup.md) → path-form, no .md suffix
    assert!(
        content.contains("[[archive/dup]]"),
        "ambiguous basename should fall back to path-form: {content}"
    );
    assert!(
        !content.contains("[[a/dup]]"),
        "old path-form link should be gone: {content}"
    );
}

// ---------------------------------------------------------------------------
// T7 — Single mv round-trip is idempotent under links fix
// ---------------------------------------------------------------------------

#[test]
fn t7_mv_roundtrip_idempotent_under_links_fix() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "iterations/iteration-10.md", "Content A.\n");
    write_md(tmp.path(), "iterations/iteration-11.md", "Content B.\n");
    write_md(tmp.path(), "iterations/iteration-12.md", "Content C.\n");
    write_md(
        tmp.path(),
        "notes/ref.md",
        "See [[iteration-10]] and [[iteration-11]] and [[iteration-12]].\n",
    );

    // Step 1: mv
    let mv_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "mv",
            "--file",
            "iterations/iteration-10.md",
            "--to",
            "iterations/done/iteration-10.md",
        ])
        .output()
        .unwrap();
    assert!(
        mv_out.status.success(),
        "mv failed: {}",
        String::from_utf8_lossy(&mv_out.stderr)
    );

    // Step 2: links fix — should produce zero findings
    let fix_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["links", "fix"])
        .output()
        .unwrap();
    assert!(
        fix_out.status.success(),
        "links fix failed: {}",
        String::from_utf8_lossy(&fix_out.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&fix_out.stdout).unwrap();
    assert_eq!(
        json["broken"].as_array().map_or(0, Vec::len),
        0,
        "zero broken links expected after mv: {json}"
    );
    assert_eq!(
        json["case_mismatches"].as_array().map_or(0, Vec::len),
        0,
        "zero case_mismatches expected after mv: {json}"
    );
    assert_eq!(
        json["ambiguous"].as_array().map_or(0, Vec::len),
        0,
        "zero ambiguous links expected after mv: {json}"
    );
}

// ---------------------------------------------------------------------------
// T8 — Batch mv output is clean under links fix
// ---------------------------------------------------------------------------

#[test]
fn t8_batch_mv_clean_under_links_fix() {
    let tmp = TempDir::new().unwrap();
    for i in 1..=5u32 {
        write_md(
            tmp.path(),
            &format!("iterations/iter-{i}.md"),
            &format!(
                "---\nstatus: completed\ntype: iteration\n---\nSee [[iter-{next}]].\n",
                next = if i < 5 { i + 1 } else { 1 }
            ),
        );
    }

    // Step 1: batch mv
    let mv_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "mv",
            "--property",
            "status=completed",
            "--to",
            "iterations/done/",
            "--apply",
        ])
        .output()
        .unwrap();
    assert!(
        mv_out.status.success(),
        "batch mv failed: {}",
        String::from_utf8_lossy(&mv_out.stderr)
    );

    // Verify files moved
    for i in 1..=5u32 {
        assert!(
            tmp.path()
                .join(format!("iterations/done/iter-{i}.md"))
                .exists(),
            "iter-{i}.md should be in done/"
        );
    }

    // Step 2: links fix — should produce zero findings
    let fix_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["links", "fix"])
        .output()
        .unwrap();
    assert!(
        fix_out.status.success(),
        "links fix failed: {}",
        String::from_utf8_lossy(&fix_out.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&fix_out.stdout).unwrap();
    assert_eq!(
        json["broken"].as_array().map_or(0, Vec::len),
        0,
        "zero broken links expected: {json}"
    );
    assert_eq!(
        json["case_mismatches"].as_array().map_or(0, Vec::len),
        0,
        "zero case_mismatches expected: {json}"
    );
    assert_eq!(
        json["ambiguous"].as_array().map_or(0, Vec::len),
        0,
        "zero ambiguous links expected: {json}"
    );
}

// ---------------------------------------------------------------------------
// T9 — Markdown link form unchanged (with .md), wikilink uses short-form
// ---------------------------------------------------------------------------

#[test]
fn t9_markdown_link_form_unchanged() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "notes/foo.md", "Content.\n");
    write_md(
        tmp.path(),
        "index.md",
        "Markdown link: [foo](notes/foo.md). Wikilink: [[notes/foo]].\n",
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "notes/foo.md", "--to", "archive/foo.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(tmp.path().join("index.md")).unwrap();
    // Markdown link keeps path-form with .md (spec-correct)
    assert!(
        content.contains("[foo](archive/foo.md)"),
        "markdown link should use path-form with .md: {content}"
    );
    // Wikilink uses short-form (stem "foo" is unique)
    assert!(
        content.contains("[[foo]]"),
        "wikilink should use short-form: {content}"
    );
    assert!(
        !content.contains("[[notes/foo]]"),
        "old wikilink path should be gone: {content}"
    );
    assert!(
        !content.contains("[[archive/foo]]"),
        "wikilink should not use path-form when stem is unique: {content}"
    );
}

// ---------------------------------------------------------------------------
// T10 — Original path-form → short-form when move makes basename unique
// ---------------------------------------------------------------------------

#[test]
fn t10_path_form_becomes_short_form_when_unique_after_rename() {
    let tmp = TempDir::new().unwrap();
    // Two files: a/note.md and b/note.md (stem "note" is ambiguous)
    write_md(tmp.path(), "a/note.md", "Note A.\n");
    write_md(tmp.path(), "b/note.md", "Note B.\n");
    // Author used path-form because "note" was ambiguous
    write_md(tmp.path(), "index.md", "See [[a/note]].\n");

    // Move a/note.md to a/renamed.md — "renamed" is unique vault-wide
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "a/note.md", "--to", "a/renamed.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(tmp.path().join("index.md")).unwrap();
    // "renamed" is now unique → short-form [[renamed]]
    assert!(
        content.contains("[[renamed]]"),
        "should switch to short-form when new basename is unique: {content}"
    );
    assert!(
        !content.contains("[[a/note]]"),
        "old path-form should be gone: {content}"
    );
}

// ---------------------------------------------------------------------------
// T_LINKS_FIX_APPLY — links fix --apply rewrites .md-suffix wikilinks to no .md
// ---------------------------------------------------------------------------

#[test]
fn t_links_fix_apply_rewrites_md_suffix_wikilink() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "notes/foo.md", "Content.\n");
    // [[foo.md]] is valid (resolves to notes/foo.md via stem) but has .md suffix
    write_md(tmp.path(), "index.md", "See [[foo.md]] for details.\n");

    // links fix without --apply: should show no broken links (foo.md suffix is valid)
    let fix_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["links", "fix"])
        .output()
        .unwrap();

    assert!(fix_out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&fix_out.stdout).unwrap();
    assert_eq!(
        json["broken"].as_array().map_or(0, Vec::len),
        0,
        "[[foo.md]] should not be broken: {json}"
    );
}
