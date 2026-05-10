/// E2E tests for iter-130 CWD-aware features:
///   1. Help banner (CWD-aware)
///   2. `--version` with kb dir
///   3. `hyalo summary` includes `kb dir:` / `dir` field
///   4. Redundant `--dir` warning
use std::fs;

use super::common::{hyalo_no_hints, write_md};
use tempfile::TempDir;

fn setup_minimal(tmp: &TempDir, subdir: Option<&str>) {
    let dir = if let Some(d) = subdir {
        fs::create_dir_all(tmp.path().join(d)).unwrap();
        tmp.path().join(d)
    } else {
        tmp.path().to_path_buf()
    };
    write_md(&dir, "note.md", "---\ntitle: Test\n---\n");
}

// ---------------------------------------------------------------------------
// 1. CWD-aware help banner
// ---------------------------------------------------------------------------

#[test]
fn help_banner_shown_when_config_in_cwd() {
    let tmp = TempDir::new().unwrap();
    setup_minimal(&tmp, Some("kb"));
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \"kb\"\n").unwrap();

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--help"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Banner should be in the help output
    assert!(
        stdout.contains("runs against `kb`"),
        "expected info banner in --help; got:\n{stdout}"
    );
}

#[test]
fn help_banner_not_shown_in_unrelated_dir() {
    let tmp = TempDir::new().unwrap();
    setup_minimal(&tmp, None);
    // No .hyalo.toml

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--help"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("runs against"),
        "expected no banner in --help for unrelated dir; got:\n{stdout}"
    );
    assert!(
        !stdout.contains("inside the kb folder"),
        "expected no inside-vault warning; got:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// 2. --version includes kb dir when .hyalo.toml present
// ---------------------------------------------------------------------------

#[test]
fn version_includes_kb_dir_when_config_present() {
    let tmp = TempDir::new().unwrap();
    setup_minimal(&tmp, Some("vault"));
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \"vault\"\n").unwrap();

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--version"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(
        stdout.contains("kb dir:"),
        "expected '(kb dir: ...)' in --version output; got: {stdout}"
    );
    assert!(
        stdout.contains("vault"),
        "expected dir 'vault' in --version output; got: {stdout}"
    );
}

#[test]
fn version_plain_when_no_config() {
    let tmp = TempDir::new().unwrap();
    setup_minimal(&tmp, None);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--version"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(
        !stdout.contains("kb dir:"),
        "expected plain version without kb dir; got: {stdout}"
    );
    // Should still contain the version number
    assert!(
        stdout.contains("hyalo"),
        "expected 'hyalo' in version output; got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// 3. hyalo summary includes kb dir
// ---------------------------------------------------------------------------

#[test]
fn summary_text_includes_kb_dir_line() {
    let tmp = TempDir::new().unwrap();
    setup_minimal(&tmp, Some("notes"));
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \"notes\"\n").unwrap();

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["summary", "--format", "text"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(
        stdout.contains("kb dir:"),
        "expected 'kb dir:' line in summary text; got: {stdout}"
    );
    assert!(
        stdout.contains("notes"),
        "expected dir 'notes' in summary text; got: {stdout}"
    );
}

#[test]
fn summary_json_includes_dir_field() {
    let tmp = TempDir::new().unwrap();
    setup_minimal(&tmp, Some("docs"));
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \"docs\"\n").unwrap();

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["summary", "--format", "json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    let json: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("invalid JSON: {e}\n{stdout}"));

    let dir_val = json["results"]["dir"].as_str().unwrap_or("");
    assert!(
        dir_val.contains("docs"),
        "expected 'docs' in summary JSON dir field; got dir={dir_val:?}"
    );
}

#[test]
fn summary_json_dir_present_without_config() {
    // When no config, dir defaults to "." — the field should still be present.
    let tmp = TempDir::new().unwrap();
    setup_minimal(&tmp, None);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["summary", "--format", "json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");

    let json: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("invalid JSON: {e}\n{stdout}"));

    assert!(
        json["results"].get("dir").is_some(),
        "expected 'dir' field in summary JSON; got: {json}"
    );
}

// ---------------------------------------------------------------------------
// 4. Redundant --dir warning
// ---------------------------------------------------------------------------

#[test]
fn redundant_dir_warning_when_same_as_config() {
    let tmp = TempDir::new().unwrap();
    setup_minimal(&tmp, Some("vault"));
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \"vault\"\n").unwrap();

    let vault_path = tmp.path().join("vault");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "--dir",
            vault_path.to_str().unwrap(),
            "summary",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success());
    assert!(
        stderr.contains("--dir is redundant"),
        "expected redundant --dir warning; got stderr: {stderr}"
    );
    // UX-1 (iter-131): the notice is prefixed with `note:` exactly once,
    // never the legacy double `warning: note:` form.
    assert!(
        stderr.contains("note: --dir is redundant"),
        "expected single `note:` prefix; got stderr: {stderr}"
    );
    assert!(
        !stderr.contains("warning: note:"),
        "must not double-prefix `warning: note:`; got stderr: {stderr}"
    );
}

#[test]
fn no_redundant_dir_warning_when_dir_differs() {
    let tmp = TempDir::new().unwrap();
    setup_minimal(&tmp, Some("other-vault"));
    fs::create_dir_all(tmp.path().join("vault")).unwrap();
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \"vault\"\n").unwrap();

    let other_path = tmp.path().join("other-vault");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "--dir",
            other_path.to_str().unwrap(),
            "summary",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success());
    assert!(
        !stderr.contains("--dir is redundant"),
        "unexpected redundant --dir warning; got stderr: {stderr}"
    );
}

#[test]
fn no_redundant_dir_warning_when_no_config() {
    let tmp = TempDir::new().unwrap();
    setup_minimal(&tmp, None);
    // No .hyalo.toml — no redundant warning possible

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "summary",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success());
    assert!(
        !stderr.contains("--dir is redundant"),
        "unexpected redundant --dir warning without config; got stderr: {stderr}"
    );
}
