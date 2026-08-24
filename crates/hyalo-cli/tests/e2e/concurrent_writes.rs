//! T-1 (iter-224): concurrency and crash-recovery coverage for the atomic
//! write path (`fs_util.rs`: temp file + fsync + rename + dir-fsync).
//!
//! Before this file, nothing exercised that machinery under contention or
//! interruption: no two-process mutation race, no kill-mid-write. Both tests
//! below are built so their assertions hold regardless of exact scheduling —
//! no sleep-and-hope timing, so there is nothing here to retry-loop around.

use super::common::md;
use std::process::Command as StdCommand;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

/// `assert_cmd::Command` doesn't expose a public `spawn()` (only blocking
/// `output()`/`ok()`), so both tests below need a raw `std::process::Command`
/// for concurrent, non-blocking children they can `kill()`/`wait()`
/// individually. Mirrors `broken_pipe.rs`'s justification for the same
/// pattern: this bypasses the `CARGO_TARGET_<TRIPLE>_RUNNER` that cross/qemu
/// configures in the aarch64 release matrix, which is why both tests below
/// carry the matching `#[cfg_attr(... ignore)]`.
fn hyalo_cmd() -> StdCommand {
    StdCommand::new(assert_cmd::cargo::cargo_bin("hyalo"))
}

/// `N` processes racing `hyalo set` on the same file, with a reader thread
/// sampling the file throughout, must never observe a torn state.
///
/// `atomic_write` never opens the destination for writing — it writes a
/// sibling temp file and `rename()`s it into place — so any reader that opens
/// the destination mid-race sees either the pre-race content or one writer's
/// complete output, never a mix of the two. This test is the regression net
/// for that guarantee: a reader thread continuously re-reads the frontmatter
/// while N `hyalo set` processes race on the same file, and every read must
/// parse cleanly with a `counter` value from the known set.
#[test]
#[cfg_attr(
    all(target_os = "linux", target_arch = "aarch64"),
    ignore = "raw std::process spawn bypasses the cross/qemu target runner \
              in the aarch64 release matrix and cannot exec the \
              target-arch binary"
)]
fn concurrent_set_never_observed_partial() {
    const WRITERS: i64 = 12;

    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("note.md"),
        md!(r"
---
title: Note
counter: -1
---
Body text.
"),
    )
    .unwrap();
    let path = tmp.path().join("note.md");

    let stop = Arc::new(AtomicBool::new(false));
    let reader_path = path.clone();
    let reader_stop = Arc::clone(&stop);
    let reader = thread::spawn(move || {
        let mut samples = 0usize;
        while !reader_stop.load(Ordering::Relaxed) {
            match hyalo_core::frontmatter::read_frontmatter(&reader_path) {
                Ok(props) => {
                    samples += 1;
                    let counter = props
                        .get("counter")
                        .and_then(serde_json::Value::as_i64)
                        .unwrap_or_else(|| panic!("torn read: no numeric `counter` in {props:?}"));
                    assert!(
                        (-1..WRITERS).contains(&counter),
                        "torn or corrupt read: counter={counter} outside the \
                         range any single writer could have produced"
                    );
                }
                Err(e) => panic!("torn/corrupt read while writers were racing: {e:#}"),
            }
        }
        samples
    });

    let mut children = Vec::new();
    for i in 0..WRITERS {
        let child = hyalo_cmd()
            .args([
                "--no-hints",
                "--dir",
                tmp.path().to_str().unwrap(),
                "set",
                "--file",
                "note.md",
                "--property",
            ])
            .arg(format!("counter={i}"))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        children.push(child);
    }

    // Collect statuses without asserting yet: `set`'s `check_mtime` guard
    // (crates/hyalo-cli/src/commands/set.rs) makes a losing writer detect
    // that the file changed since it read it and refuse to write rather than
    // silently clobber the winner — that's a *correct* refusal under
    // contention, not corruption, so some children failing here is expected.
    // The reader thread above is what actually verifies no torn state ever
    // occurs; nothing in this loop may panic before it is joined below, or a
    // panic here would drop `tmp` (deleting the directory) while the reader
    // thread is still concurrently reading from it.
    let mut successes = 0usize;
    for mut child in children {
        if child.wait().unwrap().success() {
            successes += 1;
        }
    }

    stop.store(true, Ordering::Relaxed);
    let samples = match reader.join() {
        Ok(samples) => samples,
        Err(payload) => std::panic::resume_unwind(payload),
    };

    assert!(
        successes > 0,
        "no racing writer succeeded — every `hyalo set` lost the mtime race, \
         so this run exercised nothing"
    );
    assert!(
        samples > 0,
        "reader thread never completed a read during the race — \
         widen WRITERS or the race window to make this test meaningful"
    );

    let final_props = hyalo_core::frontmatter::read_frontmatter(&path).unwrap();
    let final_counter = final_props["counter"].as_i64().unwrap();
    assert!(
        (0..WRITERS).contains(&final_counter),
        "final counter {final_counter} was not written by any of the racing processes"
    );
}

