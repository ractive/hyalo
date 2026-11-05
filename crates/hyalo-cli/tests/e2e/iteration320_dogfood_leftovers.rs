//! Iteration 320: the 2026-10-05 regression dogfood leftovers.

use super::common::{hyalo, hyalo_no_hints, write_md};
use serde_json::Value;
use std::path::Path;
use std::process::Output;
use tempfile::TempDir;

fn run(dir: &Path, args: &[&str]) -> Output {
    hyalo_no_hints()
        .current_dir(dir)
        .arg("--dir")
        .arg(".")
        .args(args)
        .output()
        .unwrap()
}

fn json(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("not JSON ({e}): {}", String::from_utf8_lossy(&out.stdout)))
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

// ---------------------------------------------------------------------------
// find --broken-links and links fix name the same anchor; text and JSON agree
// ---------------------------------------------------------------------------

fn anchor_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    // `Intro` is the prefix of `Introduction` and the stem of `1. Intro`.
    write_md(
        tmp.path(),
        "target.md",
        "# 1. Intro\n\ntext\n\n# Introduction\n",
    );
    write_md(tmp.path(), "src.md", "[[target#Intro]]\n");
    tmp
}

#[test]
fn find_broken_links_suggests_the_heading_links_fix_plans() {
    let tmp = anchor_vault();

    let found = run(
        tmp.path(),
        &[
            "find",
            "--broken-links",
            "--jq",
            ".results[].links[0].suggested_fragment",
        ],
    );
    assert_eq!(stdout(&found).trim(), "1. Intro", "{}", stderr(&found));

    let fix = run(tmp.path(), &["links", "fix"]);
    let fix = json(&fix);
    assert_eq!(fix["results"]["anchor_fixes"][0]["heading"], "1. Intro");
}

#[test]
fn links_fix_text_and_json_report_the_same_fixable_count() {
    let tmp = anchor_vault();

    let as_json = json(&run(tmp.path(), &["links", "fix"]));
    assert_eq!(
        as_json["results"]["fixable"], 0,
        "fixable is the link bucket"
    );
    assert_eq!(as_json["results"]["anchor_fixable"], 1);

    // The text sum spells out the split, so `Fixable: 1` next to JSON
    // `fixable: 0` can be reconciled from either side.
    let text = stdout(&run(tmp.path(), &["links", "fix", "--format", "text"]));
    assert!(
        text.contains("Fixable: 1 (0 links + 1 anchor repair)"),
        "{text}"
    );
    assert!(text.contains("Anchor repairs: 1 proposed"), "{text}");
}

// ---------------------------------------------------------------------------
// mv leaves a bare link alone when only its Unicode composition differs
// ---------------------------------------------------------------------------

#[test]
fn mv_does_not_recompose_a_bare_wikilink() {
    let tmp = TempDir::new().unwrap();
    // The file name is decomposed (`e` + U+0301); the link is precomposed.
    write_md(tmp.path(), "notes/Cafe\u{301}.md", "# x\n");
    write_md(tmp.path(), "a.md", "see [[Caf\u{e9}]]\n");
    std::fs::create_dir(tmp.path().join("other")).unwrap();

    let out = run(
        tmp.path(),
        &["mv", "notes/Cafe\u{301}.md", "--to", "other/", "--dry-run"],
    );
    assert!(out.status.success(), "{}", stderr(&out));
    let v = json(&out);
    assert_eq!(v["results"]["total_links_updated"], 0, "{v}");
}

// ---------------------------------------------------------------------------
// new --dry-run hints the real run
// ---------------------------------------------------------------------------

