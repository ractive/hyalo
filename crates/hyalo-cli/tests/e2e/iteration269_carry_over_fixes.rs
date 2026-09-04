//! Iteration 269 — three carry-over correctness fixes from the 261–268 batch.
//!
//! - **Part A (SCAN-1).** `mv` reports a frontmatter wikilink that spans a line
//!   break even when that file has no *other* link to the moved target, and the
//!   NEW-3 ambiguity report no longer depends on one of two same-stemmed
//!   candidates happening to sit at the vault root.
//! - **Part B (MD034-1).** MD034's autolink stops before a following HTML tag
//!   instead of swallowing it (`…/Retroma<br>`).
//! - **Part C (MD047-1).** A frontmatter-only file is not "missing a trailing
//!   newline".

use super::common::{hyalo_no_hints, typed_results, write_md};
use hyalo_cli::commands::lint::ExtLintOutput;
use tempfile::TempDir;

/// A vault with no schema requirements, so only body rules speak.
fn vault(files: &[(&str, &str)]) -> TempDir {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    for (name, content) in files {
        write_md(tmp.path(), name, content);
    }
    tmp
}

fn write(tmp: &TempDir, rel: &str, body: &str) {
    let path = tmp.path().join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, body).unwrap();
}

fn mv_json(tmp: &TempDir, args: &[&str]) -> serde_json::Value {
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
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

// ---------------------------------------------------------------------------
// Part A — SCAN-1: `mv` looks past the backlinks graph
// ---------------------------------------------------------------------------

/// The backlog repro: `References/Folded.md` references `Categories/Books.md`
/// *only* through a folded block scalar whose `[[…]]` straddles the line break.
/// The graph has no edge for it, so before this iteration `mv` said nothing and
/// left a dangling reference behind.
fn split_link_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(
        &tmp,
        "Categories/Books.md",
        "---\ntags:\n  - categories\n---\n\n# Books\n",
    );
    write(
        &tmp,
        "References/Folded.md",
        "---\nsummary: >\n  points at [[Categories/\n  Books]] somehow\n---\n\nNo other link to the shelf.\n",
    );
    tmp
}

#[test]
fn mv_reports_a_split_frontmatter_link_the_graph_never_saw() {
    let tmp = split_link_vault();
    let json = mv_json(
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
    assert_eq!(skipped[0]["line"], 3, "{json}");
}

#[test]
fn mv_text_output_warns_about_a_split_frontmatter_link_the_graph_never_saw() {
    let tmp = split_link_vault();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "mv",
            "Categories/Books.md",
            "--to",
            "Categories/Library.md",
            "--dry-run",
            "--format",
            "text",
        ])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("1 frontmatter wikilink not rewritten"),
        "expected the skip warning on stderr, got: {stderr}"
    );
}

/// NEW-3: two same-stemmed files, neither at the vault root. Moving *either*
/// one must report the ambiguous bare `[[b]]` — before this iteration the
/// report depended on which candidate happened to be at the root.
#[test]
fn mv_flags_an_ambiguous_bare_link_between_two_nested_files() {
    let tmp = vault(&[
        ("a.md", "---\ntitle: A\n---\n\nSee [[b]] here\n"),
        ("one/b.md", "---\ntitle: One B\n---\n\nContent one\n"),
        ("two/b.md", "---\ntitle: Two B\n---\n\nContent two\n"),
    ]);

    for moved in ["one/b.md", "two/b.md"] {
        let json = mv_json(&tmp, &["mv", moved, "--to", "archive/b.md", "--dry-run"]);
        let skipped = json["results"]["skipped_ambiguous"]
            .as_array()
            .unwrap_or_else(|| panic!("skipped_ambiguous missing when moving {moved}: {json}"));
        assert_eq!(skipped.len(), 1, "moving {moved}: {json}");
        assert_eq!(skipped[0]["source"], "a.md", "moving {moved}: {json}");
        assert_eq!(skipped[0]["target"], "b", "moving {moved}: {json}");
    }
}

/// The widened scan must not change what counts as a graph edge: a split
/// frontmatter link is still invisible to `backlinks`, and the file holding it
/// still reads as an orphan target.
#[test]
fn a_split_frontmatter_link_is_still_not_a_backlink() {
    let tmp = split_link_vault();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["backlinks", "Categories/Books.md", "--format", "json"])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json output");
    let count = json["results"]["backlinks"]
        .as_array()
        .map_or(0, Vec::len);
    assert_eq!(count, 0, "a split link is not a graph edge: {json}");
}

