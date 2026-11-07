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

/// DEC-409: Ctrl-C in the middle of a phase removes every temp file not yet
/// renamed, renames nothing more, says what was written and exits 130; every
/// note is whole, old or new.
#[cfg(unix)]
#[test]
fn an_interrupt_mid_phase_leaves_no_temporary_file_and_no_damaged_note() {
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    const NOTES: usize = 6000;
    let tmp = vault_of(NOTES);
    let program = hyalo_no_hints().get_program().to_owned();
    let mut child = std::process::Command::new(program)
        .current_dir(tmp.path())
        .args(["--no-hints", "lint", "--fix", "--rule", "MD047"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let staged = |dir: &std::path::Path| {
        (0..4).any(|i| {
            std::fs::read_dir(dir.join(format!("d{i}")))
                .into_iter()
                .flatten()
                .flatten()
                .any(|e| e.file_name().to_string_lossy().starts_with(".hyalo-tmp-"))
        })
    };
    let deadline = Instant::now() + Duration::from_secs(60);
    while !staged(tmp.path()) {
        assert!(Instant::now() < deadline, "the phase never staged a note");
        assert!(
            child.try_wait().unwrap().is_none(),
            "finished before staging"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let signalled = std::process::Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(signalled.success());
    let output = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(130), "{stderr}");
    assert!(stderr.contains("hyalo: interrupted"), "{stderr}");
    assert_eq!(
        leftovers(tmp.path()).len(),
        0,
        "{:?}",
        leftovers(tmp.path())
    );
    let (mut old, mut new) = (0, 0);
    for i in 0..NOTES {
        let content =
            std::fs::read_to_string(tmp.path().join(format!("d{}/n{i}.md", i % 4))).unwrap();
        if content.ends_with("body\n\n") {
            old += 1;
        } else if content.ends_with("body\n") {
            new += 1;
        } else {
            panic!("n{i}.md is damaged: {content:?}");
        }
    }
    assert_eq!(old + new, NOTES);
}
