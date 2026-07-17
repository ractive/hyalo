//! e2e tests for the `hyalo madr toc` generator (iter-173: adopt + text output).

use super::common::{hyalo_no_hints, write_md};
use std::fs;
use tempfile::TempDir;

/// Build a small ADR directory with one decision record.
fn make_adr_bundle() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "docs/decisions/0001-record.md",
        "---\ntitle: Record decisions\nstatus: accepted\ndate: 2026-07-17\n---\n# Record decisions\n",
    );
    tmp
}

/// A marker-less `README.md` in the ADR dir is adopted: its hand-written body
/// survives and the managed TOC region is appended.
#[test]
fn madr_toc_adopts_marker_less_readme() {
    let tmp = make_adr_bundle();
    write_md(
        tmp.path(),
        "docs/decisions/README.md",
        "# Decisions\n\nHand-written overview that must survive.\n",
    );
    let out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", ".", "madr", "toc", "--apply"])
        .output()
        .unwrap();
    assert!(out.status.success(), "apply failed: {out:?}");

    let readme = fs::read_to_string(tmp.path().join("docs/decisions/README.md")).unwrap();
    assert!(
        readme.contains("Hand-written overview that must survive."),
        "curated body must survive adopt: {readme}"
    );
    assert!(
        readme.contains("<!-- madr:toc:begin -->"),
        "region appended: {readme}"
    );
    assert!(
        readme.contains("Record decisions"),
        "TOC row present: {readme}"
    );
    // MD022: blank line after the begin marker.
    assert!(
        readme.contains("<!-- madr:toc:begin -->\n\n"),
        "blank line after begin marker: {readme}"
    );
}

/// `--replace` discards the marker-less body; the default adopts it.
#[test]
fn madr_toc_replace_overwrites() {
    let tmp = make_adr_bundle();
    write_md(
        tmp.path(),
        "docs/decisions/README.md",
        "# Old\n\nThrow me away.\n",
    );
    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", ".", "madr", "toc", "--apply", "--replace"])
        .output()
        .unwrap();
    let readme = fs::read_to_string(tmp.path().join("docs/decisions/README.md")).unwrap();
    assert!(
        !readme.contains("Throw me away."),
        "--replace drops body: {readme}"
    );
    assert!(readme.contains("<!-- madr:toc:begin -->"));
}

/// `madr toc --format text` renders a readable line, not a key dump.
#[test]
fn madr_toc_text_output_is_readable() {
    let tmp = make_adr_bundle();
    let out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", ".", "--format", "text", "madr", "toc", "--apply"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("madr toc:"), "readable header: {stdout}");
    assert!(stdout.contains("README.md"), "names the TOC file: {stdout}");
    assert!(!stdout.contains("adr_dir:"), "no raw key dump: {stdout}");
}
