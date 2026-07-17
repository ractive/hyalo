//! e2e tests for the `skills` profile — init, path-bound `skill` schema, the
//! hard `name`/`description` constraints (regex + length bounds), the Agent
//! Skills advisory lint rules, and `hyalo new --type skill` scaffolding.
//!
//! The profile is pure data over the iter-164/167 machinery plus one generic
//! mechanism it is the first consumer of: string `min-length`/`max-length`
//! bounds on any `string` property.

use super::common::hyalo_no_hints;
use serde_json::Value;
use std::process::Output;
use tempfile::TempDir;

/// Run `hyalo` in `dir` with `--dir . --format json` and the given args; return
/// the parsed top-level JSON plus the process output (for exit codes).
fn run(dir: &std::path::Path, args: &[&str]) -> (Value, Output) {
    let mut full = vec!["--dir", ".", "--format", "json"];
    full.extend_from_slice(args);
    let output = hyalo_no_hints()
        .current_dir(dir)
        .args(&full)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("JSON parse: {e}\n{stdout}"));
    (json, output)
}

/// `hyalo init --profile skills` in a fresh temp dir.
fn init_skills() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", ".", "init", "--profile", "skills"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "init --profile skills failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    tmp
}

/// Write `<dir>/<skill_name>/SKILL.md` with the given frontmatter fields.
fn write_skill(root: &std::path::Path, skill_dir: &str, name: &str, description: &str) {
    let skill = root.join(skill_dir);
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n\nBody.\n"),
    )
    .unwrap();
}

/// Whether a rule id appears in the lint `results.files[].rule_groups[].rule`.
fn rule_fired(results: &Value, rule_id: &str) -> bool {
    let Some(files) = results["files"].as_array() else {
        return false;
    };
    files.iter().any(|f| {
        f["rule_groups"]
            .as_array()
            .is_some_and(|gs| gs.iter().any(|g| g["rule"].as_str() == Some(rule_id)))
    })
}

// ---------------------------------------------------------------------------
// init + bind + new
// ---------------------------------------------------------------------------

#[test]
fn init_writes_skills_config() {
    let tmp = init_skills();
    let cfg = std::fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
    assert!(cfg.contains("profile = \"skills\""), "records lint profile");
    assert!(cfg.contains("[[schema.bind]]"), "writes a bind entry");
    assert!(cfg.contains("**/SKILL.md"), "binds every SKILL.md");
    assert!(
        cfg.contains("[schema.types.skill]"),
        "declares the skill type"
    );
    assert!(cfg.contains("max-length = 1024"), "description length cap");
    assert!(cfg.contains("max-length = 64"), "name length cap");
}

#[test]
fn new_skill_scaffolds_and_is_a_skill() {
    let tmp = init_skills();
    let (json, output) = run(
        tmp.path(),
        &["new", "--type", "skill", "--file", "my-skill/SKILL.md"],
    );
    assert!(output.status.success(), "new skill failed: {json}");
    let content = std::fs::read_to_string(tmp.path().join("my-skill/SKILL.md")).unwrap();
    assert!(content.contains("type: skill"));
    assert!(content.contains("name:"));
    assert!(content.contains("description:"));
}

#[test]
fn bound_skill_lints_clean_without_explicit_type() {
    let tmp = init_skills();
    // No `type:` frontmatter — the path binding supplies `skill`.
    write_skill(
        tmp.path(),
        "good-skill",
        "good-skill",
        "A concise valid description.",
    );
    let (json, output) = run(tmp.path(), &["lint", "--profile", "skills"]);
    let r = &json["results"];
    assert_eq!(r["errors"].as_u64(), Some(0), "no errors: {json}");
    assert!(output.status.success(), "lint should pass: {json}");
}

// ---------------------------------------------------------------------------
// hard schema constraints (errors)
// ---------------------------------------------------------------------------

#[test]
fn name_pattern_rejects_consecutive_hyphens() {
    let tmp = init_skills();
    write_skill(tmp.path(), "bad--name", "bad--name", "ok.");
    let (json, output) = run(tmp.path(), &["lint", "--profile", "skills"]);
    assert!(
        json["results"]["errors"].as_u64().unwrap_or(0) >= 1,
        "consecutive-hyphen name must error: {json}"
    );
    assert!(!output.status.success(), "errors fail lint");
}

#[test]
fn name_over_64_chars_errors() {
    let tmp = init_skills();
    let long = "a".repeat(70);
    write_skill(tmp.path(), &long, &long, "ok.");
    let (json, _) = run(tmp.path(), &["lint", "--profile", "skills"]);
    assert!(
        json["results"]["errors"].as_u64().unwrap_or(0) >= 1,
        "70-char name must error: {json}"
    );
}

