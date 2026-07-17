//! e2e tests for the `hyalo okf` generators (`index` and `log`).

use super::common::{hyalo_no_hints, write_md};
use std::fs;
use tempfile::TempDir;

/// Build a small OKF-shaped bundle with a couple of typed concepts and a
/// bundle-root `index.md` carrying an `okf_version` key.
fn make_bundle() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    write_md(
        dir,
        "tables/blocks.md",
        "---\ntype: BigQuery Table\ntitle: Bitcoin Blocks\ndescription: The blocks table.\n---\n# Schema\n",
    );
    write_md(
        dir,
        "tables/accounts.md",
        "---\ntype: BigQuery Table\ntitle: Accounts\n---\n# Schema\n",
    );
    write_md(
        dir,
        "references/wiki.md",
        "---\ntype: Reference\ntitle: Bitcoin Wiki\ndescription: Overview.\n---\nBody\n",
    );
    write_md(
        dir,
        "index.md",
        "---\nokf_version: \"0.1\"\n---\n\n# Index\n",
    );
    tmp
}

// ---------------------------------------------------------------------------
// okf index
// ---------------------------------------------------------------------------

#[test]
fn okf_index_dry_run_reports_drift_and_exits_nonzero() {
    let tmp = make_bundle();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", ".", "okf", "index"])
        .output()
        .unwrap();
    // Dry run: three index.md files would change → exit code 1 (CI drift signal).
    assert_eq!(
        output.status.code(),
        Some(1),
        "dry-run with drift must exit 1"
    );
    // No file should have been written.
    assert!(!tmp.path().join("tables/index.md").exists());
}

#[test]
fn okf_index_apply_generates_grouped_index() {
    let tmp = make_bundle();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", ".", "okf", "index", "--apply"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let tables = fs::read_to_string(tmp.path().join("tables/index.md")).unwrap();
    assert!(
        tables.contains("## BigQuery Table"),
        "grouped by type: {tables}"
    );
    // Relative links + sorted by title (Accounts before Bitcoin Blocks).
    assert!(tables.contains("* [Accounts](accounts.md)"), "{tables}");
    assert!(
        tables.contains("* [Bitcoin Blocks](blocks.md) - The blocks table."),
        "{tables}"
    );
    let acc = tables.find("Accounts").unwrap();
    let blk = tables.find("Bitcoin Blocks").unwrap();
    assert!(acc < blk, "sorted by title: {tables}");
    // Managed-region markers present.
    assert!(tables.contains("<!-- okf:index:begin -->"), "{tables}");
    assert!(tables.contains("<!-- okf:index:end -->"), "{tables}");

    // Root index lists subdirectories and preserves okf_version.
    let root = fs::read_to_string(tmp.path().join("index.md")).unwrap();
    assert!(root.contains("okf_version: \"0.1\""), "{root}");
    assert!(root.contains("* [tables](tables/index.md)"), "{root}");
    assert!(
        root.contains("* [references](references/index.md)"),
        "{root}"
    );
}

