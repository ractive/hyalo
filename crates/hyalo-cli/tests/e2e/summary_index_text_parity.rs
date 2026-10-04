//! Iteration 306 review (P2): `summary --index --format text` printed a raw
//! key:value dump (all tags, `dead_ends:`, `dir:`, …) instead of the compact
//! layout plain `summary --format text` uses — JSON parity was fine, only the
//! text renderer disagreed. The cause: `VaultSummary`'s `index_format_version`
//! field (present only under `--index`) produced a key signature
//! `lookup_filter` did not recognize, falling back to the generic dump.
//!
//! This pins that `--index` text output uses the same compact renderer, with
//! parity against the disk-scan text modulo the index-only fields.

use super::common::{hyalo_no_hints, write_md};
use tempfile::TempDir;

fn setup_vault() -> TempDir {
    let tmp = TempDir::new().expect("tempdir");
    write_md(
        tmp.path(),
        "a.md",
        "---\ntitle: A\nstatus: draft\n---\n# A\n",
    );
    write_md(tmp.path(), "b.md", "# B\n\n[[a]]\n");
    tmp
}

#[test]
fn summary_index_text_uses_the_compact_renderer_not_a_raw_key_dump() {
    let tmp = setup_vault();
    let dir = tmp.path().to_str().expect("utf-8 path");

    let create = hyalo_no_hints()
        .args(["--dir", dir, "create-index"])
        .output()
        .expect("create-index should run");
    assert!(
        create.status.success(),
        "create-index failed: {}",
        String::from_utf8_lossy(&create.stderr)
    );

    let output = hyalo_no_hints()
        .args(["--dir", dir, "summary", "--index", "--format", "text"])
        .output()
        .expect("summary --index should run");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("Files: "),
        "expected the compact 'Files: N (...)' line, got:\n{stdout}"
    );
    assert!(
        stdout.contains("Links: "),
        "expected the compact 'Links: ...' line, got:\n{stdout}"
    );
    // The raw-dump renderer prints every key verbatim, `index_format_version`
    // included; the compact renderer never mentions it.
    assert!(
        !stdout.contains("index_format_version"),
        "fell back to the generic key dump, got:\n{stdout}"
    );
    assert!(
        !stdout.contains("dead_ends:"),
        "fell back to the generic key dump (snake_case key line), got:\n{stdout}"
    );
}

#[test]
fn summary_index_text_matches_disk_scan_text_modulo_index_fields() {
    let tmp = setup_vault();
    let dir = tmp.path().to_str().expect("utf-8 path");

    let disk = hyalo_no_hints()
        .args(["--dir", dir, "summary", "--format", "text"])
        .output()
        .expect("disk summary should run");
    assert!(disk.status.success());
    let disk_stdout = String::from_utf8_lossy(&disk.stdout).into_owned();

    let create = hyalo_no_hints()
        .args(["--dir", dir, "create-index"])
        .output()
        .expect("create-index should run");
    assert!(create.status.success());

    let indexed = hyalo_no_hints()
        .args(["--dir", dir, "summary", "--index", "--format", "text"])
        .output()
        .expect("indexed summary should run");
    assert!(indexed.status.success());
    let indexed_stdout = String::from_utf8_lossy(&indexed.stdout).into_owned();

    // Both reports share the same compact field lines; the hinted follow-up
    // commands differ only by the trailing `--index` flag, which the disk
    // scan's hints never carry. Compare the body lines that both share
    // (everything up to the blank line before the hints block).
    let body = |s: &str| -> String {
        s.split("\n\n")
            .next()
            .unwrap_or(s)
            .lines()
            // The `note: kb dir: .` banner line and ordering are already
            // identical; keep the comparison to the report lines proper.
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(
        body(&disk_stdout),
        body(&indexed_stdout),
        "disk scan:\n{disk_stdout}\n---\nindexed:\n{indexed_stdout}"
    );
}
