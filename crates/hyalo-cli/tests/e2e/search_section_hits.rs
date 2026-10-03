//! `find PATTERN --granularity section` (DEC-334): one result per matching
//! SECTION instead of per file.

use super::common::{hyalo, hyalo_no_hints, write_md};
use tempfile::TempDir;

fn run(tmp: &TempDir, args: &[&str]) -> (serde_json::Value, std::process::Output) {
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(args)
        .output()
        .unwrap();
    let json = serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Null);
    (json, output)
}

/// A file whose preamble (before any heading) carries one term and whose
/// nested `## Nested Level` heading (under `# Top Level`) carries another, so
/// a single vault exercises the preamble shape, the heading-path shape and
/// (via known frontmatter length) the body-offset math the read hints rely
/// on. Frontmatter is exactly 3 lines, so body line 1 is file line 4.
fn nested_heading_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "doc.md",
        "---\ntitle: Doc\n---\nOpening mentions quokka77.\n\n# Top Level\n\nSome filler.\n\n## Nested Level\n\nwombat88 appears again here.\n",
    );
    tmp
}

#[test]
fn default_granularity_has_no_section_key_and_equals_explicit_file_mode() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "elephant giraffe content.\n");

    let (default_json, out1) = run(&tmp, &["find", "elephant", "--format", "json"]);
    assert!(out1.status.success(), "{out1:?}");
    let results = default_json["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert!(
        results[0].as_object().unwrap().get("section").is_none(),
        "{default_json}"
    );

    let (file_mode_json, out2) = run(
        &tmp,
        &[
            "find",
            "elephant",
            "--granularity",
            "file",
            "--format",
            "json",
        ],
    );
    assert!(out2.status.success(), "{out2:?}");
    assert_eq!(default_json, file_mode_json);
}

