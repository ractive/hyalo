mod common;

use common::{hyalo_no_hints, md, write_md};
use tempfile::TempDir;

/// Helper: create a temp dir with a sample file for testing short flags.
fn setup() -> TempDir {
    let dir = TempDir::new().unwrap();
    write_md(
        dir.path(),
        "note.md",
        md!(r"
---
title: Test Note
status: draft
tags:
  - research
---
# Heading

Some body text.

- [ ] Open task
- [x] Done task
"),
    );
    write_md(
        dir.path(),
        "other.md",
        md!(r"
---
title: Other
status: done
tags:
  - archive
---
# Other heading

Other body.
"),
    );
    dir
}

// -- Global: -d for --dir --------------------------------------------------

#[test]
fn short_d_for_dir() {
    let dir = setup();
    let output = hyalo_no_hints()
        .args(["-d", dir.path().to_str().unwrap(), "find"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("note.md"));
}

// -- find: -p, -t, -s, -f, -g, -n -----------------------------------------

#[test]
fn find_short_p_for_property() {
    let dir = setup();
    let output = hyalo_no_hints()
        .args([
            "find",
            "-p",
            "status=draft",
            "-d",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("note.md"));
    assert!(!stdout.contains("other.md"));
}

#[test]
fn find_short_t_for_tag() {
    let dir = setup();
    let output = hyalo_no_hints()
        .args(["find", "-t", "research", "-d", dir.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("note.md"));
    assert!(!stdout.contains("other.md"));
}

#[test]
fn find_short_s_for_section() {
    let dir = setup();
    let output = hyalo_no_hints()
        .args(["find", "-s", "Heading", "-d", dir.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("note.md"));
}

#[test]
fn find_short_f_for_file() {
    let dir = setup();
    let output = hyalo_no_hints()
        .args(["find", "-f", "note.md", "-d", dir.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("note.md"));
    assert!(!stdout.contains("other.md"));
}

#[test]
fn find_short_g_for_glob() {
    let dir = setup();
    let output = hyalo_no_hints()
        .args(["find", "-g", "other*", "-d", dir.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("other.md"));
    assert!(!stdout.contains("note.md"));
}

#[test]
fn find_short_n_for_limit() {
    let dir = setup();
    let output = hyalo_no_hints()
        .args(["find", "-n", "1", "-d", dir.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let v: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // Limit truncates 2 files to 1, so output is an envelope {total, results}.
    let results = v
        .as_array()
        .unwrap_or_else(|| v["results"].as_array().expect("expected array or envelope"));
    assert_eq!(results.len(), 1);
}

// -- read: -f, -s, -l ------------------------------------------------------

#[test]
fn read_short_f_for_file() {
    let dir = setup();
    let output = hyalo_no_hints()
        .args(["read", "-f", "note.md", "-d", dir.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Some body text"));
}

#[test]
fn read_short_s_for_section() {
    let dir = setup();
    let output = hyalo_no_hints()
        .args([
            "read",
            "-f",
            "note.md",
            "-s",
            "Heading",
            "-d",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Some body text"));
}

#[test]
fn read_short_l_for_lines() {
    let dir = setup();
    let output = hyalo_no_hints()
        .args([
            "read",
            "-f",
            "note.md",
            "-l",
            "1:1",
            "-d",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("# Heading"));
}

// -- properties / tags / summary: -g, -n ------------------------------------

#[test]
fn properties_short_g_for_glob() {
    let dir = setup();
    let output = hyalo_no_hints()
        .args([
            "properties",
            "summary",
            "-g",
            "note*",
            "-d",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("status"));
}

#[test]
fn tags_short_g_for_glob() {
    let dir = setup();
    let output = hyalo_no_hints()
        .args([
            "tags",
            "summary",
            "-g",
            "note*",
            "-d",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("research"));
}

#[test]
fn summary_short_g_and_n() {
    let dir = setup();
    let output = hyalo_no_hints()
        .args([
            "summary",
            "-g",
            "note*",
            "-n",
            "1",
            "-d",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
}

// -- set / remove: -p, -t, -f, -g ------------------------------------------

#[test]
fn set_short_p_t_f() {
    let dir = setup();
    let output = hyalo_no_hints()
        .args([
            "set",
            "-p",
            "status=active",
            "-t",
            "updated",
            "-f",
            "note.md",
            "-d",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());

    // Verify it took effect
    let check = hyalo_no_hints()
        .args([
            "find",
            "-p",
            "status=active",
            "-t",
            "updated",
            "-d",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8(check.stdout).unwrap();
    assert!(stdout.contains("note.md"));
}

#[test]
fn remove_short_p_t_f() {
    let dir = setup();
    // First set a tag
    let precondition = hyalo_no_hints()
        .args([
            "set",
            "-t",
            "removeme",
            "-f",
            "note.md",
            "-d",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        precondition.status.success(),
        "precondition: set tag failed"
    );

    // Now remove it with short flags
    let output = hyalo_no_hints()
        .args([
            "remove",
            "-t",
            "removeme",
            "-f",
            "note.md",
            "-d",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
}

// -- append: -p, -f ---------------------------------------------------------

#[test]
fn append_short_p_f() {
    let dir = setup();
    let output = hyalo_no_hints()
        .args([
            "append",
            "-p",
            "tags=newtag",
            "-f",
            "note.md",
            "-d",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
}

// -- task: -f, -l, -s -------------------------------------------------------

#[test]
fn task_read_short_f_l() {
    let dir = setup();
    let output = hyalo_no_hints()
        .args([
            "task",
            "read",
            "-f",
            "note.md",
            "-l",
            "11",
            "-d",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
fn task_toggle_short_f_l() {
    let dir = setup();
    let output = hyalo_no_hints()
        .args([
            "task",
            "toggle",
            "-f",
            "note.md",
            "-l",
            "11",
            "-d",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
fn task_set_status_short_f_l_s() {
    let dir = setup();
    let output = hyalo_no_hints()
        .args([
            "task",
            "set",
            "-f",
            "note.md",
            "-l",
            "11",
            "-s",
            "/",
            "-d",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
}