/// SIGKILL delivered to `hyalo set` mid-write must never leave the
/// destination file torn.
///
/// `atomic_write` can only be interrupted in two places that matter here:
/// before `rename()` (destination untouched) or after it (destination fully
/// replaced) — `rename()` itself is atomic at the OS level, so there is no
/// window in which a kill can leave a half-old/half-new file. This test
/// doesn't guess a sleep duration to land the kill inside the write: it polls
/// for the sibling temp file (`tempfile`'s default `.tmp*` prefix) that
/// `atomic_write` creates before the rename, and kills as soon as it appears.
/// Because the destination assertion below accepts *either* valid outcome,
/// the test's correctness doesn't depend on the kill actually landing inside
/// the write window — only its usefulness does, and a large body widens that
/// window so it usually does.
#[cfg(unix)]
#[test]
#[cfg_attr(
    all(target_os = "linux", target_arch = "aarch64"),
    ignore = "raw std::process spawn bypasses the cross/qemu target runner \
              in the aarch64 release matrix and cannot exec the \
              target-arch binary"
)]
fn kill_mid_write_never_leaves_torn_destination() {
    let tmp = tempfile::tempdir().unwrap();

    // Large enough that write + fsync takes long enough for the poll loop
    // below to reliably observe the temp file before the rename lands.
    let big_body = "x".repeat(8 * 1024 * 1024);
    let original = format!("---\ntitle: Note\ncounter: -1\n---\n{big_body}\n");
    std::fs::write(tmp.path().join("note.md"), &original).unwrap();

    let mut child = hyalo_cmd()
        .args([
            "--no-hints",
            "--dir",
            tmp.path().to_str().unwrap(),
            "set",
            "--file",
            "note.md",
            "--property",
            "counter=999",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let saw_temp_file = std::fs::read_dir(tmp.path())
            .into_iter()
            .flatten()
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().starts_with(".tmp"));
        if saw_temp_file {
            break;
        }
        // The process may also have already finished (fast disk); either way
        // stop polling once it's no longer racing.
        thread::sleep(Duration::from_micros(200));
    }

    let _ = child.kill();
    let _ = child.wait();

    let final_content = std::fs::read(tmp.path().join("note.md")).unwrap();
    let final_str = String::from_utf8_lossy(&final_content);
    let is_original = final_content == original.as_bytes();
    let is_complete_new = final_str.starts_with("---\n")
        && final_str.contains("counter: 999")
        && final_str.ends_with(&format!("{big_body}\n"));
    assert!(
        is_original || is_complete_new,
        "destination left in a torn state (neither original nor a complete \
         new write): {} bytes, starts with {:?}",
        final_content.len(),
        final_str.chars().take(80).collect::<String>()
    );

    // No stray temp file left behind by the kill should break the next write
    // to the same file — `NamedTempFile` names are randomized per call, so a
    // leftover `.tmp*` sibling must not collide with (or otherwise disturb) a
    // fresh `atomic_write`.
    let mut retry = assert_cmd::Command::cargo_bin("hyalo").unwrap();
    retry.args([
        "--no-hints",
        "--dir",
        tmp.path().to_str().unwrap(),
        "set",
        "--file",
        "note.md",
        "--property",
        "counter=1000",
    ]);
    let out = retry.output().unwrap();
    assert!(
        out.status.success(),
        "follow-up write failed after kill-mid-write, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// Non-unix platforms have no `Child::kill()` = SIGKILL equivalent that
// interrupts mid-syscall the way `kill_mid_write_never_leaves_torn_destination`
// needs, so that scenario is intentionally not ported. The concurrency test
// above still runs everywhere and covers the same `atomic_write` code path
// from the other angle (contention, not interruption).
