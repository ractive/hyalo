mod common;

use common::{hyalo, md, write_md};
use tempfile::TempDir;

fn setup_vault_nested() -> TempDir {
    let tmp = TempDir::new().unwrap();

    write_md(
        tmp.path(),
        "root.md",
        md!(r"
---
title: Root
status: published
---
"),
    );

    write_md(
        tmp.path(),
        "notes/n.md",
        md!(r"
---
title: N
status: draft
---
"),
    );

    write_md(
        tmp.path(),
        "notes/sub/s.md",
        md!(r"
---
title: S
status: draft
---
"),
    );

    tmp
}

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

fn setup_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();

    write_md(
        tmp.path(),
        "notes/alpha.md",
        md!(r#"
---
title: Alpha
status: draft
tags:
  - rust
  - cli
---
# Alpha

- [ ] Open task
- [x] Done task
"#),
    );

    write_md(
        tmp.path(),
        "notes/beta.md",
        md!(r#"
---
title: Beta
status: draft
tags:
  - rust
---
# Beta

- [x] Completed
"#),
    );

    write_md(
        tmp.path(),
        "docs/readme.md",
        md!(r#"
---
title: Readme
status: published
tags:
  - docs
---
# Readme

No tasks here.
"#),
    );

    write_md(
        tmp.path(),
        "plain.md",
        md!(r"
# No frontmatter

- [ ] A loose task
"),
    );

    tmp
}

// ---------------------------------------------------------------------------
// Happy paths
// ---------------------------------------------------------------------------

#[test]
fn summary_json_has_all_fields() {
    let tmp = setup_vault();
    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "summary",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    // files
    assert!(json["files"]["total"].as_u64().unwrap() >= 4);
    assert!(json["files"]["by_directory"].is_array());

    // properties
    assert!(json["properties"].is_array());

    // tags
    assert!(json["tags"]["total"].is_number());
    assert!(json["tags"]["tags"].is_array());

    // status
    assert!(json["status"].is_array());

    // tasks
    assert!(json["tasks"]["total"].is_number());
    assert!(json["tasks"]["done"].is_number());

    // recent_files
    assert!(json["recent_files"].is_array());
}

#[test]
fn summary_file_counts_by_directory() {
    let tmp = setup_vault();
    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "summary",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    let total = json["files"]["total"].as_u64().unwrap();
    assert_eq!(total, 4);

    let by_dir = json["files"]["by_directory"].as_array().unwrap();
    // Should have entries for ".", "notes", "docs"
    let dir_names: Vec<&str> = by_dir
        .iter()
        .map(|d| {
            d["directory"]
                .as_str()
                .expect("field 'directory' should be a string")
        })
        .collect();
    assert!(dir_names.contains(&"notes"));
    assert!(dir_names.contains(&"docs"));
    assert!(dir_names.contains(&"."));
}

#[test]
fn summary_task_counts() {
    let tmp = setup_vault();
    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "summary",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    // alpha: 1 open + 1 done, beta: 1 done, plain: 1 open => total 4, done 2
    let total = json["tasks"]["total"].as_u64().unwrap();
    let done = json["tasks"]["done"].as_u64().unwrap();
    assert_eq!(total, 4);
    assert_eq!(done, 2);
}

#[test]
fn summary_status_groups() {
    let tmp = setup_vault();
    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "summary",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    let status = json["status"]
        .as_array()
        .expect("field 'status' should be an array");
    let draft_group = status
        .iter()
        .find(|g| g["value"] == "draft")
        .expect("'draft' status group should be present");
    let draft_files = draft_group["files"]
        .as_array()
        .expect("field 'files' should be an array");
    assert_eq!(draft_files.len(), 2);

    let published_group = status
        .iter()
        .find(|g| g["value"] == "published")
        .expect("'published' status group should be present");
    let published_files = published_group["files"]
        .as_array()
        .expect("field 'files' should be an array");
    assert_eq!(published_files.len(), 1);
}

#[test]
fn summary_tag_counts() {
    let tmp = setup_vault();
    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "summary",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    let tags = &json["tags"];
    let total = tags["total"].as_u64().unwrap();
    assert_eq!(total, 3); // rust, cli, docs

    let tag_entries = tags["tags"]
        .as_array()
        .expect("field 'tags' should be an array");
    let rust_entry = tag_entries
        .iter()
        .find(|t| t["name"] == "rust")
        .expect("'rust' tag should be present");
    assert_eq!(
        rust_entry["count"]
            .as_u64()
            .expect("field 'count' should be a number"),
        2
    ); // alpha + beta
}

#[test]
fn summary_property_summary() {
    let tmp = setup_vault();
    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "summary",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    let props = json["properties"]
        .as_array()
        .expect("field 'properties' should be an array");
    let title_prop = props
        .iter()
        .find(|p| p["name"] == "title")
        .expect("'title' property should be present");
    assert_eq!(
        title_prop["count"]
            .as_u64()
            .expect("field 'count' should be a number"),
        3
    ); // alpha, beta, readme
    assert_eq!(title_prop["type"], "text");
}

#[test]
fn summary_recent_files_limited() {
    let tmp = setup_vault();
    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "summary",
            "--recent",
            "2",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    let recent = json["recent_files"].as_array().unwrap();
    assert_eq!(recent.len(), 2);
    // Each entry should have path and modified
    assert!(recent[0]["path"].is_string());
    assert!(recent[0]["modified"].is_string());
}

#[test]
fn summary_text_format() {
    let tmp = setup_vault();
    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "summary",
            "--format",
            "text",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());

    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("Files: 4 total"));
    assert!(text.contains("Properties:"));
    assert!(text.contains("Tags:"));
    assert!(text.contains("Status:"));
    assert!(text.contains("Tasks: 2/4"));
    assert!(text.contains("Recent:"));
}

#[test]
fn summary_glob_filter() {
    let tmp = setup_vault();
    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "summary",
            "--glob",
            "notes/*.md",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    // Only notes/alpha.md and notes/beta.md
    assert_eq!(json["files"]["total"].as_u64().unwrap(), 2);
    assert_eq!(json["tasks"]["total"].as_u64().unwrap(), 3); // 2 in alpha + 1 in beta
}

#[test]
fn summary_jq_filter() {
    let tmp = setup_vault();
    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "summary",
            "--jq",
            ".tasks.total",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap().trim().to_owned();
    assert_eq!(text, "4");
}

#[test]
fn summary_empty_vault() {
    let tmp = TempDir::new().unwrap();
    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "summary",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["files"]["total"].as_u64().unwrap(), 0);
    assert_eq!(json["tasks"]["total"].as_u64().unwrap(), 0);
    assert_eq!(json["tasks"]["done"].as_u64().unwrap(), 0);
    assert!(json["status"].as_array().unwrap().is_empty());
    assert!(json["recent_files"].as_array().unwrap().is_empty());
}

#[test]
fn summary_depth_zero_collapses_all_dirs() {
    let tmp = setup_vault_nested();
    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "summary",
            "--depth",
            "0",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let by_dir = json["files"]["by_directory"].as_array().unwrap();
    assert_eq!(by_dir.len(), 1);
    assert_eq!(by_dir[0]["directory"], ".");
    assert_eq!(by_dir[0]["count"], 3);
    // Stats are unaffected — all 3 files are still counted
    assert_eq!(json["files"]["total"].as_u64().unwrap(), 3);
}

#[test]
fn summary_depth_one_collapses_sub_into_parent() {
    let tmp = setup_vault_nested();
    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "summary",
            "--depth",
            "1",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let by_dir = json["files"]["by_directory"].as_array().unwrap();
    // "." and "notes" (notes/sub collapsed into notes)
    assert_eq!(by_dir.len(), 2);

    let dot = by_dir.iter().find(|d| d["directory"] == ".").unwrap();
    assert_eq!(dot["count"].as_u64().unwrap(), 1);

    let notes = by_dir.iter().find(|d| d["directory"] == "notes").unwrap();
    assert_eq!(notes["count"].as_u64().unwrap(), 2);

    // notes/sub must NOT appear
    assert!(by_dir.iter().all(|d| d["directory"] != "notes/sub"));
}

#[test]
fn summary_depth_no_flag_shows_all_directories() {
    let tmp = setup_vault_nested();
    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "summary",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let by_dir = json["files"]["by_directory"].as_array().unwrap();
    let dirs: Vec<&str> = by_dir
        .iter()
        .map(|d| d["directory"].as_str().unwrap())
        .collect();
    assert!(dirs.contains(&"."));
    assert!(dirs.contains(&"notes"));
    assert!(dirs.contains(&"notes/sub"));
}

#[test]
fn summary_recent_zero() {
    let tmp = setup_vault();
    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "summary",
            "--recent",
            "0",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json["recent_files"].as_array().unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// Glob negation
// ---------------------------------------------------------------------------

#[test]
fn summary_glob_negation_excludes_files() {
    let tmp = setup_vault();
    // Exclude one of the root-level files; the summary file count should be reduced
    let output = hyalo()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "summary",
            "--glob",
            "!notes/**/*.md",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let total = json["files"]["total"].as_u64().unwrap();
    // The vault has files in notes/ — after excluding them, total should be smaller
    assert!(total > 0, "should still have some files: {total}");
    // Verify no notes/ paths appear in recent files
    for entry in json["recent_files"].as_array().unwrap() {
        let path = entry["path"].as_str().unwrap_or("");
        assert!(
            !path.starts_with("notes/"),
            "notes/ file should be excluded: {path}"
        );
    }
}
