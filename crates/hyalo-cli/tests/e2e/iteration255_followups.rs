//! Iteration 255 — the three carry-over findings iteration 254 scoped out of
//! its help-text and result-shape work.
//!
//! - **BUG-2.** A mutating command that reads a file and then writes nothing
//!   (`set --property status=completed` on a file already at that value) used
//!   to leave the snapshot entry exactly as the last `create-index` saw it,
//!   even when the file's body had since changed on disk. `find --index` then
//!   answered from a body that no longer existed.
//! - **UX-3.** `read`'s invalid-UTF-8 placeholder claimed the line was "lossy
//!   in search" while `find <text>` dropped the whole file from full-text
//!   search. Both surfaces now print the same sentence.
//! - **UX-5.** `hyalo new --property k=v` failed with clap's generic
//!   unknown-argument error and no pointer to `set`.

use assert_cmd::Command;
use tempfile::TempDir;

/// The one sentence `read`'s placeholder and `find`'s skip warning must both
/// print — the pin for `commands::INVALID_UTF8_CONSEQUENCE`. Written out in
/// full here on purpose: a test that imported the constant would pass even if
/// the constant itself started lying again.
const UTF8_SENTENCE: &str = "invalid UTF-8 — the file is excluded from full-text search (`find -e` still matches it lossily)";

fn hyalo(tmp: &TempDir) -> Command {
    let mut cmd = crate::common::hyalo_no_hints();
    cmd.arg("--dir").arg(tmp.path().to_str().unwrap());
    cmd
}

fn run(tmp: &TempDir, args: &[&str]) -> std::process::Output {
    let output = hyalo(tmp).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "`hyalo {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn run_json(tmp: &TempDir, args: &[&str]) -> serde_json::Value {
    let output = run(tmp, args);
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "`hyalo {}` stdout not JSON: {e}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

/// `find --count --format json` emits a bare number, not an envelope.
fn count(tmp: &TempDir, args: &[&str]) -> u64 {
    let mut with_count = args.to_vec();
    with_count.push("--count");
    let output = run(tmp, &with_count);
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .unwrap_or_else(|e| {
            panic!(
                "`hyalo {} --count` stdout not a number: {e}: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stdout)
            )
        })
}

// ---------------------------------------------------------------------------
// BUG-2: a no-op mutation still repairs an index entry the disk has outgrown
// ---------------------------------------------------------------------------

fn bug2_vault() -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("note.md"),
        "---\ntitle: Note\nstatus: completed\n---\n\n# Note\n\nOriginal body.\n",
    )
    .unwrap();
    // A second file keeps the BM25 corpus from degenerating to one document.
    std::fs::write(
        tmp.path().join("other.md"),
        "---\ntitle: Other\nstatus: planned\n---\n\n# Other\n\nUnrelated prose.\n",
    )
    .unwrap();
    tmp
}

/// Append a line containing a word that appears nowhere in the snapshot, the
/// way an editor (or a `>>` redirect) would — no hyalo write path involved.
fn append_externally(tmp: &TempDir, rel: &str, line: &str) {
    use std::io::Write as _;
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(tmp.path().join(rel))
        .unwrap();
    writeln!(f, "{line}").unwrap();
}

/// The regression itself: `set` reports `0 modified` because the property is
/// already at its target value, and the entry must *still* come out matching
/// the bytes on disk.
#[test]
fn noop_set_with_index_refreshes_an_entry_the_disk_has_outgrown() {
    let tmp = bug2_vault();
    run(&tmp, &["create-index"]);
    append_externally(&tmp, "note.md", "zzqqx appended after the snapshot");

    // Baseline: the snapshot cannot see the appended word (it answers BM25
    // from cached tokens, never re-reading the file).
    assert_eq!(
        count(&tmp, &["find", "zzqqx", "--index"]),
        0,
        "precondition: the pre-mutation snapshot must not know the appended word"
    );

    let set = run_json(
        &tmp,
        &[
            "set",
            "note.md",
            "--property",
            "status=completed",
            "--index",
        ],
    );
    assert_eq!(
        set["results"]["modified"].as_array().unwrap().len(),
        0,
        "the property is already at its target value — this must stay a no-op write"
    );
    assert_eq!(set["results"]["skipped"].as_array().unwrap().len(), 1);

    assert_eq!(
        count(&tmp, &["find", "zzqqx", "--index"]),
        1,
        "the no-op `set` read the file, so its index entry must now match disk"
    );

    // And the repair is persisted, not just in-memory for that one process.
    let reread = run_json(&tmp, &["find", "zzqqx", "--index", "--fields", "file"]);
    assert_eq!(reread["results"][0]["file"], "note.md");
}

