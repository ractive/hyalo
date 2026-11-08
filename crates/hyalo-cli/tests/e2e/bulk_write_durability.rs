//! DEC-408: a bulk write phase stages every replacement, flushes once, then
//! renames. These pin the parts of that a user can observe from the CLI; the
//! order of the steps is pinned in `hyalo_core::rooted`'s tests.

use super::common::{hyalo_no_hints, write_md};
use tempfile::TempDir;

/// Every file under `dir` whose name is not a note: the temporary files.
fn leftovers(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(next).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_none_or(|e| e != "md")
                && path.file_name().is_none_or(|n| n != ".hyalo.toml")
            {
                found.push(path);
            }
        }
    }
    found
}

fn vault_of(count: usize) -> TempDir {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    for i in 0..count {
        write_md(
            tmp.path(),
            &format!("d{}/n{i}.md", i % 4),
            &format!("---\ntitle: Note {i}\n---\nbody\n\n"),
        );
    }
    tmp
}

/// A phase may stage more notes than the process may keep open: a staged note
/// holds a path and no descriptor.
#[cfg(unix)]
#[test]
fn a_phase_larger_than_the_open_file_limit_writes_every_note() {
    const NOTES: usize = 400;
    let tmp = vault_of(NOTES);
    let program = hyalo_no_hints().get_program().to_owned();
    // 64 descriptors is far fewer than 400 notes, and still enough for the
    // process itself (rayon workers, the vault walk, one temp file).
    for args in [
        &["set", "--glob", "**/*.md", "--property", "status=done"][..],
        &["lint", "--fix", "--rule", "MD047"][..],
    ] {
        let output = std::process::Command::new("sh")
            .current_dir(tmp.path())
            .arg("-c")
            .arg("ulimit -n 64 && exec \"$0\" --no-hints \"$@\"")
            .arg(&program)
            .args(args)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{args:?}: {stderr}");
        assert!(
            !stderr.contains("Too many open files"),
            "{args:?}: {stderr}"
        );
    }
    for i in 0..NOTES {
        let content =
            std::fs::read_to_string(tmp.path().join(format!("d{}/n{i}.md", i % 4))).unwrap();
        assert!(content.contains("status: done"), "n{i}.md: {content:?}");
        assert!(
            content.ends_with("body\n") && !content.ends_with("\n\n"),
            "n{i}.md: {content:?}"
        );
    }
    assert!(
        leftovers(tmp.path()).is_empty(),
        "{:?}",
        leftovers(tmp.path())
    );
}

