//! Regression tests for the SIGPIPE / broken-pipe panic: `hyalo find --format json | head`
//! used to crash with `thread 'main' panicked at ...: failed printing to stdout: Broken pipe
//! (os error 32)` because `println!` panics on any write error. See `broken_pipe.rs` in the
//! `hyalo-cli` crate for the fix (SIGPIPE reset on Unix + a panic-hook backstop).
//!
//! Unix-only: reliably forcing a broken-pipe write requires closing the read end of a pipe
//! while the child is still writing, which depends on precise process-signal timing that
//! only applies on Unix (`SIGPIPE`). Windows has no equivalent signal; the panic-hook half of
//! the fix still applies there but isn't exercised by this test.

#![cfg(unix)]

use std::io::Read;
use std::process::{Command, Stdio};

use super::common::write_md;

/// Populate `dir` with enough markdown files that `hyalo find --limit 0` writes more JSON
/// than a single pipe buffer can hold — necessary so the child is still writing when the
/// read end closes, which is what actually triggers a broken-pipe write error. `find`'s
/// default result cap keeps output tiny regardless of vault size, hence `--limit 0` below.
fn setup_many_files(dir: &std::path::Path) {
    for i in 0..3000 {
        write_md(
            dir,
            &format!("note-{i:04}.md"),
            &format!(
                "---\ntitle: Note {i}\ntags:\n  - bulk\n---\n\nBody text for note {i} padded to add bulk to the output so the pipe buffer fills up quickly during the test. Lorem ipsum dolor sit amet consectetur adipiscing elit.\n"
            ),
        );
    }
}

#[test]
fn find_does_not_panic_when_reader_closes_pipe_early() {
    let tmp = tempfile::tempdir().unwrap();
    setup_many_files(tmp.path());

    let hyalo_bin = assert_cmd::cargo::cargo_bin("hyalo");
    let mut child = Command::new(&hyalo_bin)
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "--no-hints",
            "find",
            "--format",
            "json",
            "--limit",
            "0",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    // Read a small prefix, then drop the handle to close our end of the pipe
    // while the child is (almost certainly) still writing the rest.
    let mut stdout = child.stdout.take().unwrap();
    let mut buf = [0u8; 64];
    let _ = stdout.read(&mut buf);
    drop(stdout);

    let output = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("panicked at"),
        "hyalo should not panic when the downstream reader closes the pipe, stderr: {stderr}"
    );
}
