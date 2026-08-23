/// E2E tests for `hyalo config` subcommand (iter-130).
use std::fs;

use super::common::{hyalo_no_hints, write_md};
use tempfile::TempDir;

fn setup_minimal(tmp: &std::path::Path) {
    write_md(tmp, "note.md", "---\ntitle: Test\n---\n");
}

// ---------------------------------------------------------------------------
// Text output
// ---------------------------------------------------------------------------

#[test]
fn config_text_output_no_config() {
    let tmp = TempDir::new().unwrap();
    setup_minimal(tmp.path());

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["config", "--format", "text"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    // When no .hyalo.toml: config path is (none), dir is "."
    assert!(
        stdout.contains("config: (none)"),
        "expected '(none)' config path; got: {stdout}"
    );
    assert!(
        stdout.contains("dir: ."),
        "expected default dir '.'; got: {stdout}"
    );
    assert!(
        stdout.contains("hints: true"),
        "expected default hints true; got: {stdout}"
    );
}

#[test]
fn config_text_output_with_config() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("kb")).unwrap();
    fs::write(
        tmp.path().join(".hyalo.toml"),
        "dir = \"kb\"\nhints = false\n",
    )
    .unwrap();
    setup_minimal(tmp.path());

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["config", "--format", "text"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    // Should show the config file path
    assert!(
        stdout.contains(".hyalo.toml"),
        "expected config file path; got: {stdout}"
    );
    // Resolved dir
    assert!(
        stdout.contains("dir: kb"),
        "expected dir 'kb'; got: {stdout}"
    );
    // hints = false from config
    assert!(
        stdout.contains("hints: false"),
        "expected hints false from config; got: {stdout}"
    );
    // iter-213: the raw file text is opt-in — a bare run must not print it.
    assert!(
        !stdout.contains("--- .hyalo.toml ---"),
        "raw contents must be behind --raw; got: {stdout}"
    );

    let raw_output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["config", "--raw", "--format", "text"])
        .output()
        .unwrap();
    let raw_stdout = String::from_utf8_lossy(&raw_output.stdout);
    assert!(
        raw_stdout.contains("--- .hyalo.toml ---"),
        "expected raw contents separator with --raw; got: {raw_stdout}"
    );
    assert!(
        raw_stdout.contains("dir = \"kb\""),
        "expected raw TOML content with --raw; got: {raw_stdout}"
    );
}

#[test]
fn config_text_shows_cwd() {
    let tmp = TempDir::new().unwrap();
    setup_minimal(tmp.path());

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["config", "--format", "text"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(stdout.contains("cwd:"), "expected cwd line; got: {stdout}");
}

// ---------------------------------------------------------------------------
// JSON output
// ---------------------------------------------------------------------------

#[test]
fn config_json_output_no_config() {
    let tmp = TempDir::new().unwrap();
    setup_minimal(tmp.path());

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["config", "--format", "json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    let json: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("invalid JSON: {e}\n{stdout}"));

    // iter-192: `hyalo config` uses the standard envelope. The config payload
    // lives under `results`; `hints` is the envelope's hint array (empty here
    // because of --no-hints), and the config's own on/off switch is reported as
    // `results.hints_enabled` so the two never collide.
    let results = &json["results"];
    assert!(
        results.get("cwd").is_some(),
        "expected 'results.cwd' field; got: {json}"
    );
    assert!(
        results.get("dir").is_some(),
        "expected 'results.dir' field; got: {json}"
    );
    assert!(
        json["hints"].is_array(),
        "envelope 'hints' must be an array; got: {json}"
    );
    assert_eq!(
        results["hints_enabled"], true,
        "expected hints_enabled = true by default; got: {json}"
    );
    assert!(
        results["config_path"].is_null(),
        "expected config_path = null when no config; got: {json}"
    );
    assert_eq!(
        results["dir"].as_str().unwrap(),
        ".",
        "expected default dir '.'; got: {json}"
    );
    // `dir` stays hoisted to the envelope root for pre-192 consumers.
    assert_eq!(
        json["dir"].as_str().unwrap(),
        ".",
        "expected hoisted top-level dir '.'; got: {json}"
    );
}

#[test]
fn config_json_output_with_config() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("vault")).unwrap();
    fs::write(
        tmp.path().join(".hyalo.toml"),
        "dir = \"vault\"\nformat = \"text\"\n",
    )
    .unwrap();
    setup_minimal(tmp.path());

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["config", "--format", "json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    let json: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("invalid JSON: {e}\n{stdout}"));

    // iter-192: config payload lives under the envelope's `results`.
    let results = &json["results"];
    assert_eq!(
        results["dir"].as_str().unwrap(),
        "vault",
        "expected dir 'vault'; got: {json}"
    );
    assert_eq!(
        results["format"].as_str().unwrap(),
        "text",
        "expected format 'text' from config; got: {json}"
    );
    // config_path should be a non-null string
    assert!(
        results["config_path"].is_string(),
        "expected config_path string; got: {json}"
    );
    assert!(
        results["config_path"]
            .as_str()
            .unwrap()
            .contains(".hyalo.toml"),
        "expected config_path to contain .hyalo.toml; got: {json}"
    );
    // iter-213: raw_contents is opt-in and the key stays present as null.
    assert!(
        results["raw_contents"].is_null(),
        "raw_contents must be null without --raw; got: {json}"
    );
    // A parseable config reports malformed: false with no parse_error.
    assert_eq!(
        results["malformed"].as_bool(),
        Some(false),
        "expected malformed false on a valid config; got: {json}"
    );
    assert!(
        results["parse_error"].is_null(),
        "expected no parse_error on a valid config; got: {json}"
    );
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

#[test]
fn config_does_not_require_valid_vault_dir() {
    // Even if dir in config points to a non-existent directory, `hyalo config` must succeed.
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join(".hyalo.toml"),
        "dir = \"nonexistent-vault\"\n",
    )
    .unwrap();

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["config"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "hyalo config should succeed even with non-existent vault dir; stderr: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Envelope + --jq (iter-192)
// ---------------------------------------------------------------------------

#[test]
fn config_jq_filters_the_envelope() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("vault")).unwrap();
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \"vault\"\n").unwrap();
    setup_minimal(tmp.path());

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["config", "--jq", ".results.dir"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Before iter-192 the filter was silently ignored and the whole object printed.
    assert_eq!(stdout.trim().trim_matches('"'), "vault", "got: {stdout}");
}

#[test]
fn config_jq_reports_a_bad_filter_instead_of_ignoring_it() {
    let tmp = TempDir::new().unwrap();
    setup_minimal(tmp.path());

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["config", "--jq", ".["])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "a malformed jq filter must be an error, not a silent no-op"
    );
}

