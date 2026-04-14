use super::common::{hyalo_no_hints, md, write_md};
use tempfile::TempDir;

#[cfg(unix)]
use std::os::unix::fs as unix_fs;

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
    let output = hyalo_no_hints()
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
    let mut cmd = hyalo_no_hints();
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

/// Helper: extract the results array from a `{total, results}` envelope.
fn unwrap_results(json: &serde_json::Value) -> &Vec<serde_json::Value> {
    json["results"]
        .as_array()
        .expect("expected {total, results} envelope")
}

/// Extract and sort file paths from a find JSON envelope.
fn sorted_files(json: &serde_json::Value) -> Vec<String> {
    let mut files: Vec<String> = unwrap_results(json)
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
    let output = hyalo_no_hints()
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
    assert!(json["results"]["path"].is_string());
    assert_eq!(json["results"]["files_indexed"].as_u64().unwrap(), 4);
}

#[test]
fn create_index_custom_output_path() {
    let tmp = setup_vault();
    let custom_path = tmp.path().join("my-custom.idx");

    let output = hyalo_no_hints()
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
        json["results"]["path"].as_str().unwrap(),
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

    let output = hyalo_no_hints()
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
    assert!(json["results"]["deleted"].is_string());
}

#[test]
fn drop_index_nonexistent_returns_error() {
    let tmp = setup_vault();
    // No index created — drop-index should fail.
    let output = hyalo_no_hints()
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
    create_default_index(&tmp);

    let disk_json = run_find(&tmp, &[]);
    let index_json = run_find(&tmp, &["--index"]);

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
    create_default_index(&tmp);

    let disk_json = run_find(&tmp, &[]);
    let index_json = run_find(&tmp, &["--index"]);

    // For each file returned by disk scan, check that index scan has matching properties.
    for disk_entry in unwrap_results(&disk_json) {
        let file = disk_entry["file"].as_str().unwrap();
        let index_entry = unwrap_results(&index_json)
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
    create_default_index(&tmp);

    let json = run_find(&tmp, &["--property", "status=draft", "--index"]);

    let files = sorted_files(&json);
    assert_eq!(files, vec!["alpha.md", "gamma.md"]);

    // Verify properties show status=draft for each result.
    for entry in unwrap_results(&json) {
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
    create_default_index(&tmp);

    let json = run_find(&tmp, &["--tag", "rust", "--index"]);

    let mut files = sorted_files(&json);
    files.sort();
    assert_eq!(files, vec!["alpha.md", "beta.md"]);
}

#[test]
fn find_with_index_content_search_falls_back_to_disk() {
    let tmp = setup_vault();
    create_default_index(&tmp);

    // "fascinating" only appears in beta.md body.
    let json = run_find(&tmp, &["fascinating", "--index"]);

    let files = sorted_files(&json);
    assert_eq!(files, vec!["beta.md"]);
}

#[test]
fn find_with_index_content_search_no_match() {
    let tmp = setup_vault();
    create_default_index(&tmp);

    let json = run_find(&tmp, &["this-string-does-not-exist-anywhere", "--index"]);

    assert!(unwrap_results(&json).is_empty());
}

// ---------------------------------------------------------------------------
// summary --index
// ---------------------------------------------------------------------------

#[test]
fn summary_with_index_matches_disk_scan() {
    let tmp = setup_vault();
    create_default_index(&tmp);

    let disk_output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["summary", "--format", "json"])
        .output()
        .unwrap();
    assert!(disk_output.status.success());
    let disk_json: serde_json::Value = serde_json::from_slice(&disk_output.stdout).unwrap();

    let index_output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--index", "summary", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        index_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&index_output.stderr)
    );
    let index_json: serde_json::Value = serde_json::from_slice(&index_output.stdout).unwrap();

    assert_eq!(
        disk_json["results"]["files"]["total"], index_json["results"]["files"]["total"],
        "file count mismatch between disk scan and index"
    );
    assert_eq!(
        disk_json["results"]["tasks"]["total"], index_json["results"]["tasks"]["total"],
        "task total mismatch between disk scan and index"
    );
    assert_eq!(
        disk_json["results"]["tasks"]["done"], index_json["results"]["tasks"]["done"],
        "tasks done mismatch between disk scan and index"
    );
}

