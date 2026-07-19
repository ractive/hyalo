use super::common::{hyalo_no_hints, md, write_md};
use std::fs;
use tempfile::TempDir;

#[cfg(unix)]
use std::os::unix::fs as unix_fs;

// ---------------------------------------------------------------------------
// `hyalo mv` — move/rename file with link updates
// ---------------------------------------------------------------------------

#[test]
fn mv_bare_wikilink_stays_short_form_when_stem_unique() {
    // When the moved file's stem is unique vault-wide, bare wikilinks
    // ([[b]]) are already in short-form and resolve correctly regardless of
    // where the file lives — no rewrite needed.
    let tmp = TempDir::new().unwrap();
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
    write_md(
        tmp.path(),
        "b.md",
        md!(r"
---
title: B
---
Content.
"),
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "archive/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["from"], "b.md");
    assert_eq!(json["results"]["to"], "archive/b.md");
    assert_eq!(json["results"]["dry_run"], false);
    // [[b]] is already short-form and correct — no rewrite needed
    assert_eq!(
        json["results"]["total_links_updated"], 0,
        "short-form [[b]] needs no rewrite when stem is unique"
    );

    // Verify file was moved
    assert!(!tmp.path().join("b.md").exists());
    assert!(tmp.path().join("archive/b.md").exists());

    // Bare wikilink should still be [[b]] (unchanged — correct short-form)
    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(
        content.contains("[[b]]"),
        "short-form wikilink should remain: {content}"
    );
}

#[test]
fn mv_path_wikilink_rewritten_to_short_form_when_stem_unique() {
    // A path-form wikilink [[sub/b]] should be rewritten to short-form [[b]]
    // when the stem is unique vault-wide after the move.
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        md!(r"
---
title: A
---
See [[sub/b]] for details.
"),
    );
    write_md(
        tmp.path(),
        "sub/b.md",
        md!(r"
---
title: B
---
Content.
"),
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "sub/b.md", "--to", "archive/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json["results"]["total_links_updated"], 1,
        "path-form wikilink should be rewritten to short-form"
    );

    assert!(!tmp.path().join("sub/b.md").exists());
    assert!(tmp.path().join("archive/b.md").exists());

    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    // Path-form is preserved — [[sub/b]] becomes [[archive/b]] (form preserved, path updated)
    assert!(
        content.contains("[[archive/b]]"),
        "path wikilink should preserve path-form and update path: {content}"
    );
    assert!(
        !content.contains("[[sub/b]]"),
        "old path wikilink should be gone: {content}"
    );
}

#[test]
fn mv_bare_wikilink_ambiguous_not_rewritten() {
    // When two files share the same stem, the bare wikilink is ambiguous
    // and must not be rewritten.
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "See [[b]] here.\n");
    write_md(tmp.path(), "b.md", "Root B.\n");
    write_md(tmp.path(), "sub/b.md", "Sub B.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "archive/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // a.md's [[b]] is ambiguous — not rewritten
    assert_eq!(json["results"]["total_links_updated"], 0);

    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(
        content.contains("[[b]]"),
        "ambiguous bare wikilink should not be changed: {content}"
    );
}

#[test]
fn mv_bare_wikilink_ambiguous_rewritten_with_allow_ambiguous() {
    // Opt-in path: with --allow-ambiguous, an ambiguous bare wikilink that
    // targets the moved file IS rewritten (BUG-2 opt-in escape hatch).
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "See [[b]] here.\n");
    write_md(tmp.path(), "b.md", "Root B.\n");
    write_md(tmp.path(), "sub/b.md", "Sub B.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "mv",
            "--file",
            "b.md",
            "--to",
            "archive/renamed.md",
            "--allow-ambiguous",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // With --allow-ambiguous, [[b]] in a.md is rewritten to [[renamed]].
    assert_eq!(
        json["results"]["total_links_updated"], 1,
        "expected one rewrite with --allow-ambiguous; got: {json}"
    );

    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(content.contains("[[renamed]]"), "a.md content: {content}");
    assert!(!content.contains("[[b]]"), "a.md content: {content}");
}

#[test]
fn mv_updates_wikilink_with_path() {
    // Path-form wikilinks are rewritten to the new path (form preserved).
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "See [[backlog/item]] for details.\n");
    write_md(tmp.path(), "backlog/item.md", "Content.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "backlog/item.md", "--to", "archive/item.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["total_links_updated"], 1);

    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    // Path-form is preserved: [[backlog/item]] → [[archive/item]]
    assert!(
        content.contains("[[archive/item]]"),
        "a.md content: {content}"
    );
    assert!(!content.contains("[[backlog/item]]"));
}

#[test]
fn mv_preserves_wikilink_alias() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "See [[sub/b|my note]] here.\n");
    write_md(tmp.path(), "sub/b.md", "Content.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "sub/b.md", "--to", "archive/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    // Path-form is preserved, path updated, alias preserved
    assert!(
        content.contains("[[archive/b|my note]]"),
        "a.md content: {content}"
    );
}

#[test]
fn mv_preserves_wikilink_fragment() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "See [[sub/b#section]] here.\n");
    write_md(tmp.path(), "sub/b.md", "Content.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "sub/b.md", "--to", "archive/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    // Path-form is preserved, path updated, fragment preserved
    assert!(
        content.contains("[[archive/b#section]]"),
        "a.md content: {content}"
    );
}

