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
        .map(|e| {
            e.expect("dir entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
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

/// A read-only vault must still be queryable, and — per DEC-267 — link
/// resolution folds case on every platform regardless of what the underlying
/// filesystem does, so no case probe is ever written in the first place.
///
/// `index.md` links to `[[sub/Note]]` via the lowercase path `sub/note`,
/// which now resolves unconditionally: on a case-insensitive filesystem
/// (macOS, Windows) it always resolved; on a case-sensitive one (typical
/// Linux) DEC-267 makes it resolve too, since resolution no longer asks the
/// filesystem.
#[cfg(unix)]
#[test]
fn find_on_read_only_vault_resolves_case_regardless_of_filesystem() {
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
    assert!(
        !stdout.contains("index.md"),
        "the read-only vault must still resolve [[sub/note]] -> sub/Note.md \
         (DEC-267: case folds on every platform); got: {stdout}"
    );
}

#[test]
fn create_index_sweeps_orphaned_case_probe_files() {
    use std::time::{Duration, SystemTime};

    let tmp = sample_vault();
    let dir = tmp.path();

    // An orphan left behind by a probe that was killed mid-flight.
    let stale = dir.join(".hyalo-case-probe-deadbeef");
    let f = fs::File::create(&stale).expect("probe file creation should succeed");
    f.set_modified(SystemTime::now() - Duration::from_hours(1))
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

// ---------------------------------------------------------------------------
// Out-of-vault link targets (iteration 193, part C)
// ---------------------------------------------------------------------------

/// A vault whose `sub/note.md` links both above the vault root (out of scope)
/// and to a missing in-vault file (genuinely broken).
fn out_of_vault_vault() -> TempDir {
    let tmp = TempDir::new().expect("tempdir creation should succeed");
    write_md(
        tmp.path(),
        "sub/note.md",
        md!(r"
---
title: Note
---
Escapes the vault: [contributing](../../CONTRIBUTING.md).
Missing in-vault file: [gone](../gone.md).
"),
    );
    tmp
}

#[test]
fn out_of_vault_links_are_reported_separately_from_broken() {
    let tmp = out_of_vault_vault();
    let dir = tmp.path().to_str().expect("temp path should be UTF-8");

    let output = hyalo_no_hints()
        .args(["--dir", dir, "links", "--format", "json"])
        .output()
        .expect("hyalo links should run");
    assert!(
        output.status.success(),
        "links failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("links output should be JSON");
    let results = json.get("results").unwrap_or(&json);

    assert_eq!(
        results["out_of_vault"].as_u64(),
        Some(1),
        "the ../../ target belongs in out_of_vault: {results}"
    );
    assert_eq!(
        results["broken"].as_u64(),
        Some(1),
        "only the in-vault miss counts as broken: {results}"
    );
    let listed = results["out_of_vault_links"]
        .as_array()
        .expect("out_of_vault_links should be an array");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["target"].as_str(), Some("../../CONTRIBUTING.md"));
}

#[test]
fn summary_keeps_out_of_vault_out_of_the_broken_count() {
    let tmp = out_of_vault_vault();
    let dir = tmp.path().to_str().expect("temp path should be UTF-8");

    let output = hyalo_no_hints()
        .args(["--dir", dir, "summary", "--format", "json"])
        .output()
        .expect("hyalo summary should run");
    assert!(output.status.success(), "summary failed");
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("summary output should be JSON");
    let links = &json.get("results").unwrap_or(&json)["links"];

    assert_eq!(links["broken"].as_u64(), Some(1), "links: {links}");
    assert_eq!(links["out_of_vault"].as_u64(), Some(1), "links: {links}");
}

#[test]
fn find_broken_links_skips_files_whose_only_miss_is_out_of_vault() {
    let tmp = TempDir::new().expect("tempdir creation should succeed");
    write_md(
        tmp.path(),
        "sub/only-escape.md",
        md!(r"
---
title: Only Escape
---
[contributing](../../CONTRIBUTING.md)
"),
    );
    write_md(
        tmp.path(),
        "sub/real-break.md",
        md!(r"
---
title: Real Break
---
[gone](../gone.md)
"),
    );
    let dir = tmp.path().to_str().expect("temp path should be UTF-8");

    let output = hyalo_no_hints()
        .args(["--dir", dir, "find", "--broken-links", "--format", "json"])
        .output()
        .expect("hyalo find should run");
    assert!(output.status.success(), "find failed");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("real-break.md"),
        "a genuine in-vault miss must still be reported: {stdout}"
    );
    assert!(
        !stdout.contains("only-escape.md"),
        "a file whose only unresolved link escapes the vault must not be \
         reported as broken: {stdout}"
    );
}