#[test]
fn config_json_hints_are_an_array_of_runnable_commands() {
    let tmp = TempDir::new().unwrap();
    setup_minimal(tmp.path());

    let output = super::common::hyalo()
        .current_dir(tmp.path())
        .args(["config", "--format", "json"])
        .output()
        .unwrap();

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let hints = json["hints"].as_array().expect("hints array");
    assert!(!hints.is_empty(), "config should emit hints: {json}");
    for hint in hints {
        let cmd = hint["cmd"].as_str().expect("hint cmd");
        assert!(
            cmd.starts_with("hyalo "),
            "hint cmd must be runnable: {cmd}"
        );
        assert!(
            hint["description"].is_string(),
            "hint needs a description: {hint}"
        );
    }
}

#[test]
fn config_text_shows_hint_arrows_by_default() {
    let tmp = TempDir::new().unwrap();
    setup_minimal(tmp.path());

    let output = super::common::hyalo()
        .current_dir(tmp.path())
        .args(["config", "--format", "text"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\n  -> hyalo "),
        "config text output should carry drill-down hints; got: {stdout}"
    );
}

#[test]
fn config_text_no_hints_suppresses_arrows() {
    let tmp = TempDir::new().unwrap();
    setup_minimal(tmp.path());

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["config", "--format", "text"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("  -> hyalo "),
        "--no-hints must suppress the arrows; got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// [links.auto] reporting (iter-195a)
// ---------------------------------------------------------------------------

#[test]
fn config_text_reports_links_auto_defaults() {
    let tmp = TempDir::new().unwrap();
    setup_minimal(tmp.path());

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["config", "--format", "text"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(
        stdout.contains("links.auto.exclude_titles: (none)"),
        "expected empty exclude_titles reported as (none); got: {stdout}"
    );
    assert!(
        stdout.contains("links.auto.exclude_target_globs: (none)"),
        "expected empty exclude_target_globs reported as (none); got: {stdout}"
    );
    assert!(
        stdout.contains("links.auto.first_only: false"),
        "expected first_only default false; got: {stdout}"
    );
    assert!(
        stdout.contains("links.auto.warn_common_titles: true"),
        "expected warn_common_titles to default to on; got: {stdout}"
    );
}

#[test]
fn config_reports_warn_common_titles_opt_out() {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join(".hyalo.toml"),
        "[links.auto]\nwarn_common_titles = false\n",
    )
    .unwrap();
    setup_minimal(tmp.path());

    let text = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["config", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&text.stdout);
    assert!(
        text.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&text.stderr)
    );
    assert!(
        stdout.contains("links.auto.warn_common_titles: false"),
        "text report should surface the opt-out; got: {stdout}"
    );

    let json_out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["config", "--format", "json"])
        .output()
        .unwrap();
    assert!(json_out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&json_out.stdout).unwrap();
    assert_eq!(
        json["results"]["links_auto"]["warn_common_titles"],
        serde_json::json!(false),
        "envelope should carry warn_common_titles: {json}"
    );
}

#[test]
fn config_text_reports_effective_links_auto() {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join(".hyalo.toml"),
        "[links.auto]\nexclude_titles = [\"permissions\", \"README\"]\n\
         exclude_target_globs = [\"templates/*\"]\nfirst_only = true\n",
    )
    .unwrap();
    setup_minimal(tmp.path());

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["config", "--format", "text"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(
        stdout.contains("links.auto.exclude_titles: permissions, README"),
        "expected both excluded titles; got: {stdout}"
    );
    assert!(
        stdout.contains("links.auto.exclude_target_globs: templates/*"),
        "expected the excluded target glob; got: {stdout}"
    );
    assert!(
        stdout.contains("links.auto.first_only: true"),
        "expected first_only true; got: {stdout}"
    );
}

#[test]
fn config_json_reports_effective_links_auto() {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join(".hyalo.toml"),
        "[links.auto]\nexclude_titles = [\"permissions\"]\nfirst_only = true\n",
    )
    .unwrap();
    setup_minimal(tmp.path());

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["config", "--format", "json"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let auto = &json["results"]["links_auto"];
    assert_eq!(
        auto["exclude_titles"],
        serde_json::json!(["permissions"]),
        "envelope should carry the config's excluded titles: {json}"
    );
    assert_eq!(
        auto["exclude_target_globs"],
        serde_json::json!([]),
        "unset list should be an empty array, not null: {json}"
    );
    assert_eq!(
        auto["first_only"],
        serde_json::json!(true),
        "envelope should carry first_only: {json}"
    );
}

/// Run `hyalo config` against `dir` and return stdout, asserting success.
fn fuzzy_config_stdout(dir: &std::path::Path, format: &str) -> String {
    let output = hyalo_no_hints()
        .args([
            "--dir",
            dir.to_str().expect("temp path should be valid UTF-8"),
            "config",
            "--format",
            format,
        ])
        .output()
        .expect("hyalo config should run");
    assert!(
        output.status.success(),
        "hyalo config exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// iter-212: `links fix --apply-fuzzy` gates on a confidence floor, so
/// `hyalo config` has to be able to answer "which floor is in force?".
#[test]
fn config_reports_the_effective_fuzzy_confidence_floor() {
    let tmp = TempDir::new().expect("tempdir creation should succeed");

    let stdout = fuzzy_config_stdout(tmp.path(), "text");
    assert!(
        stdout.contains("links.fuzzy_min_confidence: 0.8"),
        "the built-in default must be reported, not omitted: {stdout}"
    );

    fs::write(
        tmp.path().join(".hyalo.toml"),
        "[links]\nfuzzy_min_confidence = 0.6\n",
    )
    .expect("config write should succeed");

    let stdout = fuzzy_config_stdout(tmp.path(), "text");
    assert!(
        stdout.contains("links.fuzzy_min_confidence: 0.6"),
        "the configured floor must win: {stdout}"
    );

    let json: serde_json::Value = serde_json::from_str(&fuzzy_config_stdout(tmp.path(), "json"))
        .expect("config --format json should emit JSON");
    assert_eq!(
        json["results"]["links_fuzzy_min_confidence"].as_f64(),
        Some(0.6),
        "{json}"
    );
}

/// An out-of-range floor is a config mistake the user must see; it is warned
/// about and ignored rather than silently clamped.
#[test]
fn config_rejects_an_out_of_range_fuzzy_confidence_floor() {
    let tmp = TempDir::new().expect("tempdir creation should succeed");
    fs::write(
        tmp.path().join(".hyalo.toml"),
        "[links]\nfuzzy_min_confidence = 1.5\n",
    )
    .expect("config write should succeed");

    let output = hyalo_no_hints()
        .args([
            "--dir",
            tmp.path()
                .to_str()
                .expect("temp path should be valid UTF-8"),
            "config",
            "--format",
            "text",
        ])
        .output()
        .expect("hyalo config should run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("fuzzy_min_confidence"),
        "an out-of-range floor must be warned about: {stderr}"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("links.fuzzy_min_confidence: 0.8"),
        "and the built-in default must take over: {stdout}"
    );
}
