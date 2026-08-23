//! iter-211 — links resolution correctness: anchors, HYALO006 line numbers,
//! trailing-slash keying, query strings, link titles and `mv` spellings.
//!
//! Each block here pins one defect the 2026-08-23 dogfood found. They live
//! together because they all describe the same contract: **every surface that
//! answers a question about a link must answer it the same way** —
//! `find --broken-links`, `backlinks`, `links fix` and the HYALO006 lint rule
//! resolve through one shared step, and a rewrite must give back the author's
//! spelling byte-for-byte apart from the path itself.

use super::common::{hyalo_no_hints, md, write_md};
use tempfile::TempDir;

fn run_json(tmp: &TempDir, args: &[&str]) -> serde_json::Value {
    let dir = tmp.path().to_str().expect("utf-8 path");
    let mut cmd_args = vec!["--dir", dir];
    cmd_args.extend_from_slice(args);
    cmd_args.extend_from_slice(&["--format", "json"]);
    let output = hyalo_no_hints()
        .args(&cmd_args)
        .output()
        .expect("hyalo should run");
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout should be JSON for {cmd_args:?}: {e}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

// ---------------------------------------------------------------------------
// BUG-9 — HYALO006 line numbers must not double-count the frontmatter
// ---------------------------------------------------------------------------

/// Lint one file whose broken link sits on a known absolute line and return the
/// line HYALO006 reported.
fn hyalo006_reported_line(frontmatter_lines: usize) -> (usize, usize) {
    let tmp = TempDir::new().expect("tempdir");
    // Build frontmatter of the requested length (0, 3 or 5 lines including the
    // `---` fences), then a body whose broken link is on a known line.
    let frontmatter = match frontmatter_lines {
        0 => String::new(),
        3 => "---\ntitle: T\n---\n".to_owned(),
        5 => "---\ntitle: T\ntags:\n  - x\n---\n".to_owned(),
        n => panic!("unsupported frontmatter length {n}"),
    };
    let body = "# Heading\n\nfiller\n\nSee [x](does-not-exist.md).\n";
    // The link is on body line 5; absolute line = frontmatter_lines + 5.
    let expected = frontmatter_lines + 5;
    std::fs::write(tmp.path().join("a.md"), format!("{frontmatter}{body}")).expect("write fixture");

    let json = run_json(&tmp, &["lint", "--rule", "HYALO006"]);
    let reported = json["results"]["files"]
        .as_array()
        .and_then(|files| files.first())
        .and_then(|f| f["rule_groups"].as_array())
        .and_then(|groups| groups.first())
        .and_then(|g| g["violations"].as_array())
        .and_then(|v| v.first())
        .and_then(|v| v["line"].as_u64())
        .unwrap_or_else(|| panic!("expected one HYALO006 violation, got: {json}"));
    (
        expected,
        usize::try_from(reported).expect("line fits usize"),
    )
}

#[test]
fn hyalo006_line_numbers_are_exact_for_every_frontmatter_length() {
    // BUG-9: the finding's line was already file-absolute, and the body-rule
    // offset was applied on top — a 3-line frontmatter reported line 5 as 8.
    for fm in [0usize, 3, 5] {
        let (expected, reported) = hyalo006_reported_line(fm);
        assert_eq!(
            reported, expected,
            "HYALO006 line must be exact for a {fm}-line frontmatter"
        );
    }
}

#[test]
fn hyalo006_agrees_with_the_markdown_rules_on_the_same_file() {
    // The cross-check from the dogfood table: MD rules were always right on
    // the same file, so any disagreement is HYALO006's offset bug.
    let tmp = TempDir::new().expect("tempdir");
    std::fs::write(
        tmp.path().join("a.md"),
        // Line 4 is a trailing-whitespace violation (MD009), line 5 the link.
        "---\ntitle: T\n---\ntrailing   \nSee [x](nope.md).\n",
    )
    .expect("write fixture");

    let json = run_json(&tmp, &["lint"]);
    let groups = json["results"]["files"][0]["rule_groups"]
        .as_array()
        .expect("rule groups");
    let mut md009 = None;
    let mut hyalo006 = None;
    for g in groups {
        let line = g["violations"][0]["line"].as_u64();
        match g["rule"].as_str() {
            Some("MD009") => md009 = line,
            Some("HYALO006") => hyalo006 = line,
            _ => {}
        }
    }
    assert_eq!(md009, Some(4), "MD009 anchor line: {json}");
    assert_eq!(hyalo006, Some(5), "HYALO006 must agree: {json}");
}

// ---------------------------------------------------------------------------
// BUG-10 — one link occurrence, one target; `backlinks` agrees with `find`
// ---------------------------------------------------------------------------

/// The eight dogfood spellings, written into one linker file next to both a
/// `foo.md` and a `foo/index.md` (the ambiguity that triggered the
/// double-count) plus a `baz.md` with no directory of its own.
fn setup_slash_vault() -> TempDir {
    let tmp = TempDir::new().expect("tempdir");
    write_md(
        tmp.path(),
        "foo.md",
        "---\ntitle: Foo file\n---\n# Foo file\n",
    );
    write_md(
        tmp.path(),
        "foo/index.md",
        "---\ntitle: Foo index\n---\n# Foo index\n",
    );
    write_md(tmp.path(), "baz.md", "---\ntitle: Baz\n---\n# Baz\n");
    write_md(
        tmp.path(),
        "linker.md",
        md!(r"
---
title: Linker
---
- 1 [a](foo/)
- 2 [b](/foo/)
- 3 [c](foo)
- 4 [d](/foo)
- 5 [e](foo.md)
- 6 [f](/foo.md)
- 7 [g](/baz/)
- 8 [h](baz/)
"),
    );
    tmp
}

/// `backlinks <file>` count, self-links excluded as the command does.
fn backlink_count(tmp: &TempDir, file: &str) -> usize {
    let json = run_json(tmp, &["backlinks", file]);
    json["results"]["backlinks"].as_array().map_or(0, Vec::len)
}

#[test]
fn one_trailing_slash_link_produces_exactly_one_backlink() {
    // BUG-10: `[a](foo/)` was indexed under BOTH `foo` and `foo/index`, so it
    // counted as a backlink of `foo.md` *and* `foo/index.md` — while
    // `links` reported `ambiguous: 0`.
    let tmp = setup_slash_vault();

    // Spellings 1 and 2 are explicit directory references → `foo/index.md`.
    // Spellings 3–6 name the file → `foo.md`.
    assert_eq!(
        backlink_count(&tmp, "foo/index.md"),
        2,
        "only the two trailing-slash spellings point at the directory index"
    );
    assert_eq!(
        backlink_count(&tmp, "foo.md"),
        4,
        "the four file spellings point at the file, and nothing else does"
    );
}

#[test]
fn trailing_slash_target_that_falls_back_to_a_file_is_a_backlink_of_it() {
    // BUG-10, second half: `[g](/baz/)` resolved to `baz.md` in
    // `find --broken-links` but was indexed under the key `baz/`, which
    // `backlinks baz.md` can never probe.
    let tmp = setup_slash_vault();
    assert_eq!(
        backlink_count(&tmp, "baz.md"),
        2,
        "both `/baz/` and `baz/` fall back to baz.md and must be backlinks of it"
    );
}

#[test]
fn backlinks_agrees_with_find_broken_links_on_every_spelling() {
    // The parity contract: whatever `find --broken-links` resolves a link to,
    // `backlinks` must attribute that link to the same file — for all eight
    // dogfood spellings, and with nothing left over.
    let tmp = setup_slash_vault();

    let json = run_json(
        &tmp,
        &["find", "--fields", "links", "--property", "title=Linker"],
    );
    let links = json["results"][0]["links"].as_array().expect("links array");
    assert_eq!(links.len(), 8, "all eight spellings must parse: {json}");

    let mut resolved: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for l in links {
        let path = l["path"]
            .as_str()
            .unwrap_or_else(|| panic!("every spelling must resolve: {l}"));
        *resolved.entry(path.to_owned()).or_default() += 1;
    }

    for (path, count) in &resolved {
        assert_eq!(
            backlink_count(&tmp, path),
            *count,
            "backlinks({path}) must match how many links find resolved there"
        );
    }
    // And no other file collected a stray edge.
    let total: usize = resolved.values().sum();
    assert_eq!(total, 8, "every occurrence is attributed exactly once");
}

// ---------------------------------------------------------------------------
// BUG-12 — query strings and CommonMark titles survive resolution + rewrite
// ---------------------------------------------------------------------------

#[test]
fn query_string_survives_a_rename() {
    // `[x](/deep/page?x=1)` used to come back as `[x](/deep/Page)`: the query
    // was glued to the target, so the rewrite span swallowed it.
    let tmp = TempDir::new().expect("tempdir");
    write_md(
        tmp.path(),
        "deep/page.md",
        "---\ntitle: Page\n---\n# Page\n",
    );
    write_md(
        tmp.path(),
        "linker.md",
        "---\ntitle: Linker\n---\nSee [x](/deep/page?x=1) and [y](/deep/page?a=1#frag).\n",
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "deep/page.md", "deep/Page.md"])
        .output()
        .expect("mv should run");
    assert!(
        output.status.success(),
        "mv failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let written = std::fs::read_to_string(tmp.path().join("linker.md")).expect("readable");
    assert!(
        written.contains("[x](/deep/Page?x=1)"),
        "query string must survive the rewrite: {written}"
    );
    assert!(
        written.contains("[y](/deep/Page?a=1#frag)"),
        "query and fragment must both survive: {written}"
    );
}

#[test]
fn a_query_string_does_not_make_a_link_broken() {
    let tmp = TempDir::new().expect("tempdir");
    write_md(
        tmp.path(),
        "deep/page.md",
        "---\ntitle: Page\n---\n# Page\n",
    );
    write_md(
        tmp.path(),
        "linker.md",
        "---\ntitle: Linker\n---\nSee [x](/deep/page?x=1).\n",
    );
    let json = run_json(&tmp, &["find", "--broken-links"]);
    assert_eq!(
        json["results"].as_array().map(Vec::len),
        Some(0),
        "a query string is not part of the path: {json}"
    );
    assert_eq!(
        backlink_count(&tmp, "deep/page.md"),
        1,
        "the query-carrying link must still be a backlink"
    );
}

#[test]
fn commonmark_link_title_resolves_and_survives_a_rename() {
    // `[a](p.md "Title")` used to parse the title as part of the destination:
    // reported broken, missing from backlinks, and unrewritable.
    let tmp = TempDir::new().expect("tempdir");
    write_md(tmp.path(), "p.md", "---\ntitle: P\n---\n# P\n");
    write_md(
        tmp.path(),
        "linker.md",
        "---\ntitle: Linker\n---\nSee [a](p.md \"The Title\") and [b](p.md \"has ) paren\").\n",
    );

    let json = run_json(&tmp, &["find", "--broken-links"]);
    assert_eq!(
        json["results"].as_array().map(Vec::len),
        Some(0),
        "a titled link must resolve: {json}"
    );
    assert_eq!(
        backlink_count(&tmp, "p.md"),
        2,
        "both titled links must appear in backlinks"
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "p.md", "q.md"])
        .output()
        .expect("mv should run");
    assert!(
        output.status.success(),
        "mv failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let written = std::fs::read_to_string(tmp.path().join("linker.md")).expect("readable");
    assert!(
        written.contains("[a](q.md \"The Title\")"),
        "the title must survive the rewrite: {written}"
    );
    assert!(
        written.contains("[b](q.md \"has ) paren\")"),
        "a title containing `)` must not truncate the span: {written}"
    );
}

#[test]
fn an_unencoded_space_in_a_destination_is_still_tolerated() {
    // The title split must not break the long-standing tolerance for
    // hand-written destinations with a literal space.
    let tmp = TempDir::new().expect("tempdir");
    write_md(tmp.path(), "my dest.md", "---\ntitle: Dest\n---\n# Dest\n");
    write_md(
        tmp.path(),
        "linker.md",
        "---\ntitle: Linker\n---\nSee [x](my dest.md).\n",
    );
    let json = run_json(&tmp, &["find", "--broken-links"]);
    assert_eq!(
        json["results"].as_array().map(Vec::len),
        Some(0),
        "an unencoded space must still resolve: {json}"
    );
}

// ---------------------------------------------------------------------------
// BUG-12 — `mv` preserves the author's spelling for all ten forms
// ---------------------------------------------------------------------------

#[test]
fn mv_preserves_all_ten_written_spellings() {
    // iter-203's spelling guarantee, re-asserted across every form the dogfood
    // enumerated. Form 8 (`[f](foo/index)`) is the one that regressed: `mv`
    // appended a `.md` the author never wrote.
    let tmp = TempDir::new().expect("tempdir");
    write_md(tmp.path(), "foo/index.md", "---\ntitle: Foo\n---\n# Foo\n");
    write_md(
        tmp.path(),
        "linker.md",
        md!(r#"
---
title: Linker
---
- 1 [a](/foo)
- 2 [b](/foo/)
- 3 [c](/foo/index.md)
- 4 [[foo]]
- 5 [[foo/index]]
- 6 [[foo/index.md]]
- 7 [g](foo/index.md)
- 8 [h](foo/index)
- 9 [i](/foo#top)
- 10 [j](/foo/index.md "T")
"#),
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "foo/index.md", "bar/index.md"])
        .output()
        .expect("mv should run");
    assert!(
        output.status.success(),
        "mv failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let written = std::fs::read_to_string(tmp.path().join("linker.md")).expect("readable");
    for expected in [
        "[a](/bar)",
        "[b](/bar/)",
        "[c](/bar/index.md)",
        "[[bar]]",
        "[[bar/index]]",
        "[[bar/index.md]]",
        "[g](bar/index.md)",
        "[h](bar/index)",
        "[i](/bar#top)",
        "[j](/bar/index.md \"T\")",
    ] {
        assert!(
            written.contains(expected),
            "expected {expected} in rewritten linker:\n{written}"
        );
    }
    assert!(
        !written.contains("foo"),
        "no stale `foo` spelling may remain:\n{written}"
    );
}