#[test]
fn new_dry_run_hints_the_real_command() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), ".hyalo.toml", "");
    std::fs::write(
        tmp.path().join(".hyalo.toml"),
        "[schema.types.note]\nrequired = [\"title\"]\n",
    )
    .unwrap();
    let out = hyalo()
        .current_dir(tmp.path())
        .args([
            "--format",
            "json",
            "new",
            "--type",
            "note",
            "--file",
            "x.md",
            "--dry-run",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    let v = json(&out);
    let hint = &v["hints"][0];
    assert_eq!(hint["writes"], true, "{v}");
    let cmd = hint["cmd"].as_str().unwrap();
    assert!(
        cmd.starts_with("hyalo new --type note --file x.md"),
        "{cmd}"
    );
    assert!(!tmp.path().join("x.md").exists());
}

// ---------------------------------------------------------------------------
// read --frontmatter on a note without one says so
// ---------------------------------------------------------------------------

#[test]
fn read_frontmatter_on_a_note_without_frontmatter_warns() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "plain.md", "no frontmatter here\n");
    let out = run(tmp.path(), &["read", "plain.md", "--frontmatter"]);
    assert!(out.status.success());
    assert!(
        stderr(&out).contains("plain.md: no frontmatter to show"),
        "{}",
        stderr(&out)
    );

    write_md(tmp.path(), "with.md", "---\ntitle: T\n---\nbody\n");
    let out = run(tmp.path(), &["read", "with.md", "--frontmatter"]);
    assert!(!stderr(&out).contains("no frontmatter"), "{}", stderr(&out));
}

// ---------------------------------------------------------------------------
// types list/show under an unusable [schema] say so
// ---------------------------------------------------------------------------

#[test]
fn types_show_and_list_name_an_unusable_schema() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "[schema.bogus]\nx = 1\n").unwrap();
    write_md(tmp.path(), "a.md", "x\n");
    for args in [&["types", "show", "note"][..], &["types", "list"][..]] {
        let out = run(tmp.path(), args);
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        let err = stderr(&out);
        assert!(err.contains("[schema] in .hyalo.toml is unusable"), "{err}");
        assert!(!err.contains("not found"), "{err}");
    }
}

// ---------------------------------------------------------------------------
// init: an unchanged re-run says unchanged; a hand-edited managed block warns
// ---------------------------------------------------------------------------

#[test]
fn init_rerun_reports_unchanged_and_warns_about_a_hand_edited_block() {
    let tmp = TempDir::new().unwrap();
    let init = |tmp: &TempDir| {
        hyalo_no_hints()
            .current_dir(tmp.path())
            .args(["init", "--claude"])
            .output()
            .unwrap()
    };
    let first = init(&tmp);
    assert!(first.status.success(), "{}", stderr(&first));

    let again = init(&tmp);
    let text = stdout(&again);
    for path in [
        ".claude/skills/hyalo/SKILL.md",
        ".claude/skills/hyalo-tidy/SKILL.md",
        ".claude/rules/knowledgebase.md",
        ".claude/CLAUDE.md",
        ".claude/skills/hyalo-tidy/.hyalo-jev-assets.json",
    ] {
        let line = text
            .lines()
            .find(|l| l.contains(path))
            .unwrap_or_else(|| panic!("no line for {path}: {text}"));
        assert!(line.starts_with("unchanged"), "{line}");
    }
    assert!(!stderr(&again).contains("managed section differed"));

    let claude_md = tmp.path().join(".claude/CLAUDE.md");
    let edited = std::fs::read_to_string(&claude_md)
        .unwrap()
        .replace("Use `hyalo`", "Use my edited `hyalo`");
    std::fs::write(&claude_md, edited).unwrap();
    let third = init(&tmp);
    assert!(
        stderr(&third).contains("managed section differed"),
        "{}",
        stderr(&third)
    );
    assert!(stdout(&third).contains("updated  .claude/CLAUDE.md"));
    assert!(
        !std::fs::read_to_string(&claude_md)
            .unwrap()
            .contains("my edited")
    );
}

// ---------------------------------------------------------------------------
// HYALO006 leaves out-of-vault links to find/summary's own bucket
// ---------------------------------------------------------------------------

#[test]
fn hyalo006_ignores_a_link_that_walks_above_the_vault() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        "[out](../outside.md) and [[nope]] and [in](missing.md)\n",
    );
    let out = run(tmp.path(), &["lint", "--rule", "HYALO006"]);
    let v = json(&out);
    let text = v.to_string();
    assert!(text.contains("nope"), "{text}");
    assert!(text.contains("missing.md"), "{text}");
    assert!(!text.contains("outside.md"), "{text}");
}

// ---------------------------------------------------------------------------
// --index-file naming an unreadable path outside the vault hints the override
// ---------------------------------------------------------------------------