#[test]
fn summary_with_index_file_count() {
    let tmp = setup_vault();
    create_default_index(&tmp);

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--index", "summary", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    assert_eq!(json["results"]["files"]["total"].as_u64().unwrap(), 4);
}

// ---------------------------------------------------------------------------
// tags summary --index
// ---------------------------------------------------------------------------

#[test]
fn tags_summary_with_index_matches_disk_scan() {
    let tmp = setup_vault();
    create_default_index(&tmp);

    let disk_output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "summary"])
        .output()
        .unwrap();
    assert!(disk_output.status.success());
    let disk_json: serde_json::Value = serde_json::from_slice(&disk_output.stdout).unwrap();

    let index_output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--index", "tags", "summary"])
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

    // Both should have the same set of tags (bare array under "results").
    let mut disk_tags: Vec<&str> = disk_json["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    let mut index_tags: Vec<&str> = index_json["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    disk_tags.sort_unstable();
    index_tags.sort_unstable();
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
    create_default_index(&tmp);

    let disk_output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "summary"])
        .output()
        .unwrap();
    assert!(disk_output.status.success());
    let disk_json: serde_json::Value = serde_json::from_slice(&disk_output.stdout).unwrap();

    let index_output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--index", "properties", "summary"])
        .output()
        .unwrap();
    assert!(
        index_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&index_output.stderr)
    );
    let index_json: serde_json::Value = serde_json::from_slice(&index_output.stdout).unwrap();

    // Both should return arrays with the same property names (under "results").
    let mut disk_props: Vec<&str> = disk_json["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    let mut index_props: Vec<&str> = index_json["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    disk_props.sort_unstable();
    index_props.sort_unstable();
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
    create_default_index(&tmp);

    // gamma.md links to alpha.md via [[alpha]]
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--index", "backlinks", "--file", "alpha.md"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["file"], "alpha.md");
    assert!(json["total"].as_u64().unwrap() >= 1);

    let backlinks = json["results"]["backlinks"].as_array().unwrap();
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
    create_default_index(&tmp);

    let disk_output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["backlinks", "--file", "beta.md"])
        .output()
        .unwrap();
    assert!(disk_output.status.success());
    let disk_json: serde_json::Value = serde_json::from_slice(&disk_output.stdout).unwrap();

    let index_output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--index", "backlinks", "--file", "beta.md"])
        .output()
        .unwrap();
    assert!(index_output.status.success());
    let index_json: serde_json::Value = serde_json::from_slice(&index_output.stdout).unwrap();

    assert_eq!(
        disk_json["results"]["backlinks"].as_array().map(Vec::len),
        index_json["results"]["backlinks"].as_array().map(Vec::len),
        "backlinks count mismatch between disk and index"
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

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .arg(format!("--index={}", garbage_path.display()))
        .arg("find")
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
        unwrap_results(&json).len(),
        4,
        "expected 4 files from disk fallback"
    );
}

#[test]
fn incompatible_index_falls_back_for_summary() {
    let tmp = setup_vault();

    let garbage_path = tmp.path().join("bad.idx");
    std::fs::write(&garbage_path, b"NOTBINCODE").unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .arg(format!("--index={}", garbage_path.display()))
        .args(["summary", "--format", "json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "summary with garbage index should succeed; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["files"]["total"].as_u64().unwrap(), 4);
}

// ---------------------------------------------------------------------------
// Mutation commands with --index (index-aware mutations)
// ---------------------------------------------------------------------------

/// Helper: run a hyalo command with --index and assert success.
fn run_with_index(tmp: &TempDir, index_path: &std::path::Path, args: &[&str]) -> serde_json::Value {
    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.arg(format!("--index={}", index_path.display()));
    cmd.args(args);
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "command {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        let stdout = String::from_utf8_lossy(&output.stdout);
        panic!("invalid JSON from {args:?}: {e}\nstdout: {stdout}")
    })
}

