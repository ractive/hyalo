//! Iteration 238 follow-ups that survived iter-242: `find --filenames0`,
//! the NUL-delimited sibling of `--filenames-only` (GNU `find -print0`
//! precedent) for `xargs -0` / newline-safe consumption.
//!
//! The `--iteration <ID>` natural-key addressing that shipped in the same
//! iteration was removed again in iter-242 — sequence-keyed files are
//! addressed with a plain `--glob`.

use std::fs;
use std::process::{Command, Stdio};

use super::common::{hyalo_no_hints, md, write_md};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Vault fixture: an `iteration` type with a `{n}` filename template.
// Mirrors iteration_ergonomics.rs so behavior stays comparable across the two
// iterations' flags.
// ---------------------------------------------------------------------------

fn setup_iteration_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join(".hyalo.toml"),
        md!(r#"
dir = "."

[schema.types.iteration]
required = ["title", "type", "status"]
filename-template = "iterations/iteration-{n}-{slug}.md"

[schema.types.iteration.properties.status]
type = "enum"
values = ["planned", "in-progress", "completed", "superseded"]
"#),
    )
    .unwrap();

    write_md(
        tmp.path(),
        "iterations/iteration-206-agent-cli.md",
        md!(r"
---
title: Iter 206
type: iteration
status: planned
date: 2026-02-01
---
Body 206.

- [ ] one
- [x] two
"),
    );
    write_md(
        tmp.path(),
        "notes/random.md",
        md!(r"
---
title: A note
type: note
status: planned
---
Note body.
"),
    );
    tmp
}

// ===========================================================================
// find --filenames0
// ===========================================================================

#[test]
fn filenames0_terminates_each_path_with_nul() {
    let vault = setup_iteration_vault();
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args([
            "find",
            "--glob",
            "iterations/iteration-206-*.md",
            "--filenames0",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    // GNU `find -print0` precedent: every path (including the last) is
    // NUL-terminated. No trailing newline.
    assert_eq!(
        output.stdout,
        b"iterations/iteration-206-agent-cli.md\0".to_vec(),
        "stdout must be byte-exact NUL-terminated paths"
    );
}

#[test]
#[cfg(unix)]
fn filenames0_round_trips_through_xargs0() {
    let vault = setup_iteration_vault();
    // The whole point of the flag: `hyalo find ... --filenames0 | xargs -0 cat`
    // must consume the path list without shell quoting gymnastics. `cat` exits
    // non-zero if xargs fed it a mangled path, so success + body proves the
    // round-trip.
    let hyalo_bin = assert_cmd::cargo::cargo_bin("hyalo");
    let mut find = Command::new(&hyalo_bin)
        .args([
            "--dir",
            vault.path().to_str().unwrap(),
            "--no-hints",
            "find",
            "--property",
            "status=planned",
            "--glob",
            "iterations/iteration-206-*.md",
            "--filenames0",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let xargs = Command::new("xargs")
        .arg("-0")
        .arg("cat")
        .current_dir(vault.path())
        .stdin(find.stdout.take().unwrap())
        .output()
        .unwrap();
    // Surface hyalo's own stderr: the v0.21.0 release pipeline saw this
    // assertion fail under QEMU-emulated aarch64 with no diagnostic at all.
    let find = find.wait_with_output().unwrap();
    assert!(
        find.status.success(),
        "hyalo find exited {:?}: {}",
        find.status.code(),
        String::from_utf8_lossy(&find.stderr)
    );
    assert!(xargs.status.success(), "{}", stderr(&xargs));
    let content = String::from_utf8_lossy(&xargs.stdout);
    assert!(content.contains("Body 206."), "{content}");
}

#[test]
#[cfg(unix)]
fn filenames0_survives_newline_in_filename() {
    let vault = setup_iteration_vault();
    // The reason --filenames-only is unsafe for arbitrary filenames: a
    // newline inside a filename is indistinguishable from the delimiter.
    // NUL is the only byte a POSIX path cannot contain. (POSIX-only: NTFS
    // and Win32 forbid newlines in filenames, so the fixture can't exist.)
    let dir = vault.path();
    fs::create_dir_all(dir.join("notes")).unwrap();
    fs::write(
        dir.join("notes/weird\nname.md"),
        "---\ntitle: W\n---\nbody\n",
    )
    .unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", dir.to_str().unwrap()])
        .args(["find", "--glob", "notes/*", "--filenames0"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let stdout = output.stdout.clone();
    let entries: Vec<&[u8]> = stdout
        .split(|&b| b == 0)
        .filter(|e| !e.is_empty())
        .collect();
    assert_eq!(entries.len(), 2, "{stdout:?}");
    assert!(
        entries.contains(&"notes/random.md".as_bytes()),
        "{stdout:?}"
    );
    assert!(
        entries.contains(&&b"notes/weird\nname.md"[..]),
        "the newline-containing path must survive as ONE entry: {stdout:?}"
    );
}

#[test]
fn filenames0_zero_results_is_empty_output_exit_0() {
    let vault = setup_iteration_vault();
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["find", "--property", "status=nonexistent", "--filenames0"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(output.stdout, Vec::<u8>::new(), "no bytes at all");
}

#[test]
fn filenames0_strict_flips_exit_code_when_results_exist() {
    let vault = setup_iteration_vault();
    let dir = vault.path().to_str().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", dir])
        .args([
            "find",
            "--property",
            "status=planned",
            "--filenames0",
            "--strict",
        ])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "--strict must exit non-zero when results exist"
    );
    assert!(!output.stdout.is_empty(), "paths still printed");

    let output = hyalo_no_hints()
        .args(["--dir", dir])
        .args([
            "find",
            "--property",
            "status=nonexistent",
            "--filenames0",
            "--strict",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "--strict on zero results exits 0");
    assert!(output.stdout.is_empty());
}

#[test]
fn filenames0_conflicts_with_filenames_only_jq_count_and_format_json() {
    let vault = setup_iteration_vault();
    let dir = vault.path().to_str().unwrap();

    // --filenames-only: clap conflict → exit 2.
    let out = hyalo_no_hints()
        .args(["--dir", dir])
        .args(["find", "--filenames0", "--filenames-only"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));

    // --jq: clap conflict → exit 2.
    let out = hyalo_no_hints()
        .args(["--dir", dir])
        .args(["find", "--filenames0", "--jq", ".total"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));

    // --count: clap conflict → exit 2.
    let out = hyalo_no_hints()
        .args(["--dir", dir])
        .args(["find", "--filenames0", "--count"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));

    // --format json (explicit): runtime conflict → exit 1.
    let out = hyalo_no_hints()
        .args(["--dir", dir])
        .args(["find", "--filenames0", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    let err = stderr(&out);
    assert!(
        err.contains("--filenames0 cannot be combined with --format json"),
        "{err}"
    );

    // --format text (explicit) is fine.
    let out = hyalo_no_hints()
        .args(["--dir", dir])
        .args([
            "find",
            "--glob",
            "iterations/iteration-206-*.md",
            "--filenames0",
            "--format",
            "text",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        out.stdout,
        b"iterations/iteration-206-agent-cli.md\0".to_vec()
    );
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
