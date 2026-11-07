//! Iteration 329 (DEC-402): the reader honours the protected regions.
//!
//! One table of constructs against every reader and writer that shares the
//! body view: `find --fields links,tasks,sections` (title promotion included),
//! on disk and under `--index`, `backlinks`, HYALO006, `links fix --apply` and
//! `mv`. A corpus cannot prove a form it does not contain, so this is the
//! proof that raw HTML, `$$` math and code-like elements are neither a link,
//! a heading nor a task, and that the writers agree with the reader.
use super::common::{hyalo_no_hints, write_md};
use serde_json::Value;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// What the reader is expected to report for a construct.
struct Case {
    name: &'static str,
    /// The body. `{L}` is a link line, `{H}` a heading line and `{T}` a task
    /// line, so each case states where the three sit inside the construct.
    body: &'static str,
    /// `[[target]]`, `[[nowhere]]` and `[t](sub/Target.md)` are links.
    link: bool,
    /// `{H}` is a heading (and so the promoted title).
    heading: bool,
    /// `{T}` is a task.
    task: bool,
}

const L: &str = "[[target]] [[nowhere]] [t](sub/Target.md)";
const H: &str = "# Fake";
const T: &str = "- [ ] fake task";

#[rustfmt::skip]
const CASES: &[Case] = &[
    Case { name: "pre", body: "<pre>\n{H}\n{L}\n{T}\n</pre>\n", link: false, heading: false, task: false },
    Case { name: "pre_after_prose", body: "intro\n<pre>\n{H}\n{L}\n{T}\n</pre>\n", link: false, heading: false, task: false },
    Case { name: "div_without_blank_line", body: "<div>\n{H}\n{L}\n{T}\n</div>\n", link: false, heading: false, task: false },
    Case { name: "div_with_blank_line", body: "<div>\n\n{H}\n\n{L}\n\n{T}\n\n</div>\n", link: true, heading: true, task: true },
    Case { name: "details_without_blank_line", body: "<details>\n{L}\n</details>\n", link: false, heading: false, task: false },
    Case { name: "math_block", body: "$$\n{L}\n$$\n", link: false, heading: false, task: false },
    Case { name: "math_inline", body: "x $${L}$$ y\n", link: false, heading: false, task: false },
    Case { name: "code_element", body: "a <code>{L}</code> b\n", link: false, heading: false, task: false },
    Case { name: "backtick_code", body: "a `{L}` b\n", link: false, heading: false, task: false },
    Case { name: "fence", body: "```\n{H}\n{L}\n{T}\n```\n", link: false, heading: false, task: false },
    Case { name: "html_comment", body: "<!-- {L} -->\n", link: false, heading: false, task: false },
    Case { name: "percent_comment", body: "%% {L} %%\n", link: false, heading: false, task: false },
    Case { name: "link_text_in_span", body: "a <span>{L}</span> b\n", link: true, heading: false, task: false },
    Case { name: "link_text_in_anchor", body: "<a href=\"x\">{L}</a>\n", link: true, heading: false, task: false },
    Case { name: "prose", body: "{H}\n\n{L}\n\n{T}\n", link: true, heading: true, task: true },
];

fn vault(case: &Case) -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "sub/Target.md", "# Target\n");
    let body = case
        .body
        .replace("{L}", L)
        .replace("{H}", H)
        .replace("{T}", T);
    write_md(tmp.path(), "c.md", &body);
    tmp
}

fn run(dir: &Path, args: &[&str]) -> Value {
    let out = hyalo_no_hints()
        .arg("--dir")
        .arg(dir)
        .args(args)
        .args(["--format", "json"])
        .output()
        .unwrap();
    serde_json::from_slice(&out.stdout).unwrap_or(Value::Null)
}

/// What `find` reports for `c.md`: link count, tasks, heading present, title source.
fn read(dir: &Path, index: bool) -> (usize, usize, bool, String) {
    let mut args = vec![
        "find",
        "--file",
        "c.md",
        "--fields",
        "links,tasks,sections,title",
    ];
    if index {
        args.push("--index");
    }
    let v = run(dir, &args);
    let f = &v["results"][0];
    let links = f["links"].as_array().map_or(0, Vec::len);
    let tasks = f["tasks"].as_array().map_or(0, Vec::len);
    let heading = f["sections"]
        .as_array()
        .is_some_and(|s| s.iter().any(|s| s["heading"] == "Fake"));
    (
        links,
        tasks,
        heading,
        f["title_source"].as_str().unwrap_or("").to_owned(),
    )
}

