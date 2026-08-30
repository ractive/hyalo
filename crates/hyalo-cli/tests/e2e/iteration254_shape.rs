//! Iteration 254 — the two shape narrowings iteration 252 left implicit, and
//! the docs that describe them.
//!
//! - **FIND-1 (DEC-254), exact projection.** `file` is the only unconditional
//!   key. `modified`, `size` and `lines` are ordinary members of the *default*
//!   set — cheap, and the inputs an agent uses to choose its next call — but an
//!   explicit `--fields` that does not name them drops them, so `--fields
//!   title` costs what it says it costs.
//! - **FIND-3/FIND-4 (DEC-252 amendment), non-string `title`.** Every scalar
//!   promotes, stringified as written; a collection cannot promote, so it stays
//!   reachable in `properties` and `HYALO007` reports it.
//! - **COH-4.** The root `--help` JSON cookbook's key lists are asserted against
//!   the live output, so they cannot drift again.

use super::common::{hyalo_no_hints, write_md};
use std::path::Path;
use tempfile::TempDir;

/// Run `hyalo <argv…> --format json` (no hints) and return the envelope.
fn envelope(dir: &Path, argv: &[&str]) -> serde_json::Value {
    let output = hyalo_no_hints()
        .current_dir(dir)
        .args(argv)
        .args(["--format", "json"])
        .output()
        .expect("hyalo should run");
    assert!(
        output.status.success(),
        "{argv:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("an envelope")
}

/// Sorted keys of the first `find` result item.
fn first_item_keys(dir: &Path, argv: &[&str]) -> Vec<String> {
    let env = envelope(dir, argv);
    let arr = env["results"].as_array().expect("results array");
    assert!(!arr.is_empty(), "expected at least one result for {argv:?}");
    arr[0]
        .as_object()
        .expect("result item object")
        .keys()
        .cloned()
        .collect()
}

/// A small vault with everything the default field set reports.
fn vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "alpha.md",
        "---\ntitle: Alpha\nstatus: planned\ntags: [note]\n---\n\n# Alpha\n\n\
         See [[beta]].\n\n## Goal\n\nProse.\n\n## Tasks\n\n- [ ] one\n- [x] two\n",
    );
    write_md(
        tmp.path(),
        "beta.md",
        "---\ntitle: Beta\nstatus: done\n---\n\n# Beta\n\n## Goal\n\nMore prose.\n",
    );
    tmp
}

// ---------------------------------------------------------------------------
// FIND-1 — exact projection
// ---------------------------------------------------------------------------

#[test]
fn no_fields_flag_returns_the_seven_default_keys() {
    let tmp = vault();
    assert_eq!(
        first_item_keys(tmp.path(), &["find", "--limit", "1"]),
        vec![
            "file",
            "lines",
            "modified",
            "properties",
            "size",
            "tags",
            "title"
        ]
    );
}

#[test]
fn an_explicit_fields_selection_is_exact() {
    let tmp = vault();
    for (args, expected) in [
        (vec!["--fields", "title"], vec!["file", "title"]),
        (
            vec!["--fields", "size,lines"],
            vec!["file", "lines", "size"],
        ),
        (vec!["--fields", "file"], vec!["file"]),
        (vec!["--fields", "modified"], vec!["file", "modified"]),
    ] {
        let mut argv = vec!["find", "--limit", "1"];
        argv.extend(args.iter().copied());
        assert_eq!(first_item_keys(tmp.path(), &argv), expected, "for {args:?}");
    }
}

#[test]
fn a_filter_adds_its_field_on_top_of_an_exact_projection() {
    let tmp = vault();
    assert_eq!(
        first_item_keys(
            tmp.path(),
            &[
                "find",
                "--limit",
                "1",
                "--fields",
                "title",
                "--section",
                "Goal"
            ]
        ),
        vec!["file", "sections", "title"]
    );
    assert_eq!(
        first_item_keys(
            tmp.path(),
            &["find", "--limit", "1", "--fields", "title", "--task", "any"]
        ),
        vec!["file", "tasks", "title"]
    );
}

#[test]
fn fields_all_still_returns_everything() {
    let tmp = vault();
    assert_eq!(
        first_item_keys(tmp.path(), &["find", "--limit", "1", "--fields", "all"]),
        vec![
            "backlinks",
            "file",
            "lines",
            "links",
            "modified",
            "properties",
            "properties_typed",
            "sections",
            "size",
            "tags",
            "tasks",
            "title",
        ]
    );
}

#[test]
fn sorting_by_a_dropped_field_still_sorts_without_returning_it() {
    let tmp = vault();
    // `--sort modified` needs the timestamp internally; the projection says it
    // is not wanted in the output, and both must hold at once.
    let env = envelope(
        tmp.path(),
        &["find", "--fields", "title", "--sort", "modified"],
    );
    let arr = env["results"].as_array().unwrap();
    assert_eq!(arr.len(), 2);
    for item in arr {
        assert!(
            item.get("modified").is_none(),
            "modified must be dropped: {item}"
        );
    }
}