// ---------------------------------------------------------------------------
// Part B — MD034 stops before a following HTML tag
// ---------------------------------------------------------------------------

/// Reduced from `Themes/Retroma.md:65` on the Obsidian Hub vault.
const RETROMA: &str =
    "---\ntitle: Retroma\n---\n\nTheme by https://github.com/emarpiee/Retroma<br>\nMore prose.\n";

#[test]
fn lint_fix_md034_keeps_a_following_html_tag_outside_the_autolink() {
    let tmp = vault(&[("retroma.md", RETROMA)]);

    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--fix", "--fix-rule", "MD034"])
        .output()
        .unwrap();

    let after = std::fs::read_to_string(tmp.path().join("retroma.md")).unwrap();
    assert!(
        after.contains("<https://github.com/emarpiee/Retroma><br>"),
        "the `<br` must stay outside the autolink: {after:?}"
    );
    assert!(
        !after.contains("Retroma<br>>"),
        "the tag must not be swallowed: {after:?}"
    );
}

#[test]
fn lint_md034_still_reports_the_bare_url_before_an_html_tag() {
    let tmp = vault(&[("retroma.md", RETROMA)]);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--rule", "MD034", "--format", "json"])
        .output()
        .unwrap();

    let results: ExtLintOutput = typed_results(&output.stdout);
    let hits: usize = results
        .files
        .iter()
        .flat_map(|f| &f.rule_groups)
        .filter(|g| g.rule == "MD034")
        .map(|g| g.count)
        .sum();
    assert_eq!(hits, 1, "the URL is still bare: {results:?}");
}

// ---------------------------------------------------------------------------
// Part C — MD047 on a frontmatter-only file
// ---------------------------------------------------------------------------

#[test]
fn lint_does_not_report_md047_on_a_file_hyalo_new_just_created() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();

    let typed = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["types", "set", "note", "--required", "title"])
        .output()
        .unwrap();
    assert!(
        typed.status.success(),
        "`hyalo types set` failed: {}",
        String::from_utf8_lossy(&typed.stderr)
    );

    let created = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["new", "--type", "note", "--file", "notes/x.md"])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "`hyalo new` failed: {}",
        String::from_utf8_lossy(&created.stderr)
    );

    let before = std::fs::read_to_string(tmp.path().join("notes/x.md")).unwrap();

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--file", "notes/x.md", "--format", "json"])
        .output()
        .unwrap();
    let results: ExtLintOutput = typed_results(&output.stdout);
    let md047: usize = results
        .files
        .iter()
        .flat_map(|f| &f.rule_groups)
        .filter(|g| g.rule == "MD047")
        .map(|g| g.count)
        .sum();
    assert_eq!(
        md047, 0,
        "a freshly created note ends with a newline: {results:?}"
    );

    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--file", "notes/x.md", "--fix", "--fix-rule", "MD047"])
        .output()
        .unwrap();
    let after = std::fs::read_to_string(tmp.path().join("notes/x.md")).unwrap();
    assert_eq!(before, after, "`--fix` must leave the file byte-identical");
}

#[test]
fn lint_does_not_report_md047_on_a_hand_written_frontmatter_only_file() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(&tmp, "a.md", "---\nkey: v\n---\n");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--file", "a.md", "--format", "json"])
        .output()
        .unwrap();
    let results: ExtLintOutput = typed_results(&output.stdout);
    let md047: usize = results
        .files
        .iter()
        .flat_map(|f| &f.rule_groups)
        .filter(|g| g.rule == "MD047")
        .map(|g| g.count)
        .sum();
    assert_eq!(md047, 0, "the file plainly ends in a newline: {results:?}");
}

#[test]
fn lint_still_reports_md047_on_a_body_without_a_trailing_newline() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(&tmp, "b.md", "---\nkey: v\n---\n\nprose without a terminator");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--file", "b.md", "--format", "json"])
        .output()
        .unwrap();
    let results: ExtLintOutput = typed_results(&output.stdout);
    let md047: usize = results
        .files
        .iter()
        .flat_map(|f| &f.rule_groups)
        .filter(|g| g.rule == "MD047")
        .map(|g| g.count)
        .sum();
    assert_eq!(md047, 1, "a real missing newline still fires: {results:?}");
}
