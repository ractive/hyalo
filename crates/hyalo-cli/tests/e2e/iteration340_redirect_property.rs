//! Iteration 340 (DEC-415): `[links] redirect_property` resolves a site-absolute
//! link to the note that declares its URL path.
//!
//! GitHub Docs notes list every URL they used to live at in `redirect_from:`,
//! and links elsewhere still use those URLs. With the setting on, such a link
//! resolves (`via: "redirect"`), counts as a graph edge, and `links fix` and
//! `mv` leave it alone; with it off (the default) nothing changes.

use super::common::{hyalo_no_hints, write_md};
use serde_json::Value;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

const SETTING: &str = "[links]\nredirect_property = \"redirect_from\"\n";

fn config(dir: &Path, toml: &str) {
    fs::write(dir.join(".hyalo.toml"), toml).unwrap();
}

fn output(dir: &Path, args: &[&str]) -> std::process::Output {
    hyalo_no_hints()
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn run(dir: &Path, args: &[&str]) -> Value {
    let out = output(dir, args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("not JSON ({e}): {}", String::from_utf8_lossy(&out.stdout)))
}

fn note(redirects: &[&str], body: &str) -> String {
    let list = redirects.iter().fold(String::new(), |mut s, r| {
        use std::fmt::Write as _;
        let _ = writeln!(s, "  - {r}");
        s
    });
    format!("---\ntitle: T\nredirect_from:\n{list}---\n{body}\n")
}

/// The links of `src.md` as `find --fields links` reports them.
fn links(dir: &Path, indexed: bool) -> Vec<Value> {
    let mut args = vec![
        "find", "--file", "src.md", "--fields", "links", "--format", "json",
    ];
    if indexed {
        args.push("--index");
    }
    run(dir, &args)["results"][0]["links"]
        .as_array()
        .unwrap()
        .clone()
}

fn paths(links: &[Value]) -> Vec<Option<&str>> {
    links.iter().map(|l| l["path"].as_str()).collect()
}

fn vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    config(d, SETTING);
    write_md(
        d,
        "actions/page.md",
        &note(
            &["/old/page", "/Old/Other/", "/legacy/index"],
            "# Page\n\n## Jobs\n",
        ),
    );
    tmp
}

#[test]
fn a_declared_value_resolves_markdown_and_wikilink_with_fragment_and_case() {
    let tmp = vault();
    let d = tmp.path();
    write_md(
        d,
        "src.md",
        "[a](/old/page) [b](/old/page#jobs) [c](/OLD/other/) [d](/legacy) [[/old/page]] [e](/old/missing)\n",
    );
    let l = links(d, false);
    let page = Some("actions/page.md");
    assert_eq!(paths(&l), [page, page, page, page, page, None], "{l:?}");
    for link in &l[..5] {
        assert_eq!(link["via"], "redirect", "{link}");
        assert!(link.get("ambiguous").is_none(), "{link}");
        assert_ne!(link["broken_anchor"], true, "{link}");
    }
    // The kind stays what the author wrote.
    assert_eq!(l[0]["kind"], "markdown");
    assert_eq!(l[4]["kind"], "wikilink");
    assert!(l[5].get("via").is_none(), "{}", l[5]);
    // A fragment is judged against the declaring note's headings.
    write_md(d, "bad.md", "[x](/old/page#nowhere)\n");
    let broken = run(d, &["find", "--broken-links", "--format", "json"]);
    let files: Vec<&str> = broken["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["file"].as_str().unwrap())
        .collect();
    assert_eq!(files, ["bad.md", "src.md"], "{broken}");
}

#[test]
fn the_site_prefix_is_stripped_from_the_link_and_from_a_value_that_carries_it() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    config(d, &format!("site_prefix = \"docs\"\n{SETTING}"));
    write_md(d, "one.md", &note(&["/old/one"], "one"));
    write_md(d, "two.md", &note(&["/docs/old/two"], "two"));
    write_md(
        d,
        "src.md",
        "[a](/docs/old/one) [b](/old/one) [c](/docs/old/two) [d](/old/two) [e](/old/three)\n",
    );
    let l = links(d, false);
    assert_eq!(
        paths(&l),
        [
            Some("one.md"),
            Some("one.md"),
            Some("two.md"),
            Some("two.md"),
            None
        ],
        "{l:?}"
    );
}

#[test]
fn a_real_file_wins_over_a_claim_on_its_path() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    config(d, SETTING);
    write_md(d, "real.md", "real\n");
    write_md(d, "dir/index.md", "index\n");
    write_md(d, "claimant.md", &note(&["/real", "/dir", "/real.md"], "c"));
    write_md(
        d,
        "src.md",
        "[a](/real) [b](/dir) [c](/dir/) [d](/real.md)\n",
    );
    let l = links(d, false);
    assert_eq!(
        paths(&l),
        [
            Some("real.md"),
            Some("dir/index.md"),
            Some("dir/index.md"),
            Some("real.md")
        ],
        "{l:?}"
    );
    for link in &l {
        assert!(
            link.get("via").is_none(),
            "a real file is not a redirect: {link}"
        );
    }
}

