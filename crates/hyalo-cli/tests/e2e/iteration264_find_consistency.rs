//! Iteration 264 — `find`: sort direction, null-aware filters, projection shape.
//!
//! Every test here pins one finding from the v0.22.0 Obsidian-vault dogfood
//! run: a sort key that ordered the opposite way from every other key (BUG-4),
//! filters that could not express "is null" and compared mixed types as text
//! (BUG-17, BUG-18), input the parser accepted although it could only ever
//! mean a mistake (BUG-23, BUG-24, COH-13), and three output-shape mismatches
//! (BUG-20, BUG-21, BUG-22).

use super::common::{hyalo_no_hints, write_md};
use tempfile::TempDir;

/// A small vault whose link graph and frontmatter are shaped for these tests.
///
/// `hub.md` is linked from three notes, `mid.md` from one, `lonely.md` from
/// none, so `--sort backlinks_count` has a strict order to produce.
fn vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write_md(
        tmp.path(),
        "hub.md",
        "---\ntitle: Hub\nrating: 9\naliases:\n---\n\nThe hub note about kestrels.\n",
    );
    write_md(
        tmp.path(),
        "mid.md",
        "---\ntitle: Mid\nrating: \"10\"\naliases: []\nlast: 2023-09-05\n---\n\nLinks to [[hub]].\n",
    );
    write_md(
        tmp.path(),
        "a-linker.md",
        "---\ntitle: A Linker\nrating: 3\naliases: [x]\nlast: \"[[2022-04]]\"\n---\n\n[[hub]] and [[mid]].\n",
    );
    write_md(
        tmp.path(),
        "b-linker.md",
        "---\ntitle: B Linker\naliases: [null]\n---\n\n[[hub]].\n",
    );
    write_md(
        tmp.path(),
        "lonely.md",
        "---\ntitle: Lonely\naliases: ~\n---\n\nNothing points here.\n",
    );
    tmp
}

