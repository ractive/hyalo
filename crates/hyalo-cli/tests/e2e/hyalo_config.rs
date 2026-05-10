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
    // Raw contents section
    assert!(
        stdout.contains("--- .hyalo.toml ---"),
        "expected raw contents separator; got: {stdout}"
    );
    assert!(
        stdout.contains("dir = \"kb\""),
        "expected raw TOML content; got: {stdout}"
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

    // hyalo config returns a flat JSON object (not the standard envelope)
    assert!(
        json.get("cwd").is_some(),
        "expected 'cwd' field; got: {json}"
    );
    assert!(
        json.get("dir").is_some(),
        "expected 'dir' field; got: {json}"
    );
    assert_eq!(
        json["hints"], true,
        "expected hints = true by default; got: {json}"
    );
    assert!(
        json["config_path"].is_null(),
        "expected config_path = null when no config; got: {json}"
    );
    assert_eq!(
        json["dir"].as_str().unwrap(),
        ".",
        "expected default dir '.'; got: {json}"
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

    // hyalo config returns a flat JSON object (not the standard envelope)
    assert_eq!(
        json["dir"].as_str().unwrap(),
        "vault",
        "expected dir 'vault'; got: {json}"
    );
    assert_eq!(
        json["format"].as_str().unwrap(),
        "text",
        "expected format 'text' from config; got: {json}"
    );
    // config_path should be a non-null string
    assert!(
        json["config_path"].is_string(),
        "expected config_path string; got: {json}"
    );
    assert!(
        json["config_path"]
            .as_str()
            .unwrap()
            .contains(".hyalo.toml"),
        "expected config_path to contain .hyalo.toml; got: {json}"
    );
    // raw_contents should be present and contain the TOML content
    assert!(
        json["raw_contents"].as_str().unwrap().contains("vault"),
        "expected raw_contents to include dir value; got: {json}"
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