/// The same repair, observed through fields that can only come from the
/// snapshot (`size`/`lines` are recorded at scan time, never recomputed by a
/// query) — so the assertion cannot be satisfied by an accidental disk read.
#[test]
fn noop_set_with_index_refreshes_size_and_lines_from_disk() {
    let tmp = bug2_vault();
    run(&tmp, &["create-index"]);

    let stale = run_json(
        &tmp,
        &[
            "find",
            "--file",
            "note.md",
            "--index",
            "--fields",
            "file,size,lines",
        ],
    );
    let stale_size = stale["results"][0]["size"].as_u64().unwrap();
    let stale_lines = stale["results"][0]["lines"].as_u64().unwrap();

    append_externally(&tmp, "note.md", "one more paragraph of body text");
    run(
        &tmp,
        &[
            "set",
            "note.md",
            "--property",
            "status=completed",
            "--index",
        ],
    );

    let fresh = run_json(
        &tmp,
        &[
            "find",
            "--file",
            "note.md",
            "--index",
            "--fields",
            "file,size,lines",
        ],
    );
    let disk_size = std::fs::metadata(tmp.path().join("note.md")).unwrap().len();
    assert_eq!(
        fresh["results"][0]["size"].as_u64().unwrap(),
        disk_size,
        "size must match the bytes on disk after the no-op mutation"
    );
    assert_eq!(
        fresh["results"][0]["lines"].as_u64().unwrap(),
        stale_lines + 1
    );
    assert!(fresh["results"][0]["size"].as_u64().unwrap() > stale_size);
}

/// `--dry-run` promises to touch nothing. The staleness repair is a write to
/// the snapshot, so it must stay behind that promise too.
#[test]
fn dry_run_set_leaves_a_stale_entry_alone() {
    let tmp = bug2_vault();
    run(&tmp, &["create-index"]);
    append_externally(&tmp, "note.md", "zzqqx appended after the snapshot");

    run(
        &tmp,
        &[
            "set",
            "note.md",
            "--property",
            "status=completed",
            "--index",
            "--dry-run",
        ],
    );

    assert_eq!(
        count(&tmp, &["find", "zzqqx", "--index"]),
        0,
        "--dry-run must not write the snapshot, stale entry or not"
    );
}

/// `append` and `remove` share the read-then-maybe-write shape, so they get
/// the same repair. Both are exercised in their no-op form.
#[test]
fn noop_append_and_remove_with_index_also_refresh() {
    for (cmd, args) in [
        ("append", vec!["--property", "tags=a"]),
        ("remove", vec!["--property", "absent-property"]),
    ] {
        let tmp = bug2_vault();
        std::fs::write(
            tmp.path().join("note.md"),
            "---\ntitle: Note\ntags:\n  - a\n---\n\n# Note\n\nOriginal body.\n",
        )
        .unwrap();
        run(&tmp, &["create-index"]);
        append_externally(&tmp, "note.md", "zzqqx appended after the snapshot");

        let mut argv = vec![cmd, "note.md"];
        argv.extend(args);
        argv.push("--index");
        run(&tmp, &argv);

        assert_eq!(
            count(&tmp, &["find", "zzqqx", "--index"]),
            1,
            "no-op `{cmd}` must refresh the entry it read"
        );
    }
}

// ---------------------------------------------------------------------------
// UX-3: `read`'s placeholder and `find`'s skip warning state the same fact
// ---------------------------------------------------------------------------

