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

fn run_named(dir: &std::path::Path, args: &[&str]) -> (String, String) {
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(dir)
        .args(args)
        .assert()
        .success()
        .get_output()
        .clone();
    (
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

/// A named oversized file is a promise (DEC-301): the record says why it is
/// empty, and stderr says so exactly once, `-q` or not (PR #372 review).
#[test]
fn named_oversized_file_is_marked_and_announced_under_quiet() {
    let tmp = TempDir::new().unwrap();
    write_oversized(tmp.path());
    for quiet in [None, Some("-q")] {
        let mut args = vec!["find", "--file", "big.md", "--format", "json"];
        args.extend(quiet);
        let (stdout, stderr) = run_named(tmp.path(), &args);
        let value: Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(value["results"][0]["skipped"], "oversized", "{value}");
        assert_eq!(
            stderr.matches("MiB limit").count() + stderr.matches("was not read").count(),
            1,
            "{quiet:?}: expected one size-limit line, got: {stderr}"
        );
    }
}

/// `lint` reports the oversized file as a FILE violation and prints the
/// advisory once, not twice (PR #372 review).
#[test]
fn lint_reports_oversized_file_once() {
    let tmp = TempDir::new().unwrap();
    write_oversized(tmp.path());
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["lint", "big.md", "--format", "json"])
        .output()
        .unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(
        stderr.matches("MiB limit").count(),
        1,
        "expected one advisory, got: {stderr}"
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value.to_string().contains("FILE"), "{value}");
}
