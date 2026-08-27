//! Iteration 244 — index remaining deferrals.
//!
//! - **`new` link-graph upsert**: a file created by `hyalo new` must register
//!   its template's outgoing wikilinks in the persisted link graph, so
//!   `backlinks --index` sees them without a full `create-index` rebuild —
//!   the last mutating write path without BUG-1's upsert-with-links guarantee.
//! - **BUG-4 (carry-over) — post-mutation BM25 parity**: after a mutation
//!   wave under `--index`, `find --index` scores must be byte-identical to
//!   the disk scan, without an intervening `create-index`. The persisted
//!   inverted index is rebuilt from (re-scanned ∪ postings-reconstructed)
//!   tokens on journal flush.
//! - **UX-3 — nested dot-path property filters**: `--property 'a.b=v'`
//!   traverses nested YAML maps (literal dotted keys still win).
//! - **UX-6 — case-insensitive link resolution**: `links fix
//!   --case-insensitive` (or `[links.case_insensitive] resolve = true`)
//!   treats case-fold-resolving targets as resolved, so MDN-style vaults
//!   don't offer a case-mismatch rewrite plan per link.

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
// `new` link-graph upsert
// ---------------------------------------------------------------------------

/// `hyalo new` must record the created file with its outgoing links: a type
/// default of `related = "[[beta]]"` is emitted as frontmatter, and the
/// frontmatter link property feeds the link graph.
#[test]
fn new_file_template_links_visible_in_backlinks_index() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("beta.md"),
        "---\ntitle: Beta\n---\n\n# Beta\n\nBody.\n",
    )
    .unwrap();
    // Define a type whose scaffold links to `beta` via the `related`
    // frontmatter property (a default frontmatter link property).
    run(
        &tmp,
        &[
            "types",
            "set",
            "note",
            "--default",
            "related=\"\"[[beta]]\"\"",
        ],
    );
    create_index(&tmp);

    run(
        &tmp,
        &["new", "--type", "note", "--file", "created.md", "--index"],
    );

    let (_, bl) = run(&tmp, &["backlinks", "--file", "beta.md", "--index"]);
    let sources: Vec<&str> = bl["results"]["backlinks"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|l| l["source"].as_str().unwrap_or_default())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        sources.contains(&"created.md"),
        "`new` must register the created file's outgoing links in the persisted \
         graph (got sources: {sources:?})"
    );

    // And the indexed read matches the disk scan byte-for-byte.
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
        "`new` upsert: backlinks --index must match the disk scan"
    );
}

// ---------------------------------------------------------------------------
// BUG-4 — post-mutation BM25 parity
// ---------------------------------------------------------------------------

/// After a mutation wave (set + append + new) under `--index`, BM25 scores
/// from the persisted inverted index must be byte-identical to a disk scan —
/// corpus statistics (N, df, avgdl) are rebuilt incrementally at flush, not
/// only by a full `create-index`.
#[test]
fn bm25_scores_identical_index_vs_disk_after_mutation_wave() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("one.md"),
        "---\ntitle: One\n---\n\n# One\n\nThe dogfood report covers parity.\n\n```rust\nfn main() {}\n```\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("two.md"),
        "---\ntitle: Two\n---\n\n# Two\n\nAnother dogfood note.\n\n```python\nprint('dogfood')\n```\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("three.md"),
        "---\ntitle: Three\nstatus: draft\n---\n\n# Three\n\nUnrelated prose about fishing and dogfood.\n",
    )
    .unwrap();
    create_index(&tmp);

    // Mutation wave — no `create-index` afterwards.
    // 1. Rewrite a body so its tokens change (`three` gains more dogfood).
    //    The status stays `draft` so the following `set` is a real change.
    std::fs::write(
        tmp.path().join("three.md"),
        "---\ntitle: Three\nstatus: draft\n---\n\n# Three\n\nDogfood dogfood dogfood. Fishing prose stays.\n",
    )
    .unwrap();
    run(
        &tmp,
        &["set", "three.md", "--property", "status=done", "--index"],
    );
    // 2. A frontmatter append (refresh path, entry already known).
    run(
        &tmp,
        &["append", "two.md", "--property", "aliases=X", "--index"],
    );
    // 3. A brand-new file entering the corpus.
    std::fs::write(
        tmp.path().join("four.md"),
        "---\ntitle: Four\n---\n\n# Four\n\nFresh dogfood entry.\n",
    )
    .unwrap();
    run(
        &tmp,
        &["set", "four.md", "--property", "status=active", "--index"],
    );
    // 4. A file moved to a new path (entry removal + re-insert in one wave).
    std::fs::create_dir_all(tmp.path().join("sub")).unwrap();
    run(&tmp, &["mv", "two.md", "sub/two.md", "--index"]);

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
        "BUG-4: BM25 output must stay byte-identical between --index and disk \
         scan after a mutation wave (no intervening create-index)"
    );
}

