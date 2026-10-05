//! Iteration 317: backlog leftovers before 0.25.0.
//!
//! DEC-368: `create-index` stamps the snapshot's `created_at` before the
//! serialize-fsync-rename tail, and the rename bumps the directory holding
//! the snapshot. A tail slower than the one-second tolerance used to make
//! every later `create-index` rewrite the snapshot (DEC-361's no-op guard saw
//! a "moved tree") and every `--index` read re-walk the vault. Now the publish
//! re-stamps the snapshot file's mtime after the rename, and a bump of the
//! index's own directory that coincides with that mtime is attributed to the
//! publish. Every other directory, and any bump that does not coincide, still
//! compares against `created_at` — so a new note is never hidden by a later
//! `touch` of the index file.

use super::common::{hyalo_no_hints, write_md};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

fn create_index(tmp: &TempDir, extra: &[&str]) -> (Value, std::process::Output) {
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["create-index", "--format", "json"])
        .args(extra)
        .output()
        .unwrap();
    let json = serde_json::from_slice(&output.stdout).unwrap_or(Value::Null);
    (json, output)
}

/// `find <word> --index [extra]`: the matched files and stderr.
fn find_indexed(tmp: &TempDir, word: &str, extra: &[&str]) -> (Vec<String>, String) {
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", word, "--index", "--format", "json"])
        .args(extra)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    let files = json["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["file"].as_str().unwrap().to_owned())
        .collect();
    (files, String::from_utf8_lossy(&output.stderr).into_owned())
}

fn set_mtime(path: &Path, when: SystemTime) {
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(when)
        .unwrap();
}

/// `touch` the index file: refresh its mtime without rebuilding it.
fn touch(path: &Path) {
    set_mtime(path, SystemTime::now());
}

fn five_file_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    for i in 1..=5 {
        let rel = format!("f{i}.md");
        write_md(tmp.path(), &rel, &format!("leftoverword{i} body text.\n"));
        set_mtime(
            &tmp.path().join(rel),
            SystemTime::now() - Duration::from_hours(1),
        );
    }
    tmp
}

fn secs(t: SystemTime) -> u64 {
    t.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()
}

/// Sleep past the whole-second tolerance (DEC-302's ~2 s blind spot).
fn cross_tolerance() {
    std::thread::sleep(Duration::from_millis(2100));
}

const REPAIR_NOTE: &str = "changed on disk since the index was built";

/// Rewrite the snapshot so its header claims a `created_at` two seconds
/// before the vault root's mtime (the rename's bump), with the snapshot
/// file's own mtime at that root mtime — what a two-second
/// serialize-fsync-rename tail leaves once the publish re-stamps the file.
fn fake_slow_publish_tail(tmp: &TempDir) {
    let index_path = tmp.path().join(".hyalo-index");
    let root_mtime = fs::metadata(tmp.path()).unwrap().modified().unwrap();
    let root_secs = secs(root_mtime);
    let mut snapshot: Value = rmp_serde::from_slice(&fs::read(&index_path).unwrap()).unwrap();
    snapshot["header"]["created_at"] = serde_json::json!(root_secs - 2);
    // Overwriting an existing file in place does not touch its directory.
    fs::write(&index_path, rmp_serde::to_vec_named(&snapshot).unwrap()).unwrap();
    set_mtime(&index_path, root_mtime);
    assert_eq!(
        secs(fs::metadata(tmp.path()).unwrap().modified().unwrap()),
        root_secs,
        "the fake must not move the vault root itself"
    );
}

#[test]
fn slow_publish_tail_keeps_the_rerun_a_no_op() {
    let tmp = five_file_vault();
    let (json, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["results"]["written"], true, "{json}");

    fake_slow_publish_tail(&tmp);
    let index_path = tmp.path().join(".hyalo-index");
    let bytes_before = fs::read(&index_path).unwrap();

    for run in 0..2 {
        let (json, output) = create_index(&tmp, &[]);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            json["results"]["written"], false,
            "run {run}: a publish tail slower than the tolerance must not read as a moved tree: {json}"
        );
        assert_eq!(json["results"]["reused"], 5, "{json}");
    }
    assert_eq!(
        fs::read(&index_path).unwrap(),
        bytes_before,
        "the no-op must leave the snapshot bytes untouched"
    );

    let (files, stderr) = find_indexed(&tmp, "leftoverword3", &[]);
    assert_eq!(files, vec!["f3.md"]);
    assert!(
        !stderr.contains("index older than vault") && !stderr.contains(REPAIR_NOTE),
        "{stderr}"
    );
}

