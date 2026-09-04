//! Iteration 266 — metadata-command fixes from the v0.22.0 Obsidian-vault
//! dogfood run.
//!
//! Each test pins one finding: `properties rename` moving the key to the end
//! of the block and turning an empty `rating:` into `score: null` (BUG-12);
//! `tags rename --from music` doing nothing while `music/genres` existed
//! (BUG-15); `properties`/`tags` rejecting `--index` that every other reading
//! command accepts (BUG-11); schema type binding failing on a one-element list
//! or a `[[Wikilink]]`, so `types set` succeeded but never applied and
//! `--validate` passed a violating value (BUG-13); `summary` listing a
//! mixed-type property once per type and counting pairs (BUG-16); and
//! `read --frontmatter` re-serialising YAML on a read path (UX-15).

use super::common::{hyalo_no_hints, write_md};
use std::fs;
use tempfile::TempDir;

/// Run hyalo in `dir` and return `(success, stdout)`.
fn run(dir: &std::path::Path, args: &[&str]) -> (bool, String) {
    let output = hyalo_no_hints()
        .args(["--dir", dir.to_str().unwrap()])
        .args(args)
        .output()
        .unwrap();
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

/// Run hyalo in `dir` and parse the JSON envelope's `results`.
fn results(dir: &std::path::Path, args: &[&str]) -> serde_json::Value {
    let (ok, stdout) = run(dir, args);
    assert!(ok, "command failed: {args:?}\n{stdout}");
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    json["results"].clone()
}

// ---------------------------------------------------------------------------
// PROP-1 (BUG-12) — `properties rename` is a key-token rewrite
// ---------------------------------------------------------------------------

/// The kepano-obsidian shape in miniature: the key sits in the middle of the
/// block, one file leaves its value empty, another quotes it, a third gives it
/// a block list. After the rename every byte except the key token must match.
fn rename_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        "---\ntitle: A\nrating: 7\nauthor: Kevin\n---\n\n# A\n",
    );
    write_md(
        tmp.path(),
        "templates/app.md",
        "---\ntitle: Template\nrating:\ntags:\n  - x\n---\n\n# T\n",
    );
    write_md(
        tmp.path(),
        "c.md",
        "---\n# leading comment\nrating:   \"9\"  # why\ntitle: C\n---\n\n# C\n",
    );
    tmp
}

#[test]
fn properties_rename_preserves_position_and_value_bytes() {
    let tmp = rename_vault();
    let res = results(
        tmp.path(),
        &["properties", "rename", "--from", "rating", "--to", "score"],
    );
    assert_eq!(res["modified"].as_array().unwrap().len(), 3);

    assert_eq!(
        fs::read_to_string(tmp.path().join("a.md")).unwrap(),
        "---\ntitle: A\nscore: 7\nauthor: Kevin\n---\n\n# A\n",
        "the key must be renamed where it stands, not moved to the end"
    );
    assert_eq!(
        fs::read_to_string(tmp.path().join("templates/app.md")).unwrap(),
        "---\ntitle: Template\nscore:\ntags:\n  - x\n---\n\n# T\n",
        "an empty value must stay empty, never become `null`"
    );
    assert_eq!(
        fs::read_to_string(tmp.path().join("c.md")).unwrap(),
        "---\n# leading comment\nscore:   \"9\"  # why\ntitle: C\n---\n\n# C\n",
        "quoting, spacing and the trailing comment must survive verbatim"
    );
}

#[test]
fn properties_rename_dry_run_writes_nothing() {
    let tmp = rename_vault();
    let before = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    let res = results(
        tmp.path(),
        &[
            "properties",
            "rename",
            "--from",
            "rating",
            "--to",
            "score",
            "--dry-run",
        ],
    );
    assert_eq!(res["dry_run"], true);
    assert_eq!(res["modified"].as_array().unwrap().len(), 3);
    assert_eq!(fs::read_to_string(tmp.path().join("a.md")).unwrap(), before);
}

