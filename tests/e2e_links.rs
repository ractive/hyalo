mod common;

use common::{hyalo, write_md};

fn setup_vault() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    write_md(
        tmp.path(),
        "note-a.md",
        "---\ntitle: Note A\n---\nSee [[note-b]] and [[nonexistent]].\n\nAlso ![[image.png]] embed.\n",
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
        "Before\n```\n[[inside code block]]\n```\nAfter [[real-link]]\n",
    );
    tmp
}

// --- links command ---

#[test]
fn links_single_file_json() {
    let tmp = setup_vault();
    hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["links", "--path", "note-a.md"])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"target\": \"note-b\""))
        .stdout(predicates::str::contains("\"target\": \"nonexistent\""))
        .stdout(predicates::str::contains("\"target\": \"image.png\""))
        .stdout(predicates::str::contains("\"is_embed\": true"));
}

#[test]
fn links_single_file_counts() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["links", "--path", "note-a.md"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let links = parsed["links"].as_array().unwrap();
    assert_eq!(links.len(), 3); // note-b, nonexistent, image.png
}

#[test]
fn links_all_files_json() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["links"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    // note-a (3 links), note-b (2 links), sub/deep.md (2 links), code-blocks (1 link)
    // isolated has 0 links, so not included
    assert_eq!(parsed.len(), 4);
}

#[test]
fn links_wiki_vs_markdown_style() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["links", "--path", "note-b.md"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let links = parsed["links"].as_array().unwrap();
    assert_eq!(links.len(), 2);

    let styles: Vec<&str> = links.iter().map(|l| l["style"].as_str().unwrap()).collect();
    assert!(styles.contains(&"markdown"));
    assert!(styles.contains(&"wiki"));
}

#[test]
fn links_embed_flag() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["links", "--path", "note-a.md"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let links = parsed["links"].as_array().unwrap();

    let embed = links.iter().find(|l| l["target"] == "image.png").unwrap();
    assert_eq!(embed["is_embed"], true);

    // Non-embed links should not have is_embed
    let regular = links.iter().find(|l| l["target"] == "note-b").unwrap();
    assert!(regular.get("is_embed").is_none());
}

#[test]
fn links_heading_metadata() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["links", "--path", "note-b.md"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let links = parsed["links"].as_array().unwrap();

    let wiki = links.iter().find(|l| l["style"] == "wiki").unwrap();
    assert_eq!(wiki["heading"], "heading");
    assert_eq!(wiki["target"], "note-a");
}

#[test]
fn links_skips_code_blocks() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["links", "--path", "code-blocks.md"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let links = parsed["links"].as_array().unwrap();

    // Only real-link should be found, not inside code block
    assert_eq!(links.len(), 1);
    assert_eq!(links[0]["target"], "real-link");
}

#[test]
fn links_text_format() {
    let tmp = setup_vault();
    hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["--format", "text"])
        .args(["links", "--path", "note-a.md"])
        .assert()
        .success()
        .stdout(predicates::str::contains("target=note-b"));
}

#[test]
fn links_file_not_found() {
    let tmp = setup_vault();
    hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["links", "--path", "nope.md"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("file not found"));
}

// --- unresolved command ---

#[test]
fn unresolved_single_file() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["unresolved", "--path", "note-a.md"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let links = parsed["links"].as_array().unwrap();

    // nonexistent and image.png are unresolved; note-b resolves
    let targets: Vec<&str> = links
        .iter()
        .map(|l| l["target"].as_str().unwrap())
        .collect();
    assert!(targets.contains(&"nonexistent"));
    assert!(targets.contains(&"image.png"));
    assert!(!targets.contains(&"note-b"));
}

#[test]
fn unresolved_all_files() {
    let tmp = setup_vault();
    let output = hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["unresolved"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();

    // Files with unresolved links should be included
    let paths: Vec<&str> = parsed.iter().map(|v| v["path"].as_str().unwrap()).collect();
    assert!(paths.contains(&"note-a.md"));

    // code-blocks.md has [[real-link]] which is unresolved
    assert!(paths.contains(&"code-blocks.md"));
}

#[test]
fn unresolved_text_format() {
    let tmp = setup_vault();
    hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["--format", "text"])
        .args(["unresolved", "--path", "note-a.md"])
        .assert()
        .success()
        .stdout(predicates::str::contains("target=nonexistent"));
}

#[test]
fn unresolved_file_not_found() {
    let tmp = setup_vault();
    hyalo()
        .args(["--dir", &tmp.path().display().to_string()])
        .args(["unresolved", "--path", "nope.md"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("file not found"));
}
