//! Iteration 257: `init`/`deinit` honour `--dir`, and both speak JSON.
//!
//! Three bugs, all hit live during iteration 256's dogfooding pass, where an
//! `init --dir <other-tree>` / `deinit` probe run from the hyalo repo wrote a
//! self-refusing `.hyalo.toml` into the repo and then deleted the repo's own
//! `.claude` integration files:
//!
//! - BUG-1: `init --dir <other-tree>` wrote `dir = "<absolute path>"` into
//!   CWD's `.hyalo.toml` — a value every later run refuses, because a
//!   project-local config may not set an absolute `dir` (iter-221/243).
//! - BUG-2: `deinit` ignored `--dir` entirely and always deleted CWD's files.
//! - BUG-3: `--format json` was silently ignored by both.
//!
//! See DEC-261 (scoping) and DEC-262 (JSON envelope).

use super::common::hyalo_no_hints;
use std::fs;
use tempfile::TempDir;

/// `init --claude` inside `dir`, with the vault at `dir` itself.
fn init_claude(dir: &std::path::Path) {
    let output = hyalo_no_hints()
        .current_dir(dir)
        .args(["init", "--claude", "--dir", "."])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "setup init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ---------------------------------------------------------------------------
// BUG-1: `--dir` outside CWD must not produce a config that refuses itself
// ---------------------------------------------------------------------------

#[test]
fn init_dir_outside_cwd_moves_the_root_into_that_tree() {
    let cwd = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let vault = elsewhere.path().join("vault");

    let output = hyalo_no_hints()
        .current_dir(cwd.path())
        .args(["init", "--dir", vault.to_str().unwrap()])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // The config lands in the named tree, not in CWD.
    assert!(
        !cwd.path().join(".hyalo.toml").exists(),
        "CWD must not gain a config for a vault it does not contain; got: {stdout}"
    );
    let config = fs::read_to_string(vault.join(".hyalo.toml")).unwrap();
    assert!(
        config.contains("dir = \".\""),
        "the vault tree's config points at itself; got: {config}"
    );
    assert!(
        !config.contains(vault.to_str().unwrap()),
        "no absolute path is ever written as `dir`; got: {config}"
    );
    // The summary says which tree it acted on, so it cannot be read as CWD's.
    assert!(
        stdout.contains("target   "),
        "an out-of-CWD run names its target; got: {stdout}"
    );
}

#[test]
fn config_written_for_an_outside_dir_is_readable_afterwards() {
    // The regression that made BUG-1 worth fixing: the old `init` wrote an
    // absolute `dir`, which every subsequent run refused.
    let cwd = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let vault = elsewhere.path().join("vault");

    hyalo_no_hints()
        .current_dir(cwd.path())
        .args(["init", "--dir", vault.to_str().unwrap()])
        .output()
        .unwrap();

    let output = hyalo_no_hints()
        .current_dir(&vault)
        .args(["config", "--format", "json"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(
        !stderr.contains("not allowed to set") && !stderr.contains("above the config directory"),
        "the config `init` wrote must not be refused on the next run; stderr: {stderr}"
    );
}

#[test]
fn init_absolute_dir_under_cwd_is_recorded_relative() {
    // An absolute spelling of a subdirectory is still a subdirectory: the root
    // stays at CWD and `dir` is written relative, exactly as `--dir docs` does.
    let cwd = TempDir::new().unwrap();
    let docs = cwd.path().join("docs");

    let output = hyalo_no_hints()
        .current_dir(cwd.path())
        .args(["init", "--dir", docs.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let config = fs::read_to_string(cwd.path().join(".hyalo.toml")).unwrap();
    assert!(
        config.contains("dir = \"docs\""),
        "absolute --dir under CWD is recorded relative; got: {config}"
    );
    assert!(docs.is_dir(), "the vault directory is still created");
}

// ---------------------------------------------------------------------------
// BUG-2: a `--dir`-scoped `deinit` must not touch CWD
// ---------------------------------------------------------------------------

#[test]
fn deinit_with_dir_outside_cwd_leaves_cwd_untouched() {
    let cwd = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();
    init_claude(cwd.path());
    init_claude(target.path());

    let output = hyalo_no_hints()
        .current_dir(cwd.path())
        .args(["--dir", target.path().to_str().unwrap(), "deinit"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // CWD keeps everything.
    assert!(
        cwd.path().join(".hyalo.toml").exists(),
        "CWD's config must survive a --dir-scoped deinit; got: {stdout}"
    );
    assert!(
        cwd.path().join(".claude").join("CLAUDE.md").exists(),
        "CWD's CLAUDE.md must survive a --dir-scoped deinit; got: {stdout}"
    );
    assert!(
        cwd.path().join(".claude/skills/hyalo/SKILL.md").exists(),
        "CWD's skill must survive a --dir-scoped deinit; got: {stdout}"
    );

    // The named tree is the one that was cleaned.
    assert!(
        !target.path().join(".hyalo.toml").exists(),
        "the --dir tree's config should be removed; got: {stdout}"
    );
    assert!(
        !target.path().join(".claude").exists(),
        "the --dir tree's .claude should be removed; got: {stdout}"
    );
    assert!(
        stdout.contains("target   "),
        "an out-of-CWD deinit names its target; got: {stdout}"
    );
}

#[test]
fn deinit_with_dir_inside_cwd_still_cleans_cwd() {
    // `--dir docs` names a vault *inside* the project, so the project root —
    // and therefore the integration files to remove — is still CWD.
    let cwd = TempDir::new().unwrap();
    fs::create_dir(cwd.path().join("docs")).unwrap();
    hyalo_no_hints()
        .current_dir(cwd.path())
        .args(["init", "--claude", "--dir", "docs"])
        .output()
        .unwrap();

    let output = hyalo_no_hints()
        .current_dir(cwd.path())
        .args(["--dir", "docs", "deinit"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !cwd.path().join(".hyalo.toml").exists(),
        "a vault-scoped deinit still cleans its own project root; got: {stdout}"
    );
    assert!(
        !stdout.contains("target   "),
        "no target line when the root is CWD; got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// BUG-3: `--format json`
// ---------------------------------------------------------------------------

#[test]
fn init_format_json_emits_an_envelope() {
    let cwd = TempDir::new().unwrap();

    let output = hyalo_no_hints()
        .current_dir(cwd.path())
        .args(["init", "--dir", "docs", "--format", "json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let value: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not JSON ({e}): {stdout}"));
    assert_eq!(value["results"]["command"], "init");
    // `dir` is hoisted to the top level by the shared envelope builder.
    assert_eq!(value["dir"], "docs");
    assert!(
        value["hints"].is_array(),
        "the standard envelope carries hints; got: {stdout}"
    );
    let actions = value["results"]["actions"].as_array().unwrap();
    assert!(
        actions
            .iter()
            .any(|a| { a["action"] == "created" && a["target"] == ".hyalo.toml" }),
        "the config write is reported as a structured action; got: {stdout}"
    );
    assert!(
        value["results"]["root"].as_str().is_some(),
        "the envelope names the root it wrote under; got: {stdout}"
    );
}

#[test]
fn deinit_format_json_emits_an_envelope() {
    let cwd = TempDir::new().unwrap();
    init_claude(cwd.path());

    let output = hyalo_no_hints()
        .current_dir(cwd.path())
        .args(["deinit", "--format", "json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let value: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not JSON ({e}): {stdout}"));
    assert_eq!(value["results"]["command"], "deinit");
    assert!(
        value["results"].get("dir").is_none(),
        "deinit reports no vault dir; got: {stdout}"
    );
    let actions = value["results"]["actions"].as_array().unwrap();
    assert!(
        actions
            .iter()
            .any(|a| a["action"] == "removed" && a["target"] == ".hyalo.toml"),
        "the config removal is reported as a structured action; got: {stdout}"
    );
    assert!(
        actions
            .iter()
            .any(|a| a["action"] == "skipped" && a["detail"] == "not found"),
        "skips carry their reason as a field, not as prose; got: {stdout}"
    );
}

#[test]
fn init_jq_filters_the_envelope() {
    let cwd = TempDir::new().unwrap();

    let output = hyalo_no_hints()
        .current_dir(cwd.path())
        .args(["init", "--dir", "docs", "--jq", ".results.command"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(stdout.trim(), "init", "got: {stdout}");
}

#[test]
fn init_piped_output_stays_text_without_an_explicit_format() {
    // Unlike the pipeline commands, `init`/`deinit` do not flip to JSON just
    // because stdout is a pipe — their summary is a human progress report
    // (DEC-262). This test's stdout *is* a pipe.
    let cwd = TempDir::new().unwrap();

    let output = hyalo_no_hints()
        .current_dir(cwd.path())
        .args(["init", "--dir", "docs"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(
        !stdout.trim_start().starts_with('{'),
        "piped init stays text; got: {stdout}"
    );
    assert!(
        stdout.contains("created  .hyalo.toml"),
        "the text summary is unchanged; got: {stdout}"
    );
}

#[test]
fn init_rejects_format_github() {
    let cwd = TempDir::new().unwrap();

    let output = hyalo_no_hints()
        .current_dir(cwd.path())
        .args(["init", "--dir", "docs", "--format", "github"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "github format must be refused");
    assert!(
        stderr.contains("only supported by `hyalo lint`"),
        "same message every other command gives; got: {stderr}"
    );
    assert!(
        !cwd.path().join(".hyalo.toml").exists(),
        "nothing is written when the format is refused"
    );
}
