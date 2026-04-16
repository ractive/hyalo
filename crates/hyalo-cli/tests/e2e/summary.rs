use super::common::{hyalo_no_hints, md, write_md};
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
        md!(r"
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
"),
    );

    write_md(
        tmp.path(),
        "notes/beta.md",
        md!(r"
---
title: Beta
status: draft
tags:
  - rust
---
# Beta

- [x] Completed
"),
    );

    write_md(
        tmp.path(),
        "docs/readme.md",
        md!(r"
---
title: Readme
status: published
tags:
  - docs
---
# Readme

No tasks here.
"),
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
    let output = hyalo_no_hints()
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
    assert!(json["results"]["files"]["total"].as_u64().unwrap() >= 4);
    assert!(json["results"]["files"]["directories"].is_array());

    // properties
    assert!(json["results"]["properties"].is_array());

    // tags
    assert!(json["results"]["tags"]["total"].is_number());
    assert!(json["results"]["tags"]["tags"].is_array());

    // status
    assert!(json["results"]["status"].is_array());

    // tasks
    assert!(json["results"]["tasks"]["total"].is_number());
    assert!(json["results"]["tasks"]["done"].is_number());

    // recent_files
    assert!(json["results"]["recent_files"].is_array());
}

#[test]
fn summary_file_counts_by_directory() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
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

    let total = json["results"]["files"]["total"].as_u64().unwrap();
    assert_eq!(total, 4);

    let by_dir = json["results"]["files"]["directories"].as_array().unwrap();
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
    let output = hyalo_no_hints()
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
    let total = json["results"]["tasks"]["total"].as_u64().unwrap();
    let done = json["results"]["tasks"]["done"].as_u64().unwrap();
    assert_eq!(total, 4);
    assert_eq!(done, 2);
}

#[test]
fn summary_status_groups() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
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

    let status = json["results"]["status"]
        .as_array()
        .expect("field 'status' should be an array");
    let draft_group = status
        .iter()
        .find(|g| g["value"] == "draft")
        .expect("'draft' status group should be present");
    assert_eq!(draft_group["count"].as_u64().unwrap(), 2);

    let published_group = status
        .iter()
        .find(|g| g["value"] == "published")
        .expect("'published' status group should be present");
    assert_eq!(published_group["count"].as_u64().unwrap(), 1);
}

#[test]
fn summary_tag_counts() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
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

    let tags = &json["results"]["tags"];
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
    let output = hyalo_no_hints()
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

    let props = json["results"]["properties"]
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
    let output = hyalo_no_hints()
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

    let recent = json["results"]["recent_files"].as_array().unwrap();
    assert_eq!(recent.len(), 2);
    // Each entry should have path and modified
    assert!(recent[0]["path"].is_string());
    assert!(recent[0]["modified"].is_string());
}

#[test]
fn summary_text_format() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
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
    assert!(text.contains("Files: 4"));
    assert!(text.contains("Properties:"));
    assert!(text.contains("Tags:"));
    assert!(text.contains("Status:"));
    assert!(text.contains("Tasks: 2/4"));
    assert!(text.contains("Orphans:"));
    assert!(text.contains("Recent:"));
}

#[test]
fn summary_glob_filter() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
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
    assert_eq!(json["results"]["files"]["total"].as_u64().unwrap(), 2);
    assert_eq!(json["results"]["tasks"]["total"].as_u64().unwrap(), 3); // 2 in alpha + 1 in beta
}

