use super::common::{hyalo_no_hints, md, write_md};
use std::fs;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// `hyalo mv` — move/rename file with link updates
// ---------------------------------------------------------------------------

#[test]
fn mv_bare_wikilink_rewritten_unique_stem() {
    // Bare wikilinks are now rewritten via case-insensitive stem lookup
    // when the stem is unambiguous (exactly one file in the vault with that stem).
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
    assert_eq!(
        json["results"]["total_links_updated"], 1,
        "bare wikilink should be rewritten"
    );

    // Verify file was moved
    assert!(!tmp.path().join("b.md").exists());
    assert!(tmp.path().join("archive/b.md").exists());

    // Bare wikilink should be updated to the new path
    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(
        content.contains("[[archive/b]]"),
        "bare wikilink should be rewritten: {content}"
    );
    assert!(
        !content.contains("[[b]]"),
        "old bare wikilink should be gone: {content}"
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
fn mv_updates_wikilink_with_path() {
    // Wikilinks that contain a path separator ARE rewritten.
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
    // The code block should still have [[sub/b]], not [[archive/b]]
    assert!(
        content.contains("```\n[[sub/b]]\n```"),
        "code block was modified: {content}"
    );
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
fn mv_bare_wikilink_all_forms_rewritten() {
    // All bare wikilink forms are rewritten when the stem is unambiguous.
    // Covers [[b]], [[b|alias]], [[b#sec]], [[b#sec|alias]], [[B]] (case).
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
    assert_eq!(
        json["results"]["total_links_updated"].as_u64().unwrap(),
        5,
        "all 5 wikilink forms should be rewritten: {json}"
    );

    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(content.contains("[[sub/b]]"), "plain: {content}");
    assert!(content.contains("[[sub/b|alias]]"), "alias: {content}");
    assert!(content.contains("[[sub/b#sec]]"), "fragment: {content}");
    assert!(
        content.contains("[[sub/b#sec|a]]"),
        "fragment+alias: {content}"
    );
    // The original `[[B]]` (uppercase) must no longer appear as a bare wikilink;
    // it must have been rewritten to either `[[sub/b]]` or `[[sub/B]]`.
    assert!(
        !content.contains("[[B]]"),
        "case-mismatched bare wikilink [[B]] was not rewritten: {content}"
    );
}

#[test]
fn mv_bare_wikilink_dry_run_shows_rewrites() {
    // --dry-run should preview bare wikilink rewrites without writing.
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
    assert_eq!(json["results"]["total_links_updated"].as_u64().unwrap(), 2);

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
fn mv_bare_wikilink_backlinks_updated() {
    // After mv rewrites bare wikilinks, backlinks on the new path should list a.md.
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "See [[b]] here.\n");
    write_md(tmp.path(), "b.md", "Content.\n");

    // Move b.md → sub/b.md
    let mv_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "sub/b.md"])
        .output()
        .unwrap();
    assert!(mv_out.status.success(), "mv failed");

    // Check backlinks on sub/b.md
    let bl_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["backlinks", "sub/b.md"])
        .output()
        .unwrap();
    assert!(bl_out.status.success(), "backlinks failed");
    let json: serde_json::Value = serde_json::from_slice(&bl_out.stdout).unwrap();
    let backlinks = json["results"]["backlinks"].as_array().unwrap();
    assert!(
        !backlinks.is_empty(),
        "a.md should appear as backlink for sub/b.md"
    );
}