#[test]
fn set_with_index_updates_index_for_subsequent_find() {
    let tmp = setup_vault();
    let index_path = create_default_index(&tmp);

    // Set a new property via --index
    run_with_index(
        &tmp,
        &index_path,
        &["set", "--property", "reviewed=true", "--file", "alpha.md"],
    );

    // Query the index — should see the updated property without re-scanning
    let json = run_find(&tmp, &["--property", "reviewed=true", "--index"]);

    let files = sorted_files(&json);
    assert_eq!(files, vec!["alpha.md"]);
    assert_eq!(
        unwrap_results(&json)[0]["properties"]["reviewed"]
            .as_str()
            .or_else(|| unwrap_results(&json)[0]["properties"]["reviewed"]
                .as_bool()
                .map(|_| "true")),
        Some("true"),
    );
}

#[test]
fn remove_with_index_updates_index_for_subsequent_find() {
    let tmp = setup_vault();
    let index_path = create_default_index(&tmp);

    // Verify alpha has status=draft initially
    let before = run_find(
        &tmp,
        &[
            "--property",
            "status=draft",
            "--file",
            "alpha.md",
            "--index",
        ],
    );
    assert_eq!(unwrap_results(&before).len(), 1);

    // Remove the status property via --index
    run_with_index(
        &tmp,
        &index_path,
        &["remove", "--property", "status", "--file", "alpha.md"],
    );

    // Query the index — alpha should no longer match status=draft
    let after = run_find(&tmp, &["--property", "status=draft", "--index"]);

    let files = sorted_files(&after);
    assert!(
        !files.contains(&"alpha.md".to_owned()),
        "alpha.md should no longer have status=draft after remove; got: {files:?}"
    );
}

#[test]
fn append_with_index_updates_index_for_subsequent_find() {
    let tmp = setup_vault();
    let index_path = create_default_index(&tmp);

    // Append a new alias
    run_with_index(
        &tmp,
        &index_path,
        &[
            "append",
            "--property",
            "aliases=The Alpha",
            "--file",
            "alpha.md",
        ],
    );

    // Find by the new property via index
    let json = run_find(&tmp, &["--property", "aliases", "--index"]);

    let files = sorted_files(&json);
    assert_eq!(files, vec!["alpha.md"]);
}

#[test]
fn task_toggle_with_index_updates_index() {
    let tmp = setup_vault();
    let index_path = create_default_index(&tmp);

    // alpha.md has tasks at lines 31 and 32 (line 31: "- [ ] Write tests", line 32: "- [x] Write code")
    // Find the actual task line by querying
    let before = run_find(
        &tmp,
        &[
            "--task", "todo", "--file", "alpha.md", "--fields", "tasks", "--index",
        ],
    );
    let todo_tasks: Vec<&serde_json::Value> = unwrap_results(&before)[0]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| !t["done"].as_bool().unwrap())
        .collect();
    assert!(
        !todo_tasks.is_empty(),
        "alpha.md should have open tasks before toggle"
    );
    let task_line = todo_tasks[0]["line"].as_u64().unwrap();

    // Toggle the first open task
    run_with_index(
        &tmp,
        &index_path,
        &[
            "task",
            "toggle",
            "--file",
            "alpha.md",
            "--line",
            &task_line.to_string(),
        ],
    );

    // Query the index — the task should now be done
    let after = run_find(
        &tmp,
        &["--file", "alpha.md", "--fields", "tasks", "--index"],
    );
    let tasks = unwrap_results(&after)[0]["tasks"].as_array().unwrap();
    let toggled = tasks
        .iter()
        .find(|t| t["line"].as_u64().unwrap() == task_line)
        .expect("task at toggled line should still exist");
    assert!(
        toggled["done"].as_bool().unwrap(),
        "task at line {task_line} should be done after toggle"
    );
}

