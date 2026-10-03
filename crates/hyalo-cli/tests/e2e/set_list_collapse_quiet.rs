//! `set`'s DEC-270 list-collapse advisory note must respect `-q`.
//!
//! The note ("... was a list in N file(s); `set` replaced it with a scalar
//! — use `hyalo append` to keep it a list") used to be a raw `eprintln!`, so
//! `-q` / `--quiet` could not silence it like every other advisory note in
//! the CLI. It's now routed through `crate::warn::note`, which checks quiet
//! mode.

use super::common::{hyalo_no_hints, write_md};

fn write_list_property(dir: &std::path::Path) {
    write_md(
        dir,
        "note.md",
        "---\nstatus:\n  - \"[[Backlog]]\"\n---\n\nBody\n",
    );
}

/// Without `-q`, the note is printed on stderr.
#[test]
fn list_collapse_note_appears_without_quiet() {
    let tmp = tempfile::tempdir().unwrap();
    write_list_property(tmp.path());

    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["set", "note.md", "--property", "status=Draft", "--dry-run"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("note:") && stderr.contains("was a list in 1 file"),
        "expected the list-collapse note on stderr without -q, got: {stderr:?}"
    );
}

/// With `-q` / `--quiet`, the note is suppressed.
#[test]
fn list_collapse_note_suppressed_with_quiet() {
    let tmp = tempfile::tempdir().unwrap();
    write_list_property(tmp.path());

    let output = hyalo_no_hints()
        .arg("-q")
        .arg("--dir")
        .arg(tmp.path())
        .args(["set", "note.md", "--property", "status=Draft", "--dry-run"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("was a list in"),
        "expected -q to suppress the list-collapse note, got: {stderr:?}"
    );
}