fn utf8_vault() -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let mut bytes = b"---\ntitle: Bad\n---\n\nplain needle line\n".to_vec();
    bytes.extend_from_slice(b"broken \xff\xfe needle line\n");
    std::fs::write(tmp.path().join("bad.md"), bytes).unwrap();
    std::fs::write(
        tmp.path().join("good.md"),
        "---\ntitle: Good\n---\n\nanother needle line\n",
    )
    .unwrap();
    tmp
}

#[test]
fn invalid_utf8_wording_is_shared_by_read_and_find() {
    let tmp = utf8_vault();

    // `read` renders a placeholder in the offending line's position.
    let read = run(&tmp, &["read", "bad.md", "--format", "text"]);
    let read_out = String::from_utf8_lossy(&read.stdout);
    assert!(
        read_out.contains(&format!("<line skipped: {UTF8_SENTENCE}>")),
        "read placeholder drifted: {read_out}"
    );

    // `find <text>` (the default full-text path) drops the file and says so
    // in exactly the same words.
    let find = run(&tmp, &["find", "needle"]);
    let find_err = String::from_utf8_lossy(&find.stderr);
    assert!(
        find_err.contains(&format!("skipping bad.md: {UTF8_SENTENCE}")),
        "find skip warning drifted: {find_err}"
    );
}

/// The claims in that shared sentence are true, not merely identical: the
/// file really is absent from full-text results, and `find -e` really does
/// match it.
#[test]
fn invalid_utf8_sentence_matches_actual_behaviour() {
    let tmp = utf8_vault();

    let full_text = run_json(&tmp, &["find", "needle", "--fields", "file"]);
    let files: Vec<&str> = full_text["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["file"].as_str().unwrap())
        .collect();
    assert!(
        !files.contains(&"bad.md"),
        "full-text search must exclude the non-UTF-8 file, as the wording says: {files:?}"
    );

    let regex = run_json(&tmp, &["find", "-e", "needle", "--fields", "file"]);
    let regex_files: Vec<&str> = regex["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["file"].as_str().unwrap())
        .collect();
    assert!(
        regex_files.contains(&"bad.md"),
        "`find -e` must still reach the file, as the wording says: {regex_files:?}"
    );
}

/// A read error that is *not* an encoding problem keeps its own diagnostic —
/// the new wording is scoped to invalid UTF-8, not bolted onto every io error.
#[test]
fn non_utf8_wording_does_not_leak_onto_other_read_errors() {
    let tmp = utf8_vault();
    let find = run(&tmp, &["find", "needle"]);
    let err = String::from_utf8_lossy(&find.stderr);
    assert!(
        !err.contains("stream did not contain valid UTF-8"),
        "the raw io message must be replaced, not appended: {err}"
    );
}

// ---------------------------------------------------------------------------
// UX-5: `new` has no --property, and now says where properties are set
// ---------------------------------------------------------------------------

#[test]
fn new_with_property_points_at_set() {
    let tmp = tempfile::tempdir().unwrap();
    let output = hyalo(&tmp)
        .args([
            "new",
            "--type",
            "note",
            "--file",
            "notes/draft.md",
            "--property",
            "status=draft",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(
        err.contains("`hyalo new` scaffolds from the type's schema"),
        "missing the scoped error: {err}"
    );
    assert!(
        err.contains("hyalo set <FILE> --property k=v"),
        "the hint must name the command that does set properties: {err}"
    );
    assert!(
        !tmp.path().join("notes/draft.md").exists(),
        "a rejected invocation must not scaffold anything"
    );
}

#[test]
fn new_with_tag_points_at_set() {
    let tmp = tempfile::tempdir().unwrap();
    let output = hyalo(&tmp)
        .args(["new", "--type", "note", "--file", "n.md", "--tag", "x"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("hyalo set <FILE> --property k=v"),
        "--tag must get the same pointer as --property"
    );
}

/// The pointer is not only in the error path: `new --help` chains the two
/// commands so a reader finds it before making the mistake.
#[test]
fn new_help_chains_set_for_extra_properties() {
    let output = crate::common::hyalo_no_hints()
        .args(["new", "--help"])
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(
        help.contains("there is no"),
        "new --help must say `--property` does not exist: {help}"
    );
    assert!(
        help.contains("hyalo set notes/draft.md --property status=draft"),
        "new --help must show the scaffold-then-set chain: {help}"
    );
}
