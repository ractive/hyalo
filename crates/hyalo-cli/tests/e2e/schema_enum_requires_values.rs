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
use hyalo_cli::commands::lint::ExtLintOutput;
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
        "an unloadable [schema] (enum without values) must fail --strict lint: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let results: ExtLintOutput = serde_json::from_slice::<serde_json::Value>(&output.stdout)
        .ok()
        .and_then(|v| v.get("results").cloned())
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_else(|| panic!("stdout should parse as lint results"));
    let malformed_messages: Vec<&str> = results
        .files
        .iter()
        .flat_map(|f| f.rule_groups.iter())
        .flat_map(|g| g.violations.iter())
        .filter(|v| v.severity == "error")
        .map(|v| v.message.as_str())
        .collect();
    assert_eq!(malformed_messages.len(), 1, "{malformed_messages:?}");
    assert!(
        malformed_messages[0].contains("invalid [schema] in .hyalo.toml"),
        "{malformed_messages:?}"
    );
    assert!(
        malformed_messages[0].contains("enum") && malformed_messages[0].contains("values"),
        "{malformed_messages:?}"
    );
}