// ---------------------------------------------------------------------------
// UX-3 — nested dot-path property filters
// ---------------------------------------------------------------------------

/// `--property 'a.b=v'` must traverse nested YAML maps and return correct
/// results (never a silent `No results` for a value that is actually there).
#[test]
fn find_property_dot_path_traverses_nested_map() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("nested.md"),
        "---\ntitle: Nested\ncontact:\n  email: team@example.com\n  name: Team\n---\n\n# Nested\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("flat.md"),
        "---\ntitle: Flat\n---\n\n# Flat\n",
    )
    .unwrap();
    create_index(&tmp);

    // Matching nested value via --index.
    let (_, json) = run(
        &tmp,
        &[
            "find",
            "--property",
            "contact.email=team@example.com",
            "--index",
        ],
    );
    assert_eq!(
        json["total"]
            .as_u64()
            .or_else(|| json["results"].as_array().map(|a| a.len() as u64)),
        Some(1),
        "dot-path filter must match the nested map entry via --index: {json}"
    );

    // And on the disk scan path.
    let (_, json) = run(
        &tmp,
        &["find", "--property", "contact.email=team@example.com"],
    );
    assert_eq!(
        json["total"]
            .as_u64()
            .or_else(|| json["results"].as_array().map(|a| a.len() as u64)),
        Some(1),
        "dot-path filter must match the nested map entry on disk: {json}"
    );

    // Non-matching nested value genuinely returns no results.
    let (_, json) = run(&tmp, &["find", "--property", "contact.email=nomatch"]);
    assert_eq!(
        json["total"]
            .as_u64()
            .or_else(|| json["results"].as_array().map(|a| a.len() as u64)),
        Some(0),
        "non-matching dot-path value must return zero results: {json}"
    );

    // A literal dotted key still wins over traversal.
    std::fs::write(
        tmp.path().join("literal.md"),
        "---\ntitle: Literal\n\"a.b\": flat\n---\n\n# Literal\n",
    )
    .unwrap();
    let (_, json) = run(&tmp, &["find", "--property", "a.b=flat"]);
    assert_eq!(
        json["total"]
            .as_u64()
            .or_else(|| json["results"].as_array().map(|a| a.len() as u64)),
        Some(1),
        "literal dotted key must take precedence over traversal: {json}"
    );
}

// ---------------------------------------------------------------------------
// UX-6 — case-insensitive link resolution
// ---------------------------------------------------------------------------

fn write_case_folded_vault(tmp: &TempDir) {
    // Case-folded directory layout: on-disk `Docs/`, written `docs/`.
    std::fs::create_dir_all(tmp.path().join("Docs")).unwrap();
    std::fs::write(
        tmp.path().join("Docs/Note.md"),
        "---\ntitle: Note\n---\n\n# Note\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("index.md"),
        "---\ntitle: Index\n---\n\n# Index\n\nSee [[docs/Note]].\n",
    )
    .unwrap();
}