#[test]
fn a_value_two_notes_claim_is_ambiguous_and_still_broken() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    config(d, SETTING);
    write_md(d, "b/second.md", &note(&["/dup/url"], "2"));
    write_md(d, "a/first.md", &note(&["/dup/url", "/DUP/url/"], "1"));
    // One note listing a value twice (two spellings) is not a second claimant.
    write_md(d, "pages/uno.md", &note(&["/solo", "/Solo/"], "s"));
    write_md(d, "src.md", "[a](/dup/url) [b](/solo)\n");

    for indexed in [false, true] {
        if indexed {
            run(d, &["create-index"]);
        }
        let l = links(d, indexed);
        assert_eq!(l[0]["path"], Value::Null, "{l:?}");
        assert_eq!(l[0]["ambiguous"], true, "{l:?}");
        assert_eq!(
            l[0]["candidates"],
            serde_json::json!(["a/first.md", "b/second.md"])
        );
        assert!(l[0].get("via").is_none());
        assert_eq!(l[1]["path"], "pages/uno.md", "{l:?}");
        assert_eq!(l[1]["via"], "redirect");
    }

    // Still broken everywhere it is counted.
    let broken = run(d, &["find", "--broken-links", "--format", "json"]);
    assert_eq!(broken["total"], 1, "{broken}");
    assert_eq!(
        run(d, &["summary", "--format", "json"])["results"]["links"]["broken"],
        1
    );
    let lint =
        String::from_utf8(output(d, &["lint", "--rule", "HYALO006", "--format", "text"]).stdout)
            .unwrap();
    assert!(
        lint.contains(
            "ambiguous markdown link: `/dup/url` matches 2 candidates: a/first.md, b/second.md"
        ),
        "{lint}"
    );
    let fix = run(d, &["links", "fix", "--format", "json"]);
    assert_eq!(
        fix["results"]["ambiguous_links"][0]["target"], "/dup/url",
        "{fix}"
    );
    assert_eq!(fix["results"]["fixable"], 0, "{fix}");
    // Neither claimant is credited with the link.
    let back = run(
        d,
        &["backlinks", "--file", "a/first.md", "--format", "json"],
    );
    assert_eq!(
        back["results"]["backlinks"].as_array().unwrap().len(),
        0,
        "{back}"
    );
}

#[test]
fn unset_means_nothing_changes() {
    let tmp = vault();
    let d = tmp.path();
    write_md(d, "src.md", "[a](/old/page) [b](/legacy)\n");
    config(d, "");
    let l = links(d, false);
    assert_eq!(paths(&l), [None, None], "{l:?}");
    assert!(l.iter().all(|link| link.get("via").is_none()));
    let cfg = run(d, &["config", "--format", "json"]);
    assert_eq!(cfg["results"]["links"]["redirect_property"], Value::Null);
    let text = String::from_utf8(output(d, &["config", "--format", "text"]).stdout).unwrap();
    assert!(text.contains("links.redirect_property: (none)"), "{text}");
    // Set: reported, and an empty name is a warning, not a setting.
    config(d, SETTING);
    let cfg = run(d, &["config", "--format", "json"]);
    assert_eq!(
        cfg["results"]["links"]["redirect_property"],
        "redirect_from"
    );
    config(d, "[links]\nredirect_property = \"\"\n");
    let out = output(
        d,
        &[
            "find", "--file", "src.md", "--fields", "links", "--format", "json",
        ],
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("redirect_property"));
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(parsed["results"][0]["links"][0]["path"], Value::Null);
}

