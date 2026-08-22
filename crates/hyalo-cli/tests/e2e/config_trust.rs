//! Config-trust e2e gates (iter-201).
//!
//! Three ways a user's `.hyalo.toml` used to stop applying without a signal
//! strong enough to notice — two of which made CI go vacuously green:
//!
//! - **H-4** — an explicit `--dir` naming the *configured* vault discarded the
//!   whole config (schema, views, `[lint] ignore`, severity overrides) while
//!   printing "--dir is redundant".
//! - **M-2** — one malformed key anywhere in the file fell back to *all*
//!   defaults, including `dir`, and `-q` hid the warning; a mutating command
//!   would then happily rewrite a tree the config never pointed at.
//! - **truthfulness** — `hyalo config --dir X` reported `config_path: null`
//!   while a config was in effect.

use super::common::{hyalo, hyalo_no_hints, md, write_md};
use std::fs;
use tempfile::TempDir;

/// A project laid out the way `.hyalo.toml` is normally used: config at the
/// repo root, vault in a subdirectory, a schema and a `[lint] ignore` entry
/// that only apply if the config is honored.
fn build_project(tmp: &TempDir) {
    fs::write(
        tmp.path().join(".hyalo.toml"),
        md!(r#"
dir = "kb"

[schema.types.note]
required = ["title", "type", "status"]

[lint]
ignore = ["archive/**"]

[views.drafts]
properties = ["status=draft"]
"#),
    )
    .unwrap();

    let kb = tmp.path().join("kb");
    // Two notes missing the required `status`: lint findings that only appear
    // when the schema is loaded.
    write_md(&kb, "a.md", "---\ntitle: A\ntype: note\n---\n# A\n");
    write_md(&kb, "b.md", "---\ntitle: B\ntype: note\n---\n# B\n");
    // Ignored by `[lint] ignore`, so a config-less run reports *more*, not less.
    write_md(
        &kb,
        "archive/old.md",
        "---\ntitle: Old\ntype: note\n---\n# Old\n",
    );
}

/// Parse a JSON envelope from stdout, failing loudly with both streams.
fn envelope(stdout: &[u8], stderr: &[u8]) -> serde_json::Value {
    serde_json::from_slice(stdout).unwrap_or_else(|e| {
        panic!(
            "not a JSON envelope ({e})\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(stdout),
            String::from_utf8_lossy(stderr)
        )
    })
}

// ---------------------------------------------------------------------------
// H-4 — an explicit --dir naming the configured vault keeps the config
// ---------------------------------------------------------------------------

#[test]
fn redundant_dir_keeps_the_config_for_lint() {
    let tmp = TempDir::new().unwrap();
    build_project(&tmp);

    let without = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "json"])
        .output()
        .unwrap();
    let with = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--dir", "kb", "--format", "json"])
        .output()
        .unwrap();

    let a = envelope(&without.stdout, &without.stderr);
    let b = envelope(&with.stdout, &with.stderr);

    for key in [
        "errors",
        "warnings",
        "files_checked",
        "files_with_violations",
    ] {
        assert_eq!(
            a["results"][key], b["results"][key],
            "`lint --dir kb` must report the same `{key}` as a bare `lint`; \
             without: {a}\nwith: {b}"
        );
    }
    // Not vacuous: the schema really did produce findings.
    assert!(
        a["results"]["total"].as_u64().unwrap_or(0) > 0,
        "fixture stopped producing lint findings: {a}"
    );
}

#[test]
fn redundant_dir_keeps_lint_ignore() {
    let tmp = TempDir::new().unwrap();
    build_project(&tmp);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--dir", "kb", "--detailed", "--format", "json"])
        .output()
        .unwrap();
    let json = envelope(&output.stdout, &output.stderr);
    let text = json.to_string();
    assert!(
        !text.contains("archive/old.md"),
        "`[lint] ignore` must still apply under --dir; got: {text}"
    );
}