#[test]
fn section_shape_covers_preamble_and_nested_heading_path() {
    let tmp = nested_heading_vault();

    // Preamble: heading null, level 0, empty path.
    let (json, output) = run(
        &tmp,
        &[
            "find",
            "quokka77",
            "--granularity",
            "section",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["total"], 1);
    let hit = &json["results"][0];
    assert_eq!(hit["file"], "doc.md");
    assert_eq!(hit["section"]["heading"], serde_json::Value::Null);
    assert_eq!(hit["section"]["level"], 0);
    assert_eq!(hit["section"]["path"], serde_json::json!([]));
    assert_eq!(hit["section"]["line_start"], 4);
    assert_eq!(hit["section"]["line_end"], 5);
    assert!(hit["score"].as_f64().unwrap() > 0.0);
    assert!(hit["matches"].as_array().is_some());

    // Nested heading: path ends with this heading, includes its ancestor.
    let (json, output) = run(
        &tmp,
        &[
            "find",
            "wombat88",
            "--granularity",
            "section",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["total"], 1);
    let hit = &json["results"][0];
    assert_eq!(hit["section"]["heading"], "Nested Level");
    assert_eq!(hit["section"]["level"], 2);
    assert_eq!(
        hit["section"]["path"],
        serde_json::json!(["Top Level", "Nested Level"])
    );
    assert_eq!(hit["section"]["line_start"], 10);
    assert_eq!(hit["section"]["line_end"], 12);
}

#[test]
fn and_terms_must_both_land_in_the_same_section() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "split.md",
        "# Section One\n\nbadger42 text.\n\n# Section Two\n\nferret42 text.\n",
    );

    // File-level: both terms occur somewhere in the document, so it matches.
    let (json, output) = run(&tmp, &["find", "badger42 ferret42", "--format", "json"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["total"], 1);

    // Section-level: no single section has both -> zero section hits.
    let (json, output) = run(
        &tmp,
        &[
            "find",
            "badger42 ferret42",
            "--granularity",
            "section",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["total"], 0);
    assert_eq!(json["results"].as_array().unwrap().len(), 0);
}

#[test]
fn or_group_is_satisfied_per_section() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "split.md",
        "# Section One\n\nbadger43 text.\n\n# Section Two\n\nferret43 text.\n",
    );

    let (json, output) = run(
        &tmp,
        &[
            "find",
            "badger43 OR ferret43",
            "--granularity",
            "section",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["total"], 2);
    let mut headings: Vec<&str> = json["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["section"]["heading"].as_str().unwrap())
        .collect();
    headings.sort_unstable();
    assert_eq!(headings, vec!["Section One", "Section Two"]);
}

#[test]
fn negation_is_file_level_and_excludes_every_section() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "split.md",
        "# Section One\n\nalpha99 text.\n\n# Section Two\n\nbeta99 text.\n",
    );

    // Control: without the negation, Section One alone satisfies "alpha99".
    let (json, output) = run(
        &tmp,
        &[
            "find",
            "alpha99",
            "--granularity",
            "section",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["total"], 1);
    assert_eq!(json["results"][0]["section"]["heading"], "Section One");

    // The file also carries "beta99" (in its OTHER section), so "-beta99"
    // excludes the whole file -- Section One yields no hit even though it
    // never mentions beta99 itself.
    let (json, output) = run(
        &tmp,
        &[
            "find",
            "alpha99 -beta99",
            "--granularity",
            "section",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["total"], 0);
}

#[test]
fn section_filter_restricts_eligible_sections() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "doc.md",
        "# Keep This\n\nlemur64 content.\n\n# Skip This\n\nlemur64 content too.\n",
    );

    let (json, output) = run(
        &tmp,
        &[
            "find",
            "lemur64",
            "--granularity",
            "section",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["total"], 2);

    let (json, output) = run(
        &tmp,
        &[
            "find",
            "lemur64",
            "--granularity",
            "section",
            "--section",
            "Keep This",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["total"], 1);
    assert_eq!(json["results"][0]["section"]["heading"], "Keep This");
}

#[test]
fn limit_counts_sections_while_total_reflects_every_hit() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "f1.md", "# H\n\necho555 one.\n");
    write_md(tmp.path(), "f2.md", "# H\n\necho555 two.\n");
    write_md(tmp.path(), "f3.md", "# H\n\necho555 three.\n");

    let (json, output) = run(
        &tmp,
        &[
            "find",
            "echo555",
            "--granularity",
            "section",
            "--limit",
            "1",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["total"], 3);
    assert_eq!(json["results"].as_array().unwrap().len(), 1);
}

fn expect_rejected(tmp: &TempDir, args: &[&str], needle: &str) {
    let (_, output) = run(tmp, args);
    assert_eq!(output.status.code(), Some(1), "{args:?}: {output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stderr)
        .unwrap_or_else(|e| panic!("{args:?}: not JSON: {e}\nstderr: {output:?}"));
    let error = json["error"].as_str().unwrap_or_else(|| {
        panic!("{args:?}: no 'error' key in {json}");
    });
    assert!(
        error.contains(needle),
        "{args:?}: expected {error:?} to contain {needle:?}"
    );
}

#[test]
fn rejects_no_pattern() {
    let tmp = nested_heading_vault();
    expect_rejected(
        &tmp,
        &["find", "--granularity", "section"],
        "requires a ranked PATTERN",
    );
}

#[test]
fn rejects_regexp() {
    let tmp = nested_heading_vault();
    expect_rejected(
        &tmp,
        &["find", "--granularity", "section", "-e", "foo"],
        "--regexp",
    );
}

#[test]
fn rejects_sort_other_than_score() {
    let tmp = nested_heading_vault();
    expect_rejected(
        &tmp,
        &[
            "find",
            "wombat88",
            "--granularity",
            "section",
            "--sort",
            "modified",
        ],
        "--sort",
    );
}

#[test]
fn rejects_reverse() {
    let tmp = nested_heading_vault();
    expect_rejected(
        &tmp,
        &["find", "wombat88", "--granularity", "section", "--reverse"],
        "--reverse",
    );
}

#[test]
fn rejects_fields() {
    let tmp = nested_heading_vault();
    expect_rejected(
        &tmp,
        &[
            "find",
            "wombat88",
            "--granularity",
            "section",
            "--fields",
            "title",
        ],
        "--fields",
    );
}

#[test]
fn rejects_field_only_pattern() {
    let tmp = nested_heading_vault();
    expect_rejected(
        &tmp,
        &["find", "title:doc", "--granularity", "section"],
        "needs a text term",
    );
}

#[test]
fn rejects_negation_only_pattern() {
    let tmp = nested_heading_vault();
    expect_rejected(
        &tmp,
        &["find", "--granularity", "section", "--", "-wombat88"],
        "needs a text term",
    );
}

#[test]
fn identical_results_with_and_without_index() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "f1.md", "# H\n\necho555 one.\n");
    write_md(tmp.path(), "f2.md", "# H\n\necho555 two.\n");
    write_md(tmp.path(), "f3.md", "# H\n\necho555 three.\n");

    let disk = run(
        &tmp,
        &[
            "find",
            "echo555",
            "--granularity",
            "section",
            "--format",
            "json",
        ],
    )
    .0;

    let create = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .arg("create-index")
        .output()
        .unwrap();
    assert!(create.status.success(), "{create:?}");

    let indexed = run(
        &tmp,
        &[
            "find",
            "echo555",
            "--granularity",
            "section",
            "--index",
            "--format",
            "json",
        ],
    )
    .0;
    assert_eq!(disk, indexed);

    let index_path = tmp.path().join(".hyalo-index");
    let indexed_via_path = run(
        &tmp,
        &[
            "find",
            "echo555",
            "--granularity",
            "section",
            "--index-file",
            index_path.to_str().unwrap(),
            "--format",
            "json",
        ],
    )
    .0;
    assert_eq!(disk, indexed_via_path);
}

