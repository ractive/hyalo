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

/// Set a file's mtime one hour into the past, so it is safely older than
/// any snapshot built afterwards (not "racily clean").
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
            &format!("incrementword{i} content body text.\n"),
        );
        backdate(&tmp.path().join(rel));
    }
    tmp
}

/// Git's racily-clean rule (DEC-339): a file rewritten at the same size
/// within the snapshot's second keeps its recorded mtime, so size+mtime
/// cannot tell it changed. Such an entry is never reused.
#[test]
fn racily_clean_entry_is_rescanned_not_reused() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "old.md", "settled words\n");
    backdate(&tmp.path().join("old.md"));
    let same = tmp.path().join("same2.md");
    write_md(tmp.path(), "same2.md", "alpha zulu\n");
    let (_, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");
    // Same-size rewrite with the original mtime restored: invisible to size+mtime.
    let mtime = fs::metadata(&same).unwrap().modified().unwrap();
    write_md(tmp.path(), "same2.md", "omega zulu\n");
    fs::File::options()
        .write(true)
        .open(&same)
        .unwrap()
        .set_modified(mtime)
        .unwrap();
    let (json, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["results"]["reused"], 1, "{json}");
    assert_eq!(json["results"]["refreshed"], 1, "{json}");
    let indexed = find(&tmp, &["omega", "--index"]).0;
    assert_eq!(indexed["results"][0]["file"], "same2.md", "{indexed}");
    let stale = find(&tmp, &["alpha", "--index"]).0;
    assert_eq!(stale["results"], serde_json::json!([]), "{stale}");
}

/// Review item 3: one unparsable or deleted file must not leave the rest of
/// the vault stale. The repair drops it as a disk scan would, warns even
/// under `-q`, and patches every other drifted file.
#[test]
fn repair_drops_unparsable_and_deleted_files_and_patches_the_rest() {
    for quiet in [false, true] {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), "a.md", "getUserName lives here\n");
        write_md(tmp.path(), "b.md", "error handling notes\n");
        write_md(tmp.path(), "f2.md", "---\ntitle: ok\n---\nplain words\n");
        for f in ["a.md", "b.md", "f2.md"] {
            backdate(&tmp.path().join(f));
        }
        let (_, output) = create_index(&tmp, &[]);
        assert!(output.status.success(), "{output:?}");
        write_md(tmp.path(), "a.md", "renamedIdentifier lives here now\n");
        write_md(
            tmp.path(),
            "f2.md",
            "---\ntitle: [broken\n---\nplain words\n",
        );
        fs::remove_file(tmp.path().join("b.md")).unwrap();
        for query in [
            "getUserName",
            "renamedIdentifier",
            "\"error handling\"",
            "plain",
        ] {
            let mut args = vec![query, "--index"];
            if quiet {
                args.push("-q");
            }
            let (indexed, output) = find(&tmp, &args);
            assert!(output.status.success(), "{query} quiet={quiet}: {output:?}");
            let (disk, _) = find(&tmp, &[query]);
            assert_eq!(indexed["results"], disk["results"], "{query} quiet={quiet}");
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(stderr.contains("repaired in memory"), "{stderr}");
            assert!(stderr.contains("f2.md"), "{stderr}");
            assert!(stderr.contains("could not be read"), "{stderr}");
        }
        let summary = |extra: &[&str]| {
            let output = hyalo_no_hints()
                .arg("--dir")
                .arg(tmp.path())
                .args(["summary", "--format", "json"])
                .args(extra)
                .output()
                .unwrap();
            let mut json: Value = serde_json::from_slice(&output.stdout).unwrap();
            json["results"]["files"]["total"].take()
        };
        assert_eq!(summary(&["--index"]), summary(&[]));
    }
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
fn summary_index_reports_current_format_version() {
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
}

/// BUG-9 review follow-up (DEC-353): a v4 snapshot has no
/// `explicit_anchor_ids` field at all, and incremental `create-index`
/// reuses an unchanged entry verbatim (DEC-339) -- so if the format version
/// had NOT bumped, a v4 index upgraded to this binary and re-run through
/// plain `create-index` would keep serving a stale, anchor-blind entry
/// forever. Simulate exactly that pre-existing-index upgrade path: write a
/// real v5 snapshot, then roll its header back to 4 and strip
/// `explicit_anchor_ids` from the one entry that has it, matching what an
/// actual v4 binary would have written. Both a read (`find --broken-links
/// --index`) and `create-index` itself must refuse it rather than trust it.
#[test]
fn v4_snapshot_without_explicit_anchor_ids_is_refused_not_silently_served() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "target.md",
        "<a id=\"custom-x\"></a>\n\n# Target\n",
    );
    write_md(tmp.path(), "source.md", "[[target#custom-x]]\n");
    backdate(&tmp.path().join("target.md"));
    backdate(&tmp.path().join("source.md"));

    let (_, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");

    let index_path = tmp.path().join(".hyalo-index");
    let mut snapshot = load_snapshot(&index_path);
    snapshot["header"]["format_version"] = serde_json::json!(4);
    if let Some(entries) = snapshot.get_mut("entries").and_then(Value::as_array_mut) {
        for entry in entries {
            if let Some(obj) = entry.as_object_mut() {
                obj.remove("explicit_anchor_ids");
            }
        }
    }
    save_snapshot(&index_path, &snapshot);

    // A disk-scanning read sees the real anchor and reports nothing broken.
    let disk = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "--broken-links", "--limit", "0", "--format", "json"])
        .output()
        .unwrap();
    assert!(disk.status.success(), "{disk:?}");
    let disk_json: Value = serde_json::from_slice(&disk.stdout).unwrap();
    assert_eq!(disk_json["total"], 0, "{disk_json}");

    // The downgraded-and-anchor-stripped v4 snapshot must be refused and
    // fall back to disk -- never silently report `#custom-x` broken.
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args([
            "find",
            "--broken-links",
            "--limit",
            "0",
            "--index",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let indexed: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        indexed["total"], 0,
        "a v4-without-anchor-ids snapshot must fall back to disk, not report \
         #custom-x broken: {indexed}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("index format is older than this binary") && stderr.contains("v4"),
        "{stderr}"
    );

    // create-index over it rebuilds from scratch rather than reusing entries.
    let (json, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["results"]["rebuilt"], true, "{json}");
    assert_eq!(json["results"]["reused"], 0, "{json}");

    // And the freshly rebuilt index now agrees with disk under --index too.
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args([
            "find",
            "--broken-links",
            "--limit",
            "0",
            "--index",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let rebuilt: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rebuilt["total"], 0, "{rebuilt}");
}
