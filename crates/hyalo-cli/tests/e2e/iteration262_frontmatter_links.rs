//! Iteration 262 — frontmatter wikilinks are first-class links.
//!
//! - **FM-1 (BUG-1).** A `[[wikilink]]` in *any* frontmatter value is a graph
//!   edge — `categories:`, `type:`, a nested map — not just the four legacy
//!   link properties. `[links] frontmatter = false` restores the old scope.
//! - **FM-2 (BUG-1, UX-4).** `mv` rewrites those targets in place, preserving
//!   the author's quoting, and both `mv` modes print their counters in text.
//! - **FM-3 (UX-11).** A list of wikilinks renders as `["[[A]]", "[[B]]"]`
//!   instead of the unreadable `[[[A]], [[B]]]`.
//! - **FM-4 (UX-12).** `set` on a list property says it collapsed the list.

use assert_cmd::Command;
use tempfile::TempDir;

fn hyalo(tmp: &TempDir) -> Command {
    let mut cmd = crate::common::hyalo_no_hints();
    cmd.arg("--dir").arg(tmp.path().to_str().unwrap());
    cmd
}

fn run_json(tmp: &TempDir, args: &[&str]) -> serde_json::Value {
    let output = hyalo(tmp)
        .args(args)
        .args(["--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "`hyalo {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("json output")
}

fn run_text(tmp: &TempDir, args: &[&str]) -> (String, String) {
    let output = hyalo(tmp)
        .args(args)
        .args(["--format", "text"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "`hyalo {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn write(tmp: &TempDir, rel: &str, body: &str) {
    let path = tmp.path().join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, body).unwrap();
}

/// The kepano-obsidian shape that started this iteration: two notes pointing at
/// a category note through `categories:`, plus one plain body link.
fn vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write(
        &tmp,
        "Categories/Books.md",
        "---\ntags:\n  - categories\n---\n\n# Books\n",
    );
    write(
        &tmp,
        "References/Out of Control.md",
        "---\ncategories:\n  - \"[[Books]]\"\nauthor: \"[[Kevin Kelly]]\"\n---\n\n# Out of Control\n",
    );
    write(
        &tmp,
        "References/The Machine Stops.md",
        "---\ncategories: [\"[[Books]]\"]\n---\n\nSee [[Categories/Books]] for the shelf.\n",
    );
    write(&tmp, "People/Kevin Kelly.md", "---\ntags: []\n---\n\n# KK\n");
    tmp
}

// ---------------------------------------------------------------------------
// FM-1 — every frontmatter value is a link source
// ---------------------------------------------------------------------------

#[test]
fn backlinks_counts_frontmatter_links_from_any_property() {
    let tmp = vault();
    let json = run_json(&tmp, &["backlinks", "Categories/Books.md"]);
    assert_eq!(json["total"].as_u64(), Some(3), "{json}");

    let entries = json["results"]["backlinks"].as_array().unwrap();
    let frontmatter: Vec<(&str, &str)> = entries
        .iter()
        .filter(|e| e["kind"] == "frontmatter")
        .map(|e| {
            (
                e["source"].as_str().unwrap(),
                e["property"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        frontmatter,
        vec![
            ("References/Out of Control.md", "categories"),
            ("References/The Machine Stops.md", "categories"),
        ],
        "{json}"
    );
    // The body link is still a plain wikilink and carries no property.
    let body: Vec<&serde_json::Value> = entries
        .iter()
        .filter(|e| e["kind"] == "wikilink")
        .collect();
    assert_eq!(body.len(), 1, "{json}");
    assert!(body[0]["property"].is_null(), "{json}");
}

#[test]
fn frontmatter_links_report_their_own_line() {
    let tmp = vault();
    let json = run_json(&tmp, &["backlinks", "Categories/Books.md"]);
    let line = json["results"]["backlinks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["source"] == "References/Out of Control.md")
        .map(|e| e["line"].as_u64())
        .unwrap();
    // `---` is line 1, `categories:` line 2, the list item line 3.
    assert_eq!(line, Some(3), "{json}");
}

#[test]
fn find_fields_links_lists_frontmatter_and_body_links_together() {
    let tmp = vault();
    let json = run_json(
        &tmp,
        &[
            "find",
            "--file",
            "References/The Machine Stops.md",
            "--fields",
            "links",
        ],
    );
    let links = json["results"][0]["links"].as_array().unwrap();
    let kinds: Vec<&str> = links.iter().map(|l| l["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, vec!["frontmatter", "wikilink"], "{json}");
    assert_eq!(links[0]["property"], "categories", "{json}");
    assert_eq!(links[0]["path"], "Categories/Books.md", "{json}");
}

#[test]
fn a_note_linked_only_from_frontmatter_is_not_an_orphan() {
    let tmp = vault();
    let json = run_json(&tmp, &["find", "--orphan", "--fields", "file"]);
    let orphans: Vec<&str> = json["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["file"].as_str().unwrap())
        .collect();
    assert!(
        !orphans.contains(&"Categories/Books.md"),
        "linked through `categories:` — not an orphan: {json}"
    );
    assert!(
        !orphans.contains(&"People/Kevin Kelly.md"),
        "linked through `author:` — not an orphan: {json}"
    );
}

#[test]
fn broken_frontmatter_wikilink_is_reported_by_hyalo006() {
    let tmp = TempDir::new().unwrap();
    write(&tmp, "note.md", "---\ncategories: \"[[Nope]]\"\n---\n\nBody\n");
    let output = hyalo(&tmp)
        .args(["lint", "--rule", "HYALO006", "--format", "json"])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"]["violations"].as_u64(), Some(1), "{json}");
    let violation = &json["results"]["files"][0]["rule_groups"][0]["violations"][0];
    let message = violation["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("broken frontmatter wikilink"),
        "unexpected message: {message} in {json}"
    );
    assert_eq!(violation["line"].as_u64(), Some(2), "{json}");
}

#[test]
fn config_opt_out_restores_the_legacy_property_list() {
    let tmp = vault();
    write(
        &tmp,
        "References/Legacy.md",
        "---\nrelated:\n  - \"[[Categories/Books]]\"\n---\n\nBody\n",
    );
    std::fs::write(
        tmp.path().join(".hyalo.toml"),
        "[links]\nfrontmatter = false\n",
    )
    .unwrap();

    let config = run_json(&tmp, &["config"]);
    assert_eq!(config["results"]["links"]["frontmatter"], false, "{config}");

    let json = run_json(&tmp, &["backlinks", "Categories/Books.md"]);
    let sources: Vec<&str> = json["results"]["backlinks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["source"].as_str().unwrap())
        .collect();
    assert!(
        !sources.contains(&"References/Out of Control.md"),
        "`categories:` must stop counting with the opt-out: {json}"
    );
    assert!(
        sources.contains(&"References/Legacy.md"),
        "`related:` must keep counting with the opt-out: {json}"
    );
}

#[test]
fn snapshot_index_serves_the_same_frontmatter_links_as_disk() {
    let tmp = vault();
    hyalo(&tmp).arg("create-index").assert().success();

    let disk = run_json(&tmp, &["backlinks", "Categories/Books.md"]);
    let indexed = run_json(&tmp, &["backlinks", "Categories/Books.md", "--index"]);
    assert_eq!(
        disk["results"], indexed["results"],
        "snapshot backlinks must match the disk scan"
    );

    let disk_links = run_json(
        &tmp,
        &[
            "find",
            "--file",
            "References/Out of Control.md",
            "--fields",
            "links",
        ],
    );
    let indexed_links = run_json(
        &tmp,
        &[
            "find",
            "--file",
            "References/Out of Control.md",
            "--fields",
            "links",
            "--index",
        ],
    );
    assert_eq!(
        disk_links["results"][0]["links"], indexed_links["results"][0]["links"],
        "snapshot link inventory must match the disk scan"
    );
}

// ---------------------------------------------------------------------------
// FM-2 — `mv` rewrites frontmatter wikilinks and shows its counters
// ---------------------------------------------------------------------------

#[test]
fn mv_rewrites_frontmatter_wikilinks_preserving_quotes() {
    let tmp = vault();
    write(
        &tmp,
        "References/Typed.md",
        "---\ntype: \"[[Books]]\"\nrelated:\n  - '[[Books]]'\n---\n\nBody\n",
    );

    let json = run_json(
        &tmp,
        &["mv", "Categories/Books.md", "--to", "Categories/Library.md"],
    );
    assert!(
        json["results"]["total_links_updated"].as_u64().unwrap() >= 4,
        "every frontmatter occurrence must be rewritten: {json}"
    );

    let typed = std::fs::read_to_string(tmp.path().join("References/Typed.md")).unwrap();
    assert!(typed.contains("type: \"[[Library]]\""), "{typed}");
    assert!(typed.contains("- '[[Library]]'"), "{typed}");

    let flow =
        std::fs::read_to_string(tmp.path().join("References/The Machine Stops.md")).unwrap();
    assert!(flow.contains("categories: [\"[[Library]]\"]"), "{flow}");

    let block = std::fs::read_to_string(tmp.path().join("References/Out of Control.md")).unwrap();
    assert!(block.contains("  - \"[[Library]]\""), "{block}");
}

#[test]
fn mv_text_output_carries_both_counters() {
    let tmp = vault();
    let (stdout, _) = run_text(
        &tmp,
        &[
            "mv",
            "Categories/Books.md",
            "--to",
            "Categories/Library.md",
            "--dry-run",
        ],
    );
    assert!(
        stdout.contains("[dry-run] Moved Categories/Books.md → Categories/Library.md"),
        "{stdout}"
    );
    assert!(stdout.contains("files updated: 2"), "{stdout}");
    assert!(stdout.contains("links updated: 3"), "{stdout}");
}

#[test]
fn batch_mv_text_output_carries_both_counters() {
    let tmp = vault();
    let (stdout, _) = run_text(
        &tmp,
        &["mv", "--glob", "Categories/*.md", "--to", "Shelves/"],
    );
    assert!(stdout.contains("[dry-run] Moved 1 file"), "{stdout}");
    assert!(
        stdout.contains("Categories/Books.md → Shelves/Books.md"),
        "{stdout}"
    );
    // Only the path-form body link needs rewriting: a bare `[[Books]]` in
    // `categories:` still resolves to the moved file by stem, so hyalo leaves
    // the author's short form alone. The counters are what this test is for.
    assert!(stdout.contains("files updated: 1"), "{stdout}");
    assert!(stdout.contains("links updated: 1"), "{stdout}");
}

#[test]
fn mv_warns_about_a_frontmatter_wikilink_spanning_a_line_break() {
    let tmp = vault();
    write(
        &tmp,
        "References/Folded.md",
        "---\nsummary: >\n  points at [[Categories/\n  Books]] somehow\ncategories:\n  - \"[[Books]]\"\n---\n\nBody\n",
    );

    let (_, stderr) = run_text(
        &tmp,
        &[
            "mv",
            "Categories/Books.md",
            "--to",
            "Categories/Library.md",
            "--dry-run",
        ],
    );
    assert!(
        stderr.contains("frontmatter wikilink") && stderr.contains("not rewritten"),
        "expected a skip warning, got: {stderr}"
    );

    let json = run_json(
        &tmp,
        &[
            "mv",
            "Categories/Books.md",
            "--to",
            "Categories/Library.md",
            "--dry-run",
        ],
    );
    let skipped = json["results"]["frontmatter_links_skipped"]
        .as_array()
        .expect("skipped list present");
    assert_eq!(skipped.len(), 1, "{json}");
    assert_eq!(skipped[0]["source"], "References/Folded.md", "{json}");
}

// ---------------------------------------------------------------------------
// FM-3 — list-of-wikilink rendering
// ---------------------------------------------------------------------------

#[test]
fn text_output_quotes_wikilink_list_items() {
    let tmp = TempDir::new().unwrap();
    write(
        &tmp,
        "album.md",
        "---\ngenre:\n  - \"[[Futurism]]\"\n  - \"[[Nonfiction]]\"\ntags:\n  - music\n  - jazz\n---\n\nBody\n",
    );
    let (stdout, _) = run_text(&tmp, &["find", "--file", "album.md"]);
    assert!(
        stdout.contains(r#"genre: ["[[Futurism]]", "[[Nonfiction]]"]"#),
        "{stdout}"
    );
    assert!(!stdout.contains("[[[Futurism]]"), "{stdout}");
    // A plain list keeps its compact, unquoted rendering.
    assert!(stdout.contains("tags: [music, jazz]"), "{stdout}");
}

#[test]
fn properties_typed_text_output_quotes_wikilink_list_items() {
    let tmp = TempDir::new().unwrap();
    write(
        &tmp,
        "album.md",
        "---\ngenre:\n  - \"[[Futurism]]\"\n---\n\nBody\n",
    );
    let (stdout, _) = run_text(
        &tmp,
        &["find", "--file", "album.md", "--fields", "properties-typed"],
    );
    assert!(stdout.contains(r#"genre (list): ["[[Futurism]]"]"#), "{stdout}");
}

// ---------------------------------------------------------------------------
// FM-4 — `set` on a list property
// ---------------------------------------------------------------------------

#[test]
fn set_reports_a_list_property_collapsed_to_a_scalar() {
    let tmp = TempDir::new().unwrap();
    write(
        &tmp,
        "Clippings/Buy wisely.md",
        "---\nstatus:\n  - \"[[Backlog]]\"\n---\n\nBody\n",
    );

    let (_, stderr) = run_text(
        &tmp,
        &[
            "set",
            "Clippings/Buy wisely.md",
            "--property",
            "status=Draft",
            "--dry-run",
        ],
    );
    assert!(
        stderr.contains("status was a list in 1 file")
            && stderr.contains("hyalo append"),
        "expected the list-collapse note, got: {stderr}"
    );

    let json = run_json(
        &tmp,
        &[
            "set",
            "Clippings/Buy wisely.md",
            "--property",
            "status=Draft",
            "--dry-run",
        ],
    );
    assert_eq!(
        json["results"]["list_collapsed"]
            .as_array()
            .map(Vec::len)
            .unwrap_or_default(),
        1,
        "{json}"
    );
    assert_eq!(
        json["results"]["list_collapsed"][0], "Clippings/Buy wisely.md",
        "{json}"
    );
}

#[test]
fn set_on_a_scalar_property_reports_no_collapse() {
    let tmp = TempDir::new().unwrap();
    write(&tmp, "note.md", "---\nstatus: draft\n---\n\nBody\n");
    let json = run_json(
        &tmp,
        &["set", "note.md", "--property", "status=done", "--dry-run"],
    );
    assert!(
        json["results"].get("list_collapsed").is_none(),
        "no collapse happened, so the key stays absent: {json}"
    );
}
