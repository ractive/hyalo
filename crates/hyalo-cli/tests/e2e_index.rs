mod common;

use common::{hyalo, md, write_md};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Vault fixture
// ---------------------------------------------------------------------------

fn setup_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();

    // alpha.md — status=draft, rust+cli tags, has tasks, links to beta
    write_md(
        tmp.path(),
        "alpha.md",
        md!(r"
---
title: Alpha
status: draft
priority: 1
tags:
  - rust
  - cli
---
# Alpha

See [[beta]] for context.

- [ ] Write tests
- [x] Write code
"),
    );

    // beta.md — status=published, rust tag, body has unique keyword
    write_md(
        tmp.path(),
        "beta.md",
        md!(r"
---
title: Beta
status: published
tags:
  - rust
---
# Beta Content

Rust programming is fascinating.
"),
    );

    // gamma.md — status=draft, no tags, links to alpha
    write_md(
        tmp.path(),
        "gamma.md",
        md!(r"
---
title: Gamma
status: draft
---
# Gamma

See also [[alpha]].

- [ ] Pending task
"),
    );

    // sub/nested.md — status=published, nested tag
    write_md(
        tmp.path(),
        "sub/nested.md",
        md!(r"
---
title: Nested
status: published
tags:
  - project/backend
---
# Nested Content

Some nested content here.
"),
    );

    tmp
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Run `hyalo create-index --dir <dir>` and assert success.
/// Returns the default index path: `<dir>/.hyalo-index`.
fn create_default_index(tmp: &TempDir) -> std::path::PathBuf {
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .arg("create-index")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "create-index failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    tmp.path().join(".hyalo-index")
}

/// Run `hyalo find` with extra args and return parsed JSON.
fn run_find(tmp: &TempDir, extra_args: &[&str]) -> serde_json::Value {
    let mut cmd = hyalo();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.arg("find");
    cmd.args(extra_args);
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "find failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        let stdout = String::from_utf8_lossy(&output.stdout);
        panic!("invalid JSON: {e}\nstdout: {stdout}\nstderr: {stderr}")
    })
}

/// Extract and sort file paths from a find JSON array.
fn sorted_files(json: &serde_json::Value) -> Vec<String> {
    let mut files: Vec<String> = json
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["file"].as_str().unwrap().to_owned())
        .collect();
    files.sort();
    files
}

// ---------------------------------------------------------------------------
// create-index
// ---------------------------------------------------------------------------

#[test]
fn create_index_produces_file() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .arg("create-index")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let index_path = tmp.path().join(".hyalo-index");
    assert!(
        index_path.exists(),
        ".hyalo-index should exist after create-index"
    );

    // Output should be JSON with path and files_indexed
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json["path"].is_string());
    assert_eq!(json["files_indexed"].as_u64().unwrap(), 4);
}

#[test]
fn create_index_custom_output_path() {
    let tmp = setup_vault();
    let custom_path = tmp.path().join("my-custom.idx");

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["create-index", "--output", custom_path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        custom_path.exists(),
        "custom index path should exist after create-index --output"
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json["path"].as_str().unwrap(),
        custom_path.to_str().unwrap()
    );
}

// ---------------------------------------------------------------------------
// drop-index
// ---------------------------------------------------------------------------

#[test]
fn drop_index_deletes_file() {
    let tmp = setup_vault();
    let index_path = create_default_index(&tmp);
    assert!(index_path.exists());

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .arg("drop-index")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        !index_path.exists(),
        ".hyalo-index should be gone after drop-index"
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json["deleted"].is_string());
}

#[test]
fn drop_index_nonexistent_returns_error() {
    let tmp = setup_vault();
    // No index created — drop-index should fail.
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .arg("drop-index")
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "drop-index should fail when index does not exist"
    );
}

// ---------------------------------------------------------------------------
// find --index parity with disk scan
// ---------------------------------------------------------------------------

#[test]
fn find_with_index_returns_same_files_as_disk_scan() {
    let tmp = setup_vault();
    let index_path = create_default_index(&tmp);

    let disk_json = run_find(&tmp, &[]);
    let index_json = run_find(&tmp, &["--index", index_path.to_str().unwrap()]);

    let mut disk_files = sorted_files(&disk_json);
    let mut index_files = sorted_files(&index_json);
    disk_files.sort();
    index_files.sort();

    assert_eq!(
        disk_files, index_files,
        "find --index should return the same file set as a disk scan"
    );
}