#[test]
fn mv_updates_inbound_markdown_link() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "See [note](b.md) here.\n");
    write_md(tmp.path(), "b.md", "Content.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "sub/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(
        content.contains("[note](sub/b.md)"),
        "a.md content: {content}"
    );
}

#[test]
fn mv_updates_outbound_relative_link() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "Target.\n");
    write_md(tmp.path(), "b.md", "See [note](a.md) here.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "sub/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let content = fs::read_to_string(tmp.path().join("sub/b.md")).unwrap();
    assert!(
        content.contains("[note](../a.md)"),
        "sub/b.md content: {content}"
    );
}

#[test]
fn mv_outbound_wikilink_unchanged() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "Target.\n");
    write_md(tmp.path(), "b.md", "See [[a]] here.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "sub/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let content = fs::read_to_string(tmp.path().join("sub/b.md")).unwrap();
    // Wikilinks are vault-relative, should NOT change
    assert!(content.contains("[[a]]"), "sub/b.md content: {content}");
}

#[test]
fn mv_skips_links_in_code_blocks() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        md!(r"
---
title: A
---
```
[[sub/b]]
```
Real [[sub/b]] here.
"),
    );
    write_md(tmp.path(), "sub/b.md", "Content.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "sub/b.md", "--to", "archive/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["total_links_updated"], 1);

    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    // The code block should still have [[sub/b]] (unchanged)
    assert!(
        content.contains("```\n[[sub/b]]\n```"),
        "code block was modified: {content}"
    );
    // Real link is updated to new path-form (form preserved)
    assert!(
        content.contains("Real [[archive/b]] here."),
        "real link not updated: {content}"
    );
}

#[test]
fn mv_skips_links_in_inline_code() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        "Use `[[sub/b]]` syntax and real [[sub/b]]\n",
    );
    write_md(tmp.path(), "sub/b.md", "Content.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "sub/b.md", "--to", "archive/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(
        content.contains("`[[sub/b]]`"),
        "inline code was modified: {content}"
    );
    // Real link is updated to new path-form (form preserved)
    assert!(
        content.contains("real [[archive/b]]"),
        "real link not updated: {content}"
    );
}

#[test]
fn mv_dry_run_does_not_modify() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "See [[sub/b]] here.\n");
    write_md(tmp.path(), "sub/b.md", "Content.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "mv",
            "--file",
            "sub/b.md",
            "--to",
            "archive/b.md",
            "--dry-run",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["dry_run"], true);
    assert_eq!(json["results"]["total_links_updated"], 1);

    // File should NOT have been moved
    assert!(tmp.path().join("sub/b.md").exists());
    assert!(!tmp.path().join("archive/b.md").exists());

    // Link should NOT have been updated
    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(
        content.contains("[[sub/b]]"),
        "a.md was modified during dry-run: {content}"
    );
}

#[test]
fn mv_target_already_exists_error() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "Content A.\n");
    write_md(tmp.path(), "b.md", "Content B.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "a.md", "--to", "b.md"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("already exists"), "stderr: {stderr}");
}

#[test]
fn mv_source_not_found_error() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "Content.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "nonexistent.md", "--to", "new.md"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"), "stderr: {stderr}");
}

#[test]
fn mv_creates_parent_directory() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "Content.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "a.md", "--to", "deep/nested/a.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!tmp.path().join("a.md").exists());
    assert!(tmp.path().join("deep/nested/a.md").exists());
}

#[test]
fn mv_text_format() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "See [[sub/b]] here.\n");
    write_md(tmp.path(), "sub/b.md", "Content.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "text"])
        .args(["mv", "--file", "sub/b.md", "--to", "archive/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Moved sub/b.md"), "stdout: {stdout}");
    assert!(stdout.contains("archive/b.md"), "stdout: {stdout}");
    assert!(stdout.contains("[[sub/b]]"), "stdout: {stdout}");
    // path-form is preserved: [[sub/b]] → [[archive/b]]
    assert!(stdout.contains("[[archive/b]]"), "stdout: {stdout}");
}

#[test]
fn mv_no_links_to_update() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "No links here.\n");
    write_md(tmp.path(), "b.md", "Also no links.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "a.md", "--to", "sub/a.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["total_links_updated"], 0);
    assert_eq!(json["results"]["total_files_updated"], 0);

    assert!(!tmp.path().join("a.md").exists());
    assert!(tmp.path().join("sub/a.md").exists());
}

#[test]
fn mv_multiple_links_same_file() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        "See [[sub/b]] and also [[sub/b|alias]] here.\n",
    );
    write_md(tmp.path(), "sub/b.md", "Content.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "sub/b.md", "--to", "archive/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["total_links_updated"], 2);

    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    // Path-form is preserved for both links, path updated
    assert!(content.contains("[[archive/b]]"), "a.md content: {content}");
    assert!(
        content.contains("[[archive/b|alias]]"),
        "a.md content: {content}"
    );
}