#[test]
fn every_reader_and_writer_agrees_on_every_construct() {
    for case in CASES {
        let expect_links = if case.link { 3 } else { 0 };
        let expect_tasks = usize::from(case.task);
        let tmp = vault(case);
        let dir = tmp.path();
        let ctx = case.name;

        // Disk, then the same read under a snapshot.
        let disk = read(dir, false);
        run(dir, &["create-index"]);
        let indexed = read(dir, true);
        assert_eq!(disk, indexed, "{ctx}: disk and --index differ");
        let (links, tasks, heading, title_source) = disk;
        assert_eq!(links, expect_links, "{ctx}: link count");
        assert_eq!(tasks, expect_tasks, "{ctx}: task count");
        assert_eq!(heading, case.heading, "{ctx}: heading in the outline");
        if case.body.contains("{H}") {
            let promoted = if case.heading { "h1" } else { "filename" };
            assert_eq!(title_source, promoted, "{ctx}: title source");
        }
        fs::remove_file(dir.join(".hyalo-index")).ok();

        // backlinks and HYALO006.
        let back = run(dir, &["backlinks", "sub/Target.md"]);
        let sources = back["results"]["backlinks"].to_string();
        assert_eq!(sources.contains("c.md"), case.link, "{ctx}: backlinks");
        let lint = run(dir, &["lint", "--rule", "HYALO006", "--max-per-rule", "0"]);
        assert_eq!(
            lint.to_string().contains("nowhere"),
            case.link,
            "{ctx}: HYALO006"
        );

        // Writers: they rewrite exactly what the reader reports.
        let before = fs::read_to_string(dir.join("c.md")).unwrap();
        run(dir, &["links", "fix", "--apply"]);
        let after_fix = fs::read_to_string(dir.join("c.md")).unwrap();
        assert_eq!(
            after_fix.contains("[[Target]]"),
            case.link,
            "{ctx}: links fix --apply\n{after_fix}"
        );
        if !case.link {
            assert_eq!(after_fix, before, "{ctx}: links fix touched protected text");
        }
        fs::write(dir.join("c.md"), &before).unwrap();
        run(dir, &["mv", "sub/Target.md", "--to", "moved/Target.md"]);
        let after_mv = fs::read_to_string(dir.join("c.md")).unwrap();
        assert_eq!(
            after_mv.contains("moved/Target.md"),
            case.link,
            "{ctx}: mv\n{after_mv}"
        );
        if !case.link {
            assert_eq!(after_mv, before, "{ctx}: mv touched protected text");
        }
    }
}

#[test]
fn a_heading_custom_id_is_an_anchor_and_the_heading_text_is_unchanged() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    write_md(
        dir,
        "page.md",
        "## Session start hook {#session-start}\n\n<pre>\n## Fake {#fake}\n</pre>\n",
    );
    write_md(
        dir,
        "src.md",
        "[ok](page.md#session-start)\n[case](page.md#Session-Start)\n[fake](page.md#fake)\n",
    );
    let v = run(dir, &["find", "--file", "src.md", "--fields", "links"]);
    let broken: Vec<(String, bool)> = v["results"][0]["links"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            (
                l["fragment"].as_str().unwrap().to_owned(),
                l["broken_anchor"].as_bool().unwrap_or(false),
            )
        })
        .collect();
    assert_eq!(
        broken,
        vec![
            ("session-start".to_owned(), false),
            ("Session-Start".to_owned(), true),
            ("fake".to_owned(), true),
        ]
    );
    let page = run(dir, &["find", "--file", "page.md", "--fields", "sections"]);
    assert_eq!(
        page["results"][0]["sections"][0]["heading"],
        "Session start hook {#session-start}"
    );
}

#[test]
fn an_explicit_html_anchor_in_a_raw_html_block_still_resolves() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    write_md(
        dir,
        "page.md",
        "<h2 id=\"top\">Top</h2>\n<div>\n<a name=\"inner\"></a>\n</div>\n",
    );
    write_md(dir, "src.md", "[a](page.md#top) [b](page.md#inner)\n");
    let v = run(dir, &["find", "--file", "src.md", "--fields", "links"]);
    for l in v["results"][0]["links"].as_array().unwrap() {
        assert!(!l["broken_anchor"].as_bool().unwrap_or(false), "{l}");
    }
}
