mod common;

use common::{hyalo_no_hints, md, write_md};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// `hyalo backlinks` — reverse link lookup
// ---------------------------------------------------------------------------

#[test]
fn backlinks_finds_wikilinks() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "target.md",
        md!(r"
---
title: Target
---
# Target
Some content.
"),
    );
    write_md(
        tmp.path(),
        "source-a.md",
        md!(r"
---
title: Source A
---
See [[target]] for details.
"),
    );
    write_md(
        tmp.path(),
        "source-b.md",
        md!(r"
---
title: Source B
---
Also links to [[target|the target page]].
"),
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["backlinks", "--file", "target.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["file"], "target.md");
    assert_eq!(json["total"], 2);

    let backlinks = json["backlinks"].as_array().unwrap();
    let sources: Vec<&str> = backlinks
        .iter()
        .map(|b| b["source"].as_str().unwrap())
        .collect();
    assert!(sources.contains(&"source-a.md"));
    assert!(sources.contains(&"source-b.md"));
}

#[test]
fn backlinks_finds_markdown_links() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "notes/target.md", "# Target\n");
    write_md(
        tmp.path(),
        "index.md",
        "See [my note](notes/target.md) for details.\n",
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["backlinks", "--file", "notes/target.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 1);
    assert_eq!(json["backlinks"][0]["source"], "index.md");
}

#[test]
fn backlinks_empty_result() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "lonely.md", "# No one links here\n");
    write_md(tmp.path(), "other.md", "# Other\nNo links.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["backlinks", "--file", "lonely.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 0);
    assert!(json["backlinks"].as_array().unwrap().is_empty());
}

#[test]
fn backlinks_text_format() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "target.md", "# Target\n");
    write_md(tmp.path(), "source.md", "Link to [[target]]\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "text"])
        .args(["backlinks", "--file", "target.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("1 backlink"));
    assert!(text.contains("source.md:"));
}

#[test]
fn backlinks_text_format_empty() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "lonely.md", "# Alone\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "text"])
        .args(["backlinks", "--file", "lonely.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("No backlinks found"));
}

#[test]
fn backlinks_file_not_found() {
    let tmp = TempDir::new().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["backlinks", "--file", "nonexistent.md"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("file not found"));
}

#[test]
fn backlinks_ignores_links_in_code_blocks() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "target.md", "# Target\n");
    write_md(
        tmp.path(),
        "source.md",
        "```\n[[target]]\n```\nReal [[target]] link\n",
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["backlinks", "--file", "target.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // Only the real link outside the code block, not the one inside
    assert_eq!(json["total"], 1);
}

#[test]
fn backlinks_includes_line_numbers() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "target.md",
        md!(r"
---
title: Target
---
# Target
"),
    );
    write_md(
        tmp.path(),
        "source.md",
        md!(r"
---
title: Source
---
First body line.
Link is here: [[target]]
"),
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["backlinks", "--file", "target.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let backlink = &json["backlinks"][0];
    assert_eq!(backlink["source"], "source.md");
    // Lines: 1=---, 2=title, 3=---, 4=body, 5=link
    assert_eq!(backlink["line"], 5);
}

#[test]
fn backlinks_cross_directory_relative_link() {
    // source at `sub/source.md` links via `[text](../target.md)`.
    // The CLI must resolve the `../` and find the link when queried as
    // `backlinks --file target.md`.
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "target.md", "# Target\n");
    std::fs::create_dir_all(tmp.path().join("sub")).unwrap();
    write_md(
        tmp.path(),
        "sub/source.md",
        "See [the target](../target.md) for more.\n",
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["backlinks", "--file", "target.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 1);
    assert_eq!(json["backlinks"][0]["source"], "sub/source.md");
}

#[test]
fn backlinks_wikilink_without_extension_finds_md_file() {
    // source links `[[notes]]` (no extension); query uses `notes.md`.
    // The cross-form matching in `backlinks()` must bridge the gap.
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "notes.md", "# Notes\n");
    write_md(tmp.path(), "source.md", "See [[notes]] for details.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["backlinks", "--file", "notes.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 1);
    assert_eq!(json["backlinks"][0]["source"], "source.md");
}

#[test]
fn backlinks_with_jq_filter() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "target.md", "# Target\n");
    write_md(tmp.path(), "a.md", "[[target]]\n");
    write_md(tmp.path(), "b.md", "[[target]]\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", ".total"])
        .args(["backlinks", "--file", "target.md"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    assert_eq!(text, "2");
}

#[test]
fn backlinks_wikilink_with_path_separator() {
    // [[backlog/item]] written in any file must be found when querying
    // backlinks for "backlog/item.md" — vault-relative wikilinks must NOT
    // be normalized as if they were relative markdown paths.
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "backlog/item.md", "# Item\nContent\n");
    write_md(
        tmp.path(),
        "iterations/iter-1.md",
        "See [[backlog/item]] for context.\n",
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["backlinks", "--file", "backlog/item.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 1, "expected 1 backlink, got: {json}");
    assert_eq!(json["backlinks"][0]["source"], "iterations/iter-1.md");
}

#[test]
fn backlinks_resolves_absolute_links_with_dir_config() {
    // When .hyalo.toml sets `dir = "docs"`, a site-absolute link like
    // `/docs/target.md` in source.md must resolve to `target.md` within
    // the vault and show up as a backlink.
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "docs/source.md", "[link](/docs/target.md)\n");
    write_md(tmp.path(), "docs/target.md", "# Target\n");
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \"docs\"\n").unwrap();

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["backlinks", "--file", "target.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 1, "expected 1 backlink, got: {json}");
    assert_eq!(json["backlinks"][0]["source"], "source.md");
}

#[test]
fn backlinks_excludes_self_links() {
    // a.md links to itself — self-link must not appear in its own backlinks list.
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "Self-ref: [[a]]\n");
    write_md(tmp.path(), "b.md", "Link to [[a]]\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["backlinks", "--file", "a.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 1, "self-link must be excluded, got: {json}");
    assert_eq!(json["backlinks"][0]["source"], "b.md");
}

#[test]
fn backlinks_wikilink_with_path_from_subdirectory() {
    // A file in sub/ linking [[other/target]] must store the link as
    // "other/target", not "sub/other/target" (the incorrect normalized form).
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "other/target.md", "# Target\n");
    write_md(tmp.path(), "sub/source.md", "See [[other/target]] here.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["backlinks", "--file", "other/target.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 1, "expected 1 backlink, got: {json}");
    assert_eq!(json["backlinks"][0]["source"], "sub/source.md");
}
