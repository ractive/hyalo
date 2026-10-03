//! BM25F field weights (DEC-337): title, headings, tags (+ aliases) and body
//! are separate fields with separate weights (default 3/2/2/1).

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

fn ordered_files(tmp: &TempDir, query: &str) -> Vec<String> {
    let (json, output) = run(tmp, &["find", query]);
    assert!(output.status.success(), "{query}: {output:?}");
    json["results"]
        .as_array()
        .unwrap_or_else(|| panic!("no results array for {query:?} in {json}"))
        .iter()
        .map(|r| r["file"].as_str().unwrap().to_owned())
        .collect()
}

fn files(tmp: &TempDir, query: &str) -> Vec<String> {
    let mut out = ordered_files(tmp, query);
    out.sort();
    out
}

#[test]
fn title_hit_outranks_a_single_body_mention() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "title.md",
        "---\ntitle: Kiwi\n---\nsome other words appear once\n",
    );
    write_md(
        tmp.path(),
        "body.md",
        "kiwi appears once here in the body\n",
    );

    assert_eq!(ordered_files(&tmp, "kiwi"), vec!["title.md", "body.md"]);
}

#[test]
fn heading_hit_outranks_plain_body() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "heading.md",
        "# Mango\nplain text words follow\n",
    );
    write_md(
        tmp.path(),
        "body.md",
        "plain text mentions mango in passing\n",
    );

    assert_eq!(ordered_files(&tmp, "mango"), vec!["heading.md", "body.md"]);
}

#[test]
fn tag_only_note_with_no_body_mention_is_found() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "tagged.md",
        "---\ntags: [papaya]\n---\nunrelated body text\n",
    );
    write_md(tmp.path(), "other.md", "completely unrelated content\n");

    assert_eq!(files(&tmp, "papaya"), vec!["tagged.md"]);
}

#[test]
fn alias_only_note_with_no_body_mention_is_found() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "aliased.md",
        "---\naliases: [guava]\n---\nunrelated body text\n",
    );
    write_md(tmp.path(), "other.md", "completely unrelated content\n");

    assert_eq!(files(&tmp, "guava"), vec!["aliased.md"]);
}

#[test]
fn title_weight_config_flips_the_order_against_heavy_body_repetition() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "title.md",
        "---\ntitle: Lime\n---\nother words here too\n",
    );
    write_md(tmp.path(), "body.md", "lime lime lime words everywhere\n");

    // At the default weights (3/1), one title hit against three body
    // repetitions is a near-tie; raising the title weight well above that
    // gives the title hit an unambiguous win.
    write_md(
        tmp.path(),
        ".hyalo.toml",
        "[search.weights]\ntitle = 10.0\n",
    );
    assert_eq!(ordered_files(&tmp, "lime"), vec!["title.md", "body.md"]);

    // Lowering title weight well below the default lets heavy repetition win.
    write_md(tmp.path(), ".hyalo.toml", "[search.weights]\ntitle = 0.1\n");
    assert_eq!(ordered_files(&tmp, "lime"), vec!["body.md", "title.md"]);
}

#[test]
fn negative_weight_warns_and_is_ignored() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        ".hyalo.toml",
        "[search.weights]\ntitle = -1.0\n",
    );
    write_md(tmp.path(), "a.md", "---\ntitle: Kiwi\n---\nbody text\n");

    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "kiwi", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid [search] weights.title") && stderr.contains("ignoring"),
        "{stderr}"
    );

    // Falls back to the default title weight (3.0), reported by `config`.
    let config_output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["config", "--format", "json"])
        .output()
        .unwrap();
    assert!(config_output.status.success(), "{config_output:?}");
    let json: serde_json::Value = serde_json::from_slice(&config_output.stdout).unwrap();
    assert_eq!(json["results"]["search"]["weights"]["title"], 3.0);
}

/// Review item 4: a huge weight would overflow scores to inf/NaN (`score:
/// null`); values above 1000 are refused with a warning.
#[test]
fn oversized_weight_warns_and_scores_stay_finite() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        ".hyalo.toml",
        "[search.weights]\nbody = 1e308\ntitle = 1e308\n",
    );
    write_md(
        tmp.path(),
        "a.md",
        "---\ntitle: Kiwi\n---\nkiwi body kiwi\n",
    );
    write_md(tmp.path(), "b.md", "other text\n");
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "kiwi", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("between 0 and 1000"), "{stderr}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        json["results"][0]["score"]
            .as_f64()
            .is_some_and(f64::is_finite),
        "{json}"
    );
}

#[test]
fn section_granularity_scores_heading_text_in_the_headings_field() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "doc.md",
        "# Mango details\n\nmango appears here once in the body too.\n",
    );
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args([
            "find",
            "mango",
            "--granularity",
            "section",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 1, "{json}");
    assert!(
        json["results"][0]["score"].as_f64().unwrap() > 0.0,
        "{json}"
    );
}
