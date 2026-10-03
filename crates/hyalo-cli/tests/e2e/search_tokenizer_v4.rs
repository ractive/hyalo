//! Tokenizer v4 (DEC-336): NFKD diacritic folding, per-language folding via
//! frontmatter/config, identifier splitting (whole + parts), CJK bigrams, and
//! `[search] code_blocks = "skip"`.

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
fn ascii_query_finds_the_accented_document() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "accented.md",
        "Préparez le résumé avant vendredi.\n",
    );
    write_md(tmp.path(), "unrelated.md", "Nothing relevant in here.\n");

    assert_eq!(files(&tmp, "resume"), vec!["accented.md"]);
}

#[test]
fn accented_query_finds_the_plain_ascii_document() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "plain.md", "Please review the resume draft.\n");
    write_md(tmp.path(), "unrelated.md", "Nothing relevant in here.\n");

    assert_eq!(files(&tmp, "résumé"), vec!["plain.md"]);
}

#[test]
fn german_frontmatter_language_folds_umlauts() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "haus.md",
        "---\nlanguage: german\n---\nDie H\u{e4}user stehen am Rand der Stadt.\n",
    );
    // default-language note, unaffected
    write_md(tmp.path(), "other.md", "Unrelated English content here.\n");

    assert_eq!(files(&tmp, "hauser"), vec!["haus.md"]);
}

#[test]
fn german_config_language_folds_umlauts() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        ".hyalo.toml",
        "[search]\nlanguage = \"german\"\n",
    );
    write_md(
        tmp.path(),
        "haus.md",
        "Die H\u{e4}user stehen am Rand der Stadt.\n",
    );

    assert_eq!(files(&tmp, "hauser"), vec!["haus.md"]);
}

#[test]
fn identifier_splitting_whole_and_parts_across_three_spellings() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "camel.md",
        "function getUserName() { return 1; }\n",
    );
    write_md(tmp.path(), "snake.md", "fn get_user_name() -> i32 { 1 }\n");
    write_md(tmp.path(), "kebab.md", "see get-user-name for details\n");

    // A single part-word finds every identifier spelling (parts are indexed).
    assert_eq!(
        files(&tmp, "user"),
        vec!["camel.md", "kebab.md", "snake.md"]
    );

    // Querying one spelling finds the other spellings too (both the query and
    // the document split into the same whole+parts stream).
    assert_eq!(
        files(&tmp, "getUserName"),
        vec!["camel.md", "kebab.md", "snake.md"]
    );
}

#[test]
fn exact_identifier_query_matches_and_outranks_a_prose_note() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "identifier.md", "calls getUserName() once\n");
    write_md(tmp.path(), "prose.md", "a function to get the user name\n");

    // Both match: the query's whole-identifier token hits the identifier
    // note directly, and the split parts (get/user/name) all land in the
    // prose note too (query identifier = whole OR all parts).
    assert_eq!(
        files(&tmp, "getUserName"),
        vec!["identifier.md", "prose.md"]
    );

    // The exact identifier note ranks above the note that merely contains
    // the three words as prose.
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "getUserName", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let results = json["results"].as_array().unwrap();
    assert_eq!(results[0]["file"], "identifier.md", "{json}");
    assert_eq!(results[1]["file"], "prose.md", "{json}");
}

#[test]
fn cjk_bigrams_match_on_a_substring() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "jp.md",
        "\u{65e5}\u{672c}\u{8a9e}\u{3092}\u{5b66}\u{3076}\n",
    );
    assert_eq!(files(&tmp, "\u{65e5}\u{672c}"), vec!["jp.md"]);
}

#[test]
fn code_blocks_skip_excludes_fenced_text_but_keeps_inline_code() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        ".hyalo.toml",
        "[search]\ncode_blocks = \"skip\"\n",
    );
    write_md(
        tmp.path(),
        "doc.md",
        "Intro mentions `inlinecode` directly.\n\n```\nfencedsecret appears here\n```\n",
    );

    assert!(files(&tmp, "inlinecode").contains(&"doc.md".to_owned()));
    assert_eq!(files(&tmp, "fencedsecret"), Vec::<String>::new());
}

#[test]
fn code_blocks_default_indexes_fenced_text() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "doc.md",
        "Intro mentions `inlinecode` directly.\n\n```\nfencedsecret appears here\n```\n",
    );

    assert_eq!(files(&tmp, "inlinecode"), vec!["doc.md"]);
    assert_eq!(files(&tmp, "fencedsecret"), vec!["doc.md"]);
}

#[test]
fn invalid_code_blocks_value_warns_and_falls_back_to_index() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        ".hyalo.toml",
        "[search]\ncode_blocks = \"bogus\"\n",
    );
    write_md(tmp.path(), "doc.md", "```\nfencedword appears here\n```\n");

    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "fencedword", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid [search] code_blocks")
            && stderr.contains("bogus")
            && stderr.contains("ignoring"),
        "{stderr}"
    );
    // Falls back to "index": the fenced word is still searchable.
    assert_eq!(files(&tmp, "fencedword"), vec!["doc.md"]);
}

#[test]
fn config_reports_search_defaults_and_overrides() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "content\n");

    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["config", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let search = &json["results"]["search"];
    assert_eq!(search["code_blocks"], "index");
    assert_eq!(search["language"], serde_json::Value::Null);
    assert_eq!(search["weights"]["title"], 3.0);
    assert_eq!(search["weights"]["headings"], 2.0);
    assert_eq!(search["weights"]["tags"], 2.0);
    assert_eq!(search["weights"]["body"], 1.0);
    assert_eq!(search["proximity_bonus"], 0.5);

    write_md(
        tmp.path(),
        ".hyalo.toml",
        "[search]\nlanguage = \"german\"\ncode_blocks = \"skip\"\nproximity_bonus = 0.0\n\n[search.weights]\ntitle = 5.0\nbody = 2.0\n",
    );
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["config", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let search = &json["results"]["search"];
    assert_eq!(search["code_blocks"], "skip");
    assert_eq!(search["language"], "german");
    assert_eq!(search["weights"]["title"], 5.0);
    assert_eq!(search["weights"]["body"], 2.0);
    assert_eq!(search["proximity_bonus"], 0.0);
}
