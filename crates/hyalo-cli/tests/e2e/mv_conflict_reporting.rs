/// Codebase review 2026-10-03, item 6: `mv --on-conflict skip` text output,
/// hints, and batch no-op reporting.
use super::common::{hyalo, hyalo_no_hints, write_md};
use tempfile::TempDir;

/// Single-file `--on-conflict skip` used to fall through to the generic
/// key-value dump in text mode (no dedicated shape in
/// `output/filters.rs`), because the JSON shape it produces (`skipped`
/// present, no `skipped_ambiguous`/`frontmatter_links_skipped`) matched none
/// of `MV_RESULT_FILTER`'s signatures. It now gets its own filter and says
/// plainly that nothing moved, instead of a bare field listing.
#[test]
fn single_file_on_conflict_skip_has_a_proper_text_rendering() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "notes/a.md", "hello\n");
    write_md(tmp.path(), "archive/a.md", "existing\n");
    let dir = tmp.path().to_str().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", dir])
        .args([
            "mv",
            "notes/a.md",
            "archive/a.md",
            "--on-conflict",
            "skip",
            "--format",
            "text",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("not moved")
            && stdout.contains("notes/a.md")
            && stdout.contains("archive/a.md"),
        "expected a plain-language skip message, got a raw dump: {stdout:?}"
    );
    // The old raw dump printed every struct field name verbatim on its own
    // line; make sure that shape is actually gone.
    assert!(
        !stdout.contains("total_files_updated:"),
        "must not fall back to the generic key-value dump: {stdout:?}"
    );

    // Neither file actually moved.
    assert!(tmp.path().join("notes/a.md").exists());
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("archive/a.md")).unwrap(),
        "existing\n"
    );
}

/// A dry run under `--on-conflict skip` that reports a skip used to hint
/// "Apply this move" as a bare `mv --to <dest>`, which dropped
/// `--on-conflict skip` — so running the hinted command re-hit the same
/// conflict under the default `error` policy and exited 1, contradicting the
/// preview that had just reported a clean (if no-op) outcome. The hint now
/// carries `--on-conflict skip` through.
#[test]
fn dry_run_skip_hint_carries_on_conflict_through() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "notes/a.md", "hello\n");
    write_md(tmp.path(), "archive/a.md", "existing\n");
    let dir = tmp.path().to_str().unwrap();

    let preview = hyalo()
        .args(["--dir", dir])
        .args([
            "mv",
            "notes/a.md",
            "archive/a.md",
            "--dry-run",
            "--on-conflict",
            "skip",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(preview.status.success());
    let json: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    let hints = json["hints"].as_array().expect("hints array");
    let apply_hint = hints
        .iter()
        .find(|h| h["description"] == "Apply this move")
        .expect("an 'Apply this move' hint must be offered for a dry-run skip preview");
    let cmd = apply_hint["cmd"].as_str().unwrap();
    assert!(
        cmd.contains("--on-conflict") && cmd.contains("skip"),
        "hint must thread --on-conflict skip through, got: {cmd}"
    );

    // Actually running the hinted command must succeed (exit 0), not
    // re-fail under the default `error` policy.
    let args: Vec<&str> = cmd.split_whitespace().skip(1).collect(); // drop leading "hyalo"
    let rerun = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(&args)
        .output()
        .unwrap();
    assert!(
        rerun.status.success(),
        "the hinted command must not fail: {} -> {}",
        cmd,
        String::from_utf8_lossy(&rerun.stderr)
    );
}

/// Batch `mv` used to silently drop a source already sitting at its computed
/// destination (a no-op rename) from the plan, with no trace of it anywhere
/// in the output — a caller selecting N files by tag could see fewer than N
/// moves with no explanation. It is now reported under `skipped_noop`.
#[test]
fn batch_mv_reports_source_already_at_destination_as_skipped_noop() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "other/UPPERCASE.md",
        "---\ntags: [beta]\n---\nhello\n",
    );
    let dir = tmp.path().to_str().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", dir])
        .args(["mv", "--tag", "beta", "--to", "other/", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let results = &json["results"];
    assert_eq!(results["totals"]["moves"], 0);
    let skipped_noop = results["skipped_noop"]
        .as_array()
        .expect("skipped_noop must be present when a source is already at its destination");
    assert_eq!(skipped_noop.len(), 1);
    assert_eq!(skipped_noop[0], "other/UPPERCASE.md");

    // Text mode must say so too, not just drop the file from the listing.
    let text_output = hyalo_no_hints()
        .args(["--dir", dir])
        .args(["mv", "--tag", "beta", "--to", "other/", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&text_output.stdout);
    assert!(
        stdout.contains("other/UPPERCASE.md"),
        "text output must mention the no-op file: {stdout}"
    );
}