#[test]
fn a_redirect_link_is_a_graph_edge_and_links_fix_proposes_nothing() {
    let tmp = vault();
    let d = tmp.path();
    write_md(d, "src.md", "[a](/old/page) [b](/OLD/PAGE)\n");
    for indexed in [false, true] {
        if indexed {
            run(d, &["create-index"]);
        }
        let flag: &[&str] = if indexed { &["--index"] } else { &[] };
        let args = |base: &[&'static str]| -> Vec<&str> {
            base.iter().copied().chain(flag.iter().copied()).collect()
        };
        let back = run(
            d,
            &args(&["backlinks", "--file", "actions/page.md", "--format", "json"]),
        );
        assert_eq!(
            back["results"]["backlinks"].as_array().unwrap().len(),
            2,
            "{back}"
        );
        let orphans = run(
            d,
            &args(&["find", "--orphan", "--fields", "file", "--format", "json"]),
        );
        let names: Vec<&str> = orphans["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["file"].as_str().unwrap())
            .collect();
        assert!(!names.contains(&"actions/page.md"), "{names:?}");
        assert!(!names.contains(&"src.md"), "{names:?}");
        let summary = run(d, &args(&["summary", "--format", "json"]));
        assert_eq!(summary["results"]["links"]["broken"], 0, "{summary}");
        assert_eq!(
            run(d, &args(&["find", "--broken-links", "--format", "json"]))["total"],
            0
        );
        assert_eq!(
            run(
                d,
                &args(&["lint", "--rule", "HYALO006", "--format", "json"])
            )["results"]["violations"],
            0
        );
    }
    let before = fs::read(d.join("src.md")).unwrap();
    let fix = run(
        d,
        &[
            "links",
            "fix",
            "--apply",
            "--apply-fuzzy",
            "--format",
            "json",
        ],
    );
    for bucket in [
        "fixable",
        "broken",
        "case_mismatch_fixes",
        "relocation_fixes",
        "fuzzy",
        "alias_fixes",
    ] {
        let v = &fix["results"][bucket];
        assert!(
            v.as_u64().is_some_and(|n| n == 0) || v.as_array().is_some_and(Vec::is_empty),
            "{bucket}: {fix}"
        );
    }
    assert_eq!(fs::read(d.join("src.md")).unwrap(), before);
}

#[test]
fn a_snapshot_answers_like_disk_under_either_setting() {
    let tmp = vault();
    let d = tmp.path();
    write_md(d, "src.md", "[a](/old/page) [b](/old/missing)\n");
    // Built with the setting on, read with it off, and the reverse. The
    // redirect map is built from the entries' properties at load, so there is
    // nothing in the file to refuse (DEC-415).
    for (built, read) in [(SETTING, ""), ("", SETTING), (SETTING, SETTING), ("", "")] {
        config(d, built);
        run(d, &["create-index", "--force"]);
        config(d, read);
        let disk = links(d, false);
        let out = output(
            d,
            &[
                "find", "--file", "src.md", "--fields", "links", "--index", "--format", "json",
            ],
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !stderr.contains("falling back"),
            "built {built:?}, read {read:?}: {stderr}"
        );
        let indexed: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(
            indexed["results"][0]["links"].as_array().unwrap(),
            &disk,
            "built {built:?}, read {read:?}"
        );
        assert_eq!(disk[0]["path"].is_string(), !read.is_empty(), "{disk:?}");
        let bl_disk = run(
            d,
            &["backlinks", "--file", "actions/page.md", "--format", "json"],
        );
        let bl_index = run(
            d,
            &[
                "backlinks",
                "--file",
                "actions/page.md",
                "--index",
                "--format",
                "json",
            ],
        );
        assert_eq!(
            bl_disk["results"]["backlinks"], bl_index["results"]["backlinks"],
            "built {built:?}, read {read:?}"
        );
    }
}

#[test]
fn mv_leaves_a_redirect_link_and_the_declaration_alone_and_it_keeps_resolving() {
    let tmp = vault();
    let d = tmp.path();
    write_md(d, "src.md", "[a](/old/page) [[actions/page]]\n");
    let src_before = fs::read(d.join("src.md")).unwrap();
    let page_before = fs::read_to_string(d.join("actions/page.md")).unwrap();
    let out = run(
        d,
        &[
            "mv",
            "actions/page.md",
            "--to",
            "moved/page.md",
            "--format",
            "json",
        ],
    );
    // Only the real wikilink is rewritten; the redirect link is data.
    assert_eq!(out["results"]["total_links_updated"], 1, "{out}");
    let src = fs::read_to_string(d.join("src.md")).unwrap();
    assert!(src.contains("[a](/old/page)"), "{src}");
    assert!(src.contains("[[moved/page]]"), "{src}");
    assert_ne!(src.as_bytes(), src_before.as_slice());
    assert_eq!(
        fs::read_to_string(d.join("moved/page.md")).unwrap(),
        page_before
    );
    // The declaration travelled with the note.
    let l = links(d, false);
    assert_eq!(
        paths(&l),
        [Some("moved/page.md"), Some("moved/page.md")],
        "{l:?}"
    );
    assert_eq!(l[0]["via"], "redirect");
}

#[test]
fn a_move_that_touches_only_a_redirect_link_rewrites_nothing() {
    let tmp = vault();
    let d = tmp.path();
    write_md(d, "src.md", "[a](/old/page) [b](/legacy)\n");
    let before = fs::read(d.join("src.md")).unwrap();
    let out = run(
        d,
        &[
            "mv",
            "actions/page.md",
            "--to",
            "elsewhere/page.md",
            "--format",
            "json",
        ],
    );
    assert_eq!(out["results"]["total_links_updated"], 0, "{out}");
    assert_eq!(fs::read(d.join("src.md")).unwrap(), before);
}