#[test]
fn a_view_pinning_fields_behaves_like_an_explicit_fields() {
    let tmp = vault();
    let set = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["views", "set", "titles", "--fields", "title"])
        .output()
        .unwrap();
    assert!(
        set.status.success(),
        "{}",
        String::from_utf8_lossy(&set.stderr)
    );

    assert_eq!(
        first_item_keys(tmp.path(), &["find", "--view", "titles", "--limit", "1"]),
        vec!["file", "title"]
    );
    assert_eq!(
        first_item_keys(tmp.path(), &["views", "run", "titles", "--limit", "1"]),
        vec!["file", "title"]
    );
    // A CLI --fields replaces the pin rather than adding to it.
    assert_eq!(
        first_item_keys(
            tmp.path(),
            &[
                "find", "--view", "titles", "--fields", "tags", "--limit", "1"
            ]
        ),
        vec!["file", "tags"]
    );
}

#[test]
fn an_exact_projection_shrinks_the_payload() {
    let tmp = vault();
    let full = envelope(tmp.path(), &["find"]).to_string().len();
    let projected = envelope(tmp.path(), &["find", "--fields", "title"])
        .to_string()
        .len();
    assert!(
        projected * 100 <= full * 80,
        "--fields title should drop at least 20% of the payload: {projected} vs {full}"
    );
}

#[test]
fn text_mode_shows_exactly_the_projected_fields() {
    let tmp = vault();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["find", "--fields", "title", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\"alpha.md\"\n  title: Alpha"), "{stdout}");
    assert!(!stdout.contains(" B, "), "no size/lines header: {stdout}");
    assert!(
        stdout.contains("fields: file, title"),
        "the fields: line reports the projection: {stdout}"
    );
}

#[test]
fn views_run_honours_the_filename_projections() {
    let tmp = vault();
    assert!(
        hyalo_no_hints()
            .current_dir(tmp.path())
            .args(["views", "set", "all", "--sort", "file"])
            .output()
            .unwrap()
            .status
            .success()
    );
    let out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["views", "run", "all", "--filenames-only"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "alpha.md\nbeta.md\n\n"
    );

    let out0 = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["views", "run", "all", "--filenames0"])
        .output()
        .unwrap();
    assert_eq!(out0.stdout, b"alpha.md\0beta.md\0");
}

// ---------------------------------------------------------------------------
// FIND-3 / FIND-4 — non-string title
// ---------------------------------------------------------------------------

/// A vault with one file per `title:` shape the amendment names.
fn title_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    for (name, value) in [
        ("num", "42"),
        ("float", "1.0"),
        ("date", "2026-08-30"),
        ("boolean", "true"),
        ("list", "[a, b]"),
        ("map", "{k: v}"),
        ("nul", ""),
        ("blank", "\"  \""),
    ] {
        write_md(
            tmp.path(),
            &format!("{name}.md"),
            &format!("---\ntitle: {value}\n---\n\n# H1 {name}\n"),
        );
    }
    tmp
}

/// `(promoted title, raw properties.title)` for one file.
fn title_of(dir: &Path, file: &str) -> (serde_json::Value, Option<serde_json::Value>) {
    let env = envelope(dir, &["find", "--file", file]);
    let item = &env["results"][0];
    (
        item["title"].clone(),
        item["properties"].get("title").cloned(),
    )
}

#[test]
fn every_scalar_title_promotes_stringified_as_written() {
    let tmp = title_vault();
    for (file, expected) in [
        ("num.md", "42"),
        ("float.md", "1.0"),
        ("date.md", "2026-08-30"),
        ("boolean.md", "true"),
    ] {
        let (title, raw) = title_of(tmp.path(), file);
        assert_eq!(title, serde_json::json!(expected), "{file}");
        assert!(
            raw.is_none(),
            "{file}: a promoted scalar is stripped from properties, got {raw:?}"
        );
    }
}

#[test]
fn a_collection_title_falls_back_to_h1_and_keeps_the_raw_value() {
    let tmp = title_vault();
    let (title, raw) = title_of(tmp.path(), "list.md");
    assert_eq!(title, serde_json::json!("H1 list"));
    assert_eq!(raw, Some(serde_json::json!(["a", "b"])));

    let (title, raw) = title_of(tmp.path(), "map.md");
    assert_eq!(title, serde_json::json!("H1 map"));
    assert_eq!(raw, Some(serde_json::json!({"k": "v"})));
}

#[test]
fn null_and_blank_titles_count_as_absent_but_stay_in_properties() {
    let tmp = title_vault();
    for (file, heading, raw_value) in [
        ("nul.md", "H1 nul", serde_json::Value::Null),
        ("blank.md", "H1 blank", serde_json::json!("  ")),
    ] {
        let (title, raw) = title_of(tmp.path(), file);
        assert_eq!(title, serde_json::json!(heading), "{file}");
        assert_eq!(raw, Some(raw_value), "{file}");
    }
}

