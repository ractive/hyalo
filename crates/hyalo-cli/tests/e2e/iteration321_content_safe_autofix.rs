//! Iteration 321: `lint --fix` and `links auto --apply` leave user content alone.
//!
//! One protected-region model (DEC-381) decides what is not prose — raw HTML,
//! tags and their attribute values, `<a>`/`<code>`/`<pre>` content, math,
//! macro calls, block ids — and every prose rule, fixer and the auto-linker
//! reads it. These tests drive the CLI over the shapes the 2026-11-05 dogfood
//! reproduced.

use super::common::{hyalo_no_hints, write_md};
use std::path::Path;
use std::process::Output;
use tempfile::TempDir;

fn vault(files: &[(&str, &str)]) -> TempDir {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    for (name, content) in files {
        write_md(tmp.path(), name, content);
    }
    tmp
}

fn run(dir: &Path, args: &[&str]) -> Output {
    hyalo_no_hints()
        .current_dir(dir)
        .arg("--dir")
        .arg(".")
        .args(args)
        .output()
        .unwrap()
}

fn read(tmp: &TempDir, rel: &str) -> String {
    std::fs::read_to_string(tmp.path().join(rel)).unwrap()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn lint_fix(tmp: &TempDir, file: &str) -> String {
    let out = run(tmp.path(), &["lint", "--fix", "--format", "text", file]);
    String::from_utf8_lossy(&out.stdout).into_owned()
}

// ---------------------------------------------------------------------------
// B-1 / L-3: MD034 only flags prose URLs
// ---------------------------------------------------------------------------

const HTML_HEAVY: &str = "# Title\n\n\
Prose https://prose.example/x here.\n\n\
<a href=\"https://attr.example/a\">https://alink.example/b</a>\n\n\
<p><code>https://code.example/c</code></p>\n\n\
<pre>\n{ \"homepage\": \"https://pre.example/d\" }\n</pre>\n\n\
Call {{domxref(\"window.open\",\"window.open('https://macro.example/e')\")}} now.\n\n\
Your \"mailto:\" link.\n\n\
$$\na = https://math.example/m\n$$\n\n\
Inline $x = https://imath.example/i$ done.\n\n\
Inline <img\n  src=\"https://wrapped.example/i.png\"\n  alt=\"x\"> tail.\n";

#[test]
fn md034_rewrites_only_the_prose_url() {
    let tmp = vault(&[("a.md", HTML_HEAVY)]);
    lint_fix(&tmp, "a.md");
    let fixed = read(&tmp, "a.md");
    assert!(fixed.contains("<https://prose.example/x>"), "{fixed}");
    let expected = HTML_HEAVY.replace("https://prose.example/x", "<https://prose.example/x>");
    assert_eq!(fixed, expected, "only the prose URL may change");
}

#[test]
fn md034_leaves_a_multi_line_link_text_and_its_destination_alone() {
    let body = "# T\n\nA [two line\nlink text https://t.example/z](https://dest.example/q) end.\n";
    let tmp = vault(&[("a.md", body)]);
    lint_fix(&tmp, "a.md");
    assert_eq!(read(&tmp, "a.md"), body);
}

// ---------------------------------------------------------------------------
// B-2: MD011 and `()[0]`
// ---------------------------------------------------------------------------

#[test]
fn md011_does_not_read_a_call_and_index_as_a_reversed_link() {
    let body = "# T\n\nUse navigator.getGamepads()[0] and arr[1](2) here, then (see)[docs].\n";
    let tmp = vault(&[("a.md", body)]);
    lint_fix(&tmp, "a.md");
    assert_eq!(
        read(&tmp, "a.md"),
        "# T\n\nUse navigator.getGamepads()[0] and arr[1](2) here, then [see](docs).\n",
        "a real reversed link is still fixed"
    );
}

// ---------------------------------------------------------------------------
// B-3: headings inside <pre>
// ---------------------------------------------------------------------------

#[test]
fn headings_rules_stay_out_of_pre_blocks() {
    let body = "# T\n\n1. step\n\n   <pre>\n   [cluster]\n     # ipv6 = fd12\n   # other\n   </pre>\n\n<pre>\n # indented\n</pre>\n";
    let tmp = vault(&[("a.md", body)]);
    lint_fix(&tmp, "a.md");
    assert_eq!(read(&tmp, "a.md"), body);
}

// ---------------------------------------------------------------------------
// L-1: an insertion and a replacement at one offset
// ---------------------------------------------------------------------------

#[test]
fn fixes_at_the_same_offset_never_leave_stray_text() {
    let tmp = vault(&[("a.md", "# Wide\n ## Indented\ntext\n")]);
    let out = lint_fix(&tmp, "a.md");
    assert_eq!(
        read(&tmp, "a.md"),
        "# Wide\n\n## Indented\n\ntext\n",
        "{out}"
    );
}

// ---------------------------------------------------------------------------
// L-2: containers
// ---------------------------------------------------------------------------

#[test]
fn block_quote_and_list_structure_survive_the_heading_and_fence_fixes() {
    let quote = "# T\n\n> quote\n> ```sh\n> ls\n> ```\n> after\n";
    let list = "# T\n\n1. step\n\n   # not heading?\n";
    let tmp = vault(&[("q.md", quote), ("l.md", list)]);
    lint_fix(&tmp, "q.md");
    lint_fix(&tmp, "l.md");
    assert_eq!(read(&tmp, "q.md"), quote, "one quote stays one quote");
    assert_eq!(
        read(&tmp, "l.md"),
        list,
        "the heading stays in its list item"
    );
}

// ---------------------------------------------------------------------------
// L-4: MD010 reports tabs in code but never rewrites them
// ---------------------------------------------------------------------------

#[test]
fn md010_keeps_the_finding_but_not_the_fix_inside_code() {
    let body = "# T\n\n```make\nall:\n\techo hi\n```\n\nprose\ttab\n";
    let tmp = vault(&[("a.md", body)]);
    let report = run(
        tmp.path(),
        &["lint", "--rule", "MD010", "--format", "json", "a.md"],
    );
    let text = String::from_utf8_lossy(&report.stdout);
    assert!(
        text.contains("MD010"),
        "tab in code is still reported: {text}"
    );
    lint_fix(&tmp, "a.md");
    assert_eq!(
        read(&tmp, "a.md"),
        "# T\n\n```make\nall:\n\techo hi\n```\n\nprose    tab\n"
    );
}

// ---------------------------------------------------------------------------
// A-19 / B-13
// ---------------------------------------------------------------------------

#[test]
fn an_unknown_fix_rule_is_a_user_error() {
    let tmp = vault(&[("a.md", "# T\n")]);
    let out = run(tmp.path(), &["lint", "--fix", "--fix-rule", "BOGUS"]);
    assert_eq!(out.status.code(), Some(1));
    let both = format!("{}{}", String::from_utf8_lossy(&out.stdout), stderr(&out));
    assert!(both.contains("no such rule: BOGUS"), "{both}");
}

#[test]
fn a_known_fix_rule_is_case_insensitive() {
    let tmp = vault(&[("a.md", "# T\n\ntrailing   \n")]);
    let out = run(
        tmp.path(),
        &["lint", "--fix", "--fix-rule", "md009", "a.md"],
    );
    assert!(out.status.success(), "{}", stderr(&out));
}

#[test]
fn a_large_lint_fix_reports_progress_and_quiet_silences_it() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    for n in 0..210 {
        write_md(tmp.path(), &format!("n{n}.md"), "# T\n\nbody\n");
    }
    let out = run(tmp.path(), &["lint", "--fix"]);
    let err = stderr(&out);
    assert!(err.contains("lint --fix: 200/210 files"), "{err}");
    assert!(err.contains("lint --fix: 210/210 files, done"), "{err}");
    let quiet = run(tmp.path(), &["lint", "--fix", "-q"]);
    assert!(
        !stderr(&quiet).contains("lint --fix:"),
        "{}",
        stderr(&quiet)
    );
}