#[test]
fn okf_index_apply_is_idempotent() {
    let tmp = make_bundle();
    for _ in 0..2 {
        hyalo_no_hints()
            .current_dir(tmp.path())
            .args(["--dir", ".", "okf", "index", "--apply"])
            .output()
            .unwrap();
    }
    // A dry run after two applies must report no drift and exit 0.
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", ".", "okf", "index"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "idempotent: dry-run after apply exits 0; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn okf_index_preserves_prose_outside_markers() {
    let tmp = make_bundle();
    // Seed tables/index.md with hand-written prose around a managed region.
    write_md(
        tmp.path(),
        "tables/index.md",
        "# Tables\n\nHand-written intro.\n\n<!-- okf:index:begin -->\nOLD\n<!-- okf:index:end -->\n\nHand-written footer.\n",
    );
    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", ".", "okf", "index", "tables", "--apply"])
        .output()
        .unwrap();
    let tables = fs::read_to_string(tmp.path().join("tables/index.md")).unwrap();
    assert!(tables.contains("Hand-written intro."), "{tables}");
    assert!(tables.contains("Hand-written footer."), "{tables}");
    assert!(!tables.contains("OLD"), "old list replaced: {tables}");
    assert!(tables.contains("* [Accounts](accounts.md)"), "{tables}");
}

#[test]
fn okf_index_scope_limits_subtree() {
    let tmp = make_bundle();
    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", ".", "okf", "index", "tables", "--apply"])
        .output()
        .unwrap();
    assert!(tmp.path().join("tables/index.md").exists());
    // references/ was out of scope — no index.md written there.
    assert!(!tmp.path().join("references/index.md").exists());
}

#[test]
fn okf_index_generates_index_for_intermediate_dir_with_no_direct_concepts() {
    // `a/` holds no concept files directly — only a nested `a/b/concept.md` —
    // but the root index links to `a/index.md`, so `a/` must get one too.
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a/b/concept.md",
        "---\ntype: Thing\ntitle: Deep Concept\n---\nBody\n",
    );
    write_md(
        tmp.path(),
        "index.md",
        "---\nokf_version: \"0.1\"\n---\n\n# Index\n",
    );
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", ".", "okf", "index", "--apply"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let root = fs::read_to_string(tmp.path().join("index.md")).unwrap();
    assert!(root.contains("* [a](a/index.md)"), "{root}");
    assert!(
        tmp.path().join("a/index.md").exists(),
        "intermediate directory a/ must get an index.md so the root link resolves"
    );
    let a_index = fs::read_to_string(tmp.path().join("a/index.md")).unwrap();
    assert!(a_index.contains("* [b](b/index.md)"), "{a_index}");
    assert!(tmp.path().join("a/b/index.md").exists());
}

// ---------------------------------------------------------------------------
// okf log
// ---------------------------------------------------------------------------

#[test]
fn okf_log_creates_root_log_with_date_heading() {
    let tmp = make_bundle();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "--dir",
            ".",
            "okf",
            "log",
            "--message",
            "Added blocks table",
            "--apply",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let log = fs::read_to_string(tmp.path().join("log.md")).unwrap();
    assert!(log.starts_with("# Log"), "{log}");
    // A YYYY-MM-DD heading and the entry are present.
    assert!(log.contains("- Added blocks table"), "{log}");
    assert!(
        log.lines().any(|l| l.starts_with("## 20")),
        "date heading present: {log}"
    );
}

#[test]
fn okf_log_newest_first_within_a_day() {
    let tmp = make_bundle();
    for msg in ["First", "Second"] {
        hyalo_no_hints()
            .current_dir(tmp.path())
            .args(["--dir", ".", "okf", "log", "--message", msg, "--apply"])
            .output()
            .unwrap();
    }
    let log = fs::read_to_string(tmp.path().join("log.md")).unwrap();
    let first = log.find("First").unwrap();
    let second = log.find("Second").unwrap();
    assert!(second < first, "newest entry first: {log}");
    // Only one date heading.
    let headings = log.lines().filter(|l| l.starts_with("## 20")).count();
    assert_eq!(headings, 1, "single date heading: {log}");
}

#[test]
fn okf_log_action_word_prefix() {
    let tmp = make_bundle();
    hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "--dir",
            ".",
            "okf",
            "log",
            "--action",
            "Update",
            "--message",
            "Refreshed schema",
            "--apply",
        ])
        .output()
        .unwrap();
    let log = fs::read_to_string(tmp.path().join("log.md")).unwrap();
    assert!(log.contains("- **Update:** Refreshed schema"), "{log}");
}

#[test]
fn okf_log_directory_target_writes_scoped_log() {
    let tmp = make_bundle();
    hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "--dir",
            ".",
            "okf",
            "log",
            "tables",
            "--message",
            "Table note",
            "--apply",
        ])
        .output()
        .unwrap();
    assert!(tmp.path().join("tables/log.md").exists());
    // Root log untouched.
    assert!(!tmp.path().join("log.md").exists());
}