/// With case-insensitive resolution enabled, case-fold-resolving targets are
/// reported as case-mismatch fixes by default; `--case-insensitive` drains
/// that bucket so the dry run reports zero case-mismatch fixes.
#[test]
fn links_fix_case_insensitive_reports_zero_case_mismatches() {
    let tmp = tempfile::tempdir().unwrap();
    write_case_folded_vault(&tmp);
    // Force the case-insensitive index on regardless of the host filesystem
    // so the test behaves identically on Linux and macOS.
    std::fs::write(
        tmp.path().join(".hyalo.toml"),
        "[links]\ncase_insensitive = \"true\"\n",
    )
    .unwrap();
    create_index(&tmp);

    // Baseline: the case-folded link IS detected as a case mismatch.
    let (_, json) = run(&tmp, &["links", "fix", "--dry-run"]);
    let baseline = json["results"]["case_mismatches"]
        .as_u64()
        .or_else(|| json["results"]["case_mismatch_count"].as_u64())
        .or_else(|| {
            // Fall back to scanning the serialized fixes for the rule code.
            json["results"]["fixes"].as_array().map(|a| a.len() as u64)
        })
        .unwrap_or_else(|| {
            panic!("unexpected links fix output shape: {json}");
        });
    assert!(
        baseline > 0,
        "case-folded link must be reported as a case mismatch without the flag: {json}"
    );

    // Flag form: zero case-mismatch fixes.
    let (_, json) = run(&tmp, &["links", "fix", "--dry-run", "--case-insensitive"]);
    let fixes = json["results"]["case_mismatches"]
        .as_u64()
        .or_else(|| json["results"]["case_mismatch_count"].as_u64())
        .or_else(|| json["results"]["fixes"].as_array().map(|a| a.len() as u64))
        .unwrap_or(0);
    assert_eq!(
        fixes, 0,
        "case-fold-resolving targets must count as resolved under --case-insensitive: {json}"
    );

    // Config form: `[links.case_insensitive] resolve = true`.
    let tmp2 = tempfile::tempdir().unwrap();
    write_case_folded_vault(&tmp2);
    std::fs::write(
        tmp2.path().join(".hyalo.toml"),
        "[links.case_insensitive]\nresolve = true\n",
    )
    .unwrap();
    create_index(&tmp2);
    let (_, json) = run(&tmp2, &["links", "fix", "--dry-run"]);
    let fixes = json["results"]["case_mismatches"]
        .as_u64()
        .or_else(|| json["results"]["case_mismatch_count"].as_u64())
        .or_else(|| json["results"]["fixes"].as_array().map(|a| a.len() as u64))
        .unwrap_or(0);
    assert_eq!(
        fixes, 0,
        "config `[links.case_insensitive] resolve = true` must suppress case-mismatch fixes: {json}"
    );
}

/// The `--case-insensitive` flag must be a true one-shot equivalent of
/// `[links.case_insensitive] resolve = true`: it forces the case-insensitive
/// fallback **on** (so case-fold-resolving links are classified as case
/// mismatches, not broken) *and* drains the mismatch bucket. The config is
/// set to `"false"` so the test discriminates on every filesystem — without
/// the flag forcing the fallback on, the case-folded link is reported as
/// broken (not a case mismatch) regardless of the host filesystem's casing.
#[test]
fn links_fix_case_insensitive_flag_alone_forces_fallback() {
    let tmp = tempfile::tempdir().unwrap();
    write_case_folded_vault(&tmp);
    // Explicitly Off: on a case-sensitive filesystem (Linux CI) this fails
    // without the flag-forces-fallback behaviour — the case-folded link is
    // then reported as broken, not as a drained case mismatch. On a
    // case-insensitive host FS (macOS) the link resolves either way, so the
    // test is vacuously green there; the strictness lives in Linux CI.
    std::fs::write(
        tmp.path().join(".hyalo.toml"),
        "[links]\ncase_insensitive = \"false\"\n",
    )
    .unwrap();
    create_index(&tmp);

    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path().to_str().unwrap())
        .args(["links", "fix", "--dry-run", "--case-insensitive"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "flag alone must make the case-folded link resolved: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout must be JSON");
    let fixes = json["results"]["case_mismatches"]
        .as_u64()
        .or_else(|| json["results"]["case_mismatch_count"].as_u64())
        .or_else(|| json["results"]["fixes"].as_array().map(|a| a.len() as u64))
        .unwrap_or(0);
    let broken = json["results"]["broken"]
        .as_u64()
        .or_else(|| {
            json["results"]["broken_fixes"]
                .as_array()
                .map(|a| a.len() as u64)
        })
        .unwrap_or(0);
    assert_eq!(
        fixes, 0,
        "no case-mismatch fixes may be offered under the flag: {json}"
    );
    assert_eq!(
        broken, 0,
        "the case-folded link must not be reported as broken either: {json}"
    );
}
