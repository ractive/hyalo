//! Iteration 339 (DEC-414): `create-index` decides a no-op before it decodes
//! the postings, clones the entries or builds the link graph.
//!
//! The decision is made by `early_no_op`; every condition it checks has a test
//! here that fails when the condition is dropped. The conditions that already
//! had a test in `iteration313_index_honesty` (a directory that moved, a newly
//! gitignored or unparsable note) keep it, and now run through the early path.

use super::common::{hyalo_no_hints, write_md};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

fn create_index(dir: &Path, extra: &[&str]) -> (Value, std::process::Output) {
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(dir)
        .arg("create-index")
        .args(extra)
        .output()
        .unwrap();
    let json = serde_json::from_slice(&output.stdout).unwrap_or(Value::Null);
    (json, output)
}

/// Set a file's mtime `secs` into the past, safely older than any snapshot
/// built afterwards (not "racily clean", DEC-339).
fn set_age(path: &Path, secs: u64) {
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(SystemTime::now() - Duration::from_secs(secs))
        .unwrap();
}

fn five_file_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    for i in 1..=5 {
        let rel = format!("f{i}.md");
        write_md(
            tmp.path(),
            &rel,
            &format!("earlynoopword{i} content body text.\n"),
        );
        set_age(&tmp.path().join(rel), 3600);
    }
    tmp
}

/// A vault already indexed once, and the results of a rerun that must have
/// been a no-op.
fn indexed_vault() -> TempDir {
    let tmp = five_file_vault();
    let (json, output) = create_index(tmp.path(), &[]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["results"]["written"], true, "{json}");
    tmp
}

fn assert_written(json: &Value, why: &str) {
    assert_eq!(json["results"]["written"], true, "{why}: {json}");
}

