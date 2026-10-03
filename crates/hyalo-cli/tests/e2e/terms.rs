use super::common::{hyalo_no_hints, md, write_md};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// `hyalo terms` — BM25 dictionary listing
// ---------------------------------------------------------------------------

/// Vault whose BM25 dictionary is small and fully predictable:
/// - "appl" (stem of apple) appears in both files -> docs=2
/// - "banana" and "cherri" (stem of cherry) each appear in one file -> docs=1
///
/// `title: Apple` is repeated on purpose: the title is tokenized into the
/// same BM25 corpus as the body (so `find` can match on it), and reusing a
/// title stem already present in the body keeps the dictionary free of
/// incidental extra terms a distinct title would introduce.
fn setup_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        md!(r"
---
title: Apple
---
apple apple banana
"),
    );
    write_md(
        tmp.path(),
        "b.md",
        md!(r"
---
title: Apple
---
apple cherry
"),
    );
    tmp
}

#[test]
fn default_listing_sorted_by_docs_then_term() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .arg("terms")
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 3);
    let results = json["results"].as_array().unwrap();
    assert_eq!(results.len(), 3);
    // Highest doc frequency first.
    assert_eq!(results[0]["term"], "appl");
    assert_eq!(results[0]["docs"], 2);
    // Ties (docs=1) break alphabetically: "banana" < "cherri".
    assert_eq!(results[1]["term"], "banana");
    assert_eq!(results[1]["docs"], 1);
    assert_eq!(results[2]["term"], "cherri");
    assert_eq!(results[2]["docs"], 1);
}

#[test]
fn prefix_filters_and_lowercases_uppercase_input() {
    let tmp = setup_vault();
    for prefix in ["ap", "AP", "Ap"] {
        let output = hyalo_no_hints()
            .args(["--dir", tmp.path().to_str().unwrap()])
            .args(["terms", prefix])
            .output()
            .unwrap();

        assert!(output.status.success(), "{prefix}: {output:?}");
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(json["total"], 1, "prefix {prefix}: {json}");
        let results = json["results"].as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["term"], "appl");
    }
}

#[test]
fn prefix_with_no_match_returns_empty_results_exit_0() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["terms", "zzz-no-such-term"])
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 0);
    assert_eq!(json["results"].as_array().unwrap().len(), 0);
}

#[test]
fn empty_vault_returns_empty_results_exit_0() {
    let tmp = TempDir::new().unwrap();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .arg("terms")
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 0);
    assert_eq!(json["results"].as_array().unwrap().len(), 0);
}

#[test]
fn limit_caps_results_but_total_reflects_full_dictionary() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["terms", "--limit", "1"])
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 3, "total must count before --limit applies");
    assert_eq!(json["results"].as_array().unwrap().len(), 1);
    assert_eq!(json["results"][0]["term"], "appl");
}

#[test]
fn limit_zero_means_unlimited() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["terms", "--limit", "0"])
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 3);
    assert_eq!(json["results"].as_array().unwrap().len(), 3);
}

#[test]
fn count_prints_bare_total() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["terms", "--count"])
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "3", "got: {stdout:?}");
}

#[test]
fn jq_extracts_top_term() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--jq", ".results[0].term"])
        .arg("terms")
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.trim().contains("appl"), "got: {stdout}");
}

#[test]
fn format_text_prints_term_and_docs_per_line() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["terms", "--format", "text"])
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("appl"), "got: {stdout:?}");
    assert!(stdout.contains('2'), "got: {stdout:?}");
    // JSON envelope punctuation should not leak into text rendering.
    assert!(!stdout.contains('{'), "got: {stdout:?}");
}

#[test]
fn index_matches_disk_scan() {
    let tmp = setup_vault();
    let dir = tmp.path().to_str().unwrap().to_owned();

    let disk = hyalo_no_hints()
        .args(["--dir", &dir])
        .arg("terms")
        .output()
        .unwrap();
    assert!(disk.status.success(), "{disk:?}");

    let create = hyalo_no_hints()
        .args(["--dir", &dir])
        .arg("create-index")
        .output()
        .unwrap();
    assert!(create.status.success(), "{create:?}");

    let indexed = hyalo_no_hints()
        .args(["--dir", &dir])
        .args(["terms", "--index"])
        .output()
        .unwrap();
    assert!(indexed.status.success(), "{indexed:?}");

    let disk_json: serde_json::Value = serde_json::from_slice(&disk.stdout).unwrap();
    let indexed_json: serde_json::Value = serde_json::from_slice(&indexed.stdout).unwrap();
    assert_eq!(disk_json["results"], indexed_json["results"]);
    assert_eq!(disk_json["total"], indexed_json["total"]);
}

#[test]
fn german_frontmatter_language_contributes_german_stems() {
    let tmp = TempDir::new().unwrap();
    // Mirrors the German fixture in bm25.rs: the German stemmer normalises
    // the umlaut, so "Häuser" and "Haus" share the stem "haus".
    write_md(
        tmp.path(),
        "german.md",
        md!(r"
---
title: German
language: german
---
Häuser
"),
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["terms", "haus"])
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 1, "{json}");
    assert_eq!(json["results"][0]["term"], "haus");
    assert_eq!(json["results"][0]["docs"], 1);
}
