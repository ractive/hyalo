//! Iteration 338 (DEC-413): a link that names no note is never a backlink.
//!
//! `find --fields links` reported `path: null` for `[[back\slash]]`,
//! `[y](back\slash.md)` and `[p](my%20file.md)` while `backlinks` still listed
//! them under the note whose file name equals the link's raw spelling: the
//! graph filed an unresolved link under that spelling. The reader resolves a
//! `\` as a separator and percent-decodes, so those spellings miss — and a
//! miss must not be somebody's inbound link. Every reader of the graph agrees
//! now: `backlinks`, `find --fields backlinks`, `--orphan` and `summary`,
//! on disk and under `--index`.

use super::common::{hyalo_no_hints, write_md};
use serde_json::Value;
use std::path::Path;
use tempfile::TempDir;

fn run(dir: &Path, args: &[&str]) -> Value {
    let out = hyalo_no_hints()
        .current_dir(dir)
        .arg("--dir")
        .arg(".")
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("not JSON ({e}): {}", String::from_utf8_lossy(&out.stdout)))
}

/// `path` of the one link in `src.md`; `None` when the reader leaves it unresolved.
fn link_path(dir: &Path) -> Option<String> {
    let out = run(
        dir,
        &[
            "find", "--file", "src.md", "--fields", "links", "--format", "json",
        ],
    );
    let links = out["results"][0]["links"].as_array().unwrap();
    assert_eq!(links.len(), 1, "{out}");
    links[0]["path"].as_str().map(str::to_owned)
}

/// Sources listed by `backlinks FILE` and by `find --file FILE --fields
/// backlinks`; the two must be the same list.
fn backlink_sources(dir: &Path, file: &str, indexed: bool) -> Vec<String> {
    let mut a = vec!["backlinks", "--file", file, "--format", "json"];
    let mut b = vec![
        "find",
        "--file",
        file,
        "--fields",
        "backlinks",
        "--format",
        "json",
    ];
    if indexed {
        a.push("--index");
        b.push("--index");
    }
    let from_command: Vec<String> = run(dir, &a)["results"]["backlinks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["source"].as_str().unwrap().to_owned())
        .collect();
    let from_find: Vec<String> = run(dir, &b)["results"][0]["backlinks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["source"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        from_command, from_find,
        "backlinks and find --fields backlinks disagree for {file} (indexed: {indexed})"
    );
    from_command
}

/// `src.md` holds `link` and `target` is the note whose name equals the link's
/// spelling. The reader leaves the link unresolved, so `target` has no
/// backlink and, linked from nowhere, is an orphan — on disk and indexed.
fn assert_unresolved_link_is_no_backlink(target: &str, link: &str) {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    write_md(d, target, "# Target\n");
    write_md(d, "src.md", &format!("{link}\n"));

    assert_eq!(
        link_path(d),
        None,
        "{link}: precondition, the reader must leave it unresolved"
    );
    run(d, &["create-index"]);
    for indexed in [false, true] {
        assert!(
            backlink_sources(d, target, indexed).is_empty(),
            "{link} listed as a backlink of {target} (indexed: {indexed})"
        );
        let mut summary = vec!["summary", "--format", "json"];
        if indexed {
            summary.push("--index");
        }
        // `src.md` has an outbound (broken) link, `target` neither direction.
        assert_eq!(
            run(d, &summary)["results"]["orphans"].as_u64(),
            Some(1),
            "{link}: {target} is an orphan (indexed: {indexed})"
        );
    }
}

#[cfg(unix)]
#[test]
fn backslash_wikilink_is_not_a_backlink() {
    assert_unresolved_link_is_no_backlink("back\\slash.md", "[[back\\slash]]");
}

#[cfg(unix)]
#[test]
fn backslash_markdown_link_is_not_a_backlink() {
    assert_unresolved_link_is_no_backlink("back\\slash.md", "[y](back\\slash.md)");
}

#[test]
fn percent_encoded_markdown_link_is_not_a_backlink() {
    assert_unresolved_link_is_no_backlink("my%20file.md", "[p](my%20file.md)");
}

/// The links that do resolve keep listing: a `%5C`-encoded backslash reaches
/// `back\slash.md`, and an ordinary wikilink reaches `note.md`.
#[test]
fn resolving_links_still_list_on_disk_and_indexed() {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path();
    write_md(d, "note.md", "# Note\n");
    write_md(d, "src.md", "[[note]]\n");
    #[cfg(unix)]
    {
        write_md(d, "back\\slash.md", "# Back\n");
        write_md(d, "enc.md", "[[back%5Cslash]]\n[[back\\slash]]\n");
    }
    run(d, &["create-index"]);
    for indexed in [false, true] {
        assert_eq!(
            backlink_sources(d, "note.md", indexed),
            vec!["src.md"],
            "indexed: {indexed}"
        );
        #[cfg(unix)]
        assert_eq!(
            backlink_sources(d, "back\\slash.md", indexed),
            vec!["enc.md"],
            "only the `%5C` spelling resolves (indexed: {indexed})"
        );
    }
}