#[test]
fn mv_with_index_updates_index_path() {
    let tmp = setup_vault();
    let index_path = create_default_index(&tmp);

    // Move gamma.md to archive/gamma.md
    run_with_index(
        &tmp,
        &index_path,
        &["mv", "--file", "gamma.md", "--to", "archive/gamma.md"],
    );

    // The index should now have archive/gamma.md and not gamma.md
    let json = run_find(&tmp, &["--index"]);
    let files = sorted_files(&json);
    assert!(
        files.contains(&"archive/gamma.md".to_owned()),
        "archive/gamma.md should be in index; got: {files:?}"
    );
    assert!(
        !files.contains(&"gamma.md".to_owned()),
        "gamma.md should no longer be in index; got: {files:?}"
    );

    // Properties/tags on the moved file are still queryable
    let json = run_find(&tmp, &["--property", "status=draft", "--index"]);
    let files = sorted_files(&json);
    assert!(
        files.contains(&"archive/gamma.md".to_owned()),
        "moved file should still be findable by property; got: {files:?}"
    );

    // Backlinks must reflect the new path: gamma.md linked to [[alpha]],
    // so after moving to archive/gamma.md the backlink source must be updated.
    let bl_json = run_with_index(&tmp, &index_path, &["backlinks", "--file", "alpha.md"]);
    let backlinks = bl_json["results"]["backlinks"].as_array().unwrap();
    let bl_sources: Vec<&str> = backlinks
        .iter()
        .map(|b| b["source"].as_str().unwrap())
        .collect();
    assert!(
        bl_sources.contains(&"archive/gamma.md"),
        "backlink source should be updated to archive/gamma.md; got: {bl_sources:?}"
    );
    assert!(
        !bl_sources.contains(&"gamma.md"),
        "old path gamma.md should not appear in backlinks; got: {bl_sources:?}"
    );
}

#[test]
fn tags_rename_with_index_updates_index() {
    let tmp = setup_vault();
    let index_path = create_default_index(&tmp);

    // Rename tag 'cli' to 'command-line'
    run_with_index(
        &tmp,
        &index_path,
        &["tags", "rename", "--from", "cli", "--to", "command-line"],
    );

    // Query via index — alpha.md should have 'command-line' tag, not 'cli'
    let json = run_find(&tmp, &["--tag", "command-line", "--index"]);
    let files = sorted_files(&json);
    assert_eq!(files, vec!["alpha.md"]);

    // Old tag should find nothing
    let old = run_find(&tmp, &["--tag", "cli", "--index"]);
    assert!(
        unwrap_results(&old).is_empty(),
        "old tag 'cli' should match nothing after rename"
    );
}

#[test]
fn properties_rename_with_index_updates_index() {
    let tmp = setup_vault();
    let index_path = create_default_index(&tmp);

    // Rename 'priority' to 'importance' (only alpha.md has it)
    run_with_index(
        &tmp,
        &index_path,
        &[
            "properties",
            "rename",
            "--from",
            "priority",
            "--to",
            "importance",
        ],
    );

    // Query via index — alpha.md should have 'importance', not 'priority'
    let json = run_find(&tmp, &["--property", "importance", "--index"]);
    let files = sorted_files(&json);
    assert_eq!(files, vec!["alpha.md"]);

    let old = run_find(&tmp, &["--property", "priority", "--index"]);
    assert!(
        unwrap_results(&old).is_empty(),
        "old property 'priority' should match nothing after rename"
    );
}

