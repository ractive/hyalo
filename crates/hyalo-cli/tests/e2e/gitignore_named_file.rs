//! F5 (iteration 306 review): a gitignored file named explicitly is a promise
//! (DEC-301) that must survive every `--fields` combination, not just the
//! default one.
//!
//! `find --file MyNotes.md` returned the file, but `find --file MyNotes.md
//! --fields backlinks` (and `--fields all`) silently returned "No results" —
//! because any field needing a whole-vault scan (`backlinks`, `--orphan`,
//! `--dead-end`) rebuilt the scanned file list from a *plain*
//! gitignore-respecting walk, discarding the separately-validated named
//! file. `summary` also undercounted `results.files.excluded`: a
//! `.gitignore` drop was invisible next to a `[scan] exclude` drop.
//!
//! `.gitignore` is honoured by the `ignore` crate only inside something that
//! looks like a git repository, so each fixture creates a bare `.git`
//! directory (no `git init` needed) — the same technique
//! `links_resolution.rs`'s `hidden_link_vault` uses.

use super::common::{hyalo_no_hints, write_md};
use tempfile::TempDir;

fn vault_with_gitignored_named_file() -> TempDir {
    let tmp = TempDir::new().expect("tempdir");
    std::fs::create_dir_all(tmp.path().join(".git")).expect("fixture .git marker");
    std::fs::write(tmp.path().join(".gitignore"), "MyNotes.md\n").expect("write .gitignore");
    write_md(tmp.path(), "MyNotes.md", "# My Notes\n\nSecret stuff.\n");
    write_md(tmp.path(), "other.md", "# Other\n\n[[MyNotes]]\n");
    tmp
}

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

#[test]
fn gitignored_file_is_excluded_from_a_plain_vault_sweep() {
    let tmp = vault_with_gitignored_named_file();
    let json = run_json(&tmp, &["find"]);
    let files: Vec<&str> = json["results"]
        .as_array()
        .expect("results array")
        .iter()
        .map(|r| r["file"].as_str().expect("file"))
        .collect();
    assert_eq!(
        files,
        vec!["other.md"],
        "the gitignored file must not appear in an unscoped sweep: {json}"
    );
}

#[test]
fn named_gitignored_file_is_still_returned_plainly() {
    let tmp = vault_with_gitignored_named_file();
    let json = run_json(&tmp, &["find", "--file", "MyNotes.md"]);
    assert_eq!(json["total"], 1, "full json: {json}");
    assert_eq!(json["results"][0]["file"], "MyNotes.md");
}

#[test]
fn named_gitignored_file_survives_fields_backlinks() {
    let tmp = vault_with_gitignored_named_file();
    let json = run_json(
        &tmp,
        &["find", "--file", "MyNotes.md", "--fields", "backlinks"],
    );
    assert_eq!(
        json["total"], 1,
        "a named path is a promise (DEC-301); it must not vanish under \
         --fields backlinks: {json}"
    );
    let backlinks = json["results"][0]["backlinks"]
        .as_array()
        .unwrap_or_else(|| panic!("backlinks should be an array: {json}"));
    assert_eq!(backlinks.len(), 1, "full json: {json}");
    assert_eq!(backlinks[0]["source"], "other.md");
}

#[test]
fn named_gitignored_file_survives_fields_all() {
    let tmp = vault_with_gitignored_named_file();
    let json = run_json(&tmp, &["find", "--file", "MyNotes.md", "--fields", "all"]);
    assert_eq!(json["total"], 1, "full json: {json}");
    assert_eq!(json["results"][0]["file"], "MyNotes.md");
}

#[test]
fn summary_counts_the_gitignored_file_under_excluded() {
    let tmp = vault_with_gitignored_named_file();
    let json = run_json(&tmp, &["summary"]);
    assert_eq!(
        json["results"]["files"]["total"], 1,
        "the gitignored file is not part of the normal sweep total: {json}"
    );
    assert_eq!(
        json["results"]["files"]["excluded"], 1,
        "the gitignore drop should be counted like a [scan] exclude drop: {json}"
    );
}
