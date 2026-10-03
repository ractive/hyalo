//! Phrase slop and the proximity bonus (DEC-338).

use super::common::{hyalo_no_hints, write_md};
use tempfile::TempDir;

fn run(tmp: &TempDir, args: &[&str]) -> (serde_json::Value, std::process::Output) {
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(args)
        .args(["--format", "json"])
        .output()
        .unwrap();
    let json = serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Null);
    (json, output)
}

fn files(tmp: &TempDir, query: &str) -> Vec<String> {
    let (json, output) = run(tmp, &["find", query]);
    assert!(output.status.success(), "{query}: {output:?}");
    let mut out: Vec<String> = json["results"]
        .as_array()
        .unwrap_or_else(|| panic!("no results array for {query:?} in {json}"))
        .iter()
        .map(|r| r["file"].as_str().unwrap().to_owned())
        .collect();
    out.sort();
    out
}

#[test]
fn slop_phrase_matches_in_order_within_budget() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "gap.md", "error in the handling path\n");
    write_md(
        tmp.path(),
        "exact.md",
        "a note about error handling steps\n",
    );

    assert!(files(&tmp, "\"error handling\"~3").contains(&"gap.md".to_owned()));
    assert!(files(&tmp, "\"error handling\"~3").contains(&"exact.md".to_owned()));
}

#[test]
fn slop_phrase_does_not_match_reversed_order() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "reversed.md",
        "handling of an unexpected error\n",
    );

    assert_eq!(files(&tmp, "\"error handling\"~3"), Vec::<String>::new());
}

#[test]
fn bare_phrase_without_tilde_requires_exact_adjacency() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "gap.md", "error in the handling path\n");
    write_md(
        tmp.path(),
        "exact.md",
        "a note about error handling steps\n",
    );

    // No `~`: slop is 0, so only the exact-adjacent occurrence matches.
    assert_eq!(files(&tmp, "\"error handling\""), vec!["exact.md"]);
}

#[test]
fn adjacent_terms_rank_above_distant_terms() {
    let tmp = TempDir::new().unwrap();
    let filler = "word ".repeat(30);
    write_md(tmp.path(), "far.md", &format!("snapshot {filler} index\n"));
    write_md(tmp.path(), "near.md", &format!("snapshot index {filler}\n"));

    let (json, output) = run(&tmp, &["find", "snapshot index"]);
    assert!(output.status.success(), "{output:?}");
    let results = json["results"].as_array().unwrap();
    assert_eq!(results.len(), 2, "{json}");
    assert_eq!(results[0]["file"], "near.md", "{json}");
    assert_eq!(results[1]["file"], "far.md", "{json}");
    let near_score = results[0]["score"].as_f64().unwrap();
    let far_score = results[1]["score"].as_f64().unwrap();
    assert!(near_score > far_score, "{json}");
}

#[test]
fn proximity_bonus_zero_ties_adjacent_and_distant_scores() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        ".hyalo.toml",
        "[search]\nproximity_bonus = 0.0\n",
    );
    let filler = "word ".repeat(30);
    write_md(tmp.path(), "far.md", &format!("snapshot {filler} index\n"));
    write_md(tmp.path(), "near.md", &format!("snapshot index {filler}\n"));

    let (json, output) = run(&tmp, &["find", "snapshot index"]);
    assert!(output.status.success(), "{output:?}");
    let results = json["results"].as_array().unwrap();
    assert_eq!(results.len(), 2, "{json}");
    let near_score = results.iter().find(|r| r["file"] == "near.md").unwrap()["score"]
        .as_f64()
        .unwrap();
    let far_score = results.iter().find(|r| r["file"] == "far.md").unwrap()["score"]
        .as_f64()
        .unwrap();
    assert!((near_score - far_score).abs() < 1e-9, "{json}");
}

#[test]
fn snippet_prefers_the_line_with_closest_required_terms() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "doc.md",
        "Intro line without the terms.\nalpha x y z beta far apart here.\nalpha beta adjacent right here.\nOutro line.\n",
    );

    let (json, output) = run(&tmp, &["find", "alpha beta"]);
    assert!(output.status.success(), "{output:?}");
    let matches = json["results"][0]["matches"]
        .as_array()
        .unwrap_or_else(|| panic!("no matches in {json}"));
    assert!(
        matches[0]["text"]
            .as_str()
            .unwrap_or_default()
            .contains("adjacent"),
        "{json}"
    );
}
