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

/// DEC-342 (review-round perf fix): `summary --index` must read the
/// gitignore-dropped count back from the snapshot header instead of paying
/// for another `.gitignore`-disabled disk walk on every read. Proved
/// indirectly but conclusively: delete the `.gitignore` *after*
/// `create-index`, then run `summary --index` again. A live walk would now
/// see `MyNotes.md` as no longer ignored and report `excluded: 0`; the
/// snapshot-replayed count still says `1`, proving the figure came from the
/// header `create-index` recorded, not a fresh walk.
#[test]
fn summary_index_replays_the_build_time_gitignore_count_without_rewalking() {
    let tmp = vault_with_gitignored_named_file();
    let dir = tmp.path().to_str().expect("utf-8 path");

    let create = hyalo_no_hints()
        .args(["--dir", dir, "create-index"])
        .output()
        .expect("create-index should run");
    assert!(
        create.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&create.stderr)
    );

    // Remove the only reason MyNotes.md was ever gitignored.
    std::fs::remove_file(tmp.path().join(".gitignore")).expect("remove .gitignore");

    let json = run_json(&tmp, &["summary", "--index"]);
    assert_eq!(
        json["results"]["files"]["excluded"], 1,
        "must replay the build-time count from the snapshot, not re-walk disk \
         (a fresh walk would now see no .gitignore at all): {json}"
    );
}

/// Review round finding (7): `find --index --file <gitignored>` used to
/// suggest `hyalo create-index` would "fold it in" — a promise that never
/// comes true for a file an ignore rule excludes, since the next
/// `create-index` drops it again. The suggestion must be absent for an
/// ignored named file and present for a genuinely new one.
#[test]
fn find_index_drops_the_fold_in_hint_for_an_ignored_named_file_but_keeps_it_for_a_new_one() {
    let tmp = vault_with_gitignored_named_file();
    let dir = tmp.path().to_str().expect("utf-8 path");

    let create = hyalo_no_hints()
        .args(["--dir", dir, "create-index"])
        .output()
        .expect("create-index should run");
    assert!(
        create.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&create.stderr)
    );

    // A genuinely new file, created after the index, is not ignored.
    write_md(tmp.path(), "brand-new.md", "# New\n");

    let ignored_output = hyalo_no_hints()
        .args(["--dir", dir, "find", "--index", "--file", "MyNotes.md"])
        .output()
        .expect("hyalo find should run");
    assert!(ignored_output.status.success());
    let ignored_stderr = String::from_utf8_lossy(&ignored_output.stderr);
    assert!(
        ignored_stderr.contains("absent from the snapshot index"),
        "the base note must still appear: {ignored_stderr}"
    );
    assert!(
        !ignored_stderr.contains("fold it in"),
        "a gitignored file will never be folded in by create-index: {ignored_stderr}"
    );

    let new_output = hyalo_no_hints()
        .args(["--dir", dir, "find", "--index", "--file", "brand-new.md"])
        .output()
        .expect("hyalo find should run");
    assert!(new_output.status.success());
    let new_stderr = String::from_utf8_lossy(&new_output.stderr);
    assert!(
        new_stderr.contains("fold it in"),
        "a genuinely new, non-ignored file should still get the create-index suggestion: {new_stderr}"
    );
}

/// Review-round regression (caught by the broader suite, pinned here
/// directly): a file the walk discovers but cannot *parse* (unparsable
/// frontmatter) must count under `results.files.skipped`, never under
/// `results.files.excluded` — `count_gitignore_dropped_against`'s respecting
/// set originally came from `index.entries()` alone, which also excludes
/// unparsed files, so every skip was briefly double-counted as a gitignore
/// drop too.
#[test]
fn unparsable_frontmatter_is_never_counted_as_gitignore_excluded() {
    let tmp = vault_with_gitignored_named_file();
    // `{{date}}` is a template expression, not YAML — this file is discovered
    // by the walk but never parses.
    std::fs::write(
        tmp.path().join("bad.md"),
        "---\ntitle: Bad\ncreated: {{date}}\n---\nBody.\n",
    )
    .expect("write bad frontmatter");

    let json = run_json(&tmp, &["summary"]);
    assert_eq!(
        json["results"]["files"]["skipped"], 1,
        "bad.md's unparsable frontmatter must count as skipped: {json}"
    );
    assert_eq!(
        json["results"]["files"]["excluded"], 1,
        "only MyNotes.md (gitignored) counts as excluded — bad.md must not: {json}"
    );
}