#[test]
fn properties_rename_text_output_names_both_keys() {
    let tmp = rename_vault();
    let (ok, stdout) = run(
        tmp.path(),
        &[
            "properties",
            "rename",
            "--from",
            "rating",
            "--to",
            "score",
            "--format",
            "text",
        ],
    );
    assert!(ok);
    assert!(
        stdout.starts_with("rating → score: 3/3 modified"),
        "text output must lead with the rename pair: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// TAG-1 (BUG-15) — a parent rename carries its whole subtree
// ---------------------------------------------------------------------------

fn tag_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "---\ntags:\n  - music\n---\n\n# A\n");
    write_md(
        tmp.path(),
        "b.md",
        "---\ntags:\n  - music/genres\n  - other\n---\n\n# B\n",
    );
    // `musical` shares a prefix but not a `/` boundary — it must not move.
    write_md(tmp.path(), "c.md", "---\ntags:\n  - musical\n---\n\n# C\n");
    tmp
}

#[test]
fn tags_rename_renames_nested_children() {
    let tmp = tag_vault();
    let res = results(
        tmp.path(),
        &[
            "tags", "rename", "--from", "music", "--to", "audio", "--format", "json",
        ],
    );
    let renamed = res["renamed_tags"].as_array().unwrap();
    let pairs: Vec<(String, String)> = renamed
        .iter()
        .map(|r| {
            (
                r["from"].as_str().unwrap().to_owned(),
                r["to"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        pairs,
        vec![
            ("music".to_owned(), "audio".to_owned()),
            ("music/genres".to_owned(), "audio/genres".to_owned()),
        ],
        "the expansion must be visible in the result"
    );
    assert_eq!(res["modified"].as_array().unwrap().len(), 2);

    assert!(
        fs::read_to_string(tmp.path().join("b.md"))
            .unwrap()
            .contains("audio/genres")
    );
    assert!(
        fs::read_to_string(tmp.path().join("c.md"))
            .unwrap()
            .contains("musical"),
        "`music` must not match `musical` — the match needs a / boundary"
    );
}

/// The exact tag need not exist: children alone are enough to act on.
#[test]
fn tags_rename_proceeds_when_only_children_exist() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "b.md",
        "---\ntags:\n  - music/genres\n---\n\n# B\n",
    );
    let res = results(
        tmp.path(),
        &[
            "tags", "rename", "--from", "music", "--to", "audio", "--format", "json",
        ],
    );
    assert_eq!(
        res["modified"].as_array().unwrap().len(),
        1,
        "a parent with no exact occurrence must still rename its children"
    );
    assert!(
        fs::read_to_string(tmp.path().join("b.md"))
            .unwrap()
            .contains("audio/genres")
    );
}

#[test]
fn tags_rename_collapses_a_collision_it_creates() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        "---\ntags:\n  - music\n  - audio\n---\n\n# A\n",
    );
    results(
        tmp.path(),
        &["tags", "rename", "--from", "music", "--to", "audio"],
    );
    let content = fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert_eq!(
        content.matches("audio").count(),
        1,
        "the renamed tag must not be written twice: {content}"
    );
}

#[test]
fn tags_rename_text_output_lists_each_renamed_tag() {
    let tmp = tag_vault();
    let (ok, stdout) = run(
        tmp.path(),
        &[
            "tags", "rename", "--from", "music", "--to", "audio", "--format", "text",
        ],
    );
    assert!(ok);
    assert!(
        stdout.contains("music/genres → audio/genres (1 file)"),
        "text output must list the expansion: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// IDX-1 (BUG-11) — `--index` parity for `properties` and `tags`
// ---------------------------------------------------------------------------

fn index_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        "---\ntitle: A\nrating: 7\ntags:\n  - music\n---\n\n# A\n",
    );
    write_md(
        tmp.path(),
        "b.md",
        "---\ntitle: B\nrating: high\ntags:\n  - music/genres\n---\n\n# B\n",
    );
    let (ok, _) = run(tmp.path(), &["create-index"]);
    assert!(ok, "create-index must succeed");
    tmp
}

#[test]
fn properties_and_tags_accept_index_on_the_bare_group() {
    let tmp = index_vault();
    for cmd in ["properties", "tags"] {
        let scanned = results(tmp.path(), &[cmd, "--format", "json"]);
        let indexed = results(tmp.path(), &[cmd, "--index", "--format", "json"]);
        assert_eq!(
            scanned, indexed,
            "`{cmd} --index` must match the disk scan exactly"
        );
    }
}

