mod common;

use common::{hyalo, md, write_md};

fn setup_vault() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    write_md(
        tmp.path(),
        "note-a.md",
        md!(r"
---
title: Note A
---
See [[note-b]] and [[nonexistent]].

Also ![[image.png]] embed.
"),
    );
    write_md(
        tmp.path(),
        "note-b.md",
        "Link to [Note A](note-a.md) and [[note-a#heading]].\n",
    );
    write_md(tmp.path(), "isolated.md", "No links here.\n");
    write_md(tmp.path(), "sub/deep.md", "[[note-a]] and [b](note-b.md)\n");
    write_md(
        tmp.path(),
        "code-blocks.md",
        md!(r"
Before
```
[[inside code block]]
```
After [[real-link]]
"),
    );
    tmp
}

// --- links command ---

#[test]
fn links_single_file_json() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["links", "--file", "note-a.md"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed["path"], "note-a.md");
    let links = parsed["links"].as_array().unwrap();
    assert_eq!(links.len(), 3); // note-b, nonexistent, image.png
    // Check that target, path, label are present
    assert_eq!(links[0]["target"], "note-b");
    assert_eq!(links[0]["path"], "note-b.md");
}

#[test]
fn links_path_populated() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["links", "--file", "note-b.md"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let links = parsed["links"].as_array().unwrap();
    assert_eq!(links.len(), 2);
    // Both links point to note-a.md
    for link in links {
        assert_eq!(link["path"], "note-a.md");
    }
}

#[test]
fn links_path_null_for_broken() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["links", "--file", "note-a.md"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let links = parsed["links"].as_array().unwrap();
    let nonexistent = links.iter().find(|l| l["target"] == "nonexistent").unwrap();
    assert!(nonexistent["path"].is_null());
}

#[test]
fn links_label_field() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["links", "--file", "note-b.md"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let links = parsed["links"].as_array().unwrap();
    let md_link = links
        .iter()
        .find(|l| l["label"].as_str() == Some("Note A"))
        .unwrap();
    assert_eq!(md_link["target"], "note-a.md");
}

#[test]
fn links_no_style_line_is_embed_fields() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["links", "--file", "note-a.md"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Old fields should not appear
    assert!(!stdout.contains("\"style\""));
    assert!(!stdout.contains("\"line\""));
    assert!(!stdout.contains("\"is_embed\""));
    assert!(!stdout.contains("\"heading\""));
    assert!(!stdout.contains("\"block_ref\""));
}

#[test]
fn links_skips_code_blocks() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["links", "--file", "code-blocks.md"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let links = parsed["links"].as_array().unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0]["target"], "real-link");
}

#[test]
fn links_text_format() {
    let tmp = setup_vault();
    hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["--format", "text"])
        .args(["links", "--file", "note-a.md"])
        .assert()
        .success()
        .stdout(predicates::str::contains("note-b"))
        .stdout(predicates::str::contains("note-a.md"));
}

#[test]
fn links_file_not_found() {
    let tmp = setup_vault();
    hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["links", "--file", "nope.md"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("file not found"));
}

#[test]
fn links_without_file_flag_fails() {
    let tmp = setup_vault();
    hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["links"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("--file"));
}

// --- links --unresolved flag ---

#[test]
fn links_unresolved_single_file() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["links", "--file", "note-a.md", "--unresolved"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let links = parsed["links"].as_array().unwrap();

    let targets: Vec<&str> = links
        .iter()
        .map(|l| l["target"].as_str().unwrap())
        .collect();
    assert!(targets.contains(&"nonexistent"));
    assert!(targets.contains(&"image.png"));
    assert!(!targets.contains(&"note-b"));
    // All unresolved links have null path
    for link in links {
        assert!(link["path"].is_null());
    }
}

#[test]
fn links_unresolved_text_format() {
    let tmp = setup_vault();
    hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["--format", "text"])
        .args(["links", "--file", "note-a.md", "--unresolved"])
        .assert()
        .success()
        .stdout(predicates::str::contains("nonexistent"))
        .stdout(predicates::str::contains("unresolved"));
}

#[test]
fn links_unresolved_file_not_found() {
    let tmp = setup_vault();
    hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["links", "--file", "nope.md", "--unresolved"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("file not found"));
}

// --- links --resolved flag ---

#[test]
fn links_resolved_single_file() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["links", "--file", "note-a.md", "--resolved"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let links = parsed["links"].as_array().unwrap();

    let targets: Vec<&str> = links
        .iter()
        .map(|l| l["target"].as_str().unwrap())
        .collect();
    assert!(targets.contains(&"note-b"));
    assert!(!targets.contains(&"nonexistent"));
    assert!(!targets.contains(&"image.png"));
    // All resolved links have non-null path
    for link in links {
        assert!(!link["path"].is_null());
    }
}

#[test]
fn links_resolved_text_format() {
    let tmp = setup_vault();
    hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["--format", "text"])
        .args(["links", "--file", "note-a.md", "--resolved"])
        .assert()
        .success()
        .stdout(predicates::str::contains("note-b"))
        .stdout(predicates::str::contains("note-b.md"));
}

#[test]
fn links_resolved_and_unresolved_conflict() {
    let tmp = setup_vault();
    hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["links", "--file", "note-a.md", "--resolved", "--unresolved"])
        .assert()
        .failure();
}

// --- edge-case / error-path e2e tests ---

#[test]
fn links_file_is_directory() {
    let tmp = setup_vault();
    // Create a subdirectory named "subdir" and pass it as --file
    std::fs::create_dir(tmp.path().join("subdir")).unwrap();
    hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["links", "--file", "subdir"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("file not found"));
}

#[test]
fn links_nonexistent_dir() {
    hyalo()
        .args(["--dir", "/tmp/hyalo-nonexistent-vault-xyz-12345"])
        .args(["links", "--file", "any.md"])
        .assert()
        .failure();
}

#[test]
fn links_empty_file() {
    let tmp = tempfile::tempdir().unwrap();
    write_md(tmp.path(), "empty.md", "");
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["links", "--file", "empty.md"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let links = parsed["links"].as_array().unwrap();
    assert!(links.is_empty());
}

#[test]
fn links_unclosed_wikilink_in_file() {
    let tmp = tempfile::tempdir().unwrap();
    write_md(
        tmp.path(),
        "unclosed.md",
        "See [[broken link and more text\n",
    );
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["links", "--file", "unclosed.md"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let links = parsed["links"].as_array().unwrap();
    assert!(links.is_empty());
}
