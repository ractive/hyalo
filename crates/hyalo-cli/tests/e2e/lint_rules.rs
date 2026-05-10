//! e2e tests for `hyalo lint-rules set`, focused on BUG-2 (iter-131):
//! setting a property to its current default must be a true no-op (no
//! redundant TOML override, no spurious "wrote ..." line).

use super::common::hyalo_no_hints;
use tempfile::TempDir;

fn make_project() -> TempDir {
    let project = TempDir::new().unwrap();
    std::fs::write(project.path().join(".hyalo.toml"), "dir = \"kb\"\n").unwrap();
    std::fs::create_dir_all(project.path().join("kb")).unwrap();
    project
}

#[test]
fn set_severity_to_default_is_noop_when_no_prior_override() {
    // HYALO002 default severity is "error"; setting it to "error" with no
    // existing override must NOT mutate the .hyalo.toml file and must report
    // wrote=false.
    let project = make_project();
    let toml_path = project.path().join(".hyalo.toml");
    let before = std::fs::read_to_string(&toml_path).unwrap();

    let assert = hyalo_no_hints()
        .current_dir(project.path())
        .args([
            "lint-rules",
            "set",
            "HYALO002",
            "--severity",
            "error",
            "--format",
            "json",
        ])
        .assert()
        .success();

    let after = std::fs::read_to_string(&toml_path).unwrap();
    assert_eq!(
        before, after,
        "tautological set must not mutate .hyalo.toml.\nbefore:\n{before}\nafter:\n{after}"
    );

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("valid json");
    let wrote = v
        .pointer("/results/wrote")
        .and_then(serde_json::Value::as_bool);
    assert_eq!(wrote, Some(false), "wrote should be false, got: {stdout}");
}

#[test]
fn set_severity_back_to_default_prunes_override_and_parents() {
    // First set HYALO002 severity to "warn" (non-default), then back to
    // "error" (default). The override and its empty parents must be pruned.
    let project = make_project();
    let toml_path = project.path().join(".hyalo.toml");

    hyalo_no_hints()
        .current_dir(project.path())
        .args([
            "lint-rules",
            "set",
            "HYALO002",
            "--severity",
            "warn",
            "--format",
            "json",
        ])
        .assert()
        .success();

    let intermediate = std::fs::read_to_string(&toml_path).unwrap();
    assert!(
        intermediate.contains("[lint.rules.HYALO002]"),
        "non-default value should materialise the override, got:\n{intermediate}"
    );

    hyalo_no_hints()
        .current_dir(project.path())
        .args([
            "lint-rules",
            "set",
            "HYALO002",
            "--severity",
            "error",
            "--format",
            "json",
        ])
        .assert()
        .success();

    let final_contents = std::fs::read_to_string(&toml_path).unwrap();
    assert!(
        !final_contents.contains("[lint."),
        "back-to-default should prune all [lint.*] sections, got:\n{final_contents}"
    );
    assert!(
        final_contents.trim().contains("dir = \"kb\""),
        "non-lint config should be preserved, got:\n{final_contents}"
    );
}

#[test]
fn set_severity_to_non_default_still_writes_override() {
    // Regression guard for the iter-127 scalar→table promotion fix —
    // non-default values must continue to materialise the override.
    let project = make_project();
    let toml_path = project.path().join(".hyalo.toml");

    let assert = hyalo_no_hints()
        .current_dir(project.path())
        .args([
            "lint-rules",
            "set",
            "HYALO002",
            "--severity",
            "warn",
            "--format",
            "json",
        ])
        .assert()
        .success();

    let after = std::fs::read_to_string(&toml_path).unwrap();
    assert!(
        after.contains("[lint.rules.HYALO002]"),
        "non-default severity must produce the override section, got:\n{after}"
    );
    assert!(
        after.contains("severity = \"warn\""),
        "override should record the chosen severity, got:\n{after}"
    );

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("valid json");
    assert_eq!(
        v.pointer("/results/wrote")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
}

// ---------------------------------------------------------------------------
// BUG-4: lint-rules remove must leave clean TOML (no empty tables) (iter-133)
// ---------------------------------------------------------------------------

#[test]
fn remove_rule_leaves_clean_toml() {
    // Set a rule override, then remove it — the resulting TOML must not contain
    // empty [lint] or [lint.rules] tables.
    let project = make_project();
    let toml_path = project.path().join(".hyalo.toml");

    // Add an override
    hyalo_no_hints()
        .current_dir(project.path())
        .args(["lint-rules", "set", "HYALO002", "--severity", "warn"])
        .assert()
        .success();

    // Verify the override was written
    let before = std::fs::read_to_string(&toml_path).unwrap();
    assert!(
        before.contains("HYALO002"),
        "override should be present before remove, got:\n{before}"
    );

    // Remove the override
    hyalo_no_hints()
        .current_dir(project.path())
        .args(["lint-rules", "remove", "HYALO002"])
        .assert()
        .success();

    // Verify no empty tables remain
    let after = std::fs::read_to_string(&toml_path).unwrap();
    assert!(
        !after.contains("[lint.rules]"),
        "empty [lint.rules] table should be pruned, got:\n{after}"
    );
    assert!(
        !after.contains("[lint.rules.HYALO002]"),
        "removed rule section should be gone, got:\n{after}"
    );
    assert!(
        !after.contains("[lint]"),
        "empty [lint] table should be pruned, got:\n{after}"
    );
}

#[test]
fn remove_rule_with_multiple_overrides_only_prunes_target() {
    // When two rules are set and one is removed, the other must remain.
    let project = make_project();
    let toml_path = project.path().join(".hyalo.toml");

    hyalo_no_hints()
        .current_dir(project.path())
        .args(["lint-rules", "set", "HYALO001", "--severity", "warn"])
        .assert()
        .success();

    hyalo_no_hints()
        .current_dir(project.path())
        .args(["lint-rules", "set", "HYALO002", "--severity", "warn"])
        .assert()
        .success();

    hyalo_no_hints()
        .current_dir(project.path())
        .args(["lint-rules", "remove", "HYALO001"])
        .assert()
        .success();

    let after = std::fs::read_to_string(&toml_path).unwrap();
    assert!(
        !after.contains("HYALO001"),
        "removed rule should be gone, got:\n{after}"
    );
    assert!(
        after.contains("HYALO002"),
        "remaining rule should still be present, got:\n{after}"
    );
    // [lint.rules] should still exist (still has HYALO002)
    assert!(
        after.contains("[lint") && after.contains("HYALO002"),
        "lint section should remain for HYALO002, got:\n{after}"
    );
}