#[test]
fn properties_and_tags_accept_index_on_the_subcommand() {
    let tmp = index_vault();
    for cmd in ["properties", "tags"] {
        let scanned = results(tmp.path(), &[cmd, "summary", "--format", "json"]);
        let indexed = results(tmp.path(), &[cmd, "summary", "--index", "--format", "json"]);
        assert_eq!(scanned, indexed);
    }
}

#[test]
fn tags_rename_with_index_leaves_the_index_consistent() {
    let tmp = index_vault();
    results(
        tmp.path(),
        &[
            "tags", "rename", "--from", "music", "--to", "audio", "--index",
        ],
    );
    let indexed = results(tmp.path(), &["find", "--tag", "audio", "--index"]);
    let scanned = results(tmp.path(), &["find", "--tag", "audio"]);
    assert_eq!(
        indexed.as_array().unwrap().len(),
        scanned.as_array().unwrap().len(),
        "the refreshed index must agree with disk after a nested rename"
    );
    assert_eq!(indexed.as_array().unwrap().len(), 2);
}

// ---------------------------------------------------------------------------
// SCHEMA-1 (BUG-13) — type binding tolerance
// ---------------------------------------------------------------------------

/// The kepano-obsidian shape: `type:` as a one-element list holding a
/// wikilink, and as a bare wikilink.
fn schema_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write_md(
        tmp.path(),
        "kevin.md",
        "---\ntype:\n  - \"[[Authors]]\"\ntitle: Kevin\n---\n\n# K\n",
    );
    write_md(
        tmp.path(),
        "ann.md",
        "---\ntype: \"[[Authors]]\"\ntitle: Ann\ncategories: [x]\n---\n\n# A\n",
    );
    write_md(
        tmp.path(),
        "plain.md",
        "---\ntype: Authors\ntitle: Plain\n---\n\n# P\n",
    );
    write_md(tmp.path(), "other.md", "---\ntype: note\n---\n\n# O\n");
    tmp
}

#[test]
fn schema_binds_through_wikilinks_and_one_element_lists() {
    let tmp = schema_vault();
    results(
        tmp.path(),
        &["types", "set", "Authors", "--required", "categories"],
    );
    // `lint` exits non-zero when it finds errors, so read its JSON directly.
    let (_, stdout) = run(tmp.path(), &["lint", "--format", "json"]);
    let res: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let files = res["results"]["files"].as_array().unwrap();
    let mut missing: Vec<&str> = files
        .iter()
        .filter(|f| {
            f["rule_groups"].as_array().is_some_and(|rules| {
                rules.iter().any(|r| {
                    r["violations"].as_array().is_some_and(|vs| {
                        vs.iter().any(|v| {
                            v["message"].as_str().is_some_and(|m| {
                                m.contains("missing required property \"categories\"")
                            })
                        })
                    })
                })
            })
        })
        .map(|f| f["file"].as_str().unwrap())
        .collect();
    missing.sort_unstable();
    assert_eq!(
        missing,
        vec!["kevin.md", "plain.md"],
        "every shape of `type: Authors` must bind, and only bound files report"
    );
}

#[test]
fn a_multi_element_type_list_still_fails_to_bind() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write_md(tmp.path(), "bad.md", "---\ntype: [a, b]\n---\n\n# B\n");
    results(
        tmp.path(),
        &["types", "set", "Authors", "--required", "categories"],
    );
    let (_, stdout) = run(tmp.path(), &["lint", "--format", "text"]);
    assert!(
        stdout.contains("must name one type"),
        "a two-element list names no type: {stdout}"
    );
}

#[test]
fn validate_refuses_a_schema_violation_under_dry_run() {
    let tmp = schema_vault();
    results(
        tmp.path(),
        &[
            "types",
            "set",
            "Authors",
            "--property-type",
            "rating=number",
        ],
    );
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "set",
            "kevin.md",
            "--property",
            "rating=high",
            "--validate",
            "--dry-run",
        ])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "--validate must reject the write even on a dry run"
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("expected number"),
        "the schema error must name the constraint: {combined}"
    );
}