#[test]
fn okf_log_rejects_path_escape() {
    let tmp = make_bundle();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "--dir",
            ".",
            "okf",
            "log",
            "../escape",
            "--message",
            "x",
            "--apply",
        ])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "path escape must be rejected: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn okf_log_dry_run_does_not_write() {
    let tmp = make_bundle();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", ".", "okf", "log", "--message", "Nope"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        !tmp.path().join("log.md").exists(),
        "dry-run must not create log.md"
    );
}

// ---------------------------------------------------------------------------
// Drill-down hint: `okf index`/`okf log` point authors at the validator.
// ---------------------------------------------------------------------------

#[test]
fn okf_index_output_emits_lint_profile_hint() {
    let tmp = make_bundle();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", ".", "--format", "json", "okf", "index"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("hyalo lint --profile okf"),
        "okf index output must hint at the conformance validator: {stdout}"
    );
}

#[test]
fn okf_log_output_emits_lint_profile_hint() {
    let tmp = make_bundle();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "--dir",
            ".",
            "--format",
            "json",
            "okf",
            "log",
            "--message",
            "Added a table",
            "--apply",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("hyalo lint --profile okf"),
        "okf log output must hint at the conformance validator: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// iter-173: non-destructive adoption, skip-and-warn, ignore, lint-clean output
// ---------------------------------------------------------------------------

/// A marker-less `index.md` with hand-written prose must be *adopted* on the
/// first `--apply`: every original line survives and the managed region is
/// appended. A second apply is idempotent, and a dry-run then exits 0.
#[test]
fn okf_index_adopts_marker_less_index_preserving_body() {
    let tmp = make_bundle();
    // Overwrite the root index with a marker-less, hand-curated file (still
    // carrying the okf_version frontmatter, plus prose and a manual list).
    write_md(
        tmp.path(),
        "index.md",
        "---\nokf_version: \"0.1\"\n---\n\n# Curated Index\n\nHand-written intro paragraph.\n\n- a manual bullet\n- another manual bullet\n",
    );

    let out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", ".", "okf", "index", "--apply"])
        .output()
        .unwrap();
    assert!(out.status.success(), "apply failed: {out:?}");

    let root = fs::read_to_string(tmp.path().join("index.md")).unwrap();
    // Every hand-written line survives (RB-2: zero bytes lost).
    assert!(root.contains("# Curated Index"), "heading lost: {root}");
    assert!(
        root.contains("Hand-written intro paragraph."),
        "prose lost: {root}"
    );
    assert!(
        root.contains("- a manual bullet"),
        "manual list lost: {root}"
    );
    assert!(
        root.contains("- another manual bullet"),
        "manual list lost: {root}"
    );
    // The managed region was appended.
    assert!(
        root.contains("<!-- okf:index:begin -->"),
        "no region: {root}"
    );
    assert!(
        root.contains("okf_version: \"0.1\""),
        "frontmatter lost: {root}"
    );

    // Second apply is idempotent; a subsequent dry-run reports no drift.
    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", ".", "okf", "index", "--apply"])
        .output()
        .unwrap();
    let dry = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", ".", "okf", "index"])
        .output()
        .unwrap();
    assert!(
        dry.status.success(),
        "adopt then re-apply must be idempotent (dry-run exits 0); stderr: {}",
        String::from_utf8_lossy(&dry.stderr)
    );
}

/// `--dry-run` on a marker-less index prints an explicit `adopt` notice naming
/// the count of preserved lines, distinct from `update`.
#[test]
fn okf_index_dry_run_reports_adopt_action() {
    let tmp = make_bundle();
    write_md(
        tmp.path(),
        "tables/index.md",
        "# Tables\n\nCurated by hand.\n\n- one\n- two\n",
    );
    let out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", ".", "--format", "json", "okf", "index", "tables"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    // JSON output is wrapped in {hints, results, total}; the payload is `results`.
    let payload = v.get("results").unwrap_or(&v);
    let files = payload["files"].as_array().unwrap();
    let entry = files
        .iter()
        .find(|f| f["file"] == "tables/index.md")
        .expect("tables/index.md must be planned");
    assert_eq!(
        entry["action"], "adopt",
        "marker-less file adopted: {stdout}"
    );
    assert!(
        entry["preserved_lines"].as_u64().unwrap() >= 1,
        "adopt reports preserved line count: {stdout}"
    );
}

/// `--replace` on a marker-less index discards its body (opt-in destructive),
/// while the default never does.
#[test]
fn okf_index_replace_overwrites_default_adopts() {
    let tmp = make_bundle();
    write_md(
        tmp.path(),
        "tables/index.md",
        "# Old Tables\n\nThrow me away with --replace.\n",
    );
    hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "--dir",
            ".",
            "okf",
            "index",
            "tables",
            "--apply",
            "--replace",
        ])
        .output()
        .unwrap();
    let replaced = fs::read_to_string(tmp.path().join("tables/index.md")).unwrap();
    assert!(
        !replaced.contains("Throw me away"),
        "--replace discards the body: {replaced}"
    );
    assert!(replaced.contains("<!-- okf:index:begin -->"));

    // Default (adopt) on a fresh marker-less file preserves the body.
    write_md(
        tmp.path(),
        "references/index.md",
        "# Keep\n\nKeep this line.\n",
    );
    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", ".", "okf", "index", "references", "--apply"])
        .output()
        .unwrap();
    let adopted = fs::read_to_string(tmp.path().join("references/index.md")).unwrap();
    assert!(
        adopted.contains("Keep this line."),
        "default adopts: {adopted}"
    );
}

