//! Iteration 243 — index/disk parity bugfix wave (dogfood v0.20.0 BUGs).
//!
//! Theme: `--index` output must be indistinguishable from a disk scan.
//!
//! - **BUG-1**: every mutating command (`set`, `set --tag`, `task toggle`,
//!   `append`, `remove`, `lint --fix`, `links fix --apply`, `mv`,
//!   `tags rename`) must *upsert* an entry for a file the snapshot never
//!   knew — a file created by an editor/Obsidian and then mutated through
//!   hyalo used to stay invisible to every indexed read.
//! - **BUG-2**: `links fix --apply --index` must heal a drifted index
//!   before discovery (editor-appended broken links are found and fixed),
//!   and `applied` must mean "something was written", not "apply mode".
//! - **BUG-5**: `backlinks` output must be byte-identical between the
//!   index and disk paths (sorted by `(source, line)`).
//! - **BUG-4**: BM25 scores must be identical between the index and disk
//!   paths on a fresh index (code-fence delimiters and `%%` comment lines
//!   used to be tokenized on one path only).

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn hyalo_no_hints() -> Command {
    crate::common::hyalo_no_hints()
}

/// Vault with two indexed files; `beta.md` is the backlink target every
/// mutation test links at.
fn setup() -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("alpha.md"),
        "---\ntitle: Alpha\ntags: [a]\n---\n\n# Alpha\n\nBody.\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("beta.md"),
        "---\ntitle: Beta\ntags: [a]\n---\n\n# Beta\n\nBody.\n",
    )
    .unwrap();
    tmp
}

