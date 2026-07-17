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
