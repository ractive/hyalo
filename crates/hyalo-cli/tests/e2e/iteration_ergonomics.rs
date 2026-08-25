//! Iteration 235 — agent-facing CLI ergonomics: `--filenames-only`,
//! `--iteration <ID>`, and the self-healing vault-boundary error.
//!
//! These three close the highest-friction findings from the ralph-loop port
//! dogfood (research/agent-ergonomics-ralph-loop-port-2026-08-24): a compact
//! filename projection for `find`, natural-key addressing for iteration-typed
//! vaults, and a boundary error that names the vault root instead of leaving
//! the agent to guess another absolute path.

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
    // A note (not an iteration) so --iteration 16 does not accidentally widen.
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
fn filenames_only_composes_with_iteration_filter() {
    let vault = setup_iteration_vault();
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["find", "--iteration", "206", "--filenames-only"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim_end(),
        "iterations/iteration-206-agent-cli.md"
    );
}

// ===========================================================================
// --iteration <ID> on find
// ===========================================================================

#[test]
fn find_iteration_bare_integer_resolves_filename_template() {
    let vault = setup_iteration_vault();
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["find", "--iteration", "206", "--jq", ".results[].file"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "iterations/iteration-206-agent-cli.md"
    );
}

#[test]
fn find_iteration_letter_suffix_matches_only_suffixed_file() {
    let vault = setup_iteration_vault();
    // 16b must match only the suffix file, not iteration-16-baseline.
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["find", "--iteration", "16b", "--jq", ".results[].file"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "iterations/iteration-16b-suffix.md"
    );
}

#[test]
fn find_iteration_bare_integer_does_not_match_letter_suffix_file() {
    let vault = setup_iteration_vault();
    // --iteration 16 matches iteration-16-baseline only — NOT iteration-16b-*
    // (the letter suffix is a separate identifier, and the literal `-` after
    // the digits in the template is the boundary).
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["find", "--iteration", "16", "--jq", ".results[].file"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let files: Vec<&str> = stdout.trim().lines().collect();
    assert_eq!(files, vec!["iterations/iteration-16-baseline.md"]);
}

#[test]
fn find_iteration_zero_padded_id_preserved_verbatim() {
    let vault = setup_iteration_vault();
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["find", "--iteration", "01", "--jq", ".results[].file"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "iterations/iteration-01-zero-pad.md"
    );
}

#[test]
fn find_iteration_no_matching_file_returns_empty_result() {
    let vault = setup_iteration_vault();
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["find", "--iteration", "999", "--jq", ".results | length"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "0");
}

#[test]
fn find_iteration_invalid_id_is_rejected() {
    let vault = setup_iteration_vault();
    for bad in ["b16", "16-b", "1.6", "abc", ""] {
        let output = hyalo_no_hints()
            .args(["--dir", vault.path().to_str().unwrap()])
            .args(["find", "--iteration", bad])
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "iteration ID {bad:?} should be rejected: {}",
            stderr(&output)
        );
        let err = stderr(&output);
        assert!(err.contains("iteration ID"), "got: {err}");
        assert!(
            err.contains("digits optionally followed by letters"),
            "error should name the grammar: {err}"
        );
    }
}

