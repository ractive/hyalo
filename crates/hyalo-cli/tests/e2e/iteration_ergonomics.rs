//! Agent-facing CLI ergonomics: `--filenames-only` and the self-healing
//! vault-boundary error (iter-235; the `--iteration` natural-key flag that
//! shipped with them was removed again in iter-242 — sequence-keyed files
//! are addressed with a plain `--glob`).

use std::fs;

use super::common::{hyalo_no_hints, md, write_md};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Vault fixture: an `iteration` type with a `{n}` filename template.
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
        "iterations/iteration-16-baseline.md",
        md!(r"
---
title: Iter 16 Baseline
type: iteration
status: completed
date: 2026-01-01
---
Body 16.
"),
    );
    write_md(
        tmp.path(),
        "iterations/iteration-16b-suffix.md",
        md!(r"
---
title: Iter 16b
type: iteration
status: planned
date: 2026-01-02
---
Body 16b.
"),
    );
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
"),
    );
    write_md(
        tmp.path(),
        "iterations/iteration-01-zero-pad.md",
        md!(r"
---
title: Iter 1
type: iteration
status: completed
date: 2026-01-03
---
Body 01.
"),
    );
    // A note outside the iteration directory (fixture variety).
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
// --filenames-only
// ===========================================================================

#[test]
fn filenames_only_prints_one_path_per_line_no_decoration() {
    let vault = setup_iteration_vault();
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["find", "--property", "status=planned", "--filenames-only"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Every planned iteration, one path per line, no quotes / JSON.
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines.contains(&"iterations/iteration-16b-suffix.md"));
    assert!(lines.contains(&"iterations/iteration-206-agent-cli.md"));
    assert!(
        !lines.contains(&"iterations/iteration-16-baseline.md"),
        "completed not listed"
    );
    // No envelope, no hints, no count.
    assert!(!stdout.contains("\"results\""));
    assert!(!stdout.contains("\"hints\""));
    assert!(!stdout.contains("\"total\""));
    assert!(!stdout.contains('"'));
    // Trailing newline on a non-empty result set.
    assert!(
        stdout.ends_with('\n'),
        "expected trailing newline: {stdout:?}"
    );
}

#[test]
fn filenames_only_zero_results_is_empty_output_exit_0() {
    let vault = setup_iteration_vault();
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args([
            "find",
            "--property",
            "status=nonexistent",
            "--filenames-only",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "zero results must exit 0, got: {}",
        stderr(&output)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "",
        "zero results must print nothing (no trailing newline)"
    );
}

#[test]
fn filenames_only_strict_flips_exit_code_when_results_exist() {
    let vault = setup_iteration_vault();
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args([
            "find",
            "--property",
            "status=planned",
            "--filenames-only",
            "--strict",
        ])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "--strict must exit non-zero when results exist, got success: {}",
        stderr(&output)
    );
    // The paths are still printed to stdout (the CI-gate + list use case).
    assert!(String::from_utf8_lossy(&output.stdout).contains("iteration-206"));
    // And empty results with --strict still exits 0.
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args([
            "find",
            "--property",
            "status=nonexistent",
            "--filenames-only",
            "--strict",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "--strict on zero results must exit 0, got: {}",
        stderr(&output)
    );
}

#[test]
fn filenames_only_conflicts_with_jq_and_count_and_format_json() {
    let vault = setup_iteration_vault();
    let dir = vault.path().to_str().unwrap();

    // --jq: clap conflict → exit 2.
    let out = hyalo_no_hints()
        .args(["--dir", dir])
        .args(["find", "--filenames-only", "--jq", ".results[].file"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));

    // --count: clap conflict → exit 2.
    let out = hyalo_no_hints()
        .args(["--dir", dir])
        .args(["find", "--filenames-only", "--count"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));

    // --format json (explicit): runtime conflict → exit 1.
    let out = hyalo_no_hints()
        .args(["--dir", dir])
        .args(["find", "--filenames-only", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    let err = stderr(&out);
    assert!(err.contains("--filenames-only cannot be combined with --format json"));

    // --format text (explicit) is fine.
    let out = hyalo_no_hints()
        .args(["--dir", dir])
        .args([
            "find",
            "--property",
            "status=planned",
            "--filenames-only",
            "--format",
            "text",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(String::from_utf8_lossy(&out.stdout).contains("iteration-206"));
}

#[test]
fn filenames_only_combines_with_filters_and_sort() {
    let vault = setup_iteration_vault();
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args([
            "find",
            "--tag",
            "nonexistent",
            "--filenames-only",
            "--sort",
            "file",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    // No file has that tag → empty output.
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
}

#[test]
fn set_absolute_path_outside_vault_names_dir_and_hint() {
    let vault = setup_iteration_vault();
    let outside = TempDir::new().unwrap();
    let stray = outside.path().join("stray.md");
    fs::write(&stray, "---\ntitle: Stray\n---\nbody\n").unwrap();
    let stray_str = stray.to_string_lossy().into_owned();

    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["set", "--file", &stray_str, "--property", "x=1"])
        .output()
        .unwrap();
    assert!(!output.status.success(), "{}", stderr(&output));
    let err = stderr(&output);
    assert!(err.contains("outside vault boundary"), "got: {err}");
    // The effective vault dir is named. The stderr is JSON, where Windows
    // path separators are escaped (C:\\Users\\...), so a full-path
    // `contains` breaks on separators. Assert on the vault's unique
    // separator-free temp component instead (present in the raw and the
    // canonical spelling alike — the 8.3 short-name form on Windows
    // runners differs from TempDir's spelling only before this component).
    let vault_name = vault
        .path()
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    assert!(
        err.contains("vault:"),
        "expected vault dir in message: {err}"
    );
    assert!(
        !vault_name.is_empty() && err.contains(vault_name),
        "expected the actual vault path component {vault_name:?} in: {err}"
    );
    // The self-healing hint names both fixes.
    assert!(err.contains("relative to"), "got: {err}");
    assert!(err.contains("cd to a parent"), "got: {err}");
    // The offending path is reported. The JSON path field is
    // separator-normalized to forward slashes, so compare against the
    // forward-slash spelling of the input (Windows: stray_str has '\\').
    let stray_fwd = stray_str.replace('\\', "/");
    assert!(
        err.contains(&stray_fwd),
        "got: {err}; expected path: {stray_fwd}"
    );
    // And the stray file is untouched.
    assert!(
        !fs::read_to_string(&stray).unwrap().contains("x:"),
        "no write should have landed outside the vault"
    );
}

#[test]
fn find_absolute_file_outside_vault_names_dir_and_hint() {
    let vault = setup_iteration_vault();
    let outside = TempDir::new().unwrap();
    let stray = outside.path().join("stray.md");
    fs::write(&stray, "---\ntitle: Stray\n---\nbody\n").unwrap();
    let stray_str = stray.to_string_lossy().into_owned();

    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["find", "--file", &stray_str])
        .output()
        .unwrap();
    assert!(!output.status.success(), "{}", stderr(&output));
    let err = stderr(&output);
    assert!(err.contains("outside vault boundary"), "got: {err}");
    assert!(err.contains("vault:"), "got: {err}");
    assert!(err.contains("relative to"), "got: {err}");
    assert!(err.contains("cd to a parent"), "got: {err}");
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
