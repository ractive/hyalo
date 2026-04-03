mod common;

use std::fs;

use common::{hyalo, hyalo_no_hints, write_md};
use serde_json::Value;
use tempfile::TempDir;

fn setup() -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    write_md(
        tmp.path(),
        "note1.md",
        "---\nstatus: draft\ntags:\n  - project\n---\nHello world",
    );
    write_md(
        tmp.path(),
        "note2.md",
        "---\nstatus: completed\ntags:\n  - research\n---\nGoodbye world",
    );
    tmp
}

#[test]
fn views_set_and_list() {
    let tmp = setup();
    // Set a view
    let output = hyalo()
        .current_dir(tmp.path())
        .args(["views", "set", "drafts", "--property", "status=draft"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "set failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // List views
    let output = hyalo()
        .current_dir(tmp.path())
        .args(["views", "list"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "list failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 1);
    assert_eq!(json["results"][0]["name"], "drafts");
}

#[test]
fn views_remove() {
    let tmp = setup();
    // Set then remove
    hyalo()
        .current_dir(tmp.path())
        .args(["views", "set", "drafts", "--property", "status=draft"])
        .output()
        .unwrap();

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["views", "remove", "drafts"])
        .output()
        .unwrap();
    assert!(output.status.success());

    // List should be empty
    let output = hyalo()
        .current_dir(tmp.path())
        .args(["views", "list"])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 0);
}

#[test]
fn find_with_view() {
    let tmp = setup();
    // Set a view
    hyalo()
        .current_dir(tmp.path())
        .args(["views", "set", "drafts", "--property", "status=draft"])
        .output()
        .unwrap();

    // Use the view with find
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["find", "--view", "drafts"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "find --view failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 1);
    assert!(
        json["results"][0]["file"]
            .as_str()
            .unwrap()
            .contains("note1")
    );
}

#[test]
fn find_with_view_and_overrides() {
    let tmp = setup();
    // Create more files
    write_md(
        tmp.path(),
        "note3.md",
        "---\nstatus: draft\ntags:\n  - project\n---\nAnother draft",
    );

    // Set a view with property filter
    hyalo()
        .current_dir(tmp.path())
        .args(["views", "set", "drafts", "--property", "status=draft"])
        .output()
        .unwrap();

    // Use view with limit override
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["find", "--view", "drafts", "--limit", "1"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"].as_array().unwrap().len(), 1);
}

#[test]
fn find_with_unknown_view_errors() {
    let tmp = setup();
    let output = hyalo()
        .current_dir(tmp.path())
        .args(["find", "--view", "nonexistent"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown view"), "stderr: {stderr}");
}

#[test]
fn views_remove_nonexistent_errors() {
    let tmp = setup();
    let output = hyalo()
        .current_dir(tmp.path())
        .args(["views", "remove", "nonexistent"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"), "stderr: {stderr}");
}

#[test]
fn views_set_empty_filters_errors() {
    let tmp = setup();
    let output = hyalo()
        .current_dir(tmp.path())
        .args(["views", "set", "empty"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no filters"), "stderr: {stderr}");
}

#[test]
fn views_set_preserves_existing_config() {
    let tmp = setup();
    // Write a .hyalo.toml with dir set
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();

    // Set a view
    hyalo()
        .current_dir(tmp.path())
        .args(["views", "set", "drafts", "--property", "status=draft"])
        .output()
        .unwrap();

    // Verify dir is preserved
    let content = fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
    assert!(content.contains("dir = \".\""), "dir was lost: {content}");
    assert!(
        content.contains("[views.drafts]"),
        "view not written: {content}"
    );
}

#[test]
fn hint_suggests_saving_non_trivial_query_as_view() {
    let tmp = setup();
    // Two filters = non-trivial → should suggest saving as a view
    let output = hyalo()
        .current_dir(tmp.path())
        .args([
            "find",
            "--property",
            "status=draft",
            "--tag",
            "project",
            "--hints",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    let hints = json["hints"].as_array().unwrap();
    let has_view_hint = hints
        .iter()
        .any(|h| h["cmd"].as_str().unwrap_or("").contains("views set"));
    assert!(
        has_view_hint,
        "expected 'views set' hint for non-trivial query, got: {hints:?}"
    );
}

#[test]
fn hint_does_not_suggest_view_for_single_filter() {
    let tmp = setup();
    // Single filter = trivial → should NOT suggest saving as a view
    let output = hyalo()
        .current_dir(tmp.path())
        .args(["find", "--property", "status=draft", "--hints"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    let hints = json["hints"].as_array().unwrap();
    let has_view_hint = hints
        .iter()
        .any(|h| h["cmd"].as_str().unwrap_or("").contains("views set"));
    assert!(
        !has_view_hint,
        "should not suggest view for single-filter query, got: {hints:?}"
    );
}

#[test]
fn hint_does_not_suggest_view_when_using_view() {
    let tmp = setup();
    // Create a view with two filters
    let set_output = hyalo()
        .current_dir(tmp.path())
        .args([
            "views",
            "set",
            "drafts",
            "--property",
            "status=draft",
            "--tag",
            "project",
        ])
        .output()
        .unwrap();
    assert!(
        set_output.status.success(),
        "views set failed: {}",
        String::from_utf8_lossy(&set_output.stderr)
    );

    // Use it — should NOT suggest saving again
    let output = hyalo()
        .current_dir(tmp.path())
        .args(["find", "--view", "drafts", "--hints"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    let hints = json["hints"].as_array().unwrap();
    let has_view_hint = hints
        .iter()
        .any(|h| h["cmd"].as_str().unwrap_or("").contains("views set"));
    assert!(
        !has_view_hint,
        "should not suggest view when already using --view, got: {hints:?}"
    );
}
