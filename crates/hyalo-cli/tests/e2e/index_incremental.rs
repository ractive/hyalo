//! Incremental `create-index` (DEC-339): reuse unchanged entries, patch in
//! changed/new/removed files, `--force`, snapshot format refusal, and the
//! stale-repair-in-memory note on `find --index`.

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

fn find(tmp: &TempDir, args: &[&str]) -> (Value, std::process::Output) {
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find"])
        .args(args)
        .args(["--format", "json"])
        .output()
        .unwrap();
    let json = serde_json::from_slice(&output.stdout).unwrap_or(Value::Null);
    (json, output)
}

fn five_file_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    for i in 1..=5 {
        write_md(
            tmp.path(),
            &format!("f{i}.md"),
            &format!("incrementword{i} content body text.\n"),
        );
    }
    tmp
}

#[test]
fn first_build_is_a_full_rebuild() {
    let tmp = five_file_vault();
    let (json, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["results"]["rebuilt"], true, "{json}");
    assert_eq!(json["results"]["reused"], 0, "{json}");
    assert_eq!(json["results"]["removed"], 0, "{json}");
    assert_eq!(json["results"]["refreshed"], 5, "{json}");
}

#[test]
fn unchanged_second_build_reuses_everything() {
    let tmp = five_file_vault();
    let (_, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");

    let (json, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["results"]["rebuilt"], false, "{json}");
    assert_eq!(json["results"]["reused"], 5, "{json}");
    assert_eq!(json["results"]["refreshed"], 0, "{json}");
    assert_eq!(json["results"]["removed"], 0, "{json}");
}

#[test]
fn edit_delete_and_add_refresh_exactly_what_changed() {
    let tmp = five_file_vault();
    let (_, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");

    // Edit f1.md: change its SIZE (don't rely on mtime granularity).
    write_md(
        tmp.path(),
        "f1.md",
        "incrementword1 content body text, now with more words appended.\n",
    );
    // Delete f2.md.
    fs::remove_file(tmp.path().join("f2.md")).unwrap();
    // Add a new file f6.md.
    write_md(tmp.path(), "f6.md", "incrementword6 content body text.\n");

    let (json, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["results"]["rebuilt"], false, "{json}");
    assert_eq!(json["results"]["removed"], 1, "{json}");
    // f1 (edited) + f6 (new) = 2 refreshed; f3, f4, f5 reused.
    assert_eq!(json["results"]["refreshed"], 2, "{json}");
    assert_eq!(json["results"]["reused"], 3, "{json}");

    let disk = find(&tmp, &["incrementword*"]).0;
    let indexed = find(&tmp, &["incrementword*", "--index"]).0;
    assert_eq!(disk["results"], indexed["results"], "{disk} vs {indexed}");
    let disk_files: Vec<&str> = disk["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["file"].as_str().unwrap())
        .collect();
    assert!(!disk_files.contains(&"f2.md"), "{disk_files:?}");
    assert!(disk_files.contains(&"f6.md"), "{disk_files:?}");
}

#[test]
fn force_always_rebuilds_from_scratch() {
    let tmp = five_file_vault();
    let (_, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");

    let (json, output) = create_index(&tmp, &["--force"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["results"]["rebuilt"], true, "{json}");
    assert_eq!(json["results"]["reused"], 0, "{json}");
}

/// Load the msgpack snapshot into a `serde_json::Value`, mutate it, and write
/// it back — the pattern already used by iteration296_review_followups.rs.
fn load_snapshot(path: &std::path::Path) -> Value {
    rmp_serde::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn save_snapshot(path: &std::path::Path, snapshot: &Value) {
    fs::write(path, rmp_serde::to_vec_named(snapshot).unwrap()).unwrap();
}

#[test]
fn v3_snapshot_is_refused_by_find_and_rebuilt_by_create_index() {
    let tmp = five_file_vault();
    let (_, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");

    let index_path = tmp.path().join(".hyalo-index");
    let mut snapshot = load_snapshot(&index_path);
    snapshot["header"]["format_version"] = serde_json::json!(3);
    save_snapshot(&index_path, &snapshot);

    let disk = find(&tmp, &["incrementword1"]).0;
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "incrementword1", "--index", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let indexed: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(disk["results"], indexed["results"], "{disk} vs {indexed}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("index format is older than this binary")
            && stderr.contains("v3")
            && stderr.contains("falling back to disk scan"),
        "{stderr}"
    );

    // create-index over the stale-format file rebuilds rather than reusing it.
    let (json, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["results"]["rebuilt"], true, "{json}");
    assert_eq!(json["results"]["reused"], 0, "{json}");
}

#[test]
fn stale_entry_is_repaired_in_memory_with_an_unsuppressible_note() {
    let tmp = five_file_vault();
    let (_, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");
    let index_path = tmp.path().join(".hyalo-index");
    let before_bytes = fs::read(&index_path).unwrap();

    // Change f1.md's size on disk, without touching the index.
    write_md(
        tmp.path(),
        "f1.md",
        "incrementword1 content body text, now with freshnewword added.\n",
    );

    // Plain run: fresh content is returned.
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "freshnewword", "--index", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 1, "{json}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("changed on disk since the index was built")
            && stderr.contains("f1.md")
            && stderr.contains("repaired in memory"),
        "{stderr}"
    );

    // -q does NOT suppress this note.
    let output_q = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "freshnewword", "--index", "--format", "json", "-q"])
        .output()
        .unwrap();
    assert!(output_q.status.success(), "{output_q:?}");
    let stderr_q = String::from_utf8_lossy(&output_q.stderr);
    assert!(
        stderr_q.contains("changed on disk since the index was built")
            && stderr_q.contains("repaired in memory"),
        "{stderr_q}"
    );

    // The on-disk index file is untouched by the read.
    let after_bytes = fs::read(&index_path).unwrap();
    assert_eq!(before_bytes, after_bytes);
}

/// A vault with titles, headings, tags, aliases, a code fence and an
/// identifier, exercised through eight query shapes — plain, phrase, slop
/// phrase, OR/negation, prefix, a field term, section granularity and a
/// facet — to pin disk/`--index` parity across every new v4 feature at once.
fn parity_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        "---\ntitle: Configuration Guide\ntags: [config, alpha]\naliases: [Config Doc]\n---\n# Configuration Guide\n\nconfiguration details and getUserName identifier.\n\n```\nfencedterm sample\n```\n\n## Install Steps\n\nmore alpha beta words here\n",
    );
    write_md(
        tmp.path(),
        "b.md",
        "---\ntitle: Other\ntags: [beta]\n---\n# Other\nbeta gamma alpha text\n",
    );
    tmp
}

#[test]
fn index_and_disk_agree_across_eight_query_shapes() {
    let tmp = parity_vault();
    let (_, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");

    let plain_queries = [
        "alpha",
        "\"alpha beta\"",
        "\"alpha beta\"~5",
        "alpha OR gamma -beta",
        "conf*",
        "title:configuration",
    ];
    for query in plain_queries {
        let disk = find(&tmp, &[query]).0;
        let indexed = find(&tmp, &[query, "--index"]).0;
        assert_eq!(disk["results"], indexed["results"], "{query}");
    }

    let disk_section = find(&tmp, &["alpha", "--granularity", "section"]).0;
    let indexed_section = find(&tmp, &["alpha", "--granularity", "section", "--index"]).0;
    assert_eq!(
        disk_section["results"], indexed_section["results"],
        "section granularity"
    );

    let disk_facet = find(&tmp, &["alpha", "--facet", "tags"]).0;
    let indexed_facet = find(&tmp, &["alpha", "--facet", "tags", "--index"]).0;
    assert_eq!(
        disk_facet["results"], indexed_facet["results"],
        "facet results"
    );
    assert_eq!(
        disk_facet["facets"], indexed_facet["facets"],
        "facet buckets"
    );
}

#[test]
fn summary_index_reports_format_version_four() {
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
    assert_eq!(json["results"]["index_format_version"], 4, "{json}");
}