#[test]
fn text_mode_renders_file_heading_lines_score_then_snippets() {
    let tmp = nested_heading_vault();
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args([
            "find",
            "wombat88",
            "--granularity",
            "section",
            "--format",
            "text",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("doc.md#Nested Level (lines 10-12)"),
        "{stdout:?}"
    );
    assert!(
        stdout.contains("line 12: wombat88 appears again here."),
        "{stdout:?}"
    );
}

#[test]
fn read_hint_uses_section_when_heading_is_unique() {
    let tmp = nested_heading_vault();
    let output = hyalo()
        .arg("--dir")
        .arg(tmp.path())
        .args([
            "find",
            "wombat88",
            "--granularity",
            "section",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let hints = json["hints"].as_array().unwrap();
    let hint = hints
        .iter()
        .find(|h| {
            h["cmd"]
                .as_str()
                .unwrap_or_default()
                .starts_with("hyalo read")
        })
        .unwrap_or_else(|| panic!("no read hint in {hints:?}"));
    let cmd = hint["cmd"].as_str().unwrap();
    assert!(cmd.contains("--section"), "{cmd}");
    assert!(cmd.contains("Nested Level"), "{cmd}");
    assert_eq!(hint["writes"], false, "{hint}");
}

#[test]
fn read_hint_uses_body_relative_lines_when_heading_is_null() {
    let tmp = nested_heading_vault();
    let output = hyalo()
        .arg("--dir")
        .arg(tmp.path())
        .args([
            "find",
            "quokka77",
            "--granularity",
            "section",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let hints = json["hints"].as_array().unwrap();
    let hint = hints
        .iter()
        .find(|h| {
            h["cmd"]
                .as_str()
                .unwrap_or_default()
                .starts_with("hyalo read")
        })
        .unwrap_or_else(|| panic!("no read hint in {hints:?}"));
    let cmd = hint["cmd"].as_str().unwrap();
    // File-absolute lines 4-5, minus the 3-line frontmatter -> body-relative 1-2.
    assert!(cmd.contains("--lines"), "{cmd}");
    assert!(cmd.contains("1:2"), "{cmd}");
    assert_eq!(hint["writes"], false, "{hint}");
}

fn run_hinted(tmp: &TempDir, args: &[&str]) -> (serde_json::Value, std::process::Output) {
    let output = hyalo()
        .arg("--dir")
        .arg(tmp.path())
        .args(args)
        .output()
        .unwrap();
    let json = serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Null);
    (json, output)
}

fn hit_headings(json: &serde_json::Value) -> Vec<String> {
    json["results"]
        .as_array()
        .unwrap_or_else(|| panic!("no results in {json}"))
        .iter()
        .map(|h| {
            format!(
                "{}#{}",
                h["file"].as_str().unwrap_or_default(),
                h["section"]["heading"].as_str().unwrap_or("null")
            )
        })
        .collect()
}

#[test]
fn section_filter_does_not_hide_a_negated_term_elsewhere_in_the_file() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "neg.md", "# Keep\nalpha\n# Other\nbeta\n");
    let (json, output) = run(
        &tmp,
        &[
            "find",
            "alpha -beta",
            "--granularity",
            "section",
            "--section",
            "Keep",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["total"], 0, "{json}");
}

#[test]
fn double_negation_ranks_the_inner_term_per_section() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "d.md", "# Keep\nalpha\n# Other\nbeta\n");
    let (json, output) = run(
        &tmp,
        &[
            "find",
            "--granularity",
            "section",
            "--format",
            "json",
            "--",
            "-(-alpha)",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(hit_headings(&json), vec!["d.md#Keep".to_owned()]);
}

#[test]
fn nested_negated_group_matches_either_term_per_section() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "d.md",
        "# Keep\nalpha\n# Other\nbeta\n# Third\ngamma\n",
    );
    let (json, output) = run(
        &tmp,
        &[
            "find",
            "--granularity",
            "section",
            "--format",
            "json",
            "--",
            "-(-alpha -beta)",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    let mut headings = hit_headings(&json);
    headings.sort();
    assert_eq!(
        headings,
        vec!["d.md#Keep".to_owned(), "d.md#Other".to_owned()]
    );
}

#[test]
fn field_term_inside_or_holds_for_every_section_of_its_file() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "split.md",
        "---\ntitle: Split\n---\n# One\napple\n# Two\nbanana\n",
    );
    write_md(tmp.path(), "nohead.md", "kiwi words\n");
    let (json, output) = run(
        &tmp,
        &[
            "find",
            "title:Split OR kiwi",
            "--granularity",
            "section",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    let mut headings = hit_headings(&json);
    headings.sort();
    assert_eq!(
        headings,
        vec![
            "nohead.md#null".to_owned(),
            "split.md#One".to_owned(),
            "split.md#Two".to_owned()
        ]
    );
}

#[test]
fn oversized_trailing_line_extends_the_last_section() {
    let tmp = TempDir::new().unwrap();
    let long = "x".repeat(1024 * 1024 + 16);
    write_md(tmp.path(), "big.md", &format!("# Only\nalpha\n{long}\n"));
    let (json, output) = run(
        &tmp,
        &[
            "find",
            "alpha",
            "--granularity",
            "section",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["results"][0]["section"]["line_end"], 3, "{json}");
}

#[test]
fn terms_in_different_sections_explain_the_empty_answer() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "s.md", "# One\napple\n# Two\nbanana\n");
    let output = hyalo()
        .arg("--dir")
        .arg(tmp.path())
        .args([
            "find",
            "apple banana",
            "--granularity",
            "section",
            "--format",
            "text",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        all.contains("1 file matches at file level, but no single section holds all terms"),
        "{all}"
    );
    assert!(
        all.contains("'apple OR banana' --granularity section"),
        "{all}"
    );
    assert!(all.contains("--granularity file"), "{all}");
    assert!(!all.contains("hyalo terms"), "{all}");
}

#[test]
fn filenames_only_lists_each_file_once_in_section_mode() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "s.md", "# One\napple\n# Two\napple again\n");
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args([
            "find",
            "apple",
            "--granularity",
            "section",
            "--filenames-only",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout).lines().count(), 1);
}

#[test]
fn section_mode_drilldown_compares_buckets_with_the_file_count() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        "---\ntags: [x]\n---\n# One\napple\n# Two\napple\n",
    );
    let (json, output) = run_hinted(
        &tmp,
        &[
            "find",
            "apple",
            "--granularity",
            "section",
            "--facet",
            "tags",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["total"], 2);
    let cmds: Vec<&str> = json["hints"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|h| h["cmd"].as_str())
        .collect();
    assert!(!cmds.iter().any(|c| c.contains("--tag x")), "{cmds:?}");
}
