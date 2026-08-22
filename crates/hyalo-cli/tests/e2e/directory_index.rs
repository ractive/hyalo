//! iter-203 — directory link targets resolve to `<target>/index.md`.
//!
//! The fixture is the F-1 link matrix from the v0.21.0-pre dogfood report: a
//! directory-index corpus (MDN / GitHub Docs shape) where `/foo`, `foo` and
//! `/foo/` all name the page stored at `foo/index.md`. Before this iteration
//! every one of those spellings read as broken, which is what made MDN report
//! 49,703 of 49,705 links broken and `backlinks` return 0 for its most-linked
//! pages.
//!
//! Every downstream surface is asserted here, because the whole point of
//! putting the rule in the shared resolver is that they all agree:
//! `find --broken-links`, broken-anchor checking, `backlinks`, the HYALO006
//! lint rule, and `--index` parity with the on-disk scan.

use super::common::{hyalo_no_hints, md, write_md};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Fixture — the F-1 A–E link matrix
// ---------------------------------------------------------------------------

/// A directory-index vault:
///
/// ```text
/// linker.md          A–H: every directory spelling plus the controls
/// foo/index.md       the directory index, with a `## Section` heading
/// bar/page.md        an ordinary nested file (control: `/bar/page`)
/// baz.md             precedence control — beats `baz/index.md`
/// baz/index.md
/// empty/page.md      a directory with no index (control: must stay broken)
/// ```
fn setup_vault() -> TempDir {
    let tmp = TempDir::new().expect("tempdir");

    write_md(
        tmp.path(),
        "foo/index.md",
        md!(r"
---
title: Foo
---
# Foo

## Section

Body.
"),
    );
    write_md(tmp.path(), "bar/page.md", "---\ntitle: Page\n---\n# Page\n");
    write_md(tmp.path(), "baz.md", "---\ntitle: Baz file\n---\n# Baz file\n");
    write_md(
        tmp.path(),
        "baz/index.md",
        "---\ntitle: Baz index\n---\n# Baz index\n",
    );
    write_md(
        tmp.path(),
        "empty/page.md",
        "---\ntitle: Empty\n---\n# Empty\n",
    );

    write_md(
        tmp.path(),
        "linker.md",
        md!(r"
---
title: Linker
---
- A [site-absolute dir](/foo)
- B [bare dir](foo)
- C [trailing slash](/foo/)
- D [nested file](/bar/page)
- E [explicit index](/foo/index)
- F [dir with anchor](/foo#section)
- G [[foo]] as a wikilink
- H [file beats index](baz)
- I [no index here](/empty)
"),
    );

    tmp
}

/// Run `hyalo find --broken-links --fields links`, optionally through `--index`.
fn broken_links(tmp: &TempDir, indexed: bool) -> serde_json::Value {
    let dir = tmp.path().to_str().expect("utf-8 path");
    let mut args = vec![
        "--dir",
        dir,
        "find",
        "--broken-links",
        "--fields",
        "links",
        "--format",
        "json",
    ];
    if indexed {
        args.push("--index");
    }
    let output = hyalo_no_hints()
        .args(&args)
        .output()
        .expect("find --broken-links should run");
    assert!(
        output.status.success(),
        "find --broken-links exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("valid JSON")
}

/// Every link target reported as broken, across all result files.
fn broken_targets(json: &serde_json::Value) -> Vec<String> {
    json["results"]
        .as_array()
        .expect("results array")
        .iter()
        .filter_map(|r| r.get("links")?.as_array())
        .flatten()
        .filter(|l| l["path"].is_null())
        .map(|l| l["target"].as_str().unwrap_or_default().to_string())
        .collect()
}

fn create_index(tmp: &TempDir) {
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap(), "create-index"])
        .output()
        .expect("create-index should run");
    assert!(
        output.status.success(),
        "create-index failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ---------------------------------------------------------------------------
// find --broken-links
// ---------------------------------------------------------------------------

fn assert_matrix(json: &serde_json::Value, label: &str) {
    let broken = broken_targets(json);
    assert_eq!(
        broken,
        vec!["/empty".to_string()],
        "[{label}] only the directory without an index.md may be broken"
    );
}

#[test]
fn directory_targets_resolve_on_disk_scan() {
    let tmp = setup_vault();
    assert_matrix(&broken_links(&tmp, false), "disk");
}

#[test]
fn directory_targets_resolve_through_the_index() {
    // iter-190's fail-safe pattern: the rule is derived at query time, so an
    // index built before this change still resolves directory targets.
    let tmp = setup_vault();
    create_index(&tmp);
    assert_matrix(&broken_links(&tmp, true), "index");
}

#[test]
fn directory_target_resolves_to_the_index_file_path() {
    let tmp = setup_vault();
    // `--broken-links` only surfaces files that *have* a broken link, so query
    // the full link field to inspect what each spelling resolved to.
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "find", "--file", "linker.md", "--fields", "links", "--format", "json",
        ])
        .output()
        .expect("find should run");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    let links = json["results"][0]["links"].as_array().expect("links array");
    let resolved = |target: &str| -> Option<String> {
        links
            .iter()
            .find(|l| l["target"].as_str() == Some(target))?
            .get("path")?
            .as_str()
            .map(str::to_string)
    };
    assert_eq!(resolved("/foo").as_deref(), Some("foo/index.md"));
    assert_eq!(resolved("foo").as_deref(), Some("foo/index.md"));
    assert_eq!(resolved("/foo/").as_deref(), Some("foo/index.md"));
    assert_eq!(resolved("/bar/page").as_deref(), Some("bar/page.md"));
    assert_eq!(resolved("/foo/index").as_deref(), Some("foo/index.md"));
    // Precedence: a real `baz.md` outranks `baz/index.md`.
    assert_eq!(resolved("baz").as_deref(), Some("baz.md"));
}

#[test]
fn directory_target_anchor_checks_the_index_files_headings() {
    let tmp = TempDir::new().expect("tempdir");
    write_md(
        tmp.path(),
        "foo/index.md",
        md!(r"
---
title: Foo
---
# Foo

## Section
"),
    );
    write_md(
        tmp.path(),
        "good.md",
        "---\ntitle: Good\n---\nSee [x](/foo#section).\n",
    );
    write_md(
        tmp.path(),
        "bad.md",
        "---\ntitle: Bad\n---\nSee [x](/foo#missing).\n",
    );

    let json = broken_links(&tmp, false);
    let files: Vec<String> = json["results"]
        .as_array()
        .expect("results array")
        .iter()
        .map(|r| r["file"].as_str().unwrap_or_default().to_string())
        .collect();
    assert!(
        !files.contains(&"good.md".to_string()),
        "an anchor matching the index file's heading is not broken: {files:?}"
    );
    assert!(
        files.contains(&"bad.md".to_string()),
        "a missing heading on the index file must still be flagged: {files:?}"
    );
}

// ---------------------------------------------------------------------------
// backlinks
// ---------------------------------------------------------------------------

#[test]
fn backlinks_counts_every_directory_spelling() {
    let tmp = setup_vault();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["backlinks", "--file", "foo/index.md", "--format", "json"])
        .output()
        .expect("backlinks should run");
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    let total = json["total"].as_u64().unwrap_or(0);
    assert!(
        total >= 6,
        "every A/B/C/E/F/G spelling must count as a backlink of foo/index.md, got {total}: {json}"
    );
}

// ---------------------------------------------------------------------------
// HYALO006 (broken link lint rule)
// ---------------------------------------------------------------------------

#[test]
fn hyalo006_does_not_flag_resolvable_directory_targets() {
    let tmp = TempDir::new().expect("tempdir");
    write_md(tmp.path(), "foo/index.md", "---\ntitle: Foo\n---\n# Foo\n");
    write_md(
        tmp.path(),
        "linker.md",
        "---\ntitle: Linker\n---\n[a](/foo) [b](foo) [c](/foo/)\n",
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["lint", "--rule", "HYALO006", "--format", "json"])
        .output()
        .expect("lint should run");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    let findings = json["results"].as_array().map_or(0, Vec::len);
    assert_eq!(findings, 0, "no directory target may be flagged: {json}");
}

// ---------------------------------------------------------------------------
// mv — directory-spelled inbound links keep their spelling
// ---------------------------------------------------------------------------

#[test]
fn mv_rewrites_directory_spellings_without_changing_style() {
    let tmp = TempDir::new().expect("tempdir");
    write_md(tmp.path(), "foo/index.md", "---\ntitle: Foo\n---\n# Foo\n");
    write_md(
        tmp.path(),
        "linker.md",
        "---\ntitle: Linker\n---\n[a](/foo) [b](/foo/) [c](/foo/index.md) and [[foo]]\n",
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "foo/index.md", "renamed/index.md"])
        .output()
        .expect("mv should run");
    assert!(
        output.status.success(),
        "mv failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let written = std::fs::read_to_string(tmp.path().join("linker.md")).expect("readable");
    assert!(
        written.contains("[a](/renamed)"),
        "site-absolute directory spelling must stay bare: {written}"
    );
    assert!(
        written.contains("[b](/renamed/)"),
        "the author's trailing slash must survive: {written}"
    );
    assert!(
        written.contains("[c](/renamed/index.md)"),
        "an explicit index spelling must stay explicit: {written}"
    );
    assert!(
        written.contains("[[renamed]]"),
        "a directory wikilink must stay a directory wikilink: {written}"
    );
    assert!(
        !written.contains("/renamed/index)"),
        "no `.md`-less index path may be injected: {written}"
    );
}

// ---------------------------------------------------------------------------
// hyalo config — the effective site_prefix (dogfood UX-4)
// ---------------------------------------------------------------------------

#[test]
fn config_reports_the_auto_derived_site_prefix() {
    let tmp = TempDir::new().expect("tempdir");
    write_md(tmp.path(), "foo/index.md", "---\ntitle: Foo\n---\n# Foo\n");
    let dir_name = std::fs::canonicalize(tmp.path())
        .expect("canonicalize")
        .file_name()
        .and_then(|n| n.to_str())
        .expect("utf-8 dir name")
        .to_string();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["config", "--format", "json"])
        .output()
        .expect("config should run");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    assert_eq!(
        json["results"]["site_prefix"].as_str(),
        Some(dir_name.as_str()),
        "config must report the derived prefix, not null: {json}"
    );
    assert_eq!(
        json["results"]["site_prefix_source"].as_str(),
        Some("derived"),
        "and must say that it was derived: {json}"
    );
}

#[test]
fn config_reports_an_explicitly_disabled_site_prefix() {
    let tmp = TempDir::new().expect("tempdir");
    write_md(tmp.path(), "foo/index.md", "---\ntitle: Foo\n---\n# Foo\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--site-prefix", ""])
        .args(["config", "--format", "json"])
        .output()
        .expect("config should run");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    assert!(json["results"]["site_prefix"].is_null(), "{json}");
    assert_eq!(
        json["results"]["site_prefix_source"].as_str(),
        Some("disabled"),
        "{json}"
    );
}

#[test]
fn config_text_output_labels_the_site_prefix_source() {
    let tmp = TempDir::new().expect("tempdir");
    write_md(tmp.path(), "foo/index.md", "---\ntitle: Foo\n---\n# Foo\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--site-prefix", "docs"])
        .args(["config", "--format", "text"])
        .output()
        .expect("config should run");
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        text.contains("site_prefix: docs (flag)"),
        "text report must name the source: {text}"
    );
}
