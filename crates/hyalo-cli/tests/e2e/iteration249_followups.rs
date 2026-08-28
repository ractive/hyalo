//! Iteration 249 — pre-v0.21.0 dogfood fixes.
//!
//! - **UX-1 — stale-index probe depth**: covered by
//!   `stale_index_warns_for_new_file_two_directories_deep` in `e2e/index.rs`
//!   plus unit tests in `hyalo_core::index`.
//! - **BUG-1 — `task toggle --index` BM25 parity**: after `create-index`,
//!   toggling every task in a file with `--all --index` must leave `find
//!   --index` scores byte-identical to a disk scan, matching the parity
//!   already held by `set`/`append`/`mv`/`lint --fix` (BUG-4, iter-244).
//! - **UX-2 — `links fix --apply --apply-fuzzy` text label**: covered by
//!   `crate::links` (see that module for the assertion).

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn hyalo_no_hints() -> Command {
    crate::common::hyalo_no_hints()
}

fn run(tmp: &TempDir, args: &[&str]) -> (std::process::Output, Value) {
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path().to_str().unwrap())
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "`hyalo {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout not JSON: {e}: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    (output, json)
}

fn create_index(tmp: &TempDir) {
    run(tmp, &["create-index"]);
    assert!(tmp.path().join(".hyalo-index").exists());
}

// ---------------------------------------------------------------------------
// BUG-1 — `task toggle --all --index` BM25 parity
// ---------------------------------------------------------------------------

/// Repro from the dogfood report (dogfood-v0200-post-247-sweep.md, BUG-1):
/// `task toggle --all --index` on a file previously left the persisted BM25
/// corpus statistics (`avgdl`) computed from that file's stale, pre-toggle
/// token count, so `find --index` scores drifted from a disk scan by a
/// couple of decimal places (hit counts and ranking stayed correct — only
/// the score value differed). Fixed by routing `MutationJournal::update_task`
/// through a full re-scan (`SnapshotIndex::refresh_links`) of the toggled
/// file, the same "full re-index of the mutated file" every other
/// parity-preserving write path already uses.
#[test]
fn task_toggle_all_index_bm25_scores_match_disk_scan() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("tasks.md"),
        "---\ntitle: Tasks\n---\n\n# Tasks\n\n\
         Some dogfood prose about stale index parity and read_line_capped.\n\n\
         ## Tasks\n\n\
         - [ ] stale index task one\n\
         - [ ] read_line_capped task two\n\
         - [ ] a third dogfood task\n",
    )
    .unwrap();
    // A few sibling files so BM25 corpus statistics (N, avgdl) are non-trivial.
    std::fs::write(
        tmp.path().join("other-one.md"),
        "---\ntitle: Other One\n---\n\n# Other One\n\nUnrelated dogfood prose about fishing.\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("other-two.md"),
        "---\ntitle: Other Two\n---\n\n# Other Two\n\nMore unrelated dogfood prose about hiking and gear.\n",
    )
    .unwrap();
    create_index(&tmp);

    run(&tmp, &["task", "toggle", "tasks.md", "--all", "--index"]);

    let scores = |index: &[&str]| {
        let out = hyalo_no_hints()
            .arg("--dir")
            .arg(tmp.path().to_str().unwrap())
            .args(["find", "dogfood", "--limit", "0", "--format", "json"])
            .args(index)
            .output()
            .unwrap();
        assert!(out.status.success());
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let indexed = scores(&["--index"]);
    let disk = scores(&[]);
    assert_eq!(
        indexed, disk,
        "BUG-1: find --index output must stay byte-identical to a disk scan \
         after `task toggle --all --index` (no intervening create-index)"
    );
}