#[test]
fn unreadable_index_file_hint_names_allow_outside_vault_only_outside() {
    let vault = TempDir::new().unwrap();
    write_md(vault.path(), "a.md", "x\n");
    let elsewhere = TempDir::new().unwrap();

    let outside = elsewhere.path().join("missing.idx");
    let out = run(
        vault.path(),
        &["find", "--index-file", outside.to_str().unwrap()],
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("--allow-outside-vault"),
        "{}",
        stderr(&out)
    );

    let inside = vault.path().join("missing.idx");
    let out = run(
        vault.path(),
        &["find", "--index-file", inside.to_str().unwrap()],
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(
        !stderr(&out).contains("--allow-outside-vault"),
        "{}",
        stderr(&out)
    );
}

// ---------------------------------------------------------------------------
// the stale-index note says which files were added, modified or deleted
// ---------------------------------------------------------------------------

#[test]
fn stale_index_note_breaks_the_drift_down() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "keep.md", "# keep\n");
    write_md(tmp.path(), "gone.md", "# gone\n");
    let build = run(tmp.path(), &["create-index"]);
    assert!(build.status.success(), "{}", stderr(&build));

    // The directory-mtime probe has a two-second tolerance; cross it so the
    // added note is seen as drift.
    std::thread::sleep(std::time::Duration::from_millis(2100));
    std::fs::remove_file(tmp.path().join("gone.md")).unwrap();
    write_md(tmp.path(), "fresh.md", "# fresh\n");
    let out = run(tmp.path(), &["find", "--index", "--fields", "file"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let err = stderr(&out);
    assert!(
        err.contains("1 added") && err.contains("1 deleted"),
        "breakdown missing: {err}"
    );
}

// ---------------------------------------------------------------------------
// lint --rule X is not narrated with directive typos
// ---------------------------------------------------------------------------

#[test]
fn rule_filter_silences_unknown_directive_warnings() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        "# A\n\n<!-- markdownlint-disable bogus-rule -->\n\ntext\n",
    );
    let narrowed = run(tmp.path(), &["lint", "--rule", "HYALO008", "-q"]);
    assert!(
        !stderr(&narrowed).contains("unknown rule"),
        "{}",
        stderr(&narrowed)
    );
    let full = run(tmp.path(), &["lint", "-q"]);
    assert!(
        stderr(&full).contains("unknown rule `bogus-rule`"),
        "{}",
        stderr(&full)
    );
}

// ---------------------------------------------------------------------------
// --jq over the output cap exits 1 and names the way out (DEC-307)
// ---------------------------------------------------------------------------

#[test]
fn jq_output_over_the_cap_exits_one_and_names_the_way_out() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "x\n");
    let out = run(
        tmp.path(),
        &[
            "find",
            "--jq",
            r#".results[] | range(0;300000) | "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx""#,
        ],
    );
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert_eq!(out.stdout, Vec::<u8>::new());
    let err = stderr(&out);
    assert!(err.contains("exceeds 10 MiB"), "{err}");
    assert!(err.contains("--format json"), "{err}");
}

// ---------------------------------------------------------------------------
// YAML 1.2 core spellings (resolved by DEC-364/DEC-375; pinned here)
// ---------------------------------------------------------------------------

#[test]
fn yaml_signed_radix_integers_and_nan_spellings() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        "---\nn: -0x1F\nm: +0o17\nf: .NaN\nupper: 0X1F\n---\nbody\n",
    );
    let out = run(tmp.path(), &["find", "--fields", "properties"]);
    let props = &json(&out)["results"][0]["properties"];
    assert_eq!(props["n"], -31, "{props}");
    assert_eq!(props["m"], 15, "{props}");
    // JSON cannot carry NaN, so core's `.NaN` keeps its documented string
    // fallback; the uppercase-prefix hex is a YAML 1.1 extension, a string.
    assert_eq!(props["f"], ".nan", "{props}");
    assert_eq!(props["upper"], "0X1F", "{props}");
}

#[test]
fn lint_rules_show_schema_is_not_autofixable() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "x\n");
    let out = run(tmp.path(), &["lint-rules", "show", "SCHEMA"]);
    let v = json(&out);
    assert_eq!(v["results"]["autofixable"], false, "{v}");
    assert_eq!(v["results"]["configurable"], false, "{v}");
}