#[test]
fn touching_a_slow_tail_snapshot_heals_with_one_write() {
    let tmp = five_file_vault();
    let (_, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");
    fake_slow_publish_tail(&tmp);

    // A touch well after the root bump: the bump no longer coincides with
    // the index file, so it is a moved tree once — the rewrite re-stamps the
    // file and the next rerun is a no-op again.
    cross_tolerance();
    touch(&tmp.path().join(".hyalo-index"));
    let (json, _) = create_index(&tmp, &[]);
    assert_eq!(json["results"]["written"], true, "{json}");
    let (json, _) = create_index(&tmp, &[]);
    assert_eq!(json["results"]["written"], false, "{json}");
}

#[test]
fn a_real_directory_change_after_a_slow_tail_still_forces_a_write() {
    let tmp = five_file_vault();
    let (_, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");
    fake_slow_publish_tail(&tmp);

    cross_tolerance();
    fs::create_dir(tmp.path().join("x")).unwrap();
    fs::remove_dir(tmp.path().join("x")).unwrap();

    let (json, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        json["results"]["written"], true,
        "a directory that moved after the snapshot was published must still force a write: {json}"
    );
}

/// The reviewer's repro: a touch of the index file must not hide a note
/// added to a subdirectory after the snapshot was built.
#[test]
fn new_subdirectory_note_is_found_after_the_index_is_touched() {
    let tmp = five_file_vault();
    let (_, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");

    cross_tolerance();
    write_md(tmp.path(), "sub/d.md", "zebraword lives here.\n");
    touch(&tmp.path().join(".hyalo-index"));

    let (files, stderr) = find_indexed(&tmp, "zebraword", &[]);
    assert_eq!(files, vec!["sub/d.md"], "{stderr}");
    assert!(stderr.contains(REPAIR_NOTE), "{stderr}");
}

/// A root-level note added seconds after a touch does not coincide with the
/// index file's mtime, so the root bump still counts.
#[test]
fn new_root_note_after_a_touch_is_found() {
    let tmp = five_file_vault();
    let (_, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");

    cross_tolerance();
    touch(&tmp.path().join(".hyalo-index"));
    std::thread::sleep(Duration::from_secs(3));
    write_md(tmp.path(), "n4.md", "zebraword at the root.\n");

    let (files, stderr) = find_indexed(&tmp, "zebraword", &[]);
    assert_eq!(files, vec!["n4.md"], "{stderr}");
    assert!(stderr.contains(REPAIR_NOTE), "{stderr}");
}

/// A future-dated index mtime coincides with nothing, so it excuses nothing.
#[test]
fn future_dated_index_mtime_does_not_hide_a_new_note() {
    let tmp = five_file_vault();
    let (_, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");

    cross_tolerance();
    write_md(tmp.path(), "n4.md", "zebraword at the root.\n");
    set_mtime(
        &tmp.path().join(".hyalo-index"),
        SystemTime::now() + Duration::from_hours(24),
    );

    let (files, stderr) = find_indexed(&tmp, "zebraword", &[]);
    assert_eq!(files, vec!["n4.md"], "{stderr}");
    assert!(stderr.contains(REPAIR_NOTE), "{stderr}");
}

/// With `--index-file` outside the vault, no vault directory is exempt: a
/// root note added at the same instant the index file is touched is found.
#[test]
fn index_file_outside_the_vault_excuses_no_directory() {
    let tmp = five_file_vault();
    let outside = TempDir::new().unwrap();
    let index_path = outside.path().join("vault.idx");
    let index_arg = index_path.to_str().unwrap();
    let (json, output) = create_index(&tmp, &["--output", index_arg, "--allow-outside-vault"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["results"]["written"], true, "{json}");

    cross_tolerance();
    write_md(tmp.path(), "n4.md", "zebraword at the root.\n");
    touch(&index_path);

    let (files, stderr) = find_indexed(&tmp, "zebraword", &["--index-file", index_arg]);
    assert_eq!(files, vec!["n4.md"], "{stderr}");
    assert!(stderr.contains(REPAIR_NOTE), "{stderr}");
}
