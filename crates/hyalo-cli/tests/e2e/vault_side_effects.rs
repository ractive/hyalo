//! Read-only commands must not write into the vault (iteration 193).
//!
//! Historically `[links] case_insensitive = "auto"` was resolved by creating
//! and deleting a `.hyalo-case-probe-*` file in the vault root — on every
//! command, at every call site. That bumped the vault directory's mtime for
//! commands the user expects to be pure reads, and it silently disabled
//! case-insensitive link resolution on a read-only mount.

use super::common::{hyalo_no_hints, md, write_md};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Sorted list of entry names in `dir`.
fn entry_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("vault dir should be readable")
        .map(|e| e.expect("dir entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn sample_vault() -> TempDir {
    let tmp = TempDir::new().expect("tempdir creation should succeed");
    write_md(
        tmp.path(),
        "index.md",
        md!(r"
---
title: Index
tags: [root]
---
Links to [[sub/Note]].
"),
    );
    write_md(
        tmp.path(),
        "sub/Note.md",
        md!(r"
---
title: Note
tags: [leaf]
---
Body.
"),
    );
    tmp
}

/// Whether the filesystem backing `dir` folds case, determined without relying
/// on any hyalo code path.
fn host_is_case_insensitive(dir: &Path) -> bool {
    let probe = dir.join("hyalo-e2e-case-check.txt");
    fs::write(&probe, "x").expect("probe write should succeed");
    let flipped = dir.join("HYALO-E2E-CASE-CHECK.TXT");
    let result = flipped.exists();
    let _ = fs::remove_file(&probe);
    let _ = fs::remove_file(&flipped);
    result
}

#[test]
fn read_only_commands_do_not_touch_vault_dir() {
    let tmp = sample_vault();
    let dir = tmp.path();
    let dir_str = dir.to_str().expect("temp path should be UTF-8");

    let before_entries = entry_names(dir);
    let before_mtime = fs::metadata(dir)
        .expect("vault dir metadata")
        .modified()
        .expect("mtime should be available");

    for args in [
        vec!["--dir", dir_str, "find", "--count"],
        vec!["--dir", dir_str, "summary"],
        vec!["--dir", dir_str, "tags"],
        vec!["--dir", dir_str, "find", "--broken-links"],
    ] {
        let output = hyalo_no_hints()
            .args(&args)
            .output()
            .expect("hyalo should run");
        assert!(
            output.status.success(),
            "{args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        assert_eq!(
            entry_names(dir),
            before_entries,
            "{args:?} added or removed an entry in the vault root"
        );
        let after_mtime = fs::metadata(dir)
            .expect("vault dir metadata")
            .modified()
            .expect("mtime should be available");
        assert_eq!(
            after_mtime, before_mtime,
            "{args:?} bumped the vault directory's mtime"
        );
    }

    // Belt and braces: no probe file may survive under any casing.
    assert!(
        before_entries
            .iter()
            .all(|n| !n.to_ascii_lowercase().starts_with(".hyalo-case-probe-")),
        "vault root contains a case-probe file: {before_entries:?}"
    );
}

/// A read-only vault must still be queryable, and — this is the behaviour
/// change — case-insensitive link resolution must follow the *filesystem*
/// rather than silently switching off because no probe file could be written.
///
/// `index.md` links to `[[sub/Note]]` via the lowercase path `sub/note`, which
/// only resolves when case-insensitive path lookup is on. So on a
/// case-insensitive filesystem (macOS, Windows) nothing is broken; on a
/// case-sensitive one (typical Linux) the link is correctly reported broken.
#[cfg(unix)]
#[test]
fn find_on_read_only_vault_resolves_case_per_filesystem() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = TempDir::new().expect("tempdir creation should succeed");
    let dir = tmp.path();
    write_md(
        dir,
        "index.md",
        md!(r"
---
title: Index
---
Links to [[sub/note]].
"),
    );
    write_md(
        dir,
        "sub/Note.md",
        md!(r"
---
title: Note
---
Body.
"),
    );

    let case_insensitive_fs = host_is_case_insensitive(dir);

    let mut perms = fs::metadata(dir).expect("metadata").permissions();
    perms.set_mode(0o555);
    fs::set_permissions(dir, perms).expect("chmod should succeed");

    let output = hyalo_no_hints()
        .args([
            "--dir",
            dir.to_str().expect("temp path should be UTF-8"),
            "find",
            "--broken-links",
            "--format",
            "json",
        ])
        .output()
        .expect("hyalo find should run");

    let mut restore = fs::metadata(dir).expect("metadata").permissions();
    restore.set_mode(0o755);
    let _ = fs::set_permissions(dir, restore);

    assert!(
        output.status.success(),
        "find on a read-only vault must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mentions_index = stdout.contains("index.md");

    if case_insensitive_fs {
        assert!(
            !mentions_index,
            "on a case-insensitive filesystem the read-only vault must still \
             resolve [[sub/note]] -> sub/Note.md (case-insensitive mode ON); got: {stdout}"
        );
    } else {
        assert!(
            mentions_index,
            "on a case-sensitive filesystem [[sub/note]] must stay broken \
             (case-insensitive mode OFF); got: {stdout}"
        );
    }
}

#[test]
fn create_index_sweeps_orphaned_case_probe_files() {
    use std::time::{Duration, SystemTime};

    let tmp = sample_vault();
    let dir = tmp.path();

    // An orphan left behind by a probe that was killed mid-flight.
    let stale = dir.join(".hyalo-case-probe-deadbeef");
    let f = fs::File::create(&stale).expect("probe file creation should succeed");
    f.set_modified(SystemTime::now() - Duration::from_secs(3600))
        .expect("setting mtime should succeed");
    drop(f);

    // A probe file that another process may still be using right now.
    let fresh = dir.join(".hyalo-case-probe-cafe");
    fs::write(&fresh, "x").expect("probe file creation should succeed");

    let output = hyalo_no_hints()
        .args([
            "--dir",
            dir.to_str().expect("temp path should be UTF-8"),
            "create-index",
        ])
        .output()
        .expect("hyalo create-index should run");
    assert!(
        output.status.success(),
        "create-index failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(!stale.exists(), "stale probe file should have been swept");
    assert!(
        fresh.exists(),
        "a probe file younger than the threshold must be left alone"
    );
    assert!(
        dir.join("index.md").exists(),
        "the sweep must not touch real vault files"
    );
}
