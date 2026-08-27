//! Iteration 238 — agent-CLI ergonomics follow-ups: `find --filenames0` and
//! `--iteration <ID>` on `read`, `task`, and `backlinks`.
//!
//! Both fold in carry-over candidates deliberately deferred out of
//! iteration 235 (`hyalo-knowledgebase/iterations/iteration-238-agent-cli-followups`):
//! a NUL-delimited sibling of `--filenames-only` for `xargs -0` / newline-safe
//! consumption, and natural-key addressing for the single-file commands the
//! ralph-loop workflow touches every run (read a plan, tick its tasks).

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
        .args(["find", "--iteration", "206", "--filenames0"])
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
            "--iteration",
            "206",
            "--filenames0",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let xargs = Command::new("xargs")
        .arg("-0")
        .arg("cat")
        .current_dir(vault.path())
        .stdin(find.stdout.take().unwrap())
        .output()
        .unwrap();
    assert!(find.wait().unwrap().success());
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
            "--iteration",
            "206",
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

#[test]
fn filenames0_composes_with_iteration_filter() {
    let vault = setup_iteration_vault();
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["find", "--iteration", "206", "--filenames0"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        output.stdout,
        b"iterations/iteration-206-agent-cli.md\0".to_vec()
    );
}

// ===========================================================================
// --iteration <ID> on read / task / backlinks (single-file commands)
// ===========================================================================

#[test]
fn read_iteration_resolves_single_file() {
    let vault = setup_iteration_vault();
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["read", "--iteration", "206"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Body 206"), "{stdout}");
    assert!(!stdout.contains("Note body"), "{stdout}");
}

#[test]
fn read_iteration_combines_with_section_and_frontmatter_flags() {
    let vault = setup_iteration_vault();
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["read", "--iteration", "206", "--frontmatter"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("title: Iter 206"), "{stdout}");
}

#[test]
fn read_iteration_conflicts_with_file_and_glob_at_parse_time() {
    let vault = setup_iteration_vault();
    let dir = vault.path().to_str().unwrap();

    for conflicting in [
        vec!["read", "--iteration", "206", "notes/random.md"],
        vec!["read", "--iteration", "206", "--file", "notes/random.md"],
        vec!["read", "--iteration", "206", "--glob", "**/*.md"],
    ] {
        let out = hyalo_no_hints()
            .args(["--dir", dir])
            .args(&conflicting)
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(2),
            "{conflicting:?}: {}",
            stderr(&out)
        );
    }
}

#[test]
fn read_iteration_no_match_errors_naming_resolved_glob() {
    let vault = setup_iteration_vault();
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["read", "--iteration", "999"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let err = stderr(&output);
    assert!(err.contains("no file found for iteration 999"), "{err}");
    assert!(
        err.contains("iterations/iteration-999-*.md"),
        "must name the resolved glob: {err}"
    );
}

#[test]
fn read_iteration_invalid_id_rejected() {
    let vault = setup_iteration_vault();
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["read", "--iteration", "not-an-id"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
}

/// BUG-3 (review of iter-225/226): `--iteration abc` (non-empty but with no
/// leading digit) used to be misreported as "iteration ID is empty", which
/// is inaccurate — the input isn't empty, it's just not numeric. The error
/// must name the actual problem and echo the offending value.
#[test]
fn read_iteration_non_numeric_id_reports_accurate_error() {
    let vault = setup_iteration_vault();
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["read", "--iteration", "abc"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let err = stderr(&output);
    assert!(
        err.contains("'abc' is not numeric"),
        "must name the actual problem, not report an empty ID: {err}"
    );
    assert!(
        !err.contains("iteration ID is empty"),
        "must not misreport a non-empty, non-numeric ID as empty: {err}"
    );
}

#[test]
fn read_iteration_ambiguous_match_lists_candidates() {
    let vault = setup_iteration_vault();
    write_md(
        vault.path(),
        "iterations/iteration-206-x-dup.md",
        md!(r"
---
title: Iter 206 dup
type: iteration
status: planned
---
Dup body.
"),
    );
    write_md(
        vault.path(),
        "iterations/iteration-206b-second.md",
        md!(r"
---
title: Iter 206b second
type: iteration
status: planned
---
Suffix body.
"),
    );
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["read", "--iteration", "206"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let err = stderr(&output);
    assert!(err.contains("matches multiple files"), "{err}");
    assert!(err.contains("iteration-206-agent-cli.md"), "{err}");
    assert!(err.contains("iteration-206-x-dup.md"), "{err}");

    // Letter suffix disambiguates — same contract as set --iteration.
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["read", "--iteration", "206b"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Suffix body"), "{stdout}");
}

#[test]
fn task_toggle_iteration_resolves_and_toggles() {
    let vault = setup_iteration_vault();
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["task", "toggle", "--iteration", "206", "--all"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"done\": true"), "{stdout}");
    // The file was actually mutated through the resolved path.
    let body =
        fs::read_to_string(vault.path().join("iterations/iteration-206-agent-cli.md")).unwrap();
    // --all toggles every task: "one" becomes done, "two" was done and reopens.
    assert!(body.contains("- [x] one\n- [ ] two"), "{body}");
}

/// `task set --iteration` was covered by unit tests but had no e2e
/// coverage, unlike `task toggle --iteration` and `task read --iteration`
/// right above/below it.
#[test]
fn task_set_iteration_resolves_and_sets() {
    let vault = setup_iteration_vault();
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args([
            "task",
            "set",
            "--iteration",
            "206",
            "--all",
            "--status",
            "?",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"status\": \"?\""), "{stdout}");
    // The file was actually mutated through the resolved path.
    let body =
        fs::read_to_string(vault.path().join("iterations/iteration-206-agent-cli.md")).unwrap();
    assert!(body.contains("- [?] one\n- [?] two"), "{body}");
}

#[test]
fn task_read_iteration_resolves() {
    let vault = setup_iteration_vault();
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["task", "read", "--iteration", "206", "--all"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("one") && stdout.contains("two"), "{stdout}");
}

#[test]
fn backlinks_iteration_resolves_single_file() {
    let vault = setup_iteration_vault();
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["backlinks", "--iteration", "206"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("iteration-206-agent-cli.md"), "{stdout}");
}

// ---------------------------------------------------------------------------

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