#[test]
fn mv_target_must_end_with_md() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "Content.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "a.md", "--to", "b.txt"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(".md"), "stderr: {stderr}");
}

#[test]
fn mv_markdown_link_with_fragment() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "See [note](b.md#section) here.\n");
    write_md(tmp.path(), "b.md", "Content.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "sub/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(
        content.contains("[note](sub/b.md#section)"),
        "a.md content: {content}"
    );
}

#[test]
fn mv_cross_directory_markdown_link() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "sub/a.md", "See [note](../b.md) here.\n");
    write_md(tmp.path(), "b.md", "Content.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "other/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let content = fs::read_to_string(tmp.path().join("sub/a.md")).unwrap();
    assert!(
        content.contains("[note](../other/b.md)"),
        "sub/a.md content: {content}"
    );
}

#[test]
fn mv_updates_wikilink_with_path_separator() {
    // A file in a subdirectory uses [[backlog/item]] (a path-separator wikilink).
    // When backlog/item.md is moved, the wikilink must be updated.
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "backlog/item.md", "# Item\nContent.\n");
    write_md(
        tmp.path(),
        "iterations/iter-1.md",
        "See [[backlog/item]] for context.\n",
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "backlog/item.md", "--to", "archive/item.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["total_links_updated"], 1, "json: {json}");

    let content = fs::read_to_string(tmp.path().join("iterations/iter-1.md")).unwrap();
    // Path-form is preserved: [[backlog/item]] → [[archive/item]]
    assert!(
        content.contains("[[archive/item]]"),
        "iterations/iter-1.md content: {content}"
    );
    assert!(
        !content.contains("[[backlog/item]]"),
        "old link still present: {content}"
    );
}

#[test]
fn mv_wikilink_with_path_from_subdirectory_not_false_positive() {
    // sub/source.md has [[other/target]] (vault-relative wikilink).
    // Moving a different file must NOT affect this link.
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "other/target.md", "# Target\n");
    write_md(tmp.path(), "other/unrelated.md", "# Unrelated\n");
    write_md(tmp.path(), "sub/source.md", "See [[other/target]] here.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "mv",
            "--file",
            "other/unrelated.md",
            "--to",
            "archive/unrelated.md",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json["results"]["total_links_updated"], 0,
        "no links should be updated: {json}"
    );

    let content = fs::read_to_string(tmp.path().join("sub/source.md")).unwrap();
    assert!(
        content.contains("[[other/target]]"),
        "unrelated wikilink was touched: {content}"
    );
}

#[test]
fn mv_same_source_and_destination_error() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "Content.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "a.md", "--to", "a.md"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("same path"),
        "expected 'same path' in stderr, got: {stderr}"
    );
    // The file must remain untouched
    assert!(tmp.path().join("a.md").exists());
}

// ---------------------------------------------------------------------------
// Absolute-link mv tests — site_prefix derivation across invocation styles
// ---------------------------------------------------------------------------

/// Build a vault with absolute-path links and return (vault_root, docs_dir).
///
/// Layout:
///   <root>/
///     docs/
///       index.md     — links to `/docs/pages/about.md`
///       pages/
///         about.md
///         contact.md — links to `/docs/pages/about.md`
fn build_absolute_link_vault(root: &std::path::Path) -> std::path::PathBuf {
    let docs = root.join("docs");
    write_md(
        root,
        "docs/index.md",
        "---\ntitle: Index\n---\nSee [About](/docs/pages/about.md).\n",
    );
    write_md(
        root,
        "docs/pages/about.md",
        "---\ntitle: About\n---\nAbout page.\n",
    );
    write_md(
        root,
        "docs/pages/contact.md",
        "---\ntitle: Contact\n---\nSee [About](/docs/pages/about.md).\n",
    );
    docs
}