/// Run `find` in `dir` and return the parsed JSON envelope.
fn find_json(dir: &std::path::Path, args: &[&str]) -> serde_json::Value {
    let output = hyalo_no_hints()
        .current_dir(dir)
        .arg("find")
        .args(args)
        .args(["--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "find {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

/// The `file` field of every result, in order.
fn files(envelope: &serde_json::Value) -> Vec<String> {
    envelope["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["file"].as_str().unwrap().to_owned())
        .collect()
}

// ---------------------------------------------------------------------------
// BUG-4 / COH-12 — one direction for every sort key
// ---------------------------------------------------------------------------

#[test]
fn sort_backlinks_count_ascends_and_reverse_puts_the_most_linked_first() {
    let tmp = vault();

    let ascending = find_json(tmp.path(), &["--sort", "backlinks_count", "--limit", "0"]);
    let asc = files(&ascending);
    assert_eq!(
        asc.last().map(String::as_str),
        Some("hub.md"),
        "ascending must end with the most-linked file: {asc:?}"
    );
    let first_backlinks = ascending["results"][0]["backlinks"].as_array().unwrap();
    assert!(
        first_backlinks.is_empty(),
        "ascending must start at zero backlinks"
    );

    let descending = find_json(
        tmp.path(),
        &["--sort", "backlinks_count", "--reverse", "--limit", "0"],
    );
    let desc = files(&descending);
    assert_eq!(
        desc.first().map(String::as_str),
        Some("hub.md"),
        "--reverse must mean most-linked first: {desc:?}"
    );
    assert_eq!(
        descending["results"][0]["backlinks"]
            .as_array()
            .unwrap()
            .len(),
        3,
        "the top result under --reverse must carry a non-empty backlinks field"
    );
}

#[test]
fn sort_links_count_ascends_and_reverse_puts_the_most_linking_first() {
    let tmp = vault();

    let asc = files(&find_json(
        tmp.path(),
        &["--sort", "links_count", "--limit", "0"],
    ));
    assert_eq!(
        asc.last().map(String::as_str),
        Some("a-linker.md"),
        "a-linker.md has two outbound links: {asc:?}"
    );

    let desc = files(&find_json(
        tmp.path(),
        &["--sort", "links_count", "--reverse", "--limit", "0"],
    ));
    assert_eq!(desc.first().map(String::as_str), Some("a-linker.md"));
}

#[test]
fn sort_score_ranks_the_best_match_first_without_reverse() {
    let tmp = vault();
    let envelope = find_json(tmp.path(), &["kestrels", "--sort", "score", "--limit", "0"]);
    let ranked = files(&envelope);
    assert_eq!(
        ranked.first().map(String::as_str),
        Some("hub.md"),
        "score's unreversed order is best-match-first: {ranked:?}"
    );
}

#[test]
fn sort_property_keeps_missing_values_last_in_both_directions() {
    let tmp = vault();

    let asc = find_json(
        tmp.path(),
        &["--sort", "property:rating", "--limit", "0", "--fields", "properties"],
    );
    let asc_ratings: Vec<&serde_json::Value> = asc["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| &r["properties"]["rating"])
        .collect();
    assert!(
        asc_ratings.last().unwrap().is_null(),
        "files without the sort property must sort last: {asc_ratings:?}"
    );

    let desc = find_json(
        tmp.path(),
        &[
            "--sort",
            "property:rating",
            "--reverse",
            "--limit",
            "0",
            "--fields",
            "properties",
        ],
    );
    let desc_ratings: Vec<&serde_json::Value> = desc["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| &r["properties"]["rating"])
        .collect();
    assert!(
        desc_ratings.last().unwrap().is_null(),
        "--reverse must not float nulls to the front: {desc_ratings:?}"
    );
    assert!(
        !desc_ratings.first().unwrap().is_null(),
        "the reversed run must start with a real value: {desc_ratings:?}"
    );
}

#[test]
fn short_help_lists_the_score_sort_key() {
    let output = hyalo_no_hints().args(["find", "-h"]).output().unwrap();
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(
        help.contains("score"),
        "`find -h` must name the score sort key"
    );
}

// ---------------------------------------------------------------------------
// BUG-17 / BUG-18 — null-aware filters and typed comparisons
// ---------------------------------------------------------------------------

#[test]
fn property_equals_null_matches_only_a_real_null_value() {
    let tmp = vault();
    let matched = files(&find_json(
        tmp.path(),
        &["--property", "aliases=null", "--limit", "0"],
    ));
    // hub.md (`aliases:` with no value) and lonely.md (`aliases: ~`) are null;
    // mid.md is an empty list and b-linker.md a list containing a null.
    assert_eq!(matched, vec!["hub.md", "lonely.md"], "got {matched:?}");
}

#[test]
fn property_not_equals_null_matches_present_and_non_null() {
    let tmp = vault();
    let matched = files(&find_json(
        tmp.path(),
        &["--property", "aliases!=null", "--limit", "0"],
    ));
    assert_eq!(
        matched,
        vec!["a-linker.md", "b-linker.md", "mid.md"],
        "got {matched:?}"
    );
}

#[test]
fn property_equals_empty_list_matches_only_an_empty_sequence() {
    let tmp = vault();
    let matched = files(&find_json(
        tmp.path(),
        &["--property", "aliases=[]", "--limit", "0"],
    ));
    assert_eq!(matched, vec!["mid.md"], "got {matched:?}");
}

#[test]
fn null_filter_and_properties_typed_agree() {
    let tmp = vault();
    let envelope = find_json(
        tmp.path(),
        &[
            "--property",
            "aliases=null",
            "--limit",
            "0",
            "--fields",
            "properties-typed",
        ],
    );
    for result in envelope["results"].as_array().unwrap() {
        let typed = result["properties_typed"].as_array().unwrap();
        let aliases = typed
            .iter()
            .find(|p| p["name"] == "aliases")
            .unwrap_or_else(|| panic!("aliases missing from {typed:?}"));
        assert_eq!(
            aliases["type"], "null",
            "properties_typed must report the same nullness the filter matched"
        );
    }
}

#[test]
fn date_comparison_skips_a_non_date_string() {
    let tmp = vault();
    let matched = files(&find_json(
        tmp.path(),
        &["--property", "last>=2023-09-01", "--limit", "0"],
    ));
    assert_eq!(
        matched,
        vec!["mid.md"],
        "a `[[2022-04]]` wikilink string is not a date: {matched:?}"
    );
}

#[test]
fn numeric_comparison_reads_a_quoted_number() {
    let tmp = vault();
    let matched = files(&find_json(
        tmp.path(),
        &["--property", "rating>=6", "--limit", "0"],
    ));
    // mid.md's rating is the string "10" — numeric, so it matches, and it
    // sorts above 9 rather than below it as text would.
    assert_eq!(matched, vec!["hub.md", "mid.md"], "got {matched:?}");
}

// ---------------------------------------------------------------------------
// BUG-23 / BUG-24 / COH-13 — reject input that can only be a mistake
// ---------------------------------------------------------------------------

/// Run a `find` expected to fail, returning (exit code, stderr).
fn find_failure(dir: &std::path::Path, args: &[&str]) -> (i32, String) {
    let output = hyalo_no_hints()
        .current_dir(dir)
        .arg("find")
        .args(args)
        .args(["--format", "text"])
        .output()
        .unwrap();
    assert!(!output.status.success(), "expected failure for {args:?}");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn empty_property_regex_is_rejected() {
    let tmp = vault();
    for filter in ["title~=//", "title~=//i", "title~="] {
        let (code, stderr) = find_failure(tmp.path(), &["--property", filter]);
        assert_eq!(code, 1, "user error exits 1 for {filter}");
        assert!(
            stderr.contains("empty regex"),
            "{filter}: expected an empty-regex error, got {stderr}"
        );
    }
}

#[test]
fn perl_style_regex_operator_is_rejected_and_names_the_right_one() {
    let tmp = vault();
    let (code, stderr) = find_failure(tmp.path(), &["--property", "title=~/iter/"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("=~"), "{stderr}");
    assert!(
        stderr.contains("title~=/iter/"),
        "the message must show the `~=` spelling: {stderr}"
    );
}

#[test]
fn eq_tilde_no_longer_matches_every_null_by_accident() {
    let tmp = vault();
    // `aliases=~` used to parse as equality against the literal "~" and match
    // every file whose value was a YAML null.
    let (code, stderr) = find_failure(tmp.path(), &["--property", "aliases=~"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("=~"), "{stderr}");
}

#[test]
fn empty_fields_selection_is_rejected() {
    let tmp = vault();
    for value in ["", ","] {
        let (code, stderr) = find_failure(tmp.path(), &["--fields", value]);
        assert_eq!(code, 1, "user error exits 1 for --fields {value:?}");
        assert!(
            stderr.contains("unknown field") && stderr.contains("valid fields are"),
            "--fields {value:?}: expected the unknown-field message, got {stderr}"
        );
    }
}

#[test]
fn the_same_rejections_apply_to_the_shared_where_property_parser() {
    let tmp = vault();
    for (cmd, flag) in [
        ("set", "--where-property"),
        ("remove", "--where-property"),
        ("append", "--where-property"),
    ] {
        let output = hyalo_no_hints()
            .current_dir(tmp.path())
            .args([cmd, "--glob", "*.md", flag, "title=~/Hub/"])
            .args(["--property", "reviewed=true", "--dry-run"])
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "{cmd} {flag} must reject the `=~` operator too"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("=~"), "{cmd}: {stderr}");
    }
}

// ---------------------------------------------------------------------------
// BUG-20 / BUG-21 / BUG-22 — output shape
// ---------------------------------------------------------------------------

#[test]
fn filenames_only_emits_exactly_one_newline_per_path() {
    let tmp = vault();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["find", "--limit", "0", "--filenames-only"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line_count = stdout.matches('\n').count();

    let count = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["find", "--limit", "0", "--count"])
        .output()
        .unwrap();
    let expected: usize = String::from_utf8_lossy(&count.stdout).trim().parse().unwrap();

    assert_eq!(
        line_count, expected,
        "`--filenames-only | wc -l` must equal `--count`; got {stdout:?}"
    );
    assert!(
        !stdout.ends_with("\n\n"),
        "no trailing blank line: {stdout:?}"
    );
}

#[test]
fn filenames_only_on_zero_results_prints_nothing() {
    let tmp = vault();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["find", "--property", "nope=1", "--filenames-only"])
        .output()
        .unwrap();
    assert_eq!(output.stdout, b"", "an empty result set prints nothing");
}

#[test]
fn files_from_and_file_return_the_same_results_array() {
    let tmp = vault();
    let via_file = find_json(tmp.path(), &["--file", "hub.md"]);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["find", "--files-from", "-", "--format", "json"])
        .write_stdin("hub.md\n")
        .output()
        .unwrap();
    let via_stdin: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    assert_eq!(
        via_stdin["results"], via_file["results"],
        "`--files-from -` and `--file` must answer at the same path"
    );
    assert!(
        via_stdin["results"].is_array(),
        "results stays a bare array under --files-from"
    );
}

#[test]
fn files_from_counters_are_top_level_and_always_present_on_find() {
    let tmp = vault();
    let plain = find_json(tmp.path(), &["--limit", "1"]);
    for key in [
        "files_missing",
        "files_skipped_non_md",
        "files_skipped_outside_vault",
    ] {
        assert_eq!(
            plain[key], 0,
            "{key} must be present and zero without --files-from"
        );
    }

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["find", "--files-from", "-", "--format", "json"])
        .write_stdin("hub.md\nnot-there.md\nnotes.txt\n")
        .output()
        .unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["files_missing"], 1);
    assert_eq!(envelope["files_skipped_non_md"], 1);
    assert!(
        envelope["results"].is_array(),
        "counters must not wrap results in a `files` object"
    );
}

#[test]
fn properties_typed_is_addressable_and_accepts_both_spellings() {
    let tmp = vault();
    for spelling in ["properties-typed", "properties_typed"] {
        let envelope = find_json(tmp.path(), &["--file", "hub.md", "--fields", spelling]);
        assert!(
            envelope["results"][0]["properties_typed"].is_array(),
            "--fields {spelling} must produce the properties_typed key"
        );
    }
}
