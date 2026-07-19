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
    assert!(
        cfg.contains("profiles = [\"skills\"]"),
        "records lint profile"
    );
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
    // `**/SKILL.md` binds this path to `skill`, so the scaffold omits the
    // non-spec explicit `type:` key (SKILL.md carries no `type:` per the Agent
    // Skills spec) — iter-175.
    assert!(
        !content.contains("type:"),
        "bound SKILL.md omits explicit type: {content}"
    );
    // iter-181 task 4: `name` is a slug (`^[a-z0-9]+(-[a-z0-9]+)*$`) the generic
    // `TBD` placeholder cannot satisfy, so the scaffold OMITS it rather than
    // shipping an invalid `name: TBD` — the user fills a valid slug (and a later
    // `lint` flags the missing required field). `description` accepts `TBD`
    // (pattern `^[^<]*$`, min-length 1), so it keeps its placeholder.
    assert!(
        !content.contains("name:"),
        "name (slug pattern) placeholder must be omitted, not invalid TBD: {content}"
    );
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

/// Dogfooding regression: this repo's own `.claude/skills/` directory must
/// lint clean under the `skills` profile — zero `SKILL-*`/schema errors.
/// Copies the real skill dirs into a temp vault (rather than pointing `--dir`
/// at the live `.claude/skills`) so the test is hermetic and doesn't depend on
/// the current working directory of `cargo test`.
#[test]
fn this_repos_own_skills_lint_clean_under_skills_profile() {
    let tmp = init_skills();
    let repo_skills = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.claude/skills")
        .canonicalize()
        .expect(".claude/skills must exist at the repo root");
    let mut copied_any = false;
    for entry in std::fs::read_dir(&repo_skills).unwrap() {
        let entry = entry.unwrap();
        if !entry.file_type().unwrap().is_dir() {
            continue;
        }
        let src = entry.path().join("SKILL.md");
        if !src.is_file() {
            continue;
        }
        let dest_dir = tmp.path().join(entry.file_name());
        std::fs::create_dir_all(&dest_dir).unwrap();
        std::fs::copy(&src, dest_dir.join("SKILL.md")).unwrap();
        copied_any = true;
    }
    assert!(
        copied_any,
        "expected at least one SKILL.md under {repo_skills:?}"
    );

    let (json, output) = run(tmp.path(), &["lint", "--profile", "skills"]);
    assert!(output.status.success(), "lint failed: {json}");
    let errors = json["results"]["errors"].as_i64().unwrap_or(-1);
    assert_eq!(
        errors, 0,
        "this repo's own skills must lint clean under --profile skills: {json}"
    );
}

// ---------------------------------------------------------------------------
// UX-A: [scan] include reaches .claude/skills/
// ---------------------------------------------------------------------------

#[test]
fn scan_include_reaches_claude_skills_dir() {
    // The skills profile ships `[scan] include = [".claude/skills/**"]`, so a
    // SKILL.md under the canonical (hidden) Claude Code location is discoverable
    // and lintable without relocating it (ff-rdp U1 scenario).
    let tmp = init_skills();
    let skill = tmp.path().join(".claude/skills/my-skill");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: my-skill\ndescription: A discoverable dot-dir skill.\n---\n\n# My Skill\n\nBody.\n",
    )
    .unwrap();

    // The config carries the include glob.
    let cfg = std::fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
    assert!(
        cfg.contains(".claude/skills/**"),
        "skills profile ships the scan include: {cfg}"
    );

    // `find` sees the hidden-dir skill.
    let (json, output) = run(
        tmp.path(),
        &["find", "--glob", ".claude/skills/**/SKILL.md"],
    );
    assert!(output.status.success(), "find failed: {json}");
    assert_eq!(
        json["total"].as_u64(),
        Some(1),
        "the .claude/skills SKILL.md is discovered: {json}"
    );

    // And it lints clean (no relocation needed).
    let (lint_json, lint_out) = run(
        tmp.path(),
        &["lint", "--file", ".claude/skills/my-skill/SKILL.md"],
    );
    assert!(
        lint_out.status.success(),
        "lint reached the file: {lint_json}"
    );
}

#[test]
fn scan_include_never_reaches_git() {
    // Even with the include list active, `.git/**` stays hard-excluded.
    let tmp = init_skills();
    std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
    std::fs::write(tmp.path().join(".git/config.md"), "# never linted\n").unwrap();
    let (json, output) = run(tmp.path(), &["find", "--glob", "**/*.md"]);
    assert!(output.status.success(), "find failed: {json}");
    let files = json["results"].as_array().cloned().unwrap_or_default();
    let any_git = files
        .iter()
        .any(|f| f["file"].as_str().is_some_and(|p| p.starts_with(".git/")));
    assert!(!any_git, ".git is never walked: {json}");
}
