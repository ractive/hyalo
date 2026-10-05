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
fn summary_index_reports_code_blocks_and_source_after_in_memory_drift_repair() {
    // PR #379 review (SHOULD-FIX #6): a drifted file that DEC-339 repairs
    // in memory is not a refusal — the snapshot itself is still accepted
    // and used, just with one entry re-scanned on the fly. `code_blocks`/
    // `source` must keep reporting the snapshot's own values ("index"),
    // not fall back to "disk", in that case.
    let tmp = five_file_vault();
    let (_, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");

    // Edit a file's content without rerunning create-index: the snapshot
    // now disagrees with disk for this one entry.
    write_md(
        tmp.path(),
        "f1.md",
        "edited honestyword1 content, now different.\n",
    );

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
        "drift repair must not look like a refusal: {json}"
    );
    assert_eq!(json["results"]["code_blocks"], "index", "{json}");
    assert_eq!(
        json["results"]["source"], "index",
        "a drifted-but-repaired snapshot is still the index, not a disk fallback: {json}"
    );
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

// ---------------------------------------------------------------------------
// PR #379 review, MUST-FIX #1: three no-op cases the original check missed.
// ---------------------------------------------------------------------------

#[test]
fn no_op_check_catches_a_file_dropped_by_broken_frontmatter() {
    // f2.md's frontmatter breaks between runs: it is still "discovered" on
    // disk (not `removed`), but no longer scannable, so it drops out of
    // `entries` — `reused == file_count` alone cannot see that, because
    // both sides shrink together. `prev.entries().len() == file_count`
    // catches it.
    let tmp = five_file_vault();
    let (json, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["results"]["files_indexed"], 5, "{json}");

    write_md(
        tmp.path(),
        "f2.md",
        "---\nbroken: [unterminated\n---\nbody\n",
    );
    // Not backdated: a changed file must look changed to be rescanned at
    // all (mtime/size must differ from the stored entry).

    let (json, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        json["results"]["written"], true,
        "a dropped entry must never be mistaken for a no-op: {json}"
    );
    assert_eq!(json["results"]["files_indexed"], 4, "{json}");
}

#[test]
fn no_op_check_catches_a_newly_unparsable_note() {
    // A brand new file with broken frontmatter is never "removed" (it was
    // never in the previous snapshot) and never shrinks `file_count` below
    // what reuse alone would already produce (reuse only ever covers the
    // *old* files) — the previous header's own `skipped` list must be
    // compared to this run's, or the new skip is invisible.
    let tmp = five_file_vault();
    let (_, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");

    write_md(
        tmp.path(),
        "f6.md",
        "---\nbroken: [unterminated\n---\nbody\n",
    );

    let (json, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        json["results"]["written"], true,
        "a newly-skipped file must never be mistaken for a no-op: {json}"
    );
    assert_eq!(json["results"]["warnings"], 1, "{json}");
}

#[test]
fn no_op_check_catches_a_newly_gitignored_file() {
    // An `.ignore`d new file changes `gitignore_dropped` without touching
    // `reused`/`removed`/`file_count` at all — the previous header's own
    // gitignore-dropped count must be compared to this run's, or
    // `summary --index` keeps replaying the stale (lower) figure forever.
    let tmp = five_file_vault();
    let (_, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");

    write_md(tmp.path(), ".ignore", "ign/\n");
    write_md(tmp.path(), "ign/x.md", "ignored content\n");

    let (json, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        json["results"]["written"], true,
        "a newly-gitignored file must never be mistaken for a no-op: {json}"
    );

    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["summary", "--index", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json["results"]["files"]["excluded"], 1,
        "summary --index must replay the fresh gitignore-dropped count, not a stale 0: {json}"
    );
}

// ---------------------------------------------------------------------------
// PR #379 review, MUST-FIX #2: a no-op must not strand the stale-index hint
// forever when the vault's directory tree moved without any file actually
// changing (e.g. `mkdir x; rmdir x`).
// ---------------------------------------------------------------------------

#[test]
fn dir_mtime_bump_defeats_the_no_op_and_refreshes_the_snapshot() {
    // The stale-index *hint*'s wording (`snapshot_stale`) is covered at the
    // unit level in `hints::tests` (it only fires on a >500-file vault,
    // impractical to reproduce here); this e2e test pins the mechanism the
    // hint depends on: `create-index` must not take the no-op shortcut when
    // a directory moved since the snapshot was built, even if no file's own
    // content actually changed, or the snapshot's `created_at` (and its own
    // on-disk mtime) would stay behind the vault forever and the cheap
    // probe would never clear.
    let tmp = five_file_vault();
    let (_, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");
    let index_path = tmp.path().join(".hyalo-index");
    let mtime_before = fs::metadata(&index_path).unwrap().modified().unwrap();

    // Cross a whole-second boundary so the directory-mtime bump below is
    // unambiguously newer than the snapshot's `created_at` (whole-second
    // granularity plus DEC-280's 1-second tolerance).
    std::thread::sleep(std::time::Duration::from_millis(2100));
    std::fs::create_dir(tmp.path().join("x")).unwrap();
    std::fs::remove_dir(tmp.path().join("x")).unwrap();

    let (json, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        json["results"]["written"], true,
        "a dirty cheap probe must force a real write even when nothing else changed: {json}"
    );
    let mtime_after = fs::metadata(&index_path).unwrap().modified().unwrap();
    assert!(
        mtime_after > mtime_before,
        "the snapshot must be freshly written, not left behind the vault"
    );

    // Immediately rerunning now confirms a clean no-op again: the dir-mtime
    // bump's effect on the probe was fully resolved by the one real write.
    let (json, output) = create_index(&tmp, &[]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(json["results"]["written"], false, "{json}");
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

// ---------------------------------------------------------------------------
// PR #379 review, SHOULD-FIX #5: `truncated` is envelope-wide (any bare-array
// `results`), not find-only — and never appears when `results` is an object,
// even though that command's own cap did apply (`backlinks`).
// ---------------------------------------------------------------------------

#[test]
fn properties_and_terms_envelopes_also_carry_truncated() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        "---\nalpha: 1\nbeta: 2\ngamma: 3\n---\napple banana cherry\n",
    );
    write_md(
        tmp.path(),
        "b.md",
        "---\nalpha: 1\n---\ndate elderberry fig\n",
    );

    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["properties", "--limit", "1", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 3, "3 distinct property names: {json}");
    assert_eq!(json["results"].as_array().unwrap().len(), 1, "{json}");
    assert_eq!(json["truncated"], true, "{json}");

    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["terms", "--limit", "1", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        json["total"].as_u64().unwrap() > 1,
        "expected more than 1 distinct term: {json}"
    );
    assert_eq!(json["results"].as_array().unwrap().len(), 1, "{json}");
    assert_eq!(json["truncated"], true, "{json}");
}

#[test]
fn backlinks_envelope_never_carries_truncated_despite_its_own_cap() {
    // `backlinks`'s own `results` is an object (`{file, backlinks: [...]}`),
    // not a bare array, so the generic envelope-level `truncated` never
    // applies to it — its internal cap is invisible at the envelope level
    // on purpose (DEC-307-era precedent: `total` there already describes
    // the whole backlink count, not `results` itself).
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "target.md", "the target note\n");
    write_md(tmp.path(), "a.md", "links to [[target]]\n");
    write_md(tmp.path(), "b.md", "also links to [[target]]\n");

    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["backlinks", "target.md", "--limit", "1", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 2, "{json}");
    assert_eq!(
        json["results"]["backlinks"].as_array().unwrap().len(),
        1,
        "{json}"
    );
    assert!(
        json.get("truncated").is_none(),
        "backlinks' object-shaped results must never carry the envelope-level \
         truncated key, even though its own cap applied: {json}"
    );
}