/// `set` over several notes is one staged phase; its temporary files are gone
/// and nothing but the notes is left behind.
#[test]
fn a_multi_note_write_leaves_no_temporary_file() {
    let tmp = vault_of(12);
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["set", "--glob", "**/*.md", "--property", "status=done"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(leftovers(tmp.path()).len(), 0);
    for i in 0..12 {
        let content =
            std::fs::read_to_string(tmp.path().join(format!("d{}/n{i}.md", i % 4))).unwrap();
        assert!(content.contains("status: done"));
    }
}

#[cfg(unix)]
mod signals {
    use super::{hyalo_no_hints, leftovers, vault_of};
    use std::os::unix::process::{CommandExt, ExitStatusExt};
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    const NOTES: usize = 6000;

    fn lint_fix(
        dir: &std::path::Path,
        tmpdir: &std::path::Path,
        pre_exec: impl Fn() + Send + Sync + 'static,
    ) -> Child {
        let program = hyalo_no_hints().get_program().to_owned();
        let mut command = Command::new(program);
        command
            .current_dir(dir)
            .env("TMPDIR", tmpdir)
            .args(["--no-hints", "lint", "--fix", "--rule", "MD047"])
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        // SAFETY: only async-signal-safe calls (`signal`) run between fork and exec.
        unsafe {
            command.pre_exec(move || {
                // A shell background job hands SIGINT over ignored: the child
                // under test starts from the default, like a terminal's.
                for signal in [libc::SIGINT, libc::SIGTERM] {
                    libc::signal(signal, libc::SIG_DFL);
                }
                pre_exec();
                Ok(())
            });
        }
        command.spawn().unwrap()
    }

    fn any_temp(dir: &std::path::Path) -> bool {
        (0..4).any(|i| {
            std::fs::read_dir(dir.join(format!("d{i}")))
                .into_iter()
                .flatten()
                .flatten()
                .any(|e| e.file_name().to_string_lossy().starts_with(".hyalo-tmp-"))
        })
    }

    fn wait_for_staging(dir: &std::path::Path, child: &mut Child) {
        let staged = || any_temp(dir);
        let deadline = Instant::now() + Duration::from_secs(60);
        while !staged() {
            assert!(Instant::now() < deadline, "the phase never staged a note");
            assert!(
                child.try_wait().unwrap().is_none(),
                "finished before staging"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn kill(pid: i32, signal: i32) {
        // SAFETY: signals a live child (or its process group) of this test.
        assert_eq!(unsafe { libc::kill(pid, signal) }, 0);
    }

    fn assert_whole(dir: &std::path::Path) {
        let mut total = 0;
        for i in 0..NOTES {
            let content = std::fs::read_to_string(dir.join(format!("d{}/n{i}.md", i % 4))).unwrap();
            assert!(
                content.ends_with("body\n\n") || content.ends_with("body\n"),
                "n{i}.md is damaged: {content:?}"
            );
            total += 1;
        }
        assert_eq!(total, NOTES);
    }

    /// DEC-409: a signal in the middle of a phase removes every temp file not
    /// yet renamed, renames nothing more, says what was written and then dies
    /// by that signal, so a calling shell script stops.
    #[test]
    fn a_signal_mid_phase_cleans_up_and_dies_by_that_signal() {
        for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            let tmp = vault_of(NOTES);
            let scratch = tempfile::tempdir().unwrap();
            let mut child = lint_fix(tmp.path(), scratch.path(), || {
                // SAFETY: as above.
                unsafe { libc::signal(libc::SIGHUP, libc::SIG_DFL) };
            });
            wait_for_staging(tmp.path(), &mut child);
            kill(i32::try_from(child.id()).unwrap(), signal);
            let output = child.wait_with_output().unwrap();
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert_eq!(
                output.status.signal(),
                Some(signal),
                "signal {signal}, status {:?}: {}",
                output.status,
                stderr.chars().take(400).collect::<String>()
            );
            assert_eq!(output.status.code(), None, "{stderr}");
            assert!(stderr.contains("hyalo: interrupted"), "{stderr}");
            assert_eq!(
                leftovers(tmp.path()).len(),
                0,
                "{:?}",
                leftovers(tmp.path())
            );
            // The copies of the notes the workers took are gone too.
            let copies = std::fs::read_dir(scratch.path()).unwrap().count();
            assert_eq!(copies, 0, "signal {signal}: copies left in $TMPDIR");
            assert_whole(tmp.path());
        }
    }

    /// A terminal's Ctrl-C reaches a shell script's whole process group. The
    /// shell stops the script only if the child it was waiting for died by the
    /// signal; a child that exits 130 lets the loop go on.
    #[test]
    fn ctrl_c_on_a_script_calling_a_bulk_write_stops_the_script() {
        let tmp = vault_of(NOTES);
        let log = tmp.path().join("log.txt");
        let program = hyalo_no_hints().get_program().to_owned();
        let script = format!(
            "for i in 1 2 3; do \"{}\" --no-hints lint --fix --rule MD047 >/dev/null 2>&1; \
             echo \"iter $i rc=$?\" >> \"{}\"; done; echo finished >> \"{}\"",
            program.to_string_lossy(),
            log.display(),
            log.display()
        );
        let mut shell = Command::new("bash");
        shell.current_dir(tmp.path()).arg("-c").arg(script);
        // SAFETY: `setpgid` and `signal` are async-signal-safe.
        unsafe {
            shell.pre_exec(|| {
                libc::setpgid(0, 0);
                libc::signal(libc::SIGINT, libc::SIG_DFL);
                Ok(())
            });
        }
        let mut shell = shell.spawn().unwrap();
        wait_for_staging(tmp.path(), &mut shell);
        kill(-i32::try_from(shell.id()).unwrap(), libc::SIGINT);
        let status = shell.wait().unwrap();
        std::thread::sleep(Duration::from_millis(300));
        let log = std::fs::read_to_string(&log).unwrap_or_default();
        assert!(!log.contains("finished"), "the script went on: {log:?}");
        assert!(!log.contains("iter 2"), "the script went on: {log:?}");
        // The shell ends by the signal or with 130, whichever it gets to first.
        assert!(
            status.signal() == Some(libc::SIGINT) || status.code() == Some(130),
            "{status:?} {log:?}"
        );
        assert_eq!(leftovers(tmp.path()).len(), 0);
    }

    /// `nohup` hands SIGHUP over ignored, and a background job SIGINT: an
    /// ignored signal stays ignored and the run completes.
    #[test]
    fn an_ignored_signal_mid_phase_changes_nothing() {
        for signal in [libc::SIGHUP, libc::SIGINT] {
            let tmp = vault_of(NOTES);
            let scratch = tempfile::tempdir().unwrap();
            let mut child = lint_fix(tmp.path(), scratch.path(), move || {
                // SAFETY: as above.
                unsafe { libc::signal(signal, libc::SIG_IGN) };
            });
            wait_for_staging(tmp.path(), &mut child);
            kill(i32::try_from(child.id()).unwrap(), signal);
            let output = child.wait_with_output().unwrap();
            assert_eq!(output.status.code(), Some(0), "{signal}");
            for i in 0..NOTES {
                let content =
                    std::fs::read_to_string(tmp.path().join(format!("d{}/n{i}.md", i % 4)))
                        .unwrap();
                assert!(content.ends_with("body\n") && !content.ends_with("\n\n"));
            }
            assert_eq!(leftovers(tmp.path()).len(), 0);
        }
    }

    /// After the phase the signal takes the default action, with no report of
    /// an interrupted phase: stdout is a pipe nobody reads, so the process
    /// waits in its output write.
    #[test]
    fn a_signal_after_the_phase_takes_the_default_action() {
        let tmp = vault_of(NOTES);
        let program = hyalo_no_hints().get_program().to_owned();
        let mut command = Command::new(program);
        command
            .current_dir(tmp.path())
            .args([
                "--no-hints",
                "lint",
                "--fix",
                "--rule",
                "MD047",
                "--format",
                "json",
                "--limit",
                "0",
                "--max-per-rule",
                "0",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // SAFETY: as above.
        unsafe {
            command.pre_exec(|| {
                libc::signal(libc::SIGINT, libc::SIG_DFL);
                Ok(())
            });
        }
        let mut child = command.spawn().unwrap();
        let last = tmp
            .path()
            .join(format!("d{}/n{}.md", (NOTES - 1) % 4, NOTES - 1));
        let deadline = Instant::now() + Duration::from_secs(60);
        while !std::fs::read_to_string(&last).unwrap().ends_with("body\n") || any_temp(tmp.path()) {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        // Every fix is renamed, but `lint` is still writing its listing.
        std::thread::sleep(Duration::from_millis(1500));
        let early = child.try_wait().unwrap();
        assert!(early.is_none(), "finished before the signal: {early:?}");
        kill(i32::try_from(child.id()).unwrap(), libc::SIGINT);
        let stdout = child.stdout.take();
        let mut stderr = String::new();
        std::io::Read::read_to_string(&mut child.stderr.take().unwrap(), &mut stderr).unwrap();
        drop(stdout);
        let status = child.wait().unwrap();
        assert_eq!(status.signal(), Some(libc::SIGINT), "{stderr}");
        assert!(!stderr.contains("interrupted"), "{stderr}");
    }
}
