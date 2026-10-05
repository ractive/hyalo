//! Iteration 306 review: an `enum` property constraint with no `values` list
//! (or an empty one) was silently unsatisfiable — every value failed lint,
//! forever, with no diagnostic pointing at the actual problem (a config
//! shape error). `TryFrom<RawPropertyConstraint>` now rejects it the same
//! way it already rejects `pattern` on a non-`string` type or an
//! uncompilable `key-patterns` regex: the schema fails to load, `hyalo
//! config` reports `malformed: true` / `schema_error`, and `hyalo lint
//! --strict` surfaces it as a visible, always-promoted violation (the same
//! contract `lint_reports_schema_malformed_for_an_invalid_key_pattern_regex`
//! in `lint.rs` pins for the sibling regex shape error).

use super::common::{hyalo_no_hints, write_md};
use tempfile::TempDir;

fn write_schema_toml(dir: &std::path::Path, content: &str) {
    std::fs::write(dir.join(".hyalo.toml"), content).unwrap();
}

fn vault_with_valueless_enum() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_schema_toml(
        tmp.path(),
        r#"dir = "."

[schema.types.iteration.properties.status]
type = "enum"
"#,
    );
    write_md(
        tmp.path(),
        "a.md",
        "---\ntitle: A\ntype: iteration\nstatus: planned\n---\nBody\n",
    );
    tmp
}

#[test]
fn config_reports_malformed_and_schema_error_for_enum_without_values() {
    let tmp = vault_with_valueless_enum();

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["config", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "hyalo config itself must not fail");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout should be JSON: {e}\nstdout: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    assert_eq!(
        json["results"]["malformed"].as_bool(),
        Some(true),
        "an enum constraint with no values must mark the config malformed: {json}"
    );
    let schema_error = json["results"]["schema_error"].as_str().unwrap_or("");
    assert!(
        schema_error.contains("enum") && schema_error.contains("values"),
        "schema_error should name the enum/values shape error, got: {schema_error}"
    );
}

/// Pull the `{"error": ...}` refusal envelope out of stderr: DEC-307 renders
/// every user-error refusal there, pretty-printed, often after a `warning:`
/// preamble line on the same stream, so stdout stays empty for a script to
/// tell "refused" from "answered".
fn refusal_message(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let start = text
        .find('{')
        .unwrap_or_else(|| panic!("no JSON envelope in stderr: {text}"));
    let envelope: serde_json::Value = serde_json::from_str(&text[start..])
        .unwrap_or_else(|e| panic!("refusal must be a JSON envelope: {e}\n{text}"));
    envelope["error"].as_str().unwrap_or_default().to_owned()
}

/// UX-2 (DEC-362, iter-314): `hyalo config` already reported this shape as
/// `malformed: true`, but only `lint --strict` and `find --strict` refused
/// the gate — plain `lint` exited 0 with a warn-level `SCHEMA` row, and
/// `views run <view>` ran the view anyway. All four now behave identically
/// to the unclosed-`[lint` (TOML syntax error) case: exit 1 with the
/// DEC-290 "unusable .hyalo.toml" envelope naming the enum/values diagnostic,
/// never a per-violation lint report.
#[test]
fn lint_strict_refuses_for_enum_without_values() {
    let tmp = vault_with_valueless_enum();

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--strict", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code().unwrap(),
        1,
        "an unloadable [schema] (enum without values) must refuse --strict lint: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.stdout, b"",
        "the refusal is an error, not a result set: stdout must stay empty"
    );
    let message = refusal_message(&output.stderr);
    assert!(
        message.contains("invalid [schema] in .hyalo.toml"),
        "{message}"
    );
    assert!(
        message.contains("enum") && message.contains("values"),
        "{message}"
    );
}

/// Before iter-314, plain `lint` (no `--strict`) exited 0 here with a
/// warn-level `SCHEMA` row — `malformed: true` meant refusal only under
/// `--strict`. `lint`'s exit code is a gate unconditionally (DEC-279); this
/// is the "before" bug this iteration's AC names.
#[test]
fn plain_lint_also_refuses_for_enum_without_values() {
    let tmp = vault_with_valueless_enum();

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code().unwrap(),
        1,
        "plain lint must refuse too, not just --strict: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let message = refusal_message(&output.stderr);
    assert!(
        message.contains("enum") && message.contains("values"),
        "{message}"
    );
}

/// `find --strict` already refused before this iteration; pinned here next
/// to its three siblings so all four gate commands are asserted from one
/// shared repro.
#[test]
fn find_strict_refuses_for_enum_without_values() {
    let tmp = vault_with_valueless_enum();

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["find", "--strict", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code().unwrap(), 1);
    let message = refusal_message(&output.stderr);
    assert!(
        message.contains("enum") && message.contains("values"),
        "{message}"
    );
}

/// Before iter-314, `views run <view>` answered "unknown view" with the
/// schema diagnostic only on stderr when the view name wasn't found, and ran
/// the view to completion (exit 0) when it was — never refusing the gate.
#[test]
fn views_run_refuses_for_enum_without_values() {
    let tmp = vault_with_valueless_enum();
    // Extend the same config with a saved view so `views run` has a real
    // target to (not) run.
    let mut toml = std::fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
    toml.push_str("\n[views.open]\nproperties = [\"status=planned\"]\n");
    write_schema_toml(tmp.path(), &toml);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["views", "run", "open", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code().unwrap(),
        1,
        "views run must refuse, not quietly run the view on the empty fallback schema"
    );
    let message = refusal_message(&output.stderr);
    assert!(
        message.contains("whose exit code is a gate"),
        "must not be mistaken for 'unknown view': {message}"
    );
    assert!(
        message.contains("enum") && message.contains("values"),
        "{message}"
    );
}