#[test]
fn description_over_1024_chars_errors() {
    let tmp = init_skills();
    let long_desc = "x".repeat(1100);
    write_skill(tmp.path(), "toolong", "toolong", &long_desc);
    let (json, _) = run(tmp.path(), &["lint", "--profile", "skills"]);
    assert!(
        json["results"]["errors"].as_u64().unwrap_or(0) >= 1,
        "1100-char description must error: {json}"
    );
}

#[test]
fn description_with_xml_tag_errors() {
    let tmp = init_skills();
    write_skill(tmp.path(), "tagged", "tagged", "Has a <b>tag</b> inside.");
    let (json, _) = run(tmp.path(), &["lint", "--profile", "skills"]);
    assert!(
        json["results"]["errors"].as_u64().unwrap_or(0) >= 1,
        "XML tag in description must error: {json}"
    );
}

// ---------------------------------------------------------------------------
// advisory rules
// ---------------------------------------------------------------------------

#[test]
fn reserved_name_errors() {
    let tmp = init_skills();
    // Directory named `claude` so only the reserved-name rule fires (name==dir).
    write_skill(tmp.path(), "claude", "claude", "Reserved word name.");
    let (json, output) = run(tmp.path(), &["lint", "--profile", "skills"]);
    assert!(
        rule_fired(&json["results"], "SKILL-RESERVED-NAME"),
        "reserved name rule must fire: {json}"
    );
    assert!(
        json["results"]["errors"].as_u64().unwrap_or(0) >= 1,
        "SKILL-RESERVED-NAME is error-severity: {json}"
    );
    assert!(!output.status.success(), "error fails lint");
}

#[test]
fn dirname_mismatch_warns() {
    let tmp = init_skills();
    write_skill(
        tmp.path(),
        "mismatch-dir",
        "other-name",
        "Name does not match dir.",
    );
    let (json, output) = run(tmp.path(), &["lint", "--profile", "skills"]);
    assert!(
        rule_fired(&json["results"], "SKILL-NAME-DIRNAME"),
        "dirname rule must fire: {json}"
    );
    assert_eq!(
        json["results"]["errors"].as_u64(),
        Some(0),
        "dirname mismatch is warn-only: {json}"
    );
    assert!(output.status.success(), "warn does not fail lint");
}

#[test]
fn line_budget_warns_above_500() {
    use std::fmt::Write as _;
    let tmp = init_skills();
    let skill = tmp.path().join("big");
    std::fs::create_dir_all(&skill).unwrap();
    let mut body = String::from("---\nname: big\ndescription: Long body skill.\n---\n\n");
    for i in 0..520 {
        let _ = writeln!(body, "line {i}");
    }
    std::fs::write(skill.join("SKILL.md"), body).unwrap();
    let (json, output) = run(tmp.path(), &["lint", "--profile", "skills"]);
    assert!(
        rule_fired(&json["results"], "SKILL-LINE-BUDGET"),
        "line budget rule must fire: {json}"
    );
    assert!(output.status.success(), "warn does not fail lint");
}

#[test]
fn skill_rules_are_generic_catalog_entries_not_hardcoded() {
    let tmp = init_skills();
    let (json, output) = run(tmp.path(), &["lint-rules", "list"]);
    assert!(output.status.success(), "lint-rules list failed: {json}");
    let stdout_has = |id: &str| {
        json["results"]
            .as_array()
            .is_some_and(|rs| rs.iter().any(|r| r["id"].as_str() == Some(id)))
    };
    for id in [
        "SKILL-RESERVED-NAME",
        "SKILL-NAME-DIRNAME",
        "SKILL-LINE-BUDGET",
    ] {
        assert!(stdout_has(id), "{id} must be a catalog entry: {json}");
    }
}

#[test]
fn skill_rule_can_be_disabled() {
    let tmp = init_skills();
    write_skill(
        tmp.path(),
        "mismatch-dir",
        "other-name",
        "Name does not match dir.",
    );
    // Disable the dirname rule; the warning must disappear.
    let (_, out) = run(
        tmp.path(),
        &[
            "lint-rules",
            "set",
            "SKILL-NAME-DIRNAME",
            "--enabled",
            "false",
        ],
    );
    assert!(out.status.success(), "lint-rules set failed");
    let (json, _) = run(tmp.path(), &["lint", "--profile", "skills"]);
    assert!(
        !rule_fired(&json["results"], "SKILL-NAME-DIRNAME"),
        "disabled rule must not fire: {json}"
    );
}