#[test]
fn find_iteration_without_matching_template_errors_clearly() {
    // A vault whose types have no {n} template slot.
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join(".hyalo.toml"),
        md!(r#"
dir = "."

[schema.types.note]
required = ["title"]
"#),
    )
    .unwrap();
    write_md(
        tmp.path(),
        "n.md",
        md!(r"
---
title: A
type: note
---
body
"),
    );
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--iteration", "16"])
        .output()
        .unwrap();
    assert!(!output.status.success(), "{}", stderr(&output));
    let err = stderr(&output);
    assert!(
        err.contains("no type schema has a filename_template with an {n}"),
        "should name the missing-template reason: {err}"
    );
    assert!(
        err.contains("set a filename_template containing {n}"),
        "should offer the fix: {err}"
    );
}

#[test]
fn find_iteration_combines_with_other_filters() {
    let vault = setup_iteration_vault();
    // 206 is planned; narrowing to status=completed yields nothing.
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args([
            "find",
            "--iteration",
            "206",
            "--property",
            "status=completed",
            "--jq",
            ".results | length",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "0");
    // 16-baseline is completed; --iteration 16 + status=completed finds it.
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args([
            "find",
            "--iteration",
            "16",
            "--property",
            "status=completed",
            "--jq",
            ".results[].file",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "iterations/iteration-16-baseline.md"
    );
}

// ===========================================================================
// --iteration <ID> on set
// ===========================================================================

#[test]
fn set_iteration_resolves_single_file_and_mutates() {
    let vault = setup_iteration_vault();
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args([
            "set",
            "--iteration",
            "206",
            "--property",
            "status=completed",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "set --iteration should succeed: {}",
        stderr(&output)
    );
    let body =
        fs::read_to_string(vault.path().join("iterations/iteration-206-agent-cli.md")).unwrap();
    assert!(
        body.contains("status: completed"),
        "status was written: {body}"
    );
}

#[test]
fn set_iteration_where_property_filters_within_selection() {
    let vault = setup_iteration_vault();
    // 206 is planned; --where-property status=planned allows the write.
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args([
            "set",
            "--iteration",
            "206",
            "--property",
            "status=in-progress",
            "--where-property",
            "status=planned",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let body =
        fs::read_to_string(vault.path().join("iterations/iteration-206-agent-cli.md")).unwrap();
    assert!(body.contains("status: in-progress"));
    // 206 is now in-progress; a second write gated on status=planned skips.
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args([
            "set",
            "--iteration",
            "206",
            "--property",
            "status=completed",
            "--where-property",
            "status=planned",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let body =
        fs::read_to_string(vault.path().join("iterations/iteration-206-agent-cli.md")).unwrap();
    // Skipped → still in-progress.
    assert!(body.contains("status: in-progress"));
}

#[test]
fn set_iteration_conflicts_with_file_and_glob() {
    let vault = setup_iteration_vault();
    let dir = vault.path().to_str().unwrap();
    // --iteration + --file → clap conflict, exit 2.
    let out = hyalo_no_hints()
        .args(["--dir", dir])
        .args([
            "set",
            "--iteration",
            "206",
            "--file",
            "x.md",
            "--property",
            "status=completed",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    // --iteration + --glob → clap conflict, exit 2.
    let out = hyalo_no_hints()
        .args(["--dir", dir])
        .args([
            "set",
            "--iteration",
            "206",
            "--glob",
            "**/*.md",
            "--property",
            "status=completed",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
}

#[test]
fn set_iteration_ambiguous_match_lists_candidates() {
    let vault = setup_iteration_vault();
    // Create a second iteration-16 file so bare 16 is ambiguous.
    write_md(
        vault.path(),
        "iterations/iteration-16-second.md",
        md!(r"
---
title: Iter 16 second
type: iteration
status: planned
date: 2026-01-04
---
Body.
"),
    );
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args(["set", "--iteration", "16", "--property", "status=completed"])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "ambiguous --iteration must error: {}",
        stderr(&output)
    );
    let err = stderr(&output);
    assert!(err.contains("matches multiple files"), "got: {err}");
    assert!(
        err.contains("iteration-16-baseline.md"),
        "lists candidates: {err}"
    );
    assert!(
        err.contains("iteration-16-second.md"),
        "lists both candidates: {err}"
    );
    assert!(
        err.contains("disambiguate") || err.contains("--file"),
        "should suggest disambiguation: {err}"
    );
}

#[test]
fn set_iteration_no_match_errors_with_resolved_globs() {
    let vault = setup_iteration_vault();
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args([
            "set",
            "--iteration",
            "999",
            "--property",
            "status=completed",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success(), "{}", stderr(&output));
    let err = stderr(&output);
    assert!(
        err.contains("no file found for iteration 999"),
        "got: {err}"
    );
    assert!(
        err.contains("iterations/iteration-999-*.md"),
        "should name the resolved glob: {err}"
    );
}

#[test]
fn set_iteration_letter_suffix_targets_one_file() {
    let vault = setup_iteration_vault();
    let output = hyalo_no_hints()
        .args(["--dir", vault.path().to_str().unwrap()])
        .args([
            "set",
            "--iteration",
            "16b",
            "--property",
            "status=completed",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let body = fs::read_to_string(vault.path().join("iterations/iteration-16b-suffix.md")).unwrap();
    assert!(body.contains("status: completed"));
    // The baseline 16 file is untouched.
    let baseline =
        fs::read_to_string(vault.path().join("iterations/iteration-16-baseline.md")).unwrap();
    assert!(baseline.contains("status: completed"));
    // Disambiguate: bare 16 with two same-number files is ambiguous (see above).
}

// ===========================================================================
// Vault-boundary error self-healing
// ===========================================================================

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