// ---------------------------------------------------------------------------
// links auto
// ---------------------------------------------------------------------------

fn auto_vault(beta: &str) -> TempDir {
    vault(&[
        ("alpha.md", "---\ntitle: Alpha Note\n---\n# Alpha Note\n"),
        ("beta.md", beta),
    ])
}

#[test]
fn links_auto_skips_html_elements_math_and_block_ids() {
    let beta = "# Beta\n\n\
<a href=\"x\">Alpha Note</a> and $$Alpha Note$$ and $Alpha Note$ here ^alpha\n\
<div>\nAlpha Note in a block\n</div>\n\n\
Mention Alpha Note in prose.\n";
    let tmp = auto_vault(beta);
    let out = run(tmp.path(), &["links", "auto", "--apply"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        read(&tmp, "beta.md"),
        beta.replace("Mention Alpha Note", "Mention [[alpha|Alpha Note]]")
    );
}

#[test]
fn links_auto_does_not_pair_a_backtick_across_list_items() {
    let beta = "# Beta\n\n- see `@person\n- Alpha Note in the next item `\n";
    let tmp = auto_vault(beta);
    let out = run(tmp.path(), &["links", "auto", "--apply"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        read(&tmp, "beta.md").contains("[[alpha|Alpha Note]]"),
        "{}",
        read(&tmp, "beta.md")
    );
}

#[test]
fn links_auto_escapes_the_alias_pipe_in_a_table_cell() {
    let beta = "# Beta\n\n| Name | Note |\n|------|------|\n| Alpha Note | x |\n";
    let tmp = auto_vault(beta);
    let out = run(tmp.path(), &["links", "auto", "--apply"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        read(&tmp, "beta.md").contains("| [[alpha\\|Alpha Note]] | x |"),
        "{}",
        read(&tmp, "beta.md")
    );
}

#[test]
fn links_fix_escapes_the_alias_pipe_in_a_table_cell() {
    let tmp = vault(&[
        (
            "sub/beta.md",
            "---\ntitle: Beta Doc\naliases: [Bee]\n---\n# Beta Doc\n",
        ),
        (
            "t.md",
            "# T\n\n| Name | Note |\n|------|------|\n| [[Bee]] | y |\n",
        ),
    ]);
    let out = run(tmp.path(), &["links", "fix", "--apply"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        read(&tmp, "t.md").contains("| [[sub/beta\\|Bee]] | y |"),
        "{}",
        read(&tmp, "t.md")
    );
}

// ---------------------------------------------------------------------------
// Idempotence
// ---------------------------------------------------------------------------

#[test]
fn a_second_lint_fix_changes_nothing() {
    let tmp = vault(&[
        ("a.md", HTML_HEAVY),
        ("b.md", "# Wide\n ## Indented\ntext\n"),
    ]);
    lint_fix(&tmp, "a.md");
    lint_fix(&tmp, "b.md");
    let (a, b) = (read(&tmp, "a.md"), read(&tmp, "b.md"));
    lint_fix(&tmp, "a.md");
    lint_fix(&tmp, "b.md");
    assert_eq!((a, b), (read(&tmp, "a.md"), read(&tmp, "b.md")));
}

// ---------------------------------------------------------------------------
// PR #402 review round
// ---------------------------------------------------------------------------

/// Bodies whose every byte must survive `lint --fix`: each holds something the
/// default-on fixers (HYALO001, MD009/010/012/019/022/034, ...) would rewrite
/// outside the protected region.
const UNTOUCHED: &[(&str, &str)] = &[
    ("pre_task", "# T\n\n<pre>\n[] literal sample\n</pre>\n"),
    ("div_task", "# T\n\n<div>\n[x] done?\n</div>\n"),
    ("math_task", "# T\n\n$$\n[] \\cdot a\n$$\n"),
    (
        "percent_block",
        "# T\n\n%%\nhttps://x.org/a\n##  heading\n\n%%\n",
    ),
    (
        "list_pre",
        "# T\n\n- <pre>\n  raw https://x.org/a\n\n  raw https://y.org/b\n  </pre>\n",
    ),
    (
        "list_div",
        "# T\n\n1. <div>\n   https://z.org/c\n   </div>\n",
    ),
    (
        "pending_macro",
        "# T\n\nUse {{ in Jinja. See <a href=\"x\">https://b.org/y</a> for more.\n",
    ),
    (
        "pending_raw",
        "# T\n\nType Foo<a> here, then <code>https://b.org/y</code> ok.\n",
    ),
    (
        "unbalanced_math_then_fence",
        "# T\n\nEnergy is $$\nE = mc^2\n$$ which is famous.\n\n```text\n##  literal sample\n\n\nend\n```\n",
    ),
    (
        "dollar_pid",
        "# T\n\n$$ expands to the PID.\n\n```text\n##  literal sample\n\n\nend\n```\n",
    ),
    (
        "indented_code_after_list_and_div",
        "# T\n\n- item\n\n<div>x</div>\n\n    ##  indented code\n    more code\n",
    ),
];

#[test]
fn nothing_in_a_protected_region_or_code_is_rewritten() {
    for (name, body) in UNTOUCHED {
        let tmp = vault(&[("a.md", body)]);
        lint_fix(&tmp, "a.md");
        assert_eq!(&read(&tmp, "a.md"), body, "{name}");
    }
}

#[test]
fn findings_after_a_container_scoped_block_survive() {
    let quote = "# T\n\n> <pre>\n> never closed\n\nText with https://p.org/x here.\n";
    let list = "# T\n\n- a\n  <div>x</div>\n- b https://x.org/one\n";
    let tmp = vault(&[("q.md", quote), ("l.md", list)]);
    lint_fix(&tmp, "q.md");
    lint_fix(&tmp, "l.md");
    assert!(read(&tmp, "q.md").contains("<https://p.org/x>"));
    assert!(read(&tmp, "l.md").contains("<https://x.org/one>"));
}

#[test]
fn a_formula_line_with_trailing_prose_leaves_the_prose_lintable() {
    let tmp = vault(&[("a.md", "# T\n\n$$x$$ and https://p.org/z\n")]);
    lint_fix(&tmp, "a.md");
    assert!(read(&tmp, "a.md").contains("<https://p.org/z>"));
}

#[test]
fn fix_rule_accepts_the_frontmatter_pseudo_rules() {
    let tmp = vault(&[("a.md", "# T\n")]);
    for id in ["SCHEMA", "schema", "FRONTMATTER003"] {
        let out = run(
            tmp.path(),
            &["lint", "--fix", "--dry-run", "--fix-rule", id, "a.md"],
        );
        assert!(out.status.success(), "{id}: {}", stderr(&out));
    }
}

#[test]
fn links_auto_and_fix_escape_the_pipe_in_quoted_and_pipeless_tables() {
    let beta = "# Beta\n\n> | Name | Note |\n> |------|------|\n> | Alpha Note | x |\n\nName | Note\n--- | ---\nAlpha Note | y\n";
    let tmp = auto_vault(beta);
    let out = run(tmp.path(), &["links", "auto", "--apply"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let after = read(&tmp, "beta.md");
    assert!(
        after.contains("> | [[alpha\\|Alpha Note]] | x |"),
        "{after}"
    );
    assert!(after.contains("[[alpha\\|Alpha Note]] | y"), "{after}");
}

#[test]
fn a_reversed_link_glued_to_a_word_is_still_fixed() {
    let tmp = vault(&[(
        "a.md",
        "# T\n\ncheck this out(see docs)[https://example.com/guide] now\n",
    )]);
    lint_fix(&tmp, "a.md");
    assert!(read(&tmp, "a.md").contains("[see docs](https://example.com/guide)"));
}
