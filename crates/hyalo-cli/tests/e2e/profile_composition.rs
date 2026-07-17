//! e2e tests for iter-172 profile composition semantics.
//!
//! Multiple `hyalo init --profile <name>` runs must compose in one vault:
//! array config keys union (never clobber), `[lint] profiles` accumulates so
//! every activated profile's rules fire in plain `hyalo lint`, and a
//! frontmatter-less path-bound file lints clean. A `--profile` CLI overlay
//! composes with the file config exactly like the file path does.

use super::common::{hyalo, hyalo_no_hints, write_md};
use std::fs;
use std::path::Path;
use std::process::Output;

/// Run `hyalo init --profile <name>` in `dir`.
fn init_profile(dir: &Path, profile: &str) -> Output {
    hyalo()
        .current_dir(dir)
        .args(["init", "--profile", profile])
        .output()
        .unwrap()
}

/// Run `hyalo lint --format json` (no explicit `--profile`) in `dir` and return
/// the parsed `results` object plus process output.
fn lint_json(dir: &Path, extra: &[&str]) -> (serde_json::Value, Output) {
    let mut args = vec!["--dir", ".", "--format", "json", "lint"];
    args.extend_from_slice(extra);
    let output = hyalo_no_hints()
        .current_dir(dir)
        .args(&args)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("JSON parse: {e}\n{stdout}"));
    (json["results"].clone(), output)
}

/// True when any file's `rule_groups` fired a rule with the given id.
fn rule_fired(results: &serde_json::Value, rule_id: &str) -> bool {
    results["files"].as_array().is_some_and(|files| {
        files.iter().any(|f| {
            f["rule_groups"]
                .as_array()
                .is_some_and(|gs| gs.iter().any(|g| g["rule"].as_str() == Some(rule_id)))
        })
    })
}