/// Same parity requirement for `task set` (the `--status` sibling of
/// `toggle`), and specifically that toggling *every* task on a file in one
/// call re-scans the file once, not once per task (a multi-task file must
/// not desync the corpus by double-counting or skipping the re-tokenize).
#[test]
fn task_toggle_all_multi_task_file_index_matches_fresh_index() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("many.md"),
        "---\ntitle: Many\n---\n\n# Many\n\n\
         ## Tasks\n\n\
         - [ ] alpha task about dogfood\n\
         - [ ] beta task about dogfood\n\
         - [ ] gamma task about dogfood\n\
         - [ ] delta task about dogfood\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("filler.md"),
        "---\ntitle: Filler\n---\n\n# Filler\n\nUnrelated dogfood prose.\n",
    )
    .unwrap();
    create_index(&tmp);

    run(&tmp, &["task", "toggle", "many.md", "--all", "--index"]);

    // Compare the mutated snapshot's `find --index` output against a fresh
    // `create-index` rebuild's output, rather than only against a disk scan —
    // this pins the "compare against a freshly created index" form the
    // iteration asked for, distinct from the disk-scan comparison above.
    let mutated = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path().to_str().unwrap())
        .args([
            "find", "dogfood", "--limit", "0", "--index", "--format", "json",
        ])
        .output()
        .unwrap();
    assert!(mutated.status.success());
    let mutated_json: Value = serde_json::from_slice(&mutated.stdout).unwrap();

    // Rebuild a fresh index from the current (post-toggle) disk state.
    std::fs::remove_file(tmp.path().join(".hyalo-index")).unwrap();
    create_index(&tmp);
    let fresh = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path().to_str().unwrap())
        .args([
            "find", "dogfood", "--limit", "0", "--index", "--format", "json",
        ])
        .output()
        .unwrap();
    assert!(fresh.status.success());
    let fresh_json: Value = serde_json::from_slice(&fresh.stdout).unwrap();

    assert_eq!(
        mutated_json["results"], fresh_json["results"],
        "BUG-1: task-toggle-mutated index scores must match a from-scratch \
         create-index rebuild of the same (post-toggle) disk state"
    );
}

/// BUG-1 investigation (iter-249): the dogfood report also implicated
/// `lint --fix --index`'s "rescan-on-write" path. Confirms it stays correct
/// (`refresh_entry_and_links` already re-tokenizes for BM25, unlike the
/// pre-fix `update_task`) — including when the fixed file was edited
/// externally after `create-index`, so the fix operates on a stale entry.
#[test]
fn lint_fix_index_bm25_scores_match_disk_scan_after_external_edit() {
    let tmp = tempfile::tempdir().unwrap();
    // Trailing whitespace (MD009, enabled and autofixable by default) so
    // `lint --fix` has a real fix to apply.
    std::fs::write(
        tmp.path().join("violation.md"),
        "---\ntitle: Violation\n---\n\ndogfood parity prose.   \n\nAnother line.\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("other.md"),
        "---\ntitle: Other\n---\n\n# Other\n\nUnrelated dogfood prose about hiking.\n",
    )
    .unwrap();
    create_index(&tmp);

    // Simulate an editor appending to the file after create-index — an
    // in-place edit the directory-mtime staleness probe cannot see, so the
    // entry `lint --fix` rescans is stale relative to disk at fix time.
    std::thread::sleep(std::time::Duration::from_millis(2100));
    {
        use std::io::Write as _;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(tmp.path().join("violation.md"))
            .unwrap();
        writeln!(f, "\nExternally appended dogfood sentence.").unwrap();
    }

    let (_, fix_result) = run(
        &tmp,
        &[
            "lint", "--fix", "--index", "--rule", "MD009", "--format", "json",
        ],
    );
    assert_eq!(
        fix_result["results"]["total_fixed"].as_u64(),
        Some(1),
        "sanity: the trailing-whitespace violation must actually be fixed: {fix_result}"
    );

    let scores = |index: &[&str]| {
        let out = hyalo_no_hints()
            .arg("--dir")
            .arg(tmp.path().to_str().unwrap())
            .args(["find", "dogfood", "--limit", "0", "--format", "json"])
            .args(index)
            .output()
            .unwrap();
        assert!(out.status.success());
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let indexed = scores(&["--index"]);
    let disk = scores(&[]);
    assert_eq!(
        indexed, disk,
        "lint --fix --index must leave find --index byte-identical to a disk \
         scan, even when the fixed file had drifted from the index beforehand"
    );
}