#[test]
fn mv_absolute_links_bare_subdir() {
    // --dir docs (relative bare name, the common case)
    let tmp = TempDir::new().unwrap();
    let docs = build_absolute_link_vault(tmp.path());
    let docs_str = docs.to_str().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", docs_str])
        .args([
            "mv",
            "--file",
            "pages/about.md",
            "--to",
            "pages/about-us.md",
            "--dry-run",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json["results"]["total_links_updated"], 2,
        "absolute path --dir: json={json}"
    );
}

#[test]
fn mv_absolute_links_absolute_subdir_path() {
    // --dir <absolute>/docs — same as the bare test but constructed via format!
    // rather than PathBuf.join(), confirming string-composed absolute paths work.
    let tmp = TempDir::new().unwrap();
    build_absolute_link_vault(tmp.path());

    let dir_arg = format!("{}/docs", tmp.path().to_str().unwrap());
    let output = hyalo_no_hints()
        .args(["--dir", &dir_arg])
        .args([
            "mv",
            "--file",
            "pages/about.md",
            "--to",
            "pages/about-us.md",
            "--dry-run",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json["results"]["total_links_updated"], 2,
        "absolute subdir --dir: json={json}"
    );
}

#[test]
fn mv_absolute_links_no_leaked_dir_in_rewrites() {
    // Verify the rewritten link text contains only the vault-relative path,
    // not any leaked --dir value.
    let tmp = TempDir::new().unwrap();
    build_absolute_link_vault(tmp.path());
    let docs = tmp.path().join("docs");

    let output = hyalo_no_hints()
        .args(["--dir", docs.to_str().unwrap()])
        .args([
            "mv",
            "--file",
            "pages/about.md",
            "--to",
            "pages/about-us.md",
            "--dry-run",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let json_str = json.to_string();

    // Rewritten link must not embed the absolute dir path
    assert!(
        !json_str.contains(tmp.path().to_str().unwrap()),
        "leaked tmp root in output: {json_str}"
    );
    // New link text should reference vault-relative path only
    assert!(
        json_str.contains("pages/about-us.md"),
        "expected pages/about-us.md in output: {json_str}"
    );
}

#[test]
fn mv_site_prefix_cli_flag_overrides_auto_derive() {
    // --site-prefix explicitly set to "docs" when --dir is the absolute path.
    // The result should be the same as auto-derivation.
    let tmp = TempDir::new().unwrap();
    build_absolute_link_vault(tmp.path());
    let docs = tmp.path().join("docs");

    let output = hyalo_no_hints()
        .args(["--dir", docs.to_str().unwrap()])
        .args(["--site-prefix", "docs"])
        .args([
            "mv",
            "--file",
            "pages/about.md",
            "--to",
            "pages/about-us.md",
            "--dry-run",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json["results"]["total_links_updated"], 2,
        "--site-prefix=docs: json={json}"
    );
}

#[test]
fn mv_site_prefix_cli_empty_disables_prefix() {
    // --site-prefix "" disables prefix stripping: absolute links won't match.
    let tmp = TempDir::new().unwrap();
    build_absolute_link_vault(tmp.path());
    let docs = tmp.path().join("docs");

    let output = hyalo_no_hints()
        .args(["--dir", docs.to_str().unwrap()])
        .args(["--site-prefix", ""])
        .args([
            "mv",
            "--file",
            "pages/about.md",
            "--to",
            "pages/about-us.md",
            "--dry-run",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // With no prefix, `/docs/pages/about.md` is not resolved as `pages/about.md`
    // so the inbound links from absolute paths won't match.
    assert_eq!(
        json["results"]["total_links_updated"], 0,
        "--site-prefix='': expected 0 links updated, json={json}"
    );
}

#[test]
fn mv_rewrites_self_link() {
    // NEW-BUG-2 regression: `a.md` contains a self-link `[me](a.md)`. When
    // `a.md` is moved to `archive/a.md` the mv must (a) succeed without a
    // canonicalization error on the old path, and (b) leave the self-link
    // pointing at the file's new location. A relative link `a.md` from
    // `archive/a.md` already resolves to `archive/a.md` itself — so no text
    // change is needed for cross-directory moves; the important contract is
    // that the resulting link still resolves to the moved file.
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        md!(r"
---
title: A
---
See [me](a.md) for details.
"),
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "a.md", "--to", "archive/a.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["from"], "a.md");
    assert_eq!(json["results"]["to"], "archive/a.md");

    // The moved file must exist and the self-link must still resolve to the
    // file's new location (i.e. `archive/a.md`). Relative to `archive/a.md`,
    // the link target `a.md` points at `archive/a.md` — same file.
    let content = fs::read_to_string(tmp.path().join("archive/a.md")).unwrap();
    assert!(
        content.contains("[me](a.md)"),
        "self-link must remain valid (resolves to new location): {content}"
    );
}

#[test]
fn mv_rewrites_self_link_same_directory() {
    // NEW-BUG-2: self.md contains `[me](self.md)`. Rename within the same
    // directory must rewrite the self-link to the new filename.
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "self.md",
        md!(r"
---
title: Self
---
See [me](self.md) for details.
"),
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "self.md", "--to", "other.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        json["results"]["total_links_updated"].as_u64().unwrap() >= 1,
        "self-link must be rewritten on same-dir rename: {json}"
    );
    let content = fs::read_to_string(tmp.path().join("other.md")).unwrap();
    assert!(
        content.contains("[me](other.md)"),
        "self-link must point to new filename: {content}"
    );
    assert!(
        !content.contains("[me](self.md)"),
        "old self-link must be gone after rename: {content}"
    );
}

// ---------------------------------------------------------------------------
// BUG-A: bare wikilink rewriting
// ---------------------------------------------------------------------------

#[test]
fn mv_bare_wikilink_all_forms_short_form_preserved() {
    // When the moved file's stem is unique, all short-form wikilinks stay as-is
    // because they already resolve correctly. Only the case-mismatched [[B]] is
    // corrected to [[b]].
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        "See [[b]] and [[b|alias]] and [[b#sec]] and [[b#sec|a]] and [[B]] here.\n",
    );
    write_md(tmp.path(), "b.md", "Content.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "sub/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // Only [[B]] (wrong case) is rewritten to [[b]]; the rest are already correct.
    assert_eq!(
        json["results"]["total_links_updated"].as_u64().unwrap(),
        1,
        "only the case-mismatched [[B]] should be rewritten: {json}"
    );

    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    // Short-form links preserved as-is (they resolve to the moved file)
    assert!(content.contains("[[b]]"), "plain: {content}");
    assert!(content.contains("[[b|alias]]"), "alias: {content}");
    assert!(content.contains("[[b#sec]]"), "fragment: {content}");
    assert!(content.contains("[[b#sec|a]]"), "fragment+alias: {content}");
    // [[B]] corrected to [[b]]
    assert!(
        !content.contains("[[B]]"),
        "case-mismatched [[B]] should be corrected to [[b]]: {content}"
    );
}

#[test]
fn mv_bare_wikilink_dry_run_no_updates_needed_when_short_form() {
    // --dry-run: bare short-form [[b]] and [[b|alias]] need no rewrite because
    // stem "b" is unique and the links already resolve correctly.
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "See [[b]] and [[b|alias]] here.\n");
    write_md(tmp.path(), "b.md", "Content.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "archive/b.md", "--dry-run"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["dry_run"], true);
    // [[b]] and [[b|alias]] are already short-form → no rewrites needed
    assert_eq!(json["results"]["total_links_updated"].as_u64().unwrap(), 0);

    // File was NOT moved
    assert!(tmp.path().join("b.md").exists());
    // Content was NOT changed
    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(
        content.contains("[[b]]"),
        "dry-run must not modify: {content}"
    );
}

#[test]
fn mv_bare_wikilink_unrelated_left_alone() {
    // [[c]] and [[bb]] must not be rewritten when b.md is moved.
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "See [[c]] and [[bb]] here.\n");
    write_md(tmp.path(), "b.md", "B.\n");
    write_md(tmp.path(), "c.md", "C.\n");
    write_md(tmp.path(), "bb.md", "BB.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "archive/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json["results"]["total_links_updated"], 0,
        "no links updated: {json}"
    );
    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(content.contains("[[c]]"), "c link touched: {content}");
    assert!(content.contains("[[bb]]"), "bb link touched: {content}");
}

#[test]
fn mv_bare_wikilink_no_broken_links_after_move() {
    // After mv rewrites bare wikilinks, find --broken-links should report 0.
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "See [[b]] here.\n");
    write_md(tmp.path(), "b.md", "Content.\n");

    // Move b.md → archive/b.md (rewrites [[b]] → [[archive/b]])
    let mv_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "archive/b.md"])
        .output()
        .unwrap();
    assert!(
        mv_out.status.success(),
        "mv failed: {:?}",
        String::from_utf8_lossy(&mv_out.stderr)
    );

    // Now check for broken links
    let check_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--broken-links", "--fields", "links"])
        .output()
        .unwrap();
    assert!(check_out.status.success(), "find failed");
    let json: serde_json::Value = serde_json::from_slice(&check_out.stdout).unwrap();
    assert_eq!(
        json["total"], 0,
        "no broken links expected after mv: {json}"
    );
}

#[test]
fn mv_bare_wikilink_short_form_resolves_after_move() {
    // After mv, a.md's [[b]] stays as short-form (stem unique). Verify the
    // file is at its new location, a.md's body still uses [[b]], and a
    // `links fix` pass reports no broken/case-mismatch/ambiguous findings —
    // the short-form link resolves to the new path via stem lookup.
    //
    // Note: `hyalo backlinks <path>` currently keys on exact relative path
    // and does not stem-resolve short-form references, so it is not
    // asserted here. Stem-resolving backlinks is tracked separately.
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "See [[b]] here.\n");
    write_md(tmp.path(), "b.md", "Content.\n");

    let mv_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "sub/b.md"])
        .output()
        .unwrap();
    assert!(mv_out.status.success(), "mv failed");

    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(
        content.contains("[[b]]"),
        "a.md should still have short-form [[b]]: {content}"
    );
    assert!(
        !tmp.path().join("b.md").exists(),
        "b.md should have been moved"
    );
    assert!(
        tmp.path().join("sub/b.md").exists(),
        "sub/b.md should exist after move"
    );

    let fix_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["links", "fix"])
        .output()
        .unwrap();
    assert!(fix_out.status.success(), "links fix failed");
    let json: serde_json::Value = serde_json::from_slice(&fix_out.stdout).unwrap();
    assert_eq!(
        json["broken"].as_array().map_or(0, Vec::len),
        0,
        "short-form [[b]] should resolve to sub/b.md after move: {json}"
    );
    assert_eq!(
        json["case_mismatches"].as_array().map_or(0, Vec::len),
        0,
        "no case_mismatches expected: {json}"
    );
    assert_eq!(
        json["ambiguous"].as_array().map_or(0, Vec::len),
        0,
        "no ambiguous expected: {json}"
    );
}

