//! Iteration 313: index honesty and result caps.
//!
//! BUG-7: the "index format is older than this binary" refusal warning
//! survives `-q`, exactly like the missing-index fallback. BUG-8: a
//! snapshot built under a different `[search] code_blocks` than the
//! config is refused (not silently served from disk), and `summary
//! --index` exposes the snapshot's own `code_blocks`, `source` and
//! `index_format_version` whether or not the snapshot was used. A no-op
//! `create-index` rerun leaves the snapshot bytes untouched. UX-5: `find`'s
//! JSON envelope carries `truncated` whenever `--limit` cut the result
//! list short of `total`.

use super::common::{hyalo_no_hints, write_md};
use serde_json::Value;
use std::fs;
use tempfile::TempDir;

fn create_index(tmp: &TempDir, extra: &[&str]) -> (Value, std::process::Output) {
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .arg("create-index")
        .args(extra)
        .output()
        .unwrap();
    let json = serde_json::from_slice(&output.stdout).unwrap_or(Value::Null);
    (json, output)
}

/// Set a file's mtime one hour into the past, so it is safely older than any
/// snapshot built afterwards (not "racily clean" under DEC-339's tolerance).
fn backdate(path: &std::path::Path) {
    let past = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
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
        write_md(
            tmp.path(),
            &rel,
            &format!("honestyword{i} content body text.\n"),
        );
        backdate(&tmp.path().join(rel));
    }
    tmp
}

fn load_snapshot(path: &std::path::Path) -> Value {
    rmp_serde::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn save_snapshot(path: &std::path::Path, snapshot: &Value) {
    fs::write(path, rmp_serde::to_vec_named(snapshot).unwrap()).unwrap();
}

// ---------------------------------------------------------------------------
// BUG-7: the format-version refusal survives `-q`.
// ---------------------------------------------------------------------------

#[test]
fn old_format_refusal_warning_survives_quiet() {
    let tmp = five_file_vault();
    let (_, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");

    let index_path = tmp.path().join(".hyalo-index");
    let mut snapshot = load_snapshot(&index_path);
    snapshot["header"]["format_version"] = serde_json::json!(3);
    save_snapshot(&index_path, &snapshot);

    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args([
            "find",
            "honestyword1",
            "--index",
            "--quiet",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("index format is older than this binary") && stderr.contains("v3"),
        "the format-version refusal must be -q-proof like the missing-index \
         fallback already is: {stderr:?}"
    );
}

// ---------------------------------------------------------------------------
// BUG-8: a `[search] code_blocks` mismatch is refused, not silently served.
// ---------------------------------------------------------------------------

#[test]
fn code_blocks_mismatch_is_refused_with_a_quiet_proof_warning() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "doc.md",
        "Intro mentions `inlinecode` directly.\n\n```\nfencedsecretword here\n```\n",
    );
    backdate(&tmp.path().join("doc.md"));
    // Build the index under the default (`code_blocks = "index"`).
    let (_, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");

    // Switch the config to "skip" *after* the snapshot was built.
    write_md(
        tmp.path(),
        ".hyalo.toml",
        "[search]\ncode_blocks = \"skip\"\n",
    );

    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args([
            "find",
            "fencedsecretword",
            "--index",
            "--quiet",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("code_blocks")
            && stderr.contains("\"index\"")
            && stderr.contains("\"skip\""),
        "mismatch must name both settings and survive -q: {stderr:?}"
    );
    // Refused exactly like a disk scan under "skip": the fenced word is
    // dropped from the corpus.
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["results"], serde_json::json!([]), "{json}");
}

#[test]
fn summary_index_reports_code_blocks_and_source_when_used() {
    let tmp = five_file_vault();
    let (_, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");

    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["summary", "--index", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json["results"]["index_format_version"],
        hyalo_core::index::SNAPSHOT_FORMAT_VERSION,
        "{json}"
    );
    assert_eq!(json["results"]["code_blocks"], "index", "{json}");
    assert_eq!(json["results"]["source"], "index", "{json}");
}

#[test]
fn summary_index_reports_format_version_and_disk_source_when_refused() {
    let tmp = five_file_vault();
    let (_, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");

    let index_path = tmp.path().join(".hyalo-index");
    let mut snapshot = load_snapshot(&index_path);
    snapshot["header"]["format_version"] = serde_json::json!(3);
    save_snapshot(&index_path, &snapshot);

    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["summary", "--index", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    // BUG-7: before iter-313 this was `null` even though the refused
    // snapshot's own version was known a moment earlier in `run.rs`.
    assert_eq!(json["results"]["index_format_version"], 3, "{json}");
    assert_eq!(json["results"]["source"], "disk", "{json}");
}

// ---------------------------------------------------------------------------
// A no-op `create-index` rerun leaves the snapshot untouched.
// ---------------------------------------------------------------------------

#[test]
fn no_op_create_index_rerun_skips_the_write() {
    let tmp = five_file_vault();
    let (json, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["results"]["written"], true, "{json}");

    let index_path = tmp.path().join(".hyalo-index");
    let bytes_before = fs::read(&index_path).unwrap();
    let mtime_before = fs::metadata(&index_path).unwrap().modified().unwrap();

    // Nothing on disk changed since the first run.
    let (json, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["results"]["written"], false, "{json}");
    assert_eq!(json["results"]["reused"], 5, "{json}");
    assert_eq!(json["results"]["removed"], 0, "{json}");

    let bytes_after = fs::read(&index_path).unwrap();
    let mtime_after = fs::metadata(&index_path).unwrap().modified().unwrap();
    assert_eq!(
        bytes_before, bytes_after,
        "snapshot bytes must be untouched"
    );
    assert_eq!(
        mtime_before, mtime_after,
        "snapshot mtime must be untouched"
    );
}

#[test]
fn a_real_change_still_writes_and_reports_written_true() {
    let tmp = five_file_vault();
    let (_, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");

    write_md(tmp.path(), "f6.md", "a brand new honestyword6 file.\n");
    backdate(&tmp.path().join("f6.md"));

    let (json, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["results"]["written"], true, "{json}");
    assert_eq!(json["results"]["reused"], 5, "{json}");
    assert_eq!(json["results"]["refreshed"], 1, "{json}");
}

// ---------------------------------------------------------------------------
// UX-5: `find`'s JSON envelope carries `truncated` whenever `--limit` cut
// the result list.
// ---------------------------------------------------------------------------

fn ten_file_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    for i in 1..=10 {
        write_md(
            tmp.path(),
            &format!("n{i}.md"),
            &format!("---\nstatus: draft\n---\nfile number {i}\n"),
        );
    }
    tmp
}

#[test]
fn find_envelope_carries_truncated_true_when_limit_cuts_results() {
    let tmp = ten_file_vault();
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args([
            "find",
            "--property",
            "status=draft",
            "--limit",
            "3",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 10, "{json}");
    assert_eq!(json["results"].as_array().unwrap().len(), 3, "{json}");
    assert_eq!(json["truncated"], true, "{json}");
}

#[test]
fn find_envelope_has_no_truncated_true_with_limit_zero() {
    let tmp = ten_file_vault();
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args([
            "find",
            "--property",
            "status=draft",
            "--limit",
            "0",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 10, "{json}");
    assert_eq!(json["results"].as_array().unwrap().len(), 10, "{json}");
    assert_ne!(json["truncated"], serde_json::json!(true), "{json}");
}