#[test]
fn find_with_index_preserves_properties() {
    let tmp = setup_vault();
    let index_path = create_default_index(&tmp);

    let disk_json = run_find(&tmp, &[]);
    let index_json = run_find(&tmp, &["--index", index_path.to_str().unwrap()]);

    // For each file returned by disk scan, check that index scan has matching properties.
    for disk_entry in disk_json.as_array().unwrap() {
        let file = disk_entry["file"].as_str().unwrap();
        let index_entry = index_json
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["file"].as_str().unwrap() == file)
            .unwrap_or_else(|| panic!("file {file} missing from index scan"));

        assert_eq!(
            disk_entry["properties"], index_entry["properties"],
            "properties mismatch for {file}"
        );
        assert_eq!(
            disk_entry["tags"], index_entry["tags"],
            "tags mismatch for {file}"
        );
    }
}

#[test]
fn find_with_index_property_filter() {
    let tmp = setup_vault();
    let index_path = create_default_index(&tmp);

    let json = run_find(
        &tmp,
        &[
            "--property",
            "status=draft",
            "--index",
            index_path.to_str().unwrap(),
        ],
    );

    let files = sorted_files(&json);
    assert_eq!(files, vec!["alpha.md", "gamma.md"]);

    // Verify properties show status=draft for each result.
    for entry in json.as_array().unwrap() {
        assert_eq!(
            entry["properties"]["status"].as_str().unwrap(),
            "draft",
            "non-draft file returned: {}",
            entry["file"]
        );
    }
}

#[test]
fn find_with_index_tag_filter() {
    let tmp = setup_vault();
    let index_path = create_default_index(&tmp);

    let json = run_find(
        &tmp,
        &["--tag", "rust", "--index", index_path.to_str().unwrap()],
    );

    let mut files = sorted_files(&json);
    files.sort();
    assert_eq!(files, vec!["alpha.md", "beta.md"]);
}

#[test]
fn find_with_index_content_search_falls_back_to_disk() {
    let tmp = setup_vault();
    let index_path = create_default_index(&tmp);

    // "fascinating" only appears in beta.md body.
    let json = run_find(
        &tmp,
        &["fascinating", "--index", index_path.to_str().unwrap()],
    );

    let files = sorted_files(&json);
    assert_eq!(files, vec!["beta.md"]);
}

#[test]
fn find_with_index_content_search_no_match() {
    let tmp = setup_vault();
    let index_path = create_default_index(&tmp);

    let json = run_find(
        &tmp,
        &[
            "this-string-does-not-exist-anywhere",
            "--index",
            index_path.to_str().unwrap(),
        ],
    );

    assert!(json.as_array().unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// summary --index
// ---------------------------------------------------------------------------

#[test]
fn summary_with_index_matches_disk_scan() {
    let tmp = setup_vault();
    let index_path = create_default_index(&tmp);

    let disk_output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["summary", "--format", "json"])
        .output()
        .unwrap();
    assert!(disk_output.status.success());
    let disk_json: serde_json::Value = serde_json::from_slice(&disk_output.stdout).unwrap();

    let index_output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "--index",
            index_path.to_str().unwrap(),
            "summary",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        index_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&index_output.stderr)
    );
    let index_json: serde_json::Value = serde_json::from_slice(&index_output.stdout).unwrap();

    assert_eq!(
        disk_json["files"]["total"], index_json["files"]["total"],
        "file count mismatch between disk scan and index"
    );
    assert_eq!(
        disk_json["tasks"]["total"], index_json["tasks"]["total"],
        "task total mismatch between disk scan and index"
    );
    assert_eq!(
        disk_json["tasks"]["done"], index_json["tasks"]["done"],
        "tasks done mismatch between disk scan and index"
    );
}

#[test]
fn summary_with_index_file_count() {
    let tmp = setup_vault();
    let index_path = create_default_index(&tmp);

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "--index",
            index_path.to_str().unwrap(),
            "summary",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    assert_eq!(json["files"]["total"].as_u64().unwrap(), 4);
}

// ---------------------------------------------------------------------------
// tags summary --index
// ---------------------------------------------------------------------------

#[test]
fn tags_summary_with_index_matches_disk_scan() {
    let tmp = setup_vault();
    let index_path = create_default_index(&tmp);

    let disk_output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "summary"])
        .output()
        .unwrap();
    assert!(disk_output.status.success());
    let disk_json: serde_json::Value = serde_json::from_slice(&disk_output.stdout).unwrap();

    let index_output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--index", index_path.to_str().unwrap(), "tags", "summary"])
        .output()
        .unwrap();
    assert!(
        index_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&index_output.stderr)
    );
    let index_json: serde_json::Value = serde_json::from_slice(&index_output.stdout).unwrap();

    assert_eq!(
        disk_json["total"], index_json["total"],
        "tags total mismatch"
    );

    // Both should have the same set of tags.
    let mut disk_tags: Vec<&str> = disk_json["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    let mut index_tags: Vec<&str> = index_json["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    disk_tags.sort();
    index_tags.sort();
    assert_eq!(
        disk_tags, index_tags,
        "tag sets differ between disk and index"
    );
}