/// `types set --required K` auto-declares a property type for K. Inferring it
/// from the vault stops a list-valued property being declared `string` and
/// instantly violated by every file that has it.
#[test]
fn required_property_type_is_inferred_from_the_vault() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write_md(
        tmp.path(),
        "a.md",
        "---\ntype: Authors\ncategories:\n  - x\n  - y\n---\n\n# A\n",
    );
    write_md(
        tmp.path(),
        "b.md",
        "---\ntype: \"[[Authors]]\"\ncategories:\n  - z\n---\n\n# B\n",
    );
    let res = results(
        tmp.path(),
        &["types", "set", "Authors", "--required", "categories"],
    );
    let changes: Vec<&str> = res["toml_changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    assert!(
        changes.contains(&"auto-add property categories: type=list"),
        "the declared type must follow the vault's values: {changes:?}"
    );
    let (_, lint_out) = run(tmp.path(), &["lint", "--format", "json"]);
    let lint: serde_json::Value = serde_json::from_str(&lint_out).unwrap();
    let lint = &lint["results"];
    assert_eq!(
        lint["errors"], 0,
        "the inferred constraint must not be violated by the files it was inferred from"
    );
}

// ---------------------------------------------------------------------------
// OUT-1 (BUG-16, UX-15) — summary rows and raw frontmatter
// ---------------------------------------------------------------------------

#[test]
fn summary_lists_a_mixed_type_property_once() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        "---\npublished: 2024-01-01\n---\n\n# A\n",
    );
    write_md(
        tmp.path(),
        "b.md",
        "---\npublished: 2024-01-01T10:00:00Z\n---\n\n# B\n",
    );
    write_md(
        tmp.path(),
        "c.md",
        "---\npublished: 2024-02-02\ntitle: C\n---\n\n# C\n",
    );

    let res = results(tmp.path(), &["summary", "--format", "json"]);
    let props = res["properties"].as_array().unwrap();
    let names: Vec<&str> = props.iter().map(|p| p["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        vec!["published", "title"],
        "one row per property NAME, not per (name, type) pair"
    );
    let published = &props[0];
    assert_eq!(published["type"], "mixed");
    assert_eq!(published["count"], 3);
    let variants = published["mixed_types"].as_array().unwrap();
    assert_eq!(variants.len(), 2, "the breakdown must survive the collapse");

    // The headline count must equal `properties --count`.
    let (_, count_out) = run(tmp.path(), &["properties", "--count"]);
    assert_eq!(count_out.trim(), "2");

    let (_, text) = run(tmp.path(), &["summary", "--format", "text"]);
    let line = text
        .lines()
        .find(|l| l.starts_with("Properties:"))
        .expect("a Properties line");
    assert!(
        line.starts_with("Properties: 2 —")
            && line.contains("published (3: 2 date, 1 datetime-tz)"),
        "the text row must carry the type breakdown: {line}"
    );
}

#[test]
fn read_frontmatter_returns_the_block_verbatim() {
    let tmp = TempDir::new().unwrap();
    let raw =
        "title: 'Buy wisely'\ntags:\n  - a\n  - b\n# a comment\nauthor:   \"Kevin\"\nempty:\n";
    write_md(tmp.path(), "x.md", &format!("---\n{raw}---\n\n# Body\n"));

    let (ok, stdout) = run(
        tmp.path(),
        &["read", "x.md", "--frontmatter", "--format", "text"],
    );
    assert!(ok);
    assert!(
        stdout.starts_with(&format!("---\n{raw}---\n")),
        "text mode must echo the block's own bytes, not re-serialised YAML:\n{stdout}"
    );

    let res = results(
        tmp.path(),
        &["read", "x.md", "--frontmatter", "--format", "json"],
    );
    assert_eq!(
        res["frontmatter_raw"].as_str().unwrap(),
        raw,
        "JSON keeps the parsed map and adds the raw text beside it"
    );
    assert_eq!(
        res["frontmatter"]["author"], "Kevin",
        "the parsed map is unchanged"
    );
}

/// A file with no frontmatter has no raw block to report, and `read` must not
/// invent one.
#[test]
fn read_frontmatter_on_a_file_without_a_block() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "n.md", "# Just a body\n");
    let res = results(
        tmp.path(),
        &["read", "n.md", "--frontmatter", "--format", "json"],
    );
    assert!(res["frontmatter_raw"].is_null());
}