fn find_index(dir: &Path, word: &str) -> (Value, String) {
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(dir)
        .args(["find", word, "--index", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    (
        serde_json::from_slice(&output.stdout).unwrap(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

// ---------------------------------------------------------------------------
// The answer is the same as before, field for field.
// ---------------------------------------------------------------------------

#[test]
fn an_unchanged_rerun_reports_the_whole_result_and_touches_nothing() {
    let tmp = indexed_vault();
    let index = tmp.path().join(".hyalo-index");
    let bytes = fs::read(&index).unwrap();
    let mtime = fs::metadata(&index).unwrap().modified().unwrap();

    let (json, output) = create_index(tmp.path(), &[]);
    assert!(output.status.success(), "{output:?}");
    let results = &json["results"];
    assert_eq!(results["written"], false, "{json}");
    assert_eq!(results["rebuilt"], false, "{json}");
    assert_eq!(results["files_indexed"], 5, "{json}");
    assert_eq!(results["reused"], 5, "{json}");
    assert_eq!(results["refreshed"], 0, "{json}");
    assert_eq!(results["removed"], 0, "{json}");
    assert_eq!(results["skipped"], 0, "{json}");
    assert_eq!(results["warnings"], 0, "{json}");
    assert!(
        results["note"]
            .as_str()
            .is_some_and(|n| n.contains("nothing changed")),
        "{json}"
    );
    assert_eq!(fs::read(&index).unwrap(), bytes, "snapshot bytes");
    assert_eq!(
        fs::metadata(&index).unwrap().modified().unwrap(),
        mtime,
        "snapshot mtime"
    );
}

// ---------------------------------------------------------------------------
// Every way a note can differ from the snapshot defeats the early exit.
// ---------------------------------------------------------------------------

#[test]
fn a_note_with_another_mtime_and_the_same_size_is_rescanned() {
    let tmp = indexed_vault();
    // Same length, other word: only the mtime tells the two apart.
    let f3 = tmp.path().join("f3.md");
    fs::write(&f3, "earlynoopwordX content body text.\n").unwrap();
    set_age(&f3, 1800);

    let (json, _) = create_index(tmp.path(), &[]);
    assert_written(&json, "an mtime that moved");
    assert_eq!(json["results"]["reused"], 4, "{json}");
    assert_eq!(json["results"]["refreshed"], 1, "{json}");
    let (found, _) = find_index(tmp.path(), "earlynoopwordx");
    assert_eq!(
        found["total"], 1,
        "the rescan must reach the postings: {found}"
    );
}

#[test]
fn a_note_with_another_size_and_the_same_mtime_is_rescanned() {
    let tmp = indexed_vault();
    let f3 = tmp.path().join("f3.md");
    let mtime = fs::metadata(&f3).unwrap().modified().unwrap();
    fs::write(
        &f3,
        "earlynoopwordX content body text, and a longer tail.\n",
    )
    .unwrap();
    fs::File::options()
        .write(true)
        .open(&f3)
        .unwrap()
        .set_modified(mtime)
        .unwrap();

    let (json, _) = create_index(tmp.path(), &[]);
    assert_written(&json, "a size that moved");
    assert_eq!(json["results"]["refreshed"], 1, "{json}");
}

#[test]
fn an_added_note_is_indexed() {
    let tmp = indexed_vault();
    write_md(tmp.path(), "f6.md", "earlynoopword6 content body text.\n");
    set_age(&tmp.path().join("f6.md"), 3600);

    let (json, _) = create_index(tmp.path(), &[]);
    assert_written(&json, "an added note");
    assert_eq!(json["results"]["files_indexed"], 6, "{json}");
    assert_eq!(json["results"]["reused"], 5, "{json}");
}

#[test]
fn a_removed_note_is_dropped() {
    let tmp = indexed_vault();
    fs::remove_file(tmp.path().join("f3.md")).unwrap();

    let (json, _) = create_index(tmp.path(), &[]);
    assert_written(&json, "a removed note");
    assert_eq!(json["results"]["files_indexed"], 4, "{json}");
    assert_eq!(json["results"]["removed"], 1, "{json}");
}

#[test]
fn a_renamed_note_is_not_mistaken_for_the_original() {
    // Same number of notes, same sizes, same mtimes: only the name differs, so
    // the count shortcut cannot see it and the per-note check has to.
    let tmp = indexed_vault();
    fs::rename(tmp.path().join("f1.md"), tmp.path().join("g1.md")).unwrap();

    let (json, _) = create_index(tmp.path(), &[]);
    assert_written(&json, "a renamed note");
    assert_eq!(json["results"]["files_indexed"], 5, "{json}");
    assert_eq!(json["results"]["removed"], 1, "{json}");
    assert_eq!(json["results"]["refreshed"], 1, "{json}");
}

// ---------------------------------------------------------------------------
// A config change that alters what the snapshot header records.
// ---------------------------------------------------------------------------

#[test]
fn a_changed_scan_exclude_list_rewrites_the_header_even_when_no_note_moves() {
    let tmp = five_file_vault();
    let config = tmp.path().join(".hyalo.toml");
    fs::write(&config, "[scan]\nexclude = [\"nomatch-a/**\"]\n").unwrap();
    let (json, _) = create_index(tmp.path(), &[]);
    assert_written(&json, "first build");
    let (json, _) = create_index(tmp.path(), &[]);
    assert_eq!(json["results"]["written"], false, "{json}");

    // Rewritten in place: the vault's directories do not move. No note matches
    // either pattern, so every entry is still reusable.
    fs::write(&config, "[scan]\nexclude = [\"nomatch-b/**\"]\n").unwrap();
    let (json, _) = create_index(tmp.path(), &[]);
    assert_written(&json, "another [scan] exclude list");
    assert_eq!(json["results"]["reused"], 5, "{json}");
}

#[test]
fn a_changed_code_blocks_setting_rebuilds_the_postings() {
    let tmp = five_file_vault();
    let config = tmp.path().join(".hyalo.toml");
    fs::write(&config, "[search]\ncode_blocks = \"index\"\n").unwrap();
    let (json, _) = create_index(tmp.path(), &[]);
    assert_written(&json, "first build");

    fs::write(&config, "[search]\ncode_blocks = \"skip\"\n").unwrap();
    let (json, _) = create_index(tmp.path(), &[]);
    assert_written(&json, "another [search] code_blocks");
    assert_eq!(json["results"]["rebuilt"], true, "{json}");
}

#[test]
fn a_new_attachment_rewrites_the_header_even_when_no_note_moves() {
    let tmp = indexed_vault();
    fs::write(tmp.path().join("pic.png"), b"not really a png").unwrap();

    let (json, _) = create_index(tmp.path(), &[]);
    assert_written(&json, "an attachment the snapshot has never seen");
    assert_eq!(json["results"]["reused"], 5, "{json}");
}

#[test]
fn a_note_newly_hidden_by_an_existing_ignore_rule_rewrites_the_header() {
    // The rule file is in place before the first build, so the ignore-file
    // stamps match on the rerun: only the count of notes it hides moved.
    let tmp = five_file_vault();
    fs::write(tmp.path().join(".ignore"), "ignored-*.md\n").unwrap();
    let (json, _) = create_index(tmp.path(), &[]);
    assert_written(&json, "first build");
    let (json, _) = create_index(tmp.path(), &[]);
    assert_eq!(json["results"]["written"], false, "{json}");

    write_md(tmp.path(), "ignored-1.md", "hidden by the rule\n");
    let (json, _) = create_index(tmp.path(), &[]);
    assert_written(&json, "a note the rule now hides");
    assert_eq!(json["results"]["files_indexed"], 5, "{json}");
}

// ---------------------------------------------------------------------------
// Notes the previous build could not use are part of what "unchanged" means.
// ---------------------------------------------------------------------------

const BROKEN: &str = "---\nbroken: [unterminated\n---\nbody\n";

#[test]
fn an_unparsable_note_that_stays_unparsable_is_still_a_no_op() {
    let tmp = five_file_vault();
    write_md(tmp.path(), "broken.md", BROKEN);
    set_age(&tmp.path().join("broken.md"), 3600);
    let (json, _) = create_index(tmp.path(), &[]);
    assert_eq!(json["results"]["files_indexed"], 5, "{json}");
    assert_eq!(json["results"]["warnings"], 1, "{json}");
    assert_eq!(json["results"]["skipped"], 1, "{json}");

    let (json, _) = create_index(tmp.path(), &[]);
    let results = &json["results"];
    assert_eq!(results["written"], false, "{json}");
    assert_eq!(results["files_indexed"], 5, "{json}");
    assert_eq!(results["reused"], 5, "{json}");
    assert_eq!(results["warnings"], 1, "{json}");
    assert_eq!(results["skipped"], 1, "{json}");
}

#[test]
fn an_unparsable_note_that_was_fixed_is_indexed() {
    let tmp = five_file_vault();
    write_md(tmp.path(), "broken.md", BROKEN);
    set_age(&tmp.path().join("broken.md"), 3600);
    create_index(tmp.path(), &[]);

    let fixed = tmp.path().join("broken.md");
    fs::write(&fixed, "earlynoopword6 now it parses.\n").unwrap();
    set_age(&fixed, 1800);
    let (json, _) = create_index(tmp.path(), &[]);
    assert_written(&json, "a note that parses now");
    assert_eq!(json["results"]["files_indexed"], 6, "{json}");
    assert_eq!(json["results"]["skipped"], 0, "{json}");
}

#[test]
fn a_second_note_that_stops_parsing_is_not_a_no_op() {
    // One broken note was already recorded; another stops parsing. The notes
    // that parse and the notes that do not still add up to the same total.
    let tmp = five_file_vault();
    write_md(tmp.path(), "broken.md", BROKEN);
    set_age(&tmp.path().join("broken.md"), 3600);
    create_index(tmp.path(), &[]);

    fs::write(tmp.path().join("f2.md"), BROKEN).unwrap();
    set_age(&tmp.path().join("f2.md"), 3600);
    let (json, _) = create_index(tmp.path(), &[]);
    assert_written(&json, "a second note stopped parsing");
    assert_eq!(json["results"]["files_indexed"], 4, "{json}");
    assert_eq!(json["results"]["skipped"], 2, "{json}");
}

#[test]
fn a_note_that_is_not_utf8_keeps_its_warning_on_a_no_op() {
    let tmp = five_file_vault();
    let bad = tmp.path().join("bad.md");
    fs::write(&bad, b"---\ntitle: bad\n---\nbefore \xff\xfe after\n").unwrap();
    set_age(&bad, 3600);
    let (json, _) = create_index(tmp.path(), &[]);
    assert_eq!(json["results"]["warnings"], 1, "{json}");

    let (json, _) = create_index(tmp.path(), &[]);
    assert_eq!(json["results"]["written"], false, "{json}");
    assert_eq!(
        json["results"]["warnings"], 1,
        "the no-op must count the same warning the build did: {json}"
    );
    assert_eq!(json["results"]["files_indexed"], 6, "{json}");
}

// ---------------------------------------------------------------------------
// DEC-414: a torn snapshot is not a no-op, and the reader names the cure.
// ---------------------------------------------------------------------------

#[test]
fn a_truncated_postings_section_is_rebuilt_by_a_plain_rerun() {
    let tmp = indexed_vault();
    let index = tmp.path().join(".hyalo-index");
    let len = fs::metadata(&index).unwrap().len();
    fs::File::options()
        .write(true)
        .open(&index)
        .unwrap()
        .set_len(len - 40)
        .unwrap();

    // The reader degrades to a live scan, says why, and names the cure.
    let (found, stderr) = find_index(tmp.path(), "earlynoopword1");
    assert_eq!(found["total"], 1, "{found}");
    assert!(stderr.contains("unreadable BM25 section"), "{stderr}");
    assert!(stderr.contains("create-index --force"), "{stderr}");

    // A plain rerun is the cure for a torn file; it is not a no-op.
    let (json, output) = create_index(tmp.path(), &[]);
    assert!(output.status.success(), "{output:?}");
    assert_written(&json, "a truncated snapshot");
    assert_eq!(json["results"]["rebuilt"], true, "{json}");

    let (found, stderr) = find_index(tmp.path(), "earlynoopword1");
    assert_eq!(found["total"], 1, "{found}");
    assert!(
        !stderr.contains("BM25"),
        "the repaired snapshot must read without a warning: {stderr}"
    );
    let (json, _) = create_index(tmp.path(), &[]);
    assert_eq!(json["results"]["written"], false, "{json}");
}

#[test]
fn force_rebuilds_a_snapshot_a_no_op_would_leave_alone() {
    let tmp = indexed_vault();
    let (json, _) = create_index(tmp.path(), &["--force"]);
    assert_written(&json, "--force");
    assert_eq!(json["results"]["rebuilt"], true, "{json}");
    assert_eq!(json["results"]["reused"], 0, "{json}");
}