// ---------------------------------------------------------------------------
// properties summary --index
// ---------------------------------------------------------------------------

#[test]
fn properties_summary_with_index_matches_disk_scan() {
    let tmp = setup_vault();
    let index_path = create_default_index(&tmp);

    let disk_output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "summary"])
        .output()
        .unwrap();
    assert!(disk_output.status.success());
    let disk_json: serde_json::Value = serde_json::from_slice(&disk_output.stdout).unwrap();

    let index_output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "--index",
            index_path.to_str().unwrap(),
            "properties",
            "summary",
        ])
        .output()
        .unwrap();
    assert!(
        index_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&index_output.stderr)
    );
    let index_json: serde_json::Value = serde_json::from_slice(&index_output.stdout).unwrap();

    // Both should return arrays with the same property names.
    let mut disk_props: Vec<&str> = disk_json
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    let mut index_props: Vec<&str> = index_json
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    disk_props.sort();
    index_props.sort();
    assert_eq!(
        disk_props, index_props,
        "property sets differ between disk and index"
    );
}

// ---------------------------------------------------------------------------
// backlinks --index
// ---------------------------------------------------------------------------

#[test]
fn backlinks_with_index_finds_wikilinks() {
    let tmp = setup_vault();
    let index_path = create_default_index(&tmp);

    // gamma.md links to alpha.md via [[alpha]]
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "--index",
            index_path.to_str().unwrap(),
            "backlinks",
            "--file",
            "alpha.md",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["file"], "alpha.md");
    assert!(json["total"].as_u64().unwrap() >= 1);

    let backlinks = json["backlinks"].as_array().unwrap();
    let sources: Vec<&str> = backlinks
        .iter()
        .map(|b| b["source"].as_str().unwrap())
        .collect();
    assert!(
        sources.contains(&"gamma.md"),
        "gamma.md should be a backlink source for alpha.md; got: {sources:?}"
    );
}

#[test]
fn backlinks_with_index_matches_disk_scan() {
    let tmp = setup_vault();
    let index_path = create_default_index(&tmp);

    let disk_output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["backlinks", "--file", "beta.md"])
        .output()
        .unwrap();
    assert!(disk_output.status.success());
    let disk_json: serde_json::Value = serde_json::from_slice(&disk_output.stdout).unwrap();

    let index_output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "--index",
            index_path.to_str().unwrap(),
            "backlinks",
            "--file",
            "beta.md",
        ])
        .output()
        .unwrap();
    assert!(index_output.status.success());
    let index_json: serde_json::Value = serde_json::from_slice(&index_output.stdout).unwrap();

    assert_eq!(
        disk_json["total"], index_json["total"],
        "backlinks total mismatch between disk and index"
    );
}

// ---------------------------------------------------------------------------
// Incompatible / garbage index falls back gracefully
// ---------------------------------------------------------------------------

#[test]
fn incompatible_index_falls_back_to_disk_scan() {
    let tmp = setup_vault();

    // Write garbage bytes as the "index" file.
    let garbage_path = tmp.path().join("garbage.idx");
    std::fs::write(&garbage_path, b"this is not a valid msgpack snapshot").unwrap();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--index", garbage_path.to_str().unwrap(), "find"])
        .output()
        .unwrap();

    // Should succeed by falling back to disk scan.
    assert!(
        output.status.success(),
        "find with a garbage index should succeed (fall back to disk); stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Stderr should contain a warning about the incompatible index.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("warning"),
        "expected a 'warning' on stderr when index is incompatible; got: {stderr}"
    );

    // Results should still contain all 4 vault files.
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json.as_array().unwrap().len(),
        4,
        "expected 4 files from disk fallback"
    );
}

#[test]
fn incompatible_index_falls_back_for_summary() {
    let tmp = setup_vault();

    let garbage_path = tmp.path().join("bad.idx");
    std::fs::write(&garbage_path, b"NOTBINCODE").unwrap();

    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "--index",
            garbage_path.to_str().unwrap(),
            "summary",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "summary with garbage index should succeed; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["files"]["total"].as_u64().unwrap(), 4);
}
