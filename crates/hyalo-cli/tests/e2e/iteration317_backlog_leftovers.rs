//! Iteration 317: backlog leftovers before 0.25.0.
//!
//! DEC-368: `create-index` stamps the snapshot's `created_at` before the
//! serialize-fsync-rename tail, and the rename bumps the vault root's mtime.
//! A tail slower than the one-second tolerance used to make every later
//! `create-index` rewrite the snapshot (DEC-361's no-op guard saw a "moved
//! tree") and every `--index` read re-walk the vault. The directory probe now
//! compares against the later of `created_at` and the snapshot file's own
//! mtime.

use super::common::{hyalo_no_hints, write_md};
use serde_json::Value;
use std::fs;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

fn create_index(tmp: &TempDir) -> (Value, std::process::Output) {
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["create-index", "--format", "json"])
        .output()
        .unwrap();
    let json = serde_json::from_slice(&output.stdout).unwrap_or(Value::Null);
    (json, output)
}

fn backdate(path: &std::path::Path) {
    let past = SystemTime::now() - Duration::from_secs(3600);
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(past)
        .unwrap();
}

fn five_file_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    for i in 1..=5 {
        let rel = format!("f{i}.md");
        write_md(tmp.path(), &rel, &format!("leftoverword{i} body text.\n"));
        backdate(&tmp.path().join(rel));
    }
    tmp
}

fn secs(t: SystemTime) -> u64 {
    t.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()
}

/// Rewrite the snapshot so its header claims a `created_at` two seconds
/// before the vault root's mtime (the rename's bump), then put the snapshot
/// file's own mtime at that root mtime — exactly what a two-second
/// serialize-fsync-rename tail leaves behind. Returns the root mtime.
fn fake_slow_publish_tail(tmp: &TempDir) -> u64 {
    let index_path = tmp.path().join(".hyalo-index");
    let root_mtime = fs::metadata(tmp.path()).unwrap().modified().unwrap();
    let root_secs = secs(root_mtime);
    let mut snapshot: Value = rmp_serde::from_slice(&fs::read(&index_path).unwrap()).unwrap();
    snapshot["header"]["created_at"] = serde_json::json!(root_secs - 2);
    // Overwriting an existing file in place does not touch its directory.
    fs::write(&index_path, rmp_serde::to_vec_named(&snapshot).unwrap()).unwrap();
    fs::File::options()
        .write(true)
        .open(&index_path)
        .unwrap()
        .set_modified(root_mtime)
        .unwrap();
    assert_eq!(
        secs(fs::metadata(tmp.path()).unwrap().modified().unwrap()),
        root_secs,
        "the fake must not move the vault root itself"
    );
    root_secs
}

#[test]
fn slow_publish_tail_keeps_the_rerun_a_no_op() {
    let tmp = five_file_vault();
    let (json, output) = create_index(&tmp);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["results"]["written"], true, "{json}");

    fake_slow_publish_tail(&tmp);
    let index_path = tmp.path().join(".hyalo-index");
    let bytes_before = fs::read(&index_path).unwrap();

    for run in 0..2 {
        let (json, output) = create_index(&tmp);
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

    // An `--index` read is served from the snapshot with no drift note.
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "leftoverword3", "--index", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("index older than vault") && !stderr.contains("changed on disk"),
        "{stderr}"
    );
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"][0]["file"], "f3.md", "{json}");
}

#[test]
fn a_real_directory_change_after_a_slow_tail_still_forces_a_write() {
    let tmp = five_file_vault();
    let (_, output) = create_index(&tmp);
    assert!(output.status.success(), "{output:?}");
    fake_slow_publish_tail(&tmp);

    // Cross the snapshot file's mtime plus the tolerance, then move the tree.
    std::thread::sleep(Duration::from_millis(2100));
    fs::create_dir(tmp.path().join("x")).unwrap();
    fs::remove_dir(tmp.path().join("x")).unwrap();

    let (json, output) = create_index(&tmp);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        json["results"]["written"], true,
        "a directory that moved after the snapshot was published must still force a write: {json}"
    );
}