/// A malformed concept file anywhere in the vault must not abort the whole run:
/// generators skip it with a stderr warning and still generate every other
/// index. Exit code stays a drift signal (1 on dry-run), not a hard error (2).
#[test]
fn okf_index_skips_malformed_file_with_warning() {
    let tmp = make_bundle();
    // Break one concept's frontmatter (unterminated YAML mapping value).
    write_md(
        tmp.path(),
        "tables/broken.md",
        "---\ntype: [unterminated\n---\n# Broken\n",
    );
    let out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", ".", "okf", "index"])
        .output()
        .unwrap();
    // Dry-run with drift → exit 1 (not 2).
    assert_eq!(
        out.status.code(),
        Some(1),
        "skip-warn keeps drift exit 1: {out:?}"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("skipping") && stderr.contains("tables/broken.md"),
        "malformed file warned on stderr: {stderr}"
    );
}

/// A scoped run (`okf index tables`) must not die on a malformed file OUTSIDE
/// the scope (ff-rdp B3 repro).
#[test]
fn okf_index_scoped_ignores_out_of_scope_malformed_file() {
    let tmp = make_bundle();
    write_md(
        tmp.path(),
        "iterations/bad.md",
        "---\ntitle: [unterminated\n---\n# Bad\n",
    );
    let out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", ".", "okf", "index", "tables", "--apply"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "scoped run must ignore an out-of-scope bad file: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The scoped index was still generated.
    assert!(tmp.path().join("tables/index.md").is_file());
    // The out-of-scope bad file was never read → no warning about it.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("iterations/bad.md"),
        "out-of-scope file must be invisible: {stderr}"
    );
}

/// `[okf] ignore` globs keep the generators out of template/fixture trees.
#[test]
fn okf_index_honors_okf_ignore_config() {
    let tmp = make_bundle();
    fs::write(
        tmp.path().join(".hyalo.toml"),
        "dir = \".\"\n[okf]\nignore = [\"_template/**\"]\n",
    )
    .unwrap();
    write_md(
        tmp.path(),
        "_template/concept.md",
        "---\ntype: Template\ntitle: Sample\n---\n# Sample\n",
    );
    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["okf", "index", "--apply"])
        .output()
        .unwrap();
    // No index.md generated inside the ignored template tree.
    assert!(
        !tmp.path().join("_template/index.md").exists(),
        "[okf] ignore must stop generation into _template/"
    );
    // The ignored concept must not leak into the root index either.
    let root = fs::read_to_string(tmp.path().join("index.md")).unwrap();
    assert!(
        !root.contains("_template"),
        "ignored tree not listed: {root}"
    );
}