// ---------------------------------------------------------------------------
// BUG-1: mv should rewrite [[./relative]] wikilinks (iter-133)
// ---------------------------------------------------------------------------

#[test]
fn mv_dot_slash_wikilink_plain_rewritten() {
    // [[./b]] in a.md (root) should be rewritten when b.md is moved to sub/b.md.
    // iter-151 NEW-2: DotRelative preserves the `./` prefix when the linker is
    // at vault root (source_dir=""), since the new target is still reachable
    // as `./sub/b` from root. Result: [[./sub/b]].
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "See [[./b]] here.\n");
    write_md(tmp.path(), "b.md", "Content.\n");

    let mv_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "sub/b.md"])
        .output()
        .unwrap();
    assert!(
        mv_out.status.success(),
        "mv failed: {:?}",
        String::from_utf8_lossy(&mv_out.stderr)
    );

    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    // DotRelative → ./sub/b (form preserved, path updated; iter-151 NEW-2)
    assert!(
        content.contains("[[./sub/b]]"),
        "[[./b]] should be rewritten to [[./sub/b]] (dot-relative preserved), got: {content}"
    );
}

#[test]
fn mv_dot_slash_wikilink_with_alias_rewritten() {
    // [[./b|Alias]] should be rewritten with the `./` prefix preserved (iter-151 NEW-2).
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "See [[./b|My Note]] here.\n");
    write_md(tmp.path(), "b.md", "Content.\n");

    let mv_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "sub/b.md"])
        .output()
        .unwrap();
    assert!(mv_out.status.success(), "mv failed");

    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    // DotRelative preserved, alias preserved
    assert!(
        content.contains("[[./sub/b|My Note]]"),
        "[[./b|My Note]] should become [[./sub/b|My Note]], got: {content}"
    );
}

