//! Iteration 278 — the literal link probe answered from the index.
//!
//! `classify_link` asks two questions about every link: "is it correct exactly
//! as written?" and "what would it be if it were canonicalized?". The first one
//! used to go to the filesystem for every candidate path — a `stat` plus a
//! `realpath` each — because passing no case index was the only way to ask for
//! the uncanonicalized answer. Iteration 278 lets a *complete* index answer it
//! from memory (`resolve_target_literal`).
//!
//! Nothing about the verdicts may change, so these tests are parity tests: the
//! same vault, once through a disk scan and once through a snapshot index, must
//! produce the same JSON. Wall-clock is deliberately not asserted — that is
//! `xtask bench-scale`'s job, not a shared CI runner's.

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn hyalo_no_hints() -> Command {
    crate::common::hyalo_no_hints()
}

fn write_md(dir: &std::path::Path, rel: &str, body: &str) {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, body).unwrap();
}

fn json(tmp: &TempDir, args: &[&str]) -> Value {
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path().to_str().unwrap())
        .args(args)
        .args(["--format", "json"])
        .output()
        .unwrap();
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "`hyalo {}` did not produce JSON ({e}): {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

/// A vault shaped like the static-site corpora this iteration is about: a
/// directory-index tree written in one casing and linked in another, some
/// site-absolute links, a genuinely broken one, and an ordinary note.
fn site_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "web/css/flex/index.md", "# Flex\n\nBody.\n");
    write_md(tmp.path(), "web/css/grid/index.md", "# Grid\n\nBody.\n");
    write_md(tmp.path(), "web/api/document.md", "# Document\n\nBody.\n");
    write_md(
        tmp.path(),
        "guides/start.md",
        "---\ntitle: Start\n---\n\n\
         Exact: [f](/site/web/css/flex)\n\
         Case-differing: [g](/site/Web/CSS/Grid)\n\
         Explicit file: [d](/site/web/api/document.md)\n\
         Directory slash: [f2](/site/web/css/flex/)\n\
         Wikilink: [[web/api/document]]\n\
         Broken: [x](/site/web/css/missing)\n",
    );
    tmp
}

fn build_index(tmp: &TempDir) {
    hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path().to_str().unwrap())
        .args(["create-index"])
        .assert()
        .success();
}

/// The parity that matters: every command whose verdict comes from
/// `classify_link` must answer identically on disk and from the snapshot.
#[test]
fn classify_verdicts_are_identical_on_disk_and_from_the_index() {
    let tmp = site_vault();
    build_index(&tmp);

    for args in [
        vec!["summary", "--site-prefix", "site"],
        vec!["links", "fix", "--dry-run", "--site-prefix", "site"],
        vec!["find", "--broken-links", "--site-prefix", "site"],
    ] {
        let disk = json(&tmp, &args);
        let mut indexed_args = args.clone();
        indexed_args.push("--index");
        let indexed = json(&tmp, &indexed_args);
        assert_eq!(
            disk["results"],
            indexed["results"],
            "`hyalo {}` must give the same verdicts with and without --index",
            args.join(" ")
        );
    }
}

/// A site-absolute link written in the site's own casing is *resolved* — the
/// whole reason the literal probe cannot simply be dropped — and stays out of
/// the broken list either way.
#[test]
fn case_differing_site_absolute_links_resolve_from_the_index() {
    let tmp = site_vault();
    build_index(&tmp);

    let indexed = json(&tmp, &["summary", "--site-prefix", "site", "--index"]);
    let links = &indexed["results"]["links"];
    assert_eq!(
        links["broken"], 1,
        "only the genuinely missing target is broken: {links}"
    );
}

/// A link whose spelling *is* the on-disk spelling must never be reported as
/// needing a rewrite — the literal probe's only job.
#[test]
fn exactly_written_links_produce_no_fix_plan() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "sub/note.md", "# Note\n");
    write_md(
        tmp.path(),
        "hub.md",
        "See [n](sub/note.md) and [[sub/note]].\n",
    );
    build_index(&tmp);

    for extra in [vec![], vec!["--index"]] {
        let mut args = vec!["links", "fix", "--dry-run"];
        args.extend(extra.iter().copied());
        let out = json(&tmp, &args);
        let results = &out["results"];
        assert_eq!(results["broken"], 0, "{results}");
        assert_eq!(results["case_mismatches"], 0, "{results}");
        assert_eq!(results["fuzzy"], 0, "{results}");
        assert_eq!(results["relocations"], 0, "{results}");
    }
}

/// A `--file`-scoped run builds an index that covers one file, which proves
/// nothing about the rest of the vault: resolution must fall back to the
/// filesystem instead of calling every unscanned file absent.
#[test]
fn a_file_scoped_run_still_resolves_links_to_unscanned_files() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "target.md", "# Target\n");
    write_md(tmp.path(), "source.md", "Link to [[target]].\n");

    let out = json(&tmp, &["find", "--file", "source.md", "--fields", "links"]);
    let links = out["results"][0]["links"].as_array().unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(
        links[0]["path"], "target.md",
        "a link to a file outside the --file scope must still resolve: {links:?}"
    );
}
