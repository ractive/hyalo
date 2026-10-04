//! `find --view NAME` against a malformed `.hyalo.toml` must surface the
//! parse diagnostic, not "unknown view".
//!
//! `load_views` used to swallow a `.hyalo.toml` that fails to parse into an
//! empty map and print a warning; the view-resolution code then looked the
//! name up in that empty map and reported "unknown view '<name>'" — hiding
//! the real cause (the config itself is unusable) behind a message that
//! sends the user chasing a typo that doesn't exist. This is now a refusal
//! naming the actual parse error, consistent with how `lint` / `find
//! --strict` / `views run` already refuse on an unusable config (DEC-290).

use super::common::{hyalo_no_hints, write_md};

fn write_malformed_config(dir: &std::path::Path) {
    std::fs::write(dir.join(".hyalo.toml"), "[[[broken\n").unwrap();
}

#[test]
fn find_view_with_malformed_config_reports_parse_diagnostic() {
    let tmp = tempfile::tempdir().unwrap();
    write_md(tmp.path(), "note.md", "# Hello\n");
    write_malformed_config(tmp.path());

    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "--view", "somename", "--format", "json"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected exit 1 against a malformed config"
    );
    assert_eq!(output.status.code(), Some(1));

    // JSON-mode errors are written to stderr, not stdout (see
    // `json_errors.rs`'s `assert_json_error`). Here stderr also carries the
    // plain-text "malformed .hyalo.toml" warning emitted while resolving the
    // config, ahead of the JSON error envelope, so parse from the envelope's
    // opening brace rather than the whole stream.
    let stderr_text = String::from_utf8_lossy(&output.stderr);
    let json_start = stderr_text
        .find('{')
        .unwrap_or_else(|| panic!("expected a JSON error envelope on stderr: {stderr_text}"));
    let json: serde_json::Value =
        serde_json::from_str(&stderr_text[json_start..]).expect("json output");
    let error = json["error"].as_str().unwrap_or_default();
    assert!(
        !error.contains("unknown view"),
        "must not hide the parse error behind 'unknown view': {error}"
    );
    assert!(
        error.contains(".hyalo.toml is unusable") && error.contains("TOML parse error"),
        "expected the parse diagnostic in the error, got: {error}"
    );
}

/// `views list` is not a gate command, so it keeps the warn-and-continue
/// policy: a malformed config degrades to "no known views" (exit 0) rather
/// than refusing outright.
#[test]
fn views_list_with_malformed_config_degrades_to_empty() {
    let tmp = tempfile::tempdir().unwrap();
    write_md(tmp.path(), "note.md", "# Hello\n");
    write_malformed_config(tmp.path());

    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["views", "list", "--format", "json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "`views list` must keep warn-and-continue on a malformed config: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("malformed .hyalo.toml"),
        "expected the malformed-config warning on stderr, got: {stderr:?}"
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json output");
    assert_eq!(
        json["results"].as_array().map_or(usize::MAX, Vec::len),
        0,
        "{json}"
    );
}