/// Generated index files must be lint-clean (no MD022 ping-pong): a full
/// `lint --fix` followed by `okf index --apply` then `okf index --dry-run`
/// exits 0, and `lint` reports no MD022 on the generated file.
#[test]
fn okf_index_generated_output_is_md022_clean() {
    let tmp = make_bundle();
    // Generate, then apply lint --fix, then regenerate: no drift (no ping-pong).
    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", ".", "okf", "index", "--apply"])
        .output()
        .unwrap();
    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", ".", "lint", "--fix"])
        .output()
        .unwrap();
    let dry = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", ".", "okf", "index"])
        .output()
        .unwrap();
    assert!(
        dry.status.success(),
        "lint --fix must not create okf index drift (MD022 ping-pong); stderr: {}",
        String::from_utf8_lossy(&dry.stderr)
    );

    // Assert the generated region carries the MD022 blank line after the marker.
    let tables = fs::read_to_string(tmp.path().join("tables/index.md")).unwrap();
    assert!(
        tables.contains("<!-- okf:index:begin -->\n\n"),
        "blank line after begin marker (MD022): {tables}"
    );
}

/// `okf index --format text` renders readable per-file lines, not the
/// mis-nested `files: action: create` key dump.
#[test]
fn okf_index_text_output_is_readable() {
    let tmp = make_bundle();
    let out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", ".", "--format", "text", "okf", "index"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("okf index:"),
        "text output has a header line: {stdout}"
    );
    assert!(
        stdout.contains("create tables/index.md") || stdout.contains("create index.md"),
        "per-file action lines: {stdout}"
    );
    assert!(
        !stdout.contains("files: action:"),
        "must not emit the mis-nested key dump: {stdout}"
    );
}

/// Detect whether the temp filesystem is case-insensitive (macOS/Windows CI
/// legs). On a case-sensitive FS (typical Linux) the case test is skipped.
fn fs_is_case_insensitive(dir: &std::path::Path) -> bool {
    let lower = dir.join("hyalo_case_probe.tmp");
    fs::write(&lower, b"x").unwrap();
    let upper = dir.join("HYALO_CASE_PROBE.TMP");
    let insensitive = upper.exists();
    let _ = fs::remove_file(&lower);
    insensitive
}

/// On a case-insensitive filesystem an existing uppercase `INDEX.md` is
/// recognized as the reserved file: adopt targets it by its on-disk casing and
/// preserves its curated body (mapl BUG-2 / the 36 KB INDEX.md near-miss).
#[test]
fn okf_index_case_insensitive_targets_existing_upper_index() {
    let tmp = make_bundle();
    if !fs_is_case_insensitive(tmp.path()) {
        eprintln!("skipping: case-sensitive filesystem");
        return;
    }
    // Remove the lowercase root index the bundle seeded, then write an
    // uppercase, curated INDEX.md with no markers.
    let _ = fs::remove_file(tmp.path().join("index.md"));
    write_md(
        tmp.path(),
        "INDEX.md",
        "---\nokf_version: \"0.1\"\n---\n\n# Curated INDEX\n\nDo not destroy this hand-written line.\n",
    );
    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", ".", "okf", "index", "--apply"])
        .output()
        .unwrap();

    // The uppercase file was adopted (curated body preserved, region appended).
    let curated = fs::read_to_string(tmp.path().join("INDEX.md")).unwrap();
    assert!(
        curated.contains("Do not destroy this hand-written line."),
        "curated INDEX.md body must survive adopt: {curated}"
    );
    assert!(
        curated.contains("<!-- okf:index:begin -->"),
        "region appended: {curated}"
    );
}

/// `okf log --format text` and `madr toc --format text` render readable lines.
#[test]
fn okf_log_text_output_is_readable() {
    let tmp = make_bundle();
    let out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "--dir",
            ".",
            "--format",
            "text",
            "okf",
            "log",
            "--message",
            "Added blocks table",
            "--apply",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("okf log:"), "readable log header: {stdout}");
    assert!(
        stdout.contains("Added blocks table"),
        "entry shown: {stdout}"
    );
    assert!(!stdout.contains("entry: -"), "no raw key dump: {stdout}");
}