#[test]
fn a_promoted_numeric_title_is_filterable_and_sortable_like_any_string() {
    let tmp = title_vault();
    let by_property = envelope(tmp.path(), &["find", "--property", "title=42"]);
    assert_eq!(by_property["results"][0]["file"], "num.md");

    let by_title = envelope(tmp.path(), &["find", "--title", "42"]);
    assert_eq!(by_title["results"][0]["file"], "num.md");

    let sorted = envelope(
        tmp.path(),
        &["find", "--sort", "title", "--fields", "title"],
    );
    let titles: Vec<&str> = sorted["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["title"].as_str().unwrap())
        .collect();
    let mut expected = titles.clone();
    expected.sort_unstable();
    assert_eq!(titles, expected, "titles must sort as plain strings");
}

#[test]
fn text_mode_never_prints_none_for_a_file_that_has_a_raw_title() {
    let tmp = title_vault();
    let out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["find", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(!stdout.contains("title: (none)"), "{stdout}");
}

#[test]
fn lint_warns_that_a_collection_title_is_not_a_scalar() {
    let tmp = title_vault();
    let env = envelope(tmp.path(), &["lint", "--rule", "HYALO007", "--detailed"]);
    let files: Vec<&str> = env["results"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["file"].as_str().unwrap())
        .collect();
    assert_eq!(files, vec!["list.md", "map.md"], "{env}");
    let message = env["results"]["files"][0]["rule_groups"][0]["violations"][0]["message"]
        .as_str()
        .unwrap();
    assert!(message.contains("title must be a scalar"), "{message}");
}

// ---------------------------------------------------------------------------
// COH-4 — the JSON cookbook matches the live output
// ---------------------------------------------------------------------------

/// The quoted JSON keys in the cookbook snippet that follows `marker` in
/// `hyalo --help`, up to the next blank line.
fn cookbook_keys(help: &str, marker: &str) -> Vec<String> {
    let start = help
        .find(marker)
        .unwrap_or_else(|| panic!("no {marker:?} snippet in --help:\n{help}"));
    let rest = &help[start..];
    let block = rest.split_once("\n\n").map_or(rest, |(b, _)| b);
    // Collect the quoted keys of the *item* object only: `properties` is a
    // nested map in the snippet, and its keys are not result keys, so each key
    // is recorded with the brace depth it sits at and only the depth `file`
    // sits at is kept.
    let mut found: Vec<(usize, String)> = Vec::new();
    let mut depth = 0usize;
    let bytes = block.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b'"' => {
                // Consume the whole string literal, then decide whether the
                // character after it makes this a key.
                let Some(len) = block[i + 1..].find('"') else {
                    break;
                };
                let text = &block[i + 1..i + 1 + len];
                i += len + 2;
                if bytes.get(i) == Some(&b':') {
                    found.push((depth, text.to_owned()));
                }
            }
            _ => i += 1,
        }
    }
    let item_depth = found
        .iter()
        .find(|(_, k)| k == "file")
        .map_or(1, |(d, _)| *d);
    let mut keys: Vec<String> = found
        .into_iter()
        .filter(|(d, k)| *d == item_depth && !matches!(k.as_str(), "results" | "total" | "hints"))
        .map(|(_, k)| k)
        .collect();
    keys.sort_unstable();
    keys.dedup();
    keys
}

fn help_text(dir: &Path) -> String {
    let out = hyalo_no_hints()
        .current_dir(dir)
        .arg("--help")
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn the_find_cookbook_snippet_lists_the_live_default_keys() {
    let tmp = vault();
    let help = help_text(tmp.path());
    let documented = cookbook_keys(&help, "# find — results is an array of file objects");
    let mut live = first_item_keys(tmp.path(), &["find", "--limit", "1"]);
    live.sort_unstable();
    assert_eq!(documented, live, "cookbook drifted from `find --limit 1`");
}

#[test]
fn the_read_cookbook_snippet_lists_the_live_read_keys() {
    let tmp = vault();
    let help = help_text(tmp.path());
    let documented = cookbook_keys(&help, "# read — size/lines are the same numbers");
    let env = envelope(tmp.path(), &["read", "alpha.md"]);
    let mut live: Vec<String> = env["results"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    live.sort_unstable();
    assert_eq!(documented, live, "cookbook drifted from `read`");
}

#[test]
fn the_task_cookbook_snippet_lists_the_live_task_keys() {
    let tmp = vault();
    let help = help_text(tmp.path());
    let documented = cookbook_keys(&help, "# task read / toggle / set");
    let env = envelope(tmp.path(), &["task", "read", "alpha.md", "--line", "17"]);
    let mut live: Vec<String> = env["results"]
        .as_object()
        .unwrap_or_else(|| panic!("a single --line returns one object: {env}"))
        .keys()
        .cloned()
        .collect();
    live.sort_unstable();
    assert_eq!(documented, live, "cookbook drifted from `task read`");
}

#[test]
fn the_task_cookbook_snippet_documents_the_array_form() {
    let tmp = vault();
    let help = help_text(tmp.path());
    assert!(
        help.contains("an ARRAY of the\n  # same objects for --all"),
        "the cookbook must say a multi-line task op returns an array:\n{help}"
    );
    let env = envelope(tmp.path(), &["task", "read", "alpha.md", "--all"]);
    assert!(env["results"].is_array(), "--all returns an array: {env}");
}