/// Write an index-unknown file that links to `beta`.
fn write_unknown_linker(tmp: &TempDir, name: &str) {
    std::fs::write(
        tmp.path().join(name),
        "---\ntitle: Unknown\nstatus: active\ntags: [u]\n---\n\n# Unknown\n\nSee [[beta]].\n\n- [ ] task\n",
    )
    .unwrap();
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

/// BUG-1 assertion: an index-unknown file mutated with `--index` must be
/// findable via `--index` (total: 1) and its outgoing link must appear in
/// the persisted link graph.
fn assert_upserted(tmp: &TempDir, file: &str) {
    let (_, json) = run(tmp, &["find", "--file", file, "--index"]);
    assert_eq!(
        json["total"].as_u64(),
        Some(1),
        "BUG-1: {file} must be visible to --index after the mutation"
    );
    let (_, bl) = run(tmp, &["backlinks", "--file", "beta.md", "--index"]);
    let sources: Vec<&str> = bl["results"]["backlinks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["source"].as_str().unwrap())
        .collect();
    assert!(
        sources.contains(&file),
        "BUG-1: {file}'s outgoing link to beta must appear in backlinks via --index (got {sources:?})"
    );
}

// ---------------------------------------------------------------------------
// BUG-1 — upsert on miss, per mutating command
// ---------------------------------------------------------------------------

#[test]
fn set_on_unknown_file_upserts_index_entry() {
    let tmp = setup();
    create_index(&tmp);
    write_unknown_linker(&tmp, "ext.md");
    run(
        &tmp,
        &["set", "ext.md", "--property", "status=done", "--index"],
    );
    assert_upserted(&tmp, "ext.md");
}

#[test]
fn set_tag_on_unknown_file_upserts_index_entry() {
    let tmp = setup();
    create_index(&tmp);
    write_unknown_linker(&tmp, "ext.md");
    run(&tmp, &["set", "ext.md", "--tag", "extra", "--index"]);
    assert_upserted(&tmp, "ext.md");
}

#[test]
fn task_toggle_on_unknown_file_upserts_index_entry() {
    let tmp = setup();
    create_index(&tmp);
    write_unknown_linker(&tmp, "ext.md");
    run(&tmp, &["task", "toggle", "ext.md", "--all", "--index"]);
    assert_upserted(&tmp, "ext.md");
}

#[test]
fn append_on_unknown_file_upserts_index_entry() {
    let tmp = setup();
    create_index(&tmp);
    write_unknown_linker(&tmp, "ext.md");
    run(
        &tmp,
        &["append", "ext.md", "--property", "aliases=X", "--index"],
    );
    assert_upserted(&tmp, "ext.md");
}

#[test]
fn remove_on_unknown_file_upserts_index_entry() {
    let tmp = setup();
    create_index(&tmp);
    write_unknown_linker(&tmp, "ext.md");
    run(
        &tmp,
        &["append", "ext.md", "--property", "aliases=X", "--index"],
    );
    run(
        &tmp,
        &["remove", "ext.md", "--property", "aliases", "--index"],
    );
    assert_upserted(&tmp, "ext.md");
}

#[test]
fn lint_fix_on_unknown_file_upserts_index_entry() {
    let tmp = setup();
    create_index(&tmp);
    // MD012 (multiple consecutive blank lines) is auto-fixable, so the file
    // lands in the journal's `modified_files` — the pre-iter-243 journal
    // skipped unknown files there entirely.
    std::fs::write(
        tmp.path().join("dirty.md"),
        "---\ntitle: Dirty\n---\n\n# Dirty\n\n\n\n\nSee [[beta]].\n",
    )
    .unwrap();
    run(&tmp, &["lint", "dirty.md", "--fix", "--index"]);
    assert_upserted(&tmp, "dirty.md");
}

#[test]
fn links_fix_apply_on_unknown_file_upserts_index_entry() {
    let tmp = setup();
    create_index(&tmp);
    // `[[BETA]]` is a case-mismatch fix applied by plain `--apply`; the file
    // itself is unknown to the index, so the heal pass must add it before
    // discovery for the fix to land (and the entry to persist).
    std::fs::write(
        tmp.path().join("cased.md"),
        "---\ntitle: Cased\n---\n\n# Cased\n\nSee [[BETA]].\n",
    )
    .unwrap();
    run(&tmp, &["links", "fix", "--apply", "--index"]);
    assert_upserted(&tmp, "cased.md");
    let body = std::fs::read_to_string(tmp.path().join("cased.md")).unwrap();
    assert!(
        body.contains("[[beta]]"),
        "the case-mismatch fix must be applied to the unknown file:\n{body}"
    );
}

#[test]
fn mv_on_unknown_file_upserts_index_entry() {
    let tmp = setup();
    create_index(&tmp);
    write_unknown_linker(&tmp, "ext.md");
    run(&tmp, &["mv", "ext.md", "moved.md", "--index"]);
    assert_upserted(&tmp, "moved.md");
}

#[test]
fn tags_rename_on_unknown_file_upserts_index_entry() {
    let tmp = setup();
    create_index(&tmp);
    write_unknown_linker(&tmp, "ext.md");
    run(
        &tmp,
        &["tags", "rename", "--from", "u", "--to", "v", "--index"],
    );
    assert_upserted(&tmp, "ext.md");
}

// ---------------------------------------------------------------------------
// BUG-2 — heal + `applied` semantics
// ---------------------------------------------------------------------------

/// The dogfood BUG-2 repro: an indexed file edited by hand (outside hyalo)
/// gains a fixable broken link. `links fix --apply --apply-fuzzy --index`
/// must discover and fix it — no silent trust in the stale entry.
#[test]
fn links_fix_index_heals_editor_introduced_broken_link() {
    let tmp = setup();
    create_index(&tmp);
    // Editor append AFTER create-index: the snapshot's entry for alpha.md
    // does not know this link yet. Sleep past STALENESS_TOLERANCE_SECS (1s)
    // *before* writing — the heal pass compares the entry's stored mtime
    // against the file's on-disk mtime, so the drift must be more than the
    // tolerance apart, and mtime is set at write time.
    std::thread::sleep(std::time::Duration::from_secs(
        hyalo_core::index::STALENESS_TOLERANCE_SECS + 1,
    ));
    let alpha = tmp.path().join("alpha.md");
    let mut body = std::fs::read_to_string(&alpha).unwrap();
    body.push_str("See [[BETA]] too.\n");
    std::fs::write(&alpha, body).unwrap();

    let (_, json) = run(
        &tmp,
        &["links", "fix", "--apply", "--apply-fuzzy", "--index"],
    );
    // The healed link surfaces in the fix buckets (a `[[BETA]]` → `[[beta]]`
    // case mismatch is fixable, so `broken` itself may be 0).
    assert_eq!(
        json["results"]["applied_fixes"].as_array().map(Vec::len),
        Some(1),
        "the heal pass must surface the editor-introduced broken link as a fix"
    );
    assert_eq!(
        json["results"]["applied"].as_bool(),
        Some(true),
        "the case-mismatch fix must be reported as applied"
    );
    let after = std::fs::read_to_string(&alpha).unwrap();
    assert!(
        after.contains("[[beta]]"),
        "the fix must be written to disk:\n{after}"
    );
}

/// `applied` means "something was written", not "apply mode": an --apply run
/// with zero fixes must report `applied: false` in JSON (BUG-2 cosmetic) and
/// an honest text line.
#[test]
fn links_fix_apply_zero_fixes_applied_false() {
    let tmp = setup();
    create_index(&tmp);
    let (_, json) = run(&tmp, &["links", "fix", "--apply", "--index"]);
    assert_eq!(json["results"]["broken"].as_u64(), Some(0));
    assert_eq!(json["results"]["applied"].as_bool(), Some(false));
    assert_eq!(
        json["results"]["applied_fixes"].as_array().map(Vec::len),
        Some(0)
    );
    assert_eq!(json["results"]["dry_run"].as_bool(), Some(false));
}

#[test]
fn links_fix_apply_zero_fixes_text_is_honest() {
    let tmp = setup();
    create_index(&tmp);
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path().to_str().unwrap())
        .args(["links", "fix", "--apply", "--format", "text"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.contains("Applied: no (no fixes written"),
        "BUG-2: text must not read as a plain 'Applied: no' without cause — got:\n{text}"
    );
    assert!(
        !text.contains("Applied: yes"),
        "BUG-2: zero-fix apply run must never claim Applied: yes:\n{text}"
    );
}

/// Dry runs keep the explicit cause, so agents can tell the modes apart.
#[test]
fn links_fix_dry_run_text_names_dry_run() {
    let tmp = setup();
    create_index(&tmp);
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path().to_str().unwrap())
        .args(["links", "fix", "--format", "text"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.contains("Applied: no (dry run)"),
        "dry-run text should name the dry run — got:\n{text}"
    );
}

// ---------------------------------------------------------------------------
// BUG-5 — backlinks order parity between index and disk
// ---------------------------------------------------------------------------

/// After a mutation wave (`tags rename --index` rewrites linking files),
/// `backlinks --index` must be byte-identical to the disk scan.
#[test]
fn backlinks_index_matches_disk_after_mutation_wave() {
    let tmp = tempfile::tempdir().unwrap();
    // Three linkers so a refresh reorders *some* of them (BUG-5 repro).
    for name in ["l1", "l2", "l3"] {
        std::fs::write(
            tmp.path().join(format!("{name}.md")),
            format!("---\ntitle: {name}\ntags: [t]\n---\n\n# {name}\n\nSee [[beta]].\n"),
        )
        .unwrap();
    }
    std::fs::write(
        tmp.path().join("beta.md"),
        "---\ntitle: Beta\n---\n\n# Beta\n\nBody.\n",
    )
    .unwrap();
    create_index(&tmp);

    // Mutation wave: `tags rename` rewrites and re-indexes l2 (a linker), and
    // `set` touches l1 — the journal refresh used to move those entries to
    // the end of the graph's insertion order while the disk scan kept
    // directory order.
    run(
        &tmp,
        &["set", "l1.md", "--property", "status=done", "--index"],
    );
    run(
        &tmp,
        &["tags", "rename", "--from", "t", "--to", "u", "--index"],
    );

    let indexed = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path().to_str().unwrap())
        .args([
            "backlinks",
            "--file",
            "beta.md",
            "--index",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let disk = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path().to_str().unwrap())
        .args(["backlinks", "--file", "beta.md", "--format", "json"])
        .output()
        .unwrap();
    assert!(indexed.status.success() && disk.status.success());
    assert_eq!(
        String::from_utf8_lossy(&indexed.stdout),
        String::from_utf8_lossy(&disk.stdout),
        "BUG-5: backlinks --index must be byte-identical to the disk scan"
    );
    // And stable across repeated indexed reads.
    let again = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path().to_str().unwrap())
        .args([
            "backlinks",
            "--file",
            "beta.md",
            "--index",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&indexed.stdout),
        String::from_utf8_lossy(&again.stdout),
        "BUG-5: backlinks --index output must be stable across refreshes"
    );
}

// ---------------------------------------------------------------------------
// BUG-4 — BM25 parity between index and disk on a fresh index
// ---------------------------------------------------------------------------

/// A vault whose bodies exercise the divergent tokenization: code-fence
/// delimiters with a language tag (` ```rust `), `%%` comment blocks, and
/// regular prose. On a fresh index, `find --index` and the disk scan must
/// produce identical scores.
#[test]
fn bm25_scores_identical_index_vs_disk_fresh_index() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("one.md"),
        "---\ntitle: One\n---\n\n# One\n\nThe dogfood report covers parity.\n\n```rust\nfn main() {}\n```\n\n%% hidden comment dogfood %%\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("two.md"),
        "---\ntitle: Two\n---\n\n# Two\n\nAnother dogfood note.\n\n```python\nprint('dogfood')\n```\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("three.md"),
        "---\ntitle: Three\n---\n\n# Three\n\nUnrelated prose about fishing.\n",
    )
    .unwrap();
    create_index(&tmp);

    let args = |index: &[&str]| {
        hyalo_no_hints()
            .arg("--dir")
            .arg(tmp.path().to_str().unwrap())
            .args(["find", "dogfood", "--format", "json"])
            .args(index)
            .output()
            .unwrap()
    };
    let indexed = args(&["--index"]);
    let disk = args(&[]);
    assert!(indexed.status.success() && disk.status.success());
    assert_eq!(
        String::from_utf8_lossy(&indexed.stdout),
        String::from_utf8_lossy(&disk.stdout),
        "BUG-4: BM25 output must be byte-identical between --index and disk scan"
    );
}