/// Collect `(file, rule, severity, message)` tuples across all files, filtered
/// so that only files whose path ends with `path_suffix` are returned.
fn violations_for(results: &serde_json::Value, path_suffix: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let Some(files) = results["files"].as_array() else {
        return out;
    };
    for f in files {
        let file = f["file"].as_str().unwrap_or_default();
        if !file.ends_with(path_suffix) {
            continue;
        }
        for g in f["rule_groups"].as_array().cloned().unwrap_or_default() {
            let rule = g["rule"].as_str().unwrap_or_default().to_owned();
            let sev = g["severity"].as_str().unwrap_or_default().to_owned();
            for v in g["violations"].as_array().cloned().unwrap_or_default() {
                let msg = v["message"].as_str().unwrap_or_default().to_owned();
                out.push((rule.clone(), sev.clone(), msg));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// AC: three `init --profile` runs → all binds + all exemptions active at once.
// ---------------------------------------------------------------------------

#[test]
fn three_profile_inits_keep_all_binds_and_exempt() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();

    for p in ["okf", "madr", "changelog"] {
        let out = init_profile(dir, p);
        assert!(
            out.status.success(),
            "init --profile {p} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let toml = fs::read_to_string(dir.join(".hyalo.toml")).unwrap();
    let val: toml::Value = toml::from_str(&toml).unwrap();

    // All three profiles active in the list.
    let profiles: Vec<&str> = val["lint"]["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(profiles.contains(&"okf"), "okf active: {profiles:?}");
    assert!(profiles.contains(&"madr"), "madr active: {profiles:?}");
    assert!(
        profiles.contains(&"changelog"),
        "changelog active: {profiles:?}"
    );

    // Binds from madr + changelog both present (array-of-tables union).
    let raw: hyalo_core::schema::RawSchemaConfig = val["schema"].clone().try_into().unwrap();
    let cfg = hyalo_core::schema::SchemaConfig::try_from(raw).unwrap();
    assert_eq!(
        cfg.bound_type_for("docs/decisions/0001-x.md"),
        Some("adr"),
        "madr bind present"
    );
    assert_eq!(
        cfg.bound_type_for("CHANGELOG.md"),
        Some("changelog"),
        "changelog bind present"
    );

    // Exemptions from okf + changelog both present (array union).
    assert!(cfg.exempt.is_exempt("bundle/index.md"), "okf exempt kept");
    assert!(
        cfg.exempt.is_exempt("CHANGELOG.md"),
        "changelog exempt kept"
    );
}

#[test]
fn repeated_init_profile_is_byte_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();

    for p in ["okf", "madr"] {
        assert!(init_profile(dir, p).status.success());
    }
    let first = fs::read_to_string(dir.join(".hyalo.toml")).unwrap();

    // Re-run both — config must not change.
    for p in ["okf", "madr"] {
        assert!(init_profile(dir, p).status.success());
    }
    let second = fs::read_to_string(dir.join(".hyalo.toml")).unwrap();
    assert_eq!(
        first, second,
        "re-running init --profile is byte-idempotent"
    );
}

// ---------------------------------------------------------------------------
// AC: the hoppy regression — okf then madr → OKF rules still fire, reserved
// files stay exempt, error count does not regress.
// ---------------------------------------------------------------------------

#[test]
fn okf_rules_fire_after_madr_init() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    assert!(init_profile(dir, "okf").status.success());
    assert!(init_profile(dir, "madr").status.success());

    // An OKF table missing its `# Citations` section triggers the OKF advisory
    // rule — proof the OKF profile's rules are still active after madr init.
    write_md(
        dir,
        "tables/blocks.md",
        "---\ntype: BigQuery Table\ntitle: Blocks\ndescription: The blocks table.\n---\n# Schema\n\nColumns.\n",
    );
    // A reserved OKF index.md must stay exempt (no missing-type error).
    write_md(
        dir,
        "index.md",
        "---\nokf_version: \"0.1\"\n---\n\n<!-- okf:index:begin -->\n* [Blocks](tables/blocks.md)\n<!-- okf:index:end -->\n",
    );

    let (results, _out) = lint_json(dir, &[]);
    // OKF citation rule fired (proves okf profile still active post-madr).
    assert!(
        rule_fired(&results, "OKF-CITATIONS-PRESENT"),
        "OKF advisory rules must fire after madr init: {results}"
    );
    // index.md is reserved/exempt → no missing-type error for it.
    let index_type_errors = violations_for(&results, "index.md")
        .into_iter()
        .filter(|(_, sev, msg)| sev == "error" && msg.contains("type"))
        .count();
    assert_eq!(
        index_type_errors, 0,
        "reserved index.md must stay exempt: {results}"
    );
}

// ---------------------------------------------------------------------------
// AC: frontmatter-less bound files (SKILL.md, ADR) lint clean under composed
// profiles — bind = typing.
// ---------------------------------------------------------------------------

#[test]
fn frontmatterless_bound_files_lint_clean_under_composed_profiles() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    for p in ["okf", "skills", "changelog"] {
        assert!(init_profile(dir, p).status.success());
    }

    // A spec-valid SKILL.md with the two required fields but NO `type:`.
    write_md(
        dir,
        ".claude/skills/foo/SKILL.md",
        "---\nname: foo\ndescription: does a thing\n---\n# Foo\n\nBody.\n",
    );
    // A frontmatter-less CHANGELOG.md.
    fs::write(
        dir.join("CHANGELOG.md"),
        "# Changelog\n\n## [1.0.0] - 2026-07-17\n\n### Added\n\n- Initial release.\n",
    )
    .unwrap();

    let (results, out) = lint_json(dir, &[]);
    assert!(
        out.status.success(),
        "lint exited non-zero: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // No missing-type / no-type error or warning against the bound SKILL.md.
    let skill_type_issues: Vec<(String, String, String)> = violations_for(&results, "SKILL.md")
        .into_iter()
        .filter(|(_, _, msg)| {
            msg.contains("missing required property \"type\"") || msg.contains("no 'type' property")
        })
        .collect();
    assert!(
        skill_type_issues.is_empty(),
        "frontmatter-less bound SKILL.md must not flag missing type: {skill_type_issues:?}"
    );
}

// ---------------------------------------------------------------------------
// AC: `--profile <name>` flag honors user `[schema] exempt` additions exactly
// like `[lint] profiles` file activation (mapl BUG-6 flag-vs-file parity).
// ---------------------------------------------------------------------------

#[test]
fn profile_flag_honors_user_exempt_like_file_activation() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    // User hand-adds an exempt glob before any profile init.
    fs::write(
        dir.join(".hyalo.toml"),
        "[schema]\nexempt = [\"vendored/**\"]\n",
    )
    .unwrap();

    // A frontmatter-less file under the user-exempt path — must not error even
    // though okf's default schema requires `type`.
    write_md(dir, "vendored/thirdparty.md", "no frontmatter here\n");

    // Schema errors against the exempt file (the thing `[schema] exempt`
    // controls) — a frontmatter-less file would otherwise error on missing
    // `type`. Advisory OKF rules are a separate axis and fire regardless; we
    // assert only that the schema exemption is honored, and identically for
    // both activation paths.
    let schema_errors = |r: &serde_json::Value| -> usize {
        violations_for(r, "vendored/thirdparty.md")
            .into_iter()
            .filter(|(_, sev, _)| sev == "error")
            .count()
    };

    // Path A: ephemeral `--profile okf` flag.
    let (results_flag, out_flag) = lint_json(dir, &["--profile", "okf"]);
    assert!(out_flag.status.success(), "flag lint failed");
    let flag_errors = schema_errors(&results_flag);
    assert_eq!(
        flag_errors, 0,
        "--profile flag must honor user exempt (no schema error): {results_flag}"
    );

    // Path B: file activation — init okf (unions the user exempt), then plain lint.
    assert!(init_profile(dir, "okf").status.success());
    let (results_file, out_file) = lint_json(dir, &[]);
    assert!(out_file.status.success(), "file lint failed");
    let file_errors = schema_errors(&results_file);
    assert_eq!(
        file_errors, 0,
        "file activation must honor user exempt (no schema error): {results_file}"
    );
    // Flag and file paths produce identical schema-error behavior (parity).
    assert_eq!(
        flag_errors, file_errors,
        "flag vs file activation must be equivalent for user exempt"
    );
}
