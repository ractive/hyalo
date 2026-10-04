//! Iteration 309: review leftovers.
//!
//! - A wikilink heading fragment keeps an inline code span written inside its
//!   brackets (DEC-348).
//! - The scanner's oversized-file skip is an advisory: printed once without
//!   `-q`, silenced by `-q`.
use super::common::{hyalo_no_hints, write_md};
use serde_json::Value;
use tempfile::TempDir;

/// One byte over the scanner's limit (`hyalo_core::scanner::MAX_FILE_SIZE`).
const OVERSIZED: u64 = 100 * 1024 * 1024 + 1;

fn write_oversized(dir: &std::path::Path) {
    let file = std::fs::File::create(dir.join("big.md")).unwrap();
    // `set_len` extends without writing: sparse on every filesystem that
    // supports it, so the test does not pay for 100 MiB of disk.
    file.set_len(OVERSIZED).unwrap();
}

fn stderr_of(dir: &std::path::Path, extra: &[&str]) -> String {
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(dir)
        .args(["summary", "--format", "json"])
        .args(extra)
        .assert()
        .success()
        .get_output()
        .clone();
    String::from_utf8(output.stderr).unwrap()
}

#[test]
fn oversized_file_skip_is_printed_once_without_quiet() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: Note\n---\nbody\n");
    write_oversized(tmp.path());
    let stderr = stderr_of(tmp.path(), &[]);
    assert_eq!(
        stderr.matches("MiB exceeds 100 MiB limit").count(),
        1,
        "expected one oversized-file advisory, got: {stderr}"
    );
    assert!(
        stderr.contains("big.md"),
        "advisory names the file: {stderr}"
    );
}

#[test]
fn oversized_file_skip_is_silenced_by_quiet() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "note.md", "---\ntitle: Note\n---\nbody\n");
    write_oversized(tmp.path());
    for quiet in ["-q", "--quiet"] {
        let stderr = stderr_of(tmp.path(), &[quiet]);
        assert!(
            !stderr.contains("MiB limit"),
            "{quiet} must silence the oversized-file advisory, got: {stderr}"
        );
    }
}

#[test]
fn wikilink_fragment_with_inline_code_resolves() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "log.md",
        "# Log\n\n## DEC-068: `links auto` ships (2026-08-18)\n\ntext\n",
    );
    write_md(
        tmp.path(),
        "a.md",
        "# A\n\nSee [[log#DEC-068: `links auto` ships (2026-08-18)]].\n\n`[[log#nowhere]]` is code.\n",
    );
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args([
            "find", "--file", "a.md", "--fields", "links", "--format", "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&output).unwrap();
    let links = value["results"][0]["links"].as_array().unwrap();
    assert_eq!(
        links.len(),
        1,
        "the code-span wikilink is not a link: {links:?}"
    );
    assert_eq!(
        links[0]["fragment"],
        "DEC-068: `links auto` ships (2026-08-18)"
    );
    assert!(links[0]["broken_anchor"].is_null(), "{:?}", links[0]);

    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["lint", "--rule", "HYALO008", "a.md", "--format", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["results"]["violations"], 0, "{value}");
}