#[test]
fn mv_dot_slash_wikilink_with_section_rewritten() {
    // [[./b#intro]] should be rewritten with the `./` prefix preserved (iter-151 NEW-2).
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "See [[./b#intro]] here.\n");
    write_md(tmp.path(), "b.md", "Content.\n");

    let mv_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "sub/b.md"])
        .output()
        .unwrap();
    assert!(mv_out.status.success(), "mv failed");

    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    // DotRelative preserved, fragment preserved
    assert!(
        content.contains("[[./sub/b#intro]]"),
        "[[./b#intro]] should become [[./sub/b#intro]], got: {content}"
    );
}

#[test]
fn mv_dot_slash_wikilink_unrelated_not_rewritten() {
    // [[./c]] should NOT be rewritten when b.md is moved.
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "See [[./c]] here.\n");
    write_md(tmp.path(), "b.md", "Content.\n");
    write_md(tmp.path(), "c.md", "Other.\n");

    let mv_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "sub/b.md"])
        .output()
        .unwrap();
    assert!(mv_out.status.success(), "mv failed");

    let json: serde_json::Value = serde_json::from_slice(&mv_out.stdout).unwrap();
    assert_eq!(
        json["results"]["total_links_updated"], 0,
        "[[./c]] should not be rewritten when moving unrelated file b.md"
    );

    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(
        content.contains("[[./c]]"),
        "[[./c]] should not be rewritten when b.md is moved, got: {content}"
    );
}

// ---------------------------------------------------------------------------
// H-3: `mv` must not escape the vault through a symlinked destination
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn mv_single_symlink_escape_rejected() {
    // `vault/escapelink` is a symlink pointing outside the vault. Moving a
    // file into it must be rejected before any filesystem mutation happens.
    let vault = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    write_md(vault.path(), "note.md", "---\ntitle: src\n---\nbody\n");
    unix_fs::symlink(outside.path(), vault.path().join("escapelink")).unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["mv", "--file", "note.md", "--to", "escapelink/escaped.md"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected non-zero exit, got success: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("outside vault boundary"),
        "expected vault-boundary error, got: {stderr}"
    );

    // Source untouched; nothing written outside the vault.
    assert!(
        vault.path().join("note.md").exists(),
        "source file must remain unmoved"
    );
    assert!(
        !outside.path().join("escaped.md").exists(),
        "file must not have escaped the vault"
    );
}

#[cfg(unix)]
#[test]
fn mv_single_symlink_escape_nested_dirs_rejected() {
    // The destination's parent directories don't exist yet — `create_dir_all`
    // must not be allowed to fabricate directories outside the vault by
    // walking through the symlink.
    let vault = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    write_md(vault.path(), "note.md", "---\ntitle: src\n---\nbody\n");
    unix_fs::symlink(outside.path(), vault.path().join("escapelink")).unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args([
            "mv",
            "--file",
            "note.md",
            "--to",
            "escapelink/deep/nested/escaped.md",
        ])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected non-zero exit, got success: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        vault.path().join("note.md").exists(),
        "source file must remain unmoved"
    );
    assert!(
        !outside.path().join("deep").exists(),
        "create_dir_all must not fabricate directories outside the vault"
    );
}