#[test]
fn chained_mutations_with_index_keep_index_consistent() {
    let tmp = setup_vault();
    let index_path = create_default_index(&tmp);

    // Chain: set status=archived on alpha, remove tag 'rust' from alpha, add tag 'legacy'
    run_with_index(
        &tmp,
        &index_path,
        &["set", "--property", "status=archived", "--file", "alpha.md"],
    );
    run_with_index(
        &tmp,
        &index_path,
        &["remove", "--tag", "rust", "--file", "alpha.md"],
    );
    run_with_index(
        &tmp,
        &index_path,
        &["set", "--tag", "legacy", "--file", "alpha.md"],
    );

    // Query via index — should reflect all three mutations
    let json = run_find(&tmp, &["--file", "alpha.md", "--index"]);
    let entry = &unwrap_results(&json)[0];
    assert_eq!(entry["properties"]["status"], "archived");
    let tags: Vec<&str> = entry["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap())
        .collect();
    assert!(
        !tags.contains(&"rust"),
        "tag 'rust' should be removed; got: {tags:?}"
    );
    assert!(
        tags.contains(&"legacy"),
        "tag 'legacy' should be present; got: {tags:?}"
    );

    // Now do a fresh disk scan (no --index) and verify it matches
    let disk_json = run_find(&tmp, &["--file", "alpha.md"]);
    let disk_entry = &unwrap_results(&disk_json)[0];
    assert_eq!(disk_entry["properties"]["status"], "archived");
    let disk_tags: Vec<&str> = disk_entry["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap())
        .collect();
    assert_eq!(
        tags, disk_tags,
        "index and disk scan should agree after chained mutations"
    );
}

// ---------------------------------------------------------------------------
// Regression: --property regex filter with YAML array values
// ---------------------------------------------------------------------------

/// Set up a vault where `status` is stored as a block-style YAML array
/// (e.g. `status:\n  - deprecated`) rather than a scalar string.
/// This mirrors real-world MDN-style frontmatter.
fn setup_array_status_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();

    // deprecated-1.md  — status: [deprecated]
    write_md(
        tmp.path(),
        "deprecated-1.md",
        md!(r"
---
title: Deprecated One
status:
  - deprecated
---
# Deprecated One
"),
    );

    // deprecated-2.md  — status: [deprecated]
    write_md(
        tmp.path(),
        "deprecated-2.md",
        md!(r"
---
title: Deprecated Two
status:
  - deprecated
---
# Deprecated Two
"),
    );

    // experimental-1.md — status: [experimental]
    write_md(
        tmp.path(),
        "experimental-1.md",
        md!(r"
---
title: Experimental One
status:
  - experimental
---
# Experimental One
"),
    );

    // experimental-2.md — status: [experimental]
    write_md(
        tmp.path(),
        "experimental-2.md",
        md!(r"
---
title: Experimental Two
status:
  - experimental
---
# Experimental Two
"),
    );

    tmp
}

/// Helper: run `hyalo --jq '<filter>' find <extra_args>` and return stdout trimmed.
fn run_find_jq(tmp: &TempDir, jq_filter: &str, extra_args: &[&str]) -> String {
    let mut cmd = hyalo_no_hints();
    cmd.args(["--dir", tmp.path().to_str().unwrap()]);
    cmd.args(["--jq", jq_filter]);
    cmd.arg("find");
    cmd.args(extra_args);
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "find failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// Regression: `--property 'status~=deprecated'` must return the same count
/// when querying via disk scan vs. index, where `status` is a YAML array.
#[test]
fn find_property_regex_yaml_array_disk_and_index_agree() {
    let tmp = setup_array_status_vault();

    // Disk scan (no index).
    let disk_total = run_find_jq(&tmp, ".total", &["--property", "status~=deprecated"]);

    // Build index, then query via it.
    create_default_index(&tmp);
    let index_total = run_find_jq(
        &tmp,
        ".total",
        &["--property", "status~=deprecated", "--index"],
    );

    assert_eq!(
        disk_total, index_total,
        "disk scan and index returned different totals for --property 'status~=deprecated' \
         with YAML array values (disk={disk_total}, index={index_total})"
    );

    // Both should find exactly the 2 deprecated files.
    assert_eq!(
        disk_total, "2",
        "expected 2 deprecated files from disk scan, got {disk_total}"
    );
}

/// Complementary: exact-match `--property 'status=deprecated'` via index
/// should also find the same 2 files (array element equality).
#[test]
fn find_property_exact_yaml_array_index_returns_correct_count() {
    let tmp = setup_array_status_vault();
    create_default_index(&tmp);

    let disk_total = run_find_jq(&tmp, ".total", &["--property", "status=deprecated"]);
    let index_total = run_find_jq(
        &tmp,
        ".total",
        &["--property", "status=deprecated", "--index"],
    );

    assert_eq!(
        disk_total, index_total,
        "disk scan and index returned different totals for --property 'status=deprecated' \
         with YAML array values (disk={disk_total}, index={index_total})"
    );

    // Both should find exactly the 2 deprecated files.
    assert_eq!(
        disk_total, "2",
        "expected 2 deprecated files from exact match, got {disk_total}"
    );
}

// ---------------------------------------------------------------------------
// Vault boundary checks — create-index
// ---------------------------------------------------------------------------

#[test]
fn create_index_rejects_output_outside_vault() {
    let vault = setup_vault();
    let outside = TempDir::new().unwrap();
    let outside_path = outside.path().join("evil.idx");

    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["create-index", "--output", outside_path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "create-index should fail for paths outside vault"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stderr}{stdout}");
    assert!(
        combined.contains("outside the vault boundary")
            || combined.contains("--allow-outside-vault"),
        "should mention vault boundary or escape hatch, got: {combined}"
    );
    assert!(
        !outside_path.exists(),
        "index file should not have been written outside vault"
    );
}

#[test]
fn create_index_allow_outside_vault_flag() {
    let vault = setup_vault();
    let outside = TempDir::new().unwrap();
    let outside_path = outside.path().join("allowed.idx");

    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args([
            "create-index",
            "--output",
            outside_path.to_str().unwrap(),
            "--allow-outside-vault",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "create-index with --allow-outside-vault should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        outside_path.exists(),
        "index file should have been written with escape hatch"
    );
}

#[test]
fn create_index_normal_in_vault_path_works() {
    let vault = setup_vault();
    let in_vault_path = vault.path().join("subdir");
    std::fs::create_dir_all(&in_vault_path).unwrap();
    let index_path = in_vault_path.join("my.idx");

    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["create-index", "--output", index_path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "create-index with in-vault path should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(index_path.exists(), "index file should exist inside vault");
}

// ---------------------------------------------------------------------------
// Vault boundary checks — drop-index
// ---------------------------------------------------------------------------

#[test]
fn drop_index_rejects_path_outside_vault() {
    let vault = setup_vault();
    // Create a real file outside the vault to try to delete
    let outside = TempDir::new().unwrap();
    let outside_file = outside.path().join("victim.idx");
    std::fs::write(&outside_file, b"fake index").unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["drop-index", "--path", outside_file.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "drop-index should fail for paths outside vault"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stderr}{stdout}");
    assert!(
        combined.contains("outside the vault boundary")
            || combined.contains("--allow-outside-vault"),
        "should mention vault boundary or escape hatch, got: {combined}"
    );
    assert!(
        outside_file.exists(),
        "file outside vault should NOT have been deleted"
    );
}

#[test]
fn drop_index_allow_outside_vault_flag() {
    let vault = setup_vault();
    let outside = TempDir::new().unwrap();
    let outside_file = outside.path().join("allowed.idx");
    std::fs::write(&outside_file, b"fake index").unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args([
            "drop-index",
            "--path",
            outside_file.to_str().unwrap(),
            "--allow-outside-vault",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "drop-index with --allow-outside-vault should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !outside_file.exists(),
        "file should have been deleted with escape hatch"
    );
}

#[test]
fn drop_index_normal_in_vault_path_works() {
    let vault = setup_vault();
    let index_path = create_default_index(&vault);
    assert!(index_path.exists());

    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["drop-index", "--path", index_path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "drop-index with in-vault path should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!index_path.exists(), "index should have been deleted");
}

// ---------------------------------------------------------------------------
// Symlink boundary checks — discover_files (via find)
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn find_skips_symlinked_md_outside_vault() {
    let vault = TempDir::new().unwrap();
    write_md(
        vault.path(),
        "real.md",
        md!(r"
---
title: Real
---
Content
"),
    );

    // Create a file outside the vault and symlink to it
    let outside = TempDir::new().unwrap();
    let outside_file = outside.path().join("secret.md");
    std::fs::write(&outside_file, "---\ntitle: Secret\n---\nSecret content\n").unwrap();
    unix_fs::symlink(&outside_file, vault.path().join("evil.md")).unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .arg("find")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "find should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let files: Vec<&str> = unwrap_results(&json)
        .iter()
        .map(|v| v["file"].as_str().unwrap())
        .collect();
    assert!(
        files.contains(&"real.md"),
        "real.md should be found: {files:?}"
    );
    assert!(
        !files.iter().any(|f| f.contains("evil")),
        "symlinked file outside vault should be skipped: {files:?}"
    );

    // Should emit a warning on stderr
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("warning") && stderr.contains("outside vault"),
        "should warn about skipped symlink: {stderr}"
    );
}

#[cfg(unix)]
#[test]
fn find_includes_symlinked_md_inside_vault() {
    let vault = TempDir::new().unwrap();
    write_md(
        vault.path(),
        "real.md",
        md!(r"
---
title: Real
---
Content
"),
    );

    // Create a subdirectory with a file, then symlink within the vault
    std::fs::create_dir_all(vault.path().join("sub")).unwrap();
    write_md(
        vault.path(),
        "sub/target.md",
        md!(r"
---
title: Target
---
Target content
"),
    );
    unix_fs::symlink(
        vault.path().join("sub/target.md"),
        vault.path().join("link.md"),
    )
    .unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .arg("find")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "find should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let files: Vec<&str> = unwrap_results(&json)
        .iter()
        .map(|v| v["file"].as_str().unwrap())
        .collect();
    assert!(
        files.contains(&"real.md"),
        "real.md should be found: {files:?}"
    );
    // The symlink resolves inside the vault, so it should be included
    assert!(
        files.iter().any(|f| f.contains("link")),
        "symlinked file inside vault should be included: {files:?}"
    );
}

// ---------------------------------------------------------------------------
// Bare --index flag (no path argument)
// ---------------------------------------------------------------------------

#[test]
fn bare_index_flag_defaults_to_hyalo_index_in_vault_dir() {
    let tmp = setup_vault();
    create_default_index(&tmp);

    // Use bare --index (no path) — should auto-resolve to {dir}/.hyalo-index
    let index_json = run_find(&tmp, &["--index"]);
    let disk_json = run_find(&tmp, &[]);

    assert_eq!(
        sorted_files(&disk_json),
        sorted_files(&index_json),
        "bare --index should use .hyalo-index and return the same file set as a disk scan"
    );
}

#[test]
fn explicit_index_path_with_equals_syntax() {
    let tmp = setup_vault();
    let index_path = create_default_index(&tmp);

    // Use --index=path (explicit, require_equals form)
    let arg = format!("--index={}", index_path.display());
    let index_json = run_find(&tmp, &[&arg]);
    let disk_json = run_find(&tmp, &[]);

    assert_eq!(
        sorted_files(&disk_json),
        sorted_files(&index_json),
        "--index=path should work with explicit equals syntax"
    );
}

#[test]
fn bare_index_flag_without_index_file_falls_back_to_disk_scan() {
    let tmp = setup_vault();
    // Do NOT create an index — bare --index should fall back gracefully.

    let result = run_find(&tmp, &["--index"]);
    let files = sorted_files(&result);

    // Should still return results via disk scan fallback.
    assert!(
        !files.is_empty(),
        "bare --index without an index file should fall back to disk scan"
    );
}

#[test]
fn bare_index_works_from_different_cwd() {
    let tmp = setup_vault();
    create_default_index(&tmp);

    // Run from a different CWD (root dir) — bare --index should still resolve
    // to {dir}/.hyalo-index, not {cwd}/.hyalo-index.
    let other_dir = TempDir::new().unwrap();
    let output = hyalo_no_hints()
        .current_dir(other_dir.path())
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--index"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "bare --index from different CWD should work: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let files = sorted_files(&json);
    assert!(
        !files.is_empty(),
        "bare --index from different CWD should find files via <dir>/.hyalo-index"
    );
}