#[test]
fn summary_jq_filter() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "summary",
            "--jq",
            ".results.tasks.total",
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
    let output = hyalo_no_hints()
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
    assert_eq!(json["results"]["files"]["total"].as_u64().unwrap(), 0);
    assert_eq!(json["results"]["tasks"]["total"].as_u64().unwrap(), 0);
    assert_eq!(json["results"]["tasks"]["done"].as_u64().unwrap(), 0);
    assert!(json["results"]["status"].as_array().unwrap().is_empty());
    assert!(
        json["results"]["recent_files"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn summary_depth_zero_collapses_all_dirs() {
    let tmp = setup_vault_nested();
    let output = hyalo_no_hints()
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
    let by_dir = json["results"]["files"]["directories"].as_array().unwrap();
    assert_eq!(by_dir.len(), 1);
    assert_eq!(by_dir[0]["directory"], ".");
    assert_eq!(by_dir[0]["count"], 3);
    // Stats are unaffected — all 3 files are still counted
    assert_eq!(json["results"]["files"]["total"].as_u64().unwrap(), 3);
}

#[test]
fn summary_depth_one_collapses_sub_into_parent() {
    let tmp = setup_vault_nested();
    let output = hyalo_no_hints()
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
    let by_dir = json["results"]["files"]["directories"].as_array().unwrap();
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
fn summary_depth_no_flag_defaults_to_depth_one() {
    let tmp = setup_vault_nested();
    let output = hyalo_no_hints()
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
    let by_dir = json["results"]["files"]["directories"].as_array().unwrap();
    // Default depth is 1: "notes/sub" collapsed into "notes"
    assert_eq!(by_dir.len(), 2);
    let dirs: Vec<&str> = by_dir
        .iter()
        .map(|d| d["directory"].as_str().unwrap())
        .collect();
    assert!(dirs.contains(&"."));
    assert!(dirs.contains(&"notes"));
}

#[test]
fn summary_recent_zero() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
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
    assert!(
        json["results"]["recent_files"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

// ---------------------------------------------------------------------------
// Glob negation
// ---------------------------------------------------------------------------

#[test]
fn summary_json_has_orphans_field() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
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
    assert!(json["results"]["orphans"].is_number());
    assert!(json["results"]["dead_ends"].is_number());
}

#[test]
fn summary_orphans_detects_unlinked_files() {
    let tmp = TempDir::new().unwrap();

    // a.md links to b, so a has outbound (not orphan) and b has inbound (not orphan).
    // Only c.md is fully isolated (no links in or out).
    write_md(
        tmp.path(),
        "a.md",
        md!(r"
---
title: A
---
See [[b]]
"),
    );
    write_md(
        tmp.path(),
        "b.md",
        md!(r"
---
title: B
---
Content
"),
    );
    write_md(
        tmp.path(),
        "c.md",
        md!(r"
---
title: C
---
No links to me
"),
    );

    let output = hyalo_no_hints()
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
    // c.md is the only orphan: no inbound AND no outbound links
    assert_eq!(json["results"]["orphans"].as_u64().unwrap(), 1);
}

#[test]
fn summary_orphans_no_orphans_when_all_linked() {
    let tmp = TempDir::new().unwrap();

    // Circular links: a→b, b→a — neither is an orphan
    write_md(
        tmp.path(),
        "a.md",
        md!(r"
---
title: A
---
See [[b]]
"),
    );
    write_md(
        tmp.path(),
        "b.md",
        md!(r"
---
title: B
---
See [[a]]
"),
    );

    let output = hyalo_no_hints()
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

    assert_eq!(json["results"]["orphans"].as_u64().unwrap(), 0);
}

#[test]
fn summary_orphans_all_orphans_when_no_links() {
    let tmp = TempDir::new().unwrap();

    write_md(
        tmp.path(),
        "a.md",
        md!(r"
---
title: A
---
No links
"),
    );
    write_md(
        tmp.path(),
        "b.md",
        md!(r"
---
title: B
---
No links
"),
    );

    let output = hyalo_no_hints()
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

    // Both files are orphans
    assert_eq!(json["results"]["orphans"].as_u64().unwrap(), 2);
}

#[test]
fn summary_orphans_text_format() {
    let tmp = TempDir::new().unwrap();

    write_md(
        tmp.path(),
        "a.md",
        md!(r"
---
title: A
---
No links
"),
    );

    let output = hyalo_no_hints()
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
    assert!(
        text.contains("Orphans: 1"),
        "expected 'Orphans: 1' in: {text}"
    );
}

#[test]
fn summary_orphans_empty_vault() {
    let tmp = TempDir::new().unwrap();
    let output = hyalo_no_hints()
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
    assert_eq!(json["results"]["orphans"].as_u64().unwrap(), 0);
}

// ---------------------------------------------------------------------------
// Orphan + glob interaction
// ---------------------------------------------------------------------------

#[test]
fn summary_orphans_glob_uses_vault_wide_links() {
    let tmp = TempDir::new().unwrap();

    // root.md (outside glob) links to notes/a.md (inside glob).
    // notes/b.md has no links at all.
    write_md(
        tmp.path(),
        "root.md",
        md!(r"
---
title: Root
---
See [[notes/a]]
"),
    );
    write_md(
        tmp.path(),
        "notes/a.md",
        md!(r"
---
title: A
---
Content
"),
    );
    write_md(
        tmp.path(),
        "notes/b.md",
        md!(r"
---
title: B
---
No links
"),
    );

    let output = hyalo_no_hints()
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
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // Summary only counts glob-matched files
    assert_eq!(json["results"]["files"]["total"].as_u64().unwrap(), 2);

    // notes/a.md is linked from root.md (outside glob) — NOT an orphan.
    // notes/b.md has no links in or out — orphan. So 1 orphan.
    assert_eq!(json["results"]["orphans"].as_u64().unwrap(), 1);
}

// ---------------------------------------------------------------------------
// Glob negation
// ---------------------------------------------------------------------------

#[test]
fn summary_glob_negation_excludes_files() {
    let tmp = setup_vault();
    // Exclude one of the root-level files; the summary file count should be reduced
    let output = hyalo_no_hints()
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
    let total = json["results"]["files"]["total"].as_u64().unwrap();
    // The vault has files in notes/ — after excluding them, total should be smaller
    assert!(total > 0, "should still have some files: {total}");
    // Verify no notes/ paths appear in recent files
    for entry in json["results"]["recent_files"].as_array().unwrap() {
        let path = entry["path"].as_str().unwrap_or("");
        assert!(
            !path.starts_with("notes/"),
            "notes/ file should be excluded: {path}"
        );
    }
}

/// Orphan lists from disk scan and snapshot index must be identical,
/// even when absolute links and `--site-prefix` are involved.
#[test]
fn summary_orphan_parity_disk_vs_index_with_site_prefix() {
    let tmp = TempDir::new().unwrap();
    let dir_str = tmp.path().to_str().unwrap();

    // a.md → b via wikilink
    write_md(
        tmp.path(),
        "a.md",
        md!(r"
---
title: A
---
[[b]]
"),
    );
    // b.md → c via absolute link (needs site_prefix to resolve)
    write_md(
        tmp.path(),
        "b.md",
        md!(r"
---
title: B
---
[see c](/docs/c.md)
"),
    );
    // c.md: inbound from b only if site_prefix=docs
    write_md(
        tmp.path(),
        "c.md",
        md!(r"
---
title: C
---
No links.
"),
    );
    // orphan.md: no links
    write_md(
        tmp.path(),
        "orphan.md",
        md!(r"
---
title: Orphan
---
No links.
"),
    );

    // Disk scan with site_prefix
    let disk_out = hyalo_no_hints()
        .args([
            "--dir",
            dir_str,
            "--site-prefix",
            "docs",
            "--format",
            "json",
        ])
        .arg("summary")
        .output()
        .unwrap();
    assert!(
        disk_out.status.success(),
        "disk summary failed: {}",
        String::from_utf8_lossy(&disk_out.stderr)
    );
    let disk_json: serde_json::Value = serde_json::from_slice(&disk_out.stdout).unwrap();
    let disk_orphans = disk_json["results"]["orphans"].as_u64().unwrap();

    // Create index with same site_prefix
    let idx_create = hyalo_no_hints()
        .args(["--dir", dir_str, "--site-prefix", "docs"])
        .arg("create-index")
        .output()
        .unwrap();
    assert!(
        idx_create.status.success(),
        "create-index failed: {}",
        String::from_utf8_lossy(&idx_create.stderr)
    );

    // Index-based summary
    let idx_out = hyalo_no_hints()
        .args(["--dir", dir_str, "--site-prefix", "docs"])
        .args(["summary", "--index", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        idx_out.status.success(),
        "index summary failed: {}",
        String::from_utf8_lossy(&idx_out.stderr)
    );
    let idx_json: serde_json::Value = serde_json::from_slice(&idx_out.stdout).unwrap();
    let idx_orphans = idx_json["results"]["orphans"].as_u64().unwrap();

    assert_eq!(
        disk_orphans, idx_orphans,
        "disk scan and index orphan counts must match"
    );
    // orphan.md is the only orphan (c.md is not orphan with site_prefix=docs)
    assert_eq!(disk_orphans, 1);
}

// ---------------------------------------------------------------------------
// Dead-end detection
// ---------------------------------------------------------------------------

/// Build a vault where:
/// - a.md links to b and c (has outbound, no inbound → not a dead-end, not an orphan)
/// - b.md links to c (has inbound from a, has outbound → not a dead-end)
/// - c.md has no links but is linked to by a and b → dead-end
/// - d.md has no links and nothing links to it → orphan
fn setup_dead_end_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();

    write_md(
        tmp.path(),
        "a.md",
        md!(r"
---
title: A
---
[[b]]
[[c]]
"),
    );
    write_md(
        tmp.path(),
        "b.md",
        md!(r"
---
title: B
---
[[c]]
"),
    );
    write_md(
        tmp.path(),
        "c.md",
        md!(r"
---
title: C
---
No links.
"),
    );
    write_md(
        tmp.path(),
        "d.md",
        md!(r"
---
title: D
---
No links.
"),
    );

    tmp
}

#[test]
fn summary_dead_ends_json() {
    let tmp = setup_dead_end_vault();
    let output = hyalo_no_hints()
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

    assert_eq!(
        json["results"]["dead_ends"].as_u64().unwrap(),
        1,
        "expected 1 dead-end"
    );
    // d.md is the orphan, not a dead-end
    assert_eq!(json["results"]["orphans"].as_u64().unwrap(), 1);
}

#[test]
fn summary_dead_ends_text_format() {
    let tmp = setup_dead_end_vault();
    let output = hyalo_no_hints()
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
    assert!(
        text.contains("Dead-ends: 1"),
        "expected 'Dead-ends: 1' in: {text}"
    );
}

#[test]
fn summary_dead_ends_empty_when_no_links() {
    let tmp = TempDir::new().unwrap();

    // Two isolated files: both are orphans, neither is a dead-end
    write_md(
        tmp.path(),
        "a.md",
        md!(r"
---
title: A
---
No links.
"),
    );
    write_md(
        tmp.path(),
        "b.md",
        md!(r"
---
title: B
---
No links.
"),
    );

    let output = hyalo_no_hints()
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
    assert_eq!(json["results"]["dead_ends"].as_u64().unwrap(), 0);
    assert_eq!(json["results"]["orphans"].as_u64().unwrap(), 2);
}

/// Dead-end results must be identical between disk-scan and index-based summary.
#[test]
fn summary_dead_end_parity_disk_vs_index() {
    let tmp = setup_dead_end_vault();
    let dir_str = tmp.path().to_str().unwrap();

    // Disk scan
    let disk_out = hyalo_no_hints()
        .args(["--dir", dir_str, "summary", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        disk_out.status.success(),
        "disk summary failed: {}",
        String::from_utf8_lossy(&disk_out.stderr)
    );
    let disk_json: serde_json::Value = serde_json::from_slice(&disk_out.stdout).unwrap();
    let disk_dead_ends = disk_json["results"]["dead_ends"].as_u64().unwrap();

    // Create index
    let idx_create = hyalo_no_hints()
        .args(["--dir", dir_str, "create-index"])
        .output()
        .unwrap();
    assert!(
        idx_create.status.success(),
        "create-index failed: {}",
        String::from_utf8_lossy(&idx_create.stderr)
    );

    // Index-based summary
    let idx_out = hyalo_no_hints()
        .args(["--dir", dir_str, "summary", "--index", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        idx_out.status.success(),
        "index summary failed: {}",
        String::from_utf8_lossy(&idx_out.stderr)
    );
    let idx_json: serde_json::Value = serde_json::from_slice(&idx_out.stdout).unwrap();
    let idx_dead_ends = idx_json["results"]["dead_ends"].as_u64().unwrap();

    assert_eq!(
        disk_dead_ends, idx_dead_ends,
        "disk scan and index dead-end counts must match"
    );
    assert_eq!(disk_dead_ends, 1);
}

#[test]
fn summary_status_sorted_by_count_descending() {
    let tmp = TempDir::new().unwrap();

    // 3 files with status=done, 1 with status=planned
    write_md(tmp.path(), "done1.md", "---\nstatus: done\n---\nBody one.");
    write_md(tmp.path(), "done2.md", "---\nstatus: done\n---\nBody two.");
    write_md(
        tmp.path(),
        "done3.md",
        "---\nstatus: done\n---\nBody three.",
    );
    write_md(
        tmp.path(),
        "planned1.md",
        "---\nstatus: planned\n---\nBody four.",
    );

    let output = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "summary",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "summary failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    let status = json["results"]["status"]
        .as_array()
        .expect("field 'status' should be an array");

    // There must be at least two entries
    assert!(
        status.len() >= 2,
        "expected at least 2 status groups, got: {status:?}"
    );

    // The first entry should be "done" (count=3) before "planned" (count=1)
    assert_eq!(
        status[0]["value"], "done",
        "first status entry should be 'done' (highest count), got: {status:?}"
    );
    assert_eq!(
        status[0]["count"].as_u64().unwrap(),
        3,
        "done count should be 3"
    );
    assert_eq!(
        status[1]["value"], "planned",
        "second status entry should be 'planned', got: {status:?}"
    );
    assert_eq!(
        status[1]["count"].as_u64().unwrap(),
        1,
        "planned count should be 1"
    );
}