#[test]
fn redundant_dir_keeps_schema_types_and_views() {
    let tmp = TempDir::new().unwrap();
    build_project(&tmp);

    let types = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["types", "list", "--dir", "kb", "--format", "json"])
        .output()
        .unwrap();
    let json = envelope(&types.stdout, &types.stderr);
    assert_eq!(
        json["total"].as_u64(),
        Some(1),
        "the `note` type must survive --dir: {json}"
    );

    let views = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["views", "list", "--dir", "kb", "--format", "json"])
        .output()
        .unwrap();
    let json = envelope(&views.stdout, &views.stderr);
    assert_eq!(
        json["total"].as_u64(),
        Some(1),
        "the `drafts` view must survive --dir: {json}"
    );
}

#[test]
fn redundant_dir_still_says_so_but_no_longer_claims_the_config_is_gone() {
    let tmp = TempDir::new().unwrap();
    build_project(&tmp);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["summary", "--dir", "kb", "--format", "json"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--dir is redundant"),
        "the redundancy note stays; stderr: {stderr}"
    );
    assert!(
        !stderr.contains("does not apply"),
        "a redundant --dir does not shadow anything; stderr: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// H-4 — a --dir naming a *different* vault says which config is in effect
// ---------------------------------------------------------------------------

#[test]
fn dir_to_another_tree_announces_that_the_cwd_config_no_longer_applies() {
    let tmp = TempDir::new().unwrap();
    build_project(&tmp);
    let other = tmp.path().join("other");
    write_md(&other, "c.md", "---\ntitle: C\n---\n# C\n");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["summary", "--dir", "other", "--format", "json"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("does not apply") && stderr.contains("built-in defaults"),
        "switching vaults must name the config in effect; stderr: {stderr}"
    );
}

#[test]
fn dir_to_a_tree_with_its_own_config_names_that_file() {
    let tmp = TempDir::new().unwrap();
    build_project(&tmp);
    let other = tmp.path().join("other");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join(".hyalo.toml"), "hints = false\n").unwrap();
    write_md(&other, "c.md", "---\ntitle: C\n---\n# C\n");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["summary", "--dir", "other", "--format", "json"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("does not apply") && stderr.contains(".hyalo.toml is in effect"),
        "the target's own config must be named; stderr: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// H-4 — `hyalo config` tells the truth and emits runnable hints
// ---------------------------------------------------------------------------

#[test]
fn config_reports_the_config_path_under_a_redundant_dir() {
    let tmp = TempDir::new().unwrap();
    build_project(&tmp);

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["config", "--dir", "kb", "--format", "json"])
        .output()
        .unwrap();
    let json = envelope(&output.stdout, &output.stderr);
    let path = json["results"]["config_path"]
        .as_str()
        .unwrap_or_else(|| panic!("config_path must not be null while a config applies: {json}"));
    assert!(
        path.ends_with(".hyalo.toml"),
        "unexpected config_path: {path}"
    );
}

#[test]
fn config_hints_omit_dir_when_it_was_not_overridden() {
    let tmp = TempDir::new().unwrap();
    build_project(&tmp);

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["config", "--format", "json"])
        .output()
        .unwrap();
    let json = envelope(&output.stdout, &output.stderr);
    let hints = json["hints"].as_array().cloned().unwrap_or_default();
    assert!(!hints.is_empty(), "config must emit hints: {json}");
    for hint in &hints {
        let cmd = hint["cmd"].as_str().unwrap_or_default();
        assert!(
            !cmd.contains("--dir"),
            "a non-overridden config must not suggest --dir: {cmd}"
        );
        assert_eq!(
            hint["writes"].as_bool(),
            Some(false),
            "config drill-downs are read-only: {hint}"
        );
    }
}

#[test]
fn config_hints_run_and_return_non_degraded_results() {
    let tmp = TempDir::new().unwrap();
    build_project(&tmp);

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["config", "--format", "json"])
        .output()
        .unwrap();
    let json = envelope(&output.stdout, &output.stderr);
    for hint in json["hints"].as_array().cloned().unwrap_or_default() {
        let cmd = hint["cmd"].as_str().unwrap_or_default();
        let argv: Vec<&str> = cmd.split_whitespace().skip(1).collect();
        let run = hyalo()
            .current_dir(tmp.path())
            .args(&argv)
            .args(["--format", "json"])
            .output()
            .unwrap();
        let result = envelope(&run.stdout, &run.stderr);
        assert!(
            result.get("error").is_none(),
            "config hint `{cmd}` failed: {result}"
        );
        // "Non-degraded" concretely: `types list` must still see the schema.
        if cmd.contains("types list") {
            assert_eq!(
                result["total"].as_u64(),
                Some(1),
                "config hint `{cmd}` returned a config-less result: {result}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// M-2 — a malformed .hyalo.toml blocks writes and cannot be silenced
// ---------------------------------------------------------------------------

/// A project whose config has one unknown key — everything else is valid.
fn build_malformed_project(tmp: &TempDir) {
    fs::write(
        tmp.path().join(".hyalo.toml"),
        "dir = \"kb\"\nbogus_key = 1\n",
    )
    .unwrap();
    write_md(
        &tmp.path().join("kb"),
        "a.md",
        "---\ntitle: A\n---\n# A\n\n- [ ] open\n",
    );
}

#[test]
fn malformed_config_refuses_a_mutating_command_and_touches_nothing() {
    let tmp = TempDir::new().unwrap();
    build_malformed_project(&tmp);
    let note = tmp.path().join("kb").join("a.md");
    let before = fs::read(&note).unwrap();

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "set",
            "--property",
            "status=done",
            "--file",
            "a.md",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(1),
        "a writer on an unusable config must exit 1; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // User errors are emitted as a JSON object on stderr; the config warning
    // precedes it, so parse from the first `{`.
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let json: serde_json::Value = serde_json::from_str(
        &stderr[stderr
            .find('{')
            .unwrap_or_else(|| panic!("no JSON error on stderr: {stderr}"))..],
    )
    .unwrap_or_else(|e| panic!("stderr is not a JSON error ({e}): {stderr}"));
    assert!(
        json["error"]
            .as_str()
            .unwrap_or_default()
            .contains("unusable .hyalo.toml"),
        "unexpected error: {json}"
    );
    assert_eq!(
        fs::read(&note).unwrap(),
        before,
        "the refused command must not have written"
    );
}

#[test]
fn malformed_config_refuses_links_auto_apply_even_under_quiet() {
    let tmp = TempDir::new().unwrap();
    build_malformed_project(&tmp);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["links", "auto", "--apply", "-q", "--format", "json"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(1),
        "`links auto --apply -q` must not proceed on defaults; stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("malformed .hyalo.toml"),
        "the config diagnostic must survive -q; stderr: {stderr}"
    );
}

#[test]
fn malformed_config_still_warns_on_a_read_under_quiet() {
    let tmp = TempDir::new().unwrap();
    build_malformed_project(&tmp);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["summary", "-q", "--format", "json"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "reads still work on a malformed config"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("malformed .hyalo.toml"),
        "config-integrity warnings are not chatter; stderr: {stderr}"
    );
}

#[test]
fn malformed_config_allows_a_dry_run() {
    let tmp = TempDir::new().unwrap();
    build_malformed_project(&tmp);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "set",
            "--property",
            "status=done",
            "--file",
            "a.md",
            "--dry-run",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "--dry-run writes nothing, so it is not gated; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn malformed_config_keeps_the_configured_dir_for_reads() {
    // `dir` is salvaged from an otherwise-unusable file, so a read does not
    // silently re-root at the config directory and scan the whole repo.
    let tmp = TempDir::new().unwrap();
    build_malformed_project(&tmp);
    write_md(
        tmp.path(),
        "outside.md",
        "---\ntitle: Outside\n---\n# Outside\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["find", "--format", "json"])
        .output()
        .unwrap();
    let json = envelope(&output.stdout, &output.stderr);
    let text = json.to_string();
    assert!(
        !text.contains("outside.md"),
        "a salvaged `dir` must keep reads inside the vault: {text}"
    );
}

#[test]
fn malformed_config_reports_once_not_twice() {
    // The loader used to parse `.hyalo.toml` twice per invocation, so every run
    // ended with "1 additional identical warning(s) suppressed" (dogfood L-14).
    let tmp = TempDir::new().unwrap();
    build_malformed_project(&tmp);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["summary", "--format", "json"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.matches("malformed .hyalo.toml").count(),
        1,
        "the diagnostic must be emitted exactly once; stderr: {stderr}"
    );
    assert!(
        !stderr.contains("additional identical warning"),
        "no suppression notice means no double parse; stderr: {stderr}"
    );
}