#[cfg(unix)]
#[test]
fn mv_single_symlink_free_still_works() {
    // Sanity check: a legitimate in-vault move with no symlinks involved
    // must continue to work after the H-3 vault-boundary check was added.
    let vault = TempDir::new().unwrap();
    write_md(vault.path(), "note.md", "---\ntitle: src\n---\nbody\n");

    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["mv", "--file", "note.md", "--to", "sub/dir/note.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!vault.path().join("note.md").exists());
    assert!(vault.path().join("sub/dir/note.md").exists());
}

// ---------------------------------------------------------------------------
// H-4: single-file `mv` must rewrite inbound frontmatter wikilinks too
// ---------------------------------------------------------------------------

#[test]
fn mv_single_file_rewrites_frontmatter_wikilink() {
    // Regression test for H-4: single-file mv previously only rewrote body
    // wikilinks via `plan_inbound_rewrites`, leaving frontmatter link
    // properties (related/depends-on/supersedes/superseded-by) dangling.
    // Mirrors `t12_frontmatter_wikilink_rewrite` (mv_batch.rs) for the
    // single-file path.
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        md!(r#"
---
title: A
related:
  - "[[notes/b]]"
---
body [[notes/b]]
"#),
    );
    write_md(tmp.path(), "notes/b.md", "---\ntitle: B\n---\nc\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "notes/b.md", "--to", "archive/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json["results"]["total_links_updated"], 2,
        "both the frontmatter and body wikilinks must be counted: {json}"
    );

    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(
        content.contains("- \"[[archive/b]]\""),
        "frontmatter wikilink must be rewritten: {content}"
    );
    assert!(
        content.contains("body [[archive/b]]"),
        "body wikilink must be rewritten: {content}"
    );
    assert!(
        !content.contains("notes/b"),
        "old target must be gone everywhere: {content}"
    );
}

#[test]
fn mv_single_file_rewrites_aliased_frontmatter_wikilink() {
    // Aliased wikilinks (`[[path|Label]]`) inside frontmatter must preserve
    // the alias while the path portion is rewritten.
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        md!(r#"
---
title: A
related:
  - "[[notes/b|My Label]]"
---
body.
"#),
    );
    write_md(tmp.path(), "notes/b.md", "---\ntitle: B\n---\nc\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "notes/b.md", "--to", "archive/b.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(
        content.contains("[[archive/b|My Label]]"),
        "alias must be preserved while path is rewritten: {content}"
    );
}

// ---------------------------------------------------------------------------
// iter-178 Phase A: frontmatter anchor / self-link / stale-graph / case-rename
// ---------------------------------------------------------------------------

#[test]
fn mv_inbound_frontmatter_anchor_preserved() {
    // L-2: an anchored frontmatter wikilink in another file's `related` list is
    // rewritten AND keeps its `#anchor` when the target moves.
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        md!(r#"
---
title: A
related:
  - "[[decision-log#DEC-041]]"
---
Body.
"#),
    );
    write_md(
        tmp.path(),
        "decision-log.md",
        md!(r"
---
title: Log
---
Content.
"),
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "mv",
            "--file",
            "decision-log.md",
            "--to",
            "decision-log-archive.md",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(
        content.contains("[[decision-log-archive#DEC-041]]"),
        "frontmatter anchor must be preserved on mv: {content}"
    );
    assert!(
        !content.contains("[[decision-log#DEC-041]]"),
        "stale anchored frontmatter link must be gone: {content}"
    );
}

#[test]
fn mv_self_referencing_frontmatter_link_rewritten() {
    // L-1: the moved file's OWN frontmatter self-link survives a plain rename
    // (previously left dangling at the old path).
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        md!(r#"
---
title: A
related:
  - "[[a]]"
---
Self body [[a]].
"#),
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "a.md", "--to", "a-renamed.md"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(tmp.path().join("a-renamed.md")).unwrap();
    assert!(
        content.contains("related:")
            && content.contains("[[a-renamed]]")
            && !content.contains("[[a]]"),
        "self-referencing frontmatter link must be rewritten: {content}"
    );
    // Both frontmatter and body occurrences point at the new name.
    assert_eq!(
        content.matches("[[a-renamed]]").count(),
        2,
        "frontmatter + body self-links both rewritten: {content}"
    );
}

#[test]
fn mv_then_links_fix_frontmatter_anchor_roundtrip_no_lost_anchor() {
    // Locking matrix: even if a user renames a file the "wrong" way and then
    // runs `links fix`, the anchor must never be dropped. Here we simulate a
    // pre-existing broken anchored frontmatter link and repair it.
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        md!(r#"
---
title: A
related:
  - "[[decision-log#DEC-041]]"
---
Body.
"#),
    );
    // Only the archive file exists on disk → the frontmatter link is broken and
    // `links fix` should repair it to the archive stem, keeping the anchor.
    write_md(
        tmp.path(),
        "decision-log-archive.md",
        md!(r"
---
title: Log
---
Content.
"),
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["links", "fix", "--apply"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(
        content.contains("[[decision-log-archive#DEC-041]]"),
        "links fix must keep the anchor when repairing frontmatter: {content}"
    );
}

#[test]
fn mv_index_refreshes_source_link_graph() {
    // L-5: after `mv --index`, a fresh `backlinks --index` query must reflect
    // the rewritten source outbound links (previously refresh_entry left the
    // persisted graph stale, so this returned zero).
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        md!(r"
---
title: A
---
See [[sub/b]] for details.
"),
    );
    write_md(
        tmp.path(),
        "sub/b.md",
        md!(r"
---
title: B
---
Content.
"),
    );

    let dir = tmp.path().to_str().unwrap();

    let create = hyalo_no_hints()
        .args(["--dir", dir])
        .args(["--format", "json"])
        .args(["create-index"])
        .output()
        .unwrap();
    assert!(create.status.success(), "create-index failed: {create:?}");

    let mv = hyalo_no_hints()
        .args(["--dir", dir])
        .args(["--format", "json"])
        .args([
            "mv",
            "--file",
            "sub/b.md",
            "--to",
            "archive/b.md",
            "--index",
        ])
        .output()
        .unwrap();
    assert!(mv.status.success(), "mv --index failed: {mv:?}");

    // A fresh process querying backlinks --index must see the rewritten source.
    let indexed = hyalo_no_hints()
        .args(["--dir", dir])
        .args(["--format", "json"])
        .args(["backlinks", "archive/b.md", "--index"])
        .output()
        .unwrap();
    assert!(
        indexed.status.success(),
        "backlinks --index failed: {indexed:?}"
    );
    let indexed_json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&indexed.stdout)).unwrap();

    let live = hyalo_no_hints()
        .args(["--dir", dir])
        .args(["--format", "json"])
        .args(["backlinks", "archive/b.md"])
        .output()
        .unwrap();
    assert!(live.status.success(), "backlinks (live) failed: {live:?}");
    let live_json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&live.stdout)).unwrap();

    assert_eq!(
        indexed_json["total"], live_json["total"],
        "indexed and live backlinks must agree after mv --index"
    );
    assert_eq!(
        indexed_json["total"], 1,
        "a.md should back-link to the moved file: {indexed_json}"
    );
    assert_eq!(indexed_json["results"]["backlinks"][0]["source"], "a.md");
}

/// Returns true when the temp dir lives on a case-insensitive filesystem
/// (macOS APFS default, Windows NTFS). On such filesystems `a.md` and `A.md`
/// resolve to the same inode.
fn fs_is_case_insensitive(dir: &std::path::Path) -> bool {
    let lower = dir.join("__case_probe__.tmp");
    let upper = dir.join("__CASE_PROBE__.tmp");
    let _ = fs::write(&lower, b"x");
    let insensitive = upper.exists();
    let _ = fs::remove_file(&lower);
    let _ = fs::remove_file(&upper);
    insensitive
}

#[test]
fn mv_case_only_rename_on_case_insensitive_fs() {
    // L-14: `a.md` -> `A.md` must succeed on a case-insensitive filesystem
    // (previously rejected as "target file already exists" because the
    // destination resolves to the same inode as the source).
    let tmp = TempDir::new().unwrap();
    if !fs_is_case_insensitive(tmp.path()) {
        // On a case-sensitive FS this is just an ordinary rename; the specific
        // regression can't be reproduced here, so skip.
        return;
    }
    write_md(
        tmp.path(),
        "a.md",
        md!(r"
---
title: A
---
Content.
"),
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "a.md", "--to", "A.md"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "case-only rename must succeed; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // The on-disk name should now be the uppercase form.
    let entries: Vec<String> = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(std::result::Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.eq_ignore_ascii_case("a.md"))
        .collect();
    assert_eq!(entries, vec!["A.md".to_string()], "expected A.md on disk");
}

// ---------------------------------------------------------------------------
// iter-181 task 5: positional destination is an alias for --to
// ---------------------------------------------------------------------------

#[test]
fn mv_positional_destination_moves_file() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "old.md", "---\ntitle: Old\n---\nBody.\n");
    write_md(tmp.path(), "ref.md", "---\ntitle: Ref\n---\nSee [[old]].\n");

    // `hyalo mv old.md new.md` — DEST positionally, no --to.
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "old.md", "new.md"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        !tmp.path().join("old.md").exists(),
        "old.md should have been moved"
    );
    assert!(tmp.path().join("new.md").exists(), "new.md should exist");
    // Links are rewritten just like the --to form.
    let refbody = fs::read_to_string(tmp.path().join("ref.md")).unwrap();
    assert!(
        refbody.contains("[[new]]"),
        "link should be rewritten, got:\n{refbody}"
    );
}

#[test]
fn mv_positional_destination_matches_to_flag() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "---\ntitle: A\n---\n");

    let positional = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "a.md", "b.md"])
        .output()
        .unwrap();
    assert!(positional.status.success());
    assert!(tmp.path().join("b.md").exists());
}

#[test]
fn mv_positional_and_to_flag_conflict() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "---\ntitle: A\n---\n");

    // Providing both the positional DEST and --to is rejected by clap.
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "a.md", "b.md", "--to", "c.md"])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "expected conflict between positional DEST and --to"
    );
}

#[test]
fn mv_batch_without_destination_is_user_error() {
    // Batch mode (selector-driven) with neither --to nor a positional DEST:
    // clap allows this (DEST requires the positional source, which conflicts
    // with --glob), so dispatch.rs must reject it itself rather than panicking
    // or unwrapping a `None` destination.
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "---\ntitle: A\n---\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--glob", "*.md"])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "expected a user error, not success"
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "expected exit 1 (user error)"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("destination"),
        "expected a destination-related error, got: {stderr}"
    );
}
