//! Config-trust e2e gates (iter-201).
//!
//! Three ways a user's `.hyalo.toml` used to stop applying without a signal
//! strong enough to notice — two of which made CI go vacuously green:
//!
//! - **H-4** — an explicit `--dir` naming the *configured* vault discarded the
//!   whole config (schema, views, `[lint] ignore`, severity overrides) while
//!   printing "--dir is redundant".
//! - **M-2** — one malformed key anywhere in the file fell back to *all*
//!   defaults, including `dir`, and `-q` hid the warning; a mutating command
//!   would then happily rewrite a tree the config never pointed at.
//! - **truthfulness** — `hyalo config --dir X` reported `config_path: null`
//!   while a config was in effect.

use super::common::{hyalo, hyalo_no_hints, md, write_md};
use std::fs;
use tempfile::TempDir;

/// A project laid out the way `.hyalo.toml` is normally used: config at the
/// repo root, vault in a subdirectory, a schema and a `[lint] ignore` entry
/// that only apply if the config is honored.
fn build_project(tmp: &TempDir) {
    fs::write(
        tmp.path().join(".hyalo.toml"),
        md!(r#"
dir = "kb"

[schema.types.note]
required = ["title", "type", "status"]

[lint]
ignore = ["archive/**"]

[views.drafts]
properties = ["status=draft"]
"#),
    )
    .unwrap();

    let kb = tmp.path().join("kb");
    // Two notes missing the required `status`: lint findings that only appear
    // when the schema is loaded.
    write_md(&kb, "a.md", "---\ntitle: A\ntype: note\n---\n# A\n");
    write_md(&kb, "b.md", "---\ntitle: B\ntype: note\n---\n# B\n");
    // Ignored by `[lint] ignore`, so a config-less run reports *more*, not less.
    write_md(
        &kb,
        "archive/old.md",
        "---\ntitle: Old\ntype: note\n---\n# Old\n",
    );
}

/// Parse a JSON envelope from stdout, failing loudly with both streams.
fn envelope(stdout: &[u8], stderr: &[u8]) -> serde_json::Value {
    serde_json::from_slice(stdout).unwrap_or_else(|e| {
        panic!(
            "not a JSON envelope ({e})\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(stdout),
            String::from_utf8_lossy(stderr)
        )
    })
}

// ---------------------------------------------------------------------------
// H-4 — an explicit --dir naming the configured vault keeps the config
// ---------------------------------------------------------------------------

#[test]
fn redundant_dir_keeps_the_config_for_lint() {
    let tmp = TempDir::new().unwrap();
    build_project(&tmp);

    let without = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "json"])
        .output()
        .unwrap();
    let with = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--dir", "kb", "--format", "json"])
        .output()
        .unwrap();

    let a = envelope(&without.stdout, &without.stderr);
    let b = envelope(&with.stdout, &with.stderr);

    for key in [
        "errors",
        "warnings",
        "files_checked",
        "files_with_violations",
    ] {
        assert_eq!(
            a["results"][key], b["results"][key],
            "`lint --dir kb` must report the same `{key}` as a bare `lint`; \
             without: {a}\nwith: {b}"
        );
    }
    // Not vacuous: the schema really did produce findings.
    assert!(
        a["results"]["violations"].as_u64().unwrap_or(0) > 0,
        "fixture stopped producing lint findings: {a}"
    );
}

#[test]
fn redundant_dir_keeps_lint_ignore() {
    let tmp = TempDir::new().unwrap();
    build_project(&tmp);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--dir", "kb", "--detailed", "--format", "json"])
        .output()
        .unwrap();
    let json = envelope(&output.stdout, &output.stderr);
    let text = json.to_string();
    assert!(
        !text.contains("archive/old.md"),
        "`[lint] ignore` must still apply under --dir; got: {text}"
    );
}

#[test]
fn redundant_dir_keeps_schema_types_and_views() {
    let tmp = TempDir::new().unwrap();
    build_project(&tmp);

    let types = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["types", "list", "--dir", "kb", "--format", "json"])
        .output()
        .unwrap();
    let json = envelope(&types.stdout, &types.stderr);
    assert_eq!(
        json["total"].as_u64(),
        Some(1),
        "the `note` type must survive --dir: {json}"
    );

    let views = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["views", "list", "--dir", "kb", "--format", "json"])
        .output()
        .unwrap();
    let json = envelope(&views.stdout, &views.stderr);
    assert_eq!(
        json["total"].as_u64(),
        Some(1),
        "the `drafts` view must survive --dir: {json}"
    );
}

#[test]
fn redundant_dir_still_says_so_but_no_longer_claims_the_config_is_gone() {
    let tmp = TempDir::new().unwrap();
    build_project(&tmp);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["summary", "--dir", "kb", "--format", "json"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--dir is redundant"),
        "the redundancy note stays; stderr: {stderr}"
    );
    assert!(
        !stderr.contains("does not apply"),
        "a redundant --dir does not shadow anything; stderr: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// H-4 — a --dir naming a *different* vault says which config is in effect
// ---------------------------------------------------------------------------

#[test]
fn dir_to_another_tree_announces_that_the_cwd_config_no_longer_applies() {
    let tmp = TempDir::new().unwrap();
    build_project(&tmp);
    let other = tmp.path().join("other");
    write_md(&other, "c.md", "---\ntitle: C\n---\n# C\n");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["summary", "--dir", "other", "--format", "json"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("does not apply") && stderr.contains("built-in defaults"),
        "switching vaults must name the config in effect; stderr: {stderr}"
    );
}

#[test]
fn dir_to_a_tree_with_its_own_config_names_that_file() {
    let tmp = TempDir::new().unwrap();
    build_project(&tmp);
    let other = tmp.path().join("other");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join(".hyalo.toml"), "hints = false\n").unwrap();
    write_md(&other, "c.md", "---\ntitle: C\n---\n# C\n");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["summary", "--dir", "other", "--format", "json"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("does not apply") && stderr.contains(".hyalo.toml is in effect"),
        "the target's own config must be named; stderr: {stderr}"
    );
}

/// NEW-17 (dogfood pre3): `--dir .` naming the config's own root (not its
/// configured vault) used to print the identical `./.hyalo.toml` path on
/// both halves of one sentence — "does not apply" and "is in effect" about
/// the very same file. It must say the file is still governing the run
/// instead of contradicting itself.
#[test]
fn dir_dot_at_the_config_root_does_not_contradict_itself() {
    let tmp = TempDir::new().unwrap();
    build_project(&tmp);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["summary", "--dir", ".", "--format", "json"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !(stderr.contains("does not apply") && stderr.contains("is in effect")),
        "must not claim the same file both does not apply and is in effect: {stderr}"
    );
    assert!(
        stderr.contains("still in effect"),
        "expected the file to be named as still governing the run: {stderr}"
    );
}

/// NEW-17 (dogfood pre3): `--dir <foreign-subdir>` used to check only that
/// exact directory for a `.hyalo.toml`, so a subdirectory of an unrelated
/// tree with its *own* ancestor config reported "no .hyalo.toml — built-in
/// defaults" — even though `cd`-ing into that same subdirectory and running
/// the identical command would have silently adopted the ancestor config.
#[test]
fn dir_to_a_foreign_subdir_adopts_its_own_ancestor_config() {
    let tmp = TempDir::new().unwrap();
    // No config at `tmp` itself — the run's own CWD config is built-in
    // defaults, so nothing here is "shadowed"; this is purely about whether
    // `--dir` discovers the *target* tree's ancestor config.
    let other = tmp.path().join("other");
    fs::create_dir_all(other.join("deep/sub")).unwrap();
    fs::write(other.join(".hyalo.toml"), "site_prefix = \"adopted\"\n").unwrap();
    write_md(&other, "c.md", "---\ntitle: C\n---\n# C\n");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "config",
            "--dir",
            other.join("deep/sub").to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let json = envelope(&output.stdout, &output.stderr);
    assert_eq!(
        json["results"]["site_prefix"].as_str(),
        Some("adopted"),
        "the foreign subdir's own ancestor config must be adopted: {json}"
    );
    assert_eq!(
        json["results"]["malformed"].as_bool(),
        Some(false),
        "must not report built-in defaults when an ancestor config exists: {json}"
    );
    // PR #251 review H1: `announce_ancestor_config` used to fire
    // unconditionally from inside `load_config_for_dir`, describing the
    // ancestor's own *configured* vault ("other") as "the vault" even though
    // this run only ever scans the narrower `--dir` target
    // ("other/deep/sub") — actively wrong advice ("pass --dir ." made no
    // sense from this cwd either). The existing assertions above only ever
    // checked stdout, which is why this slipped through; assert on stderr too.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("from a parent directory"),
        "an ancestor-adoption note naming the wrong vault must not fire on \
         the --dir path (no cwd config to shadow, and the vault it would \
         name is not what this run scans): {stderr}"
    );
}

/// PR #251 review H1 (finding 11): when the CWD *also* has its own shadowed
/// config, the wrong `announce_ancestor_config` note used to fire alongside
/// `dir_override_note`'s own, correct one — two notes, the first misleading.
#[test]
fn dir_to_a_foreign_subdir_with_shadowed_cwd_config_prints_exactly_one_note() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \"kb\"\n").unwrap();
    fs::create_dir_all(tmp.path().join("kb")).unwrap();

    let other = tmp.path().join("other");
    fs::create_dir_all(other.join("deep/sub")).unwrap();
    fs::write(other.join(".hyalo.toml"), "site_prefix = \"adopted\"\n").unwrap();
    write_md(&other, "c.md", "---\ntitle: C\n---\n# C\n");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "config",
            "--dir",
            other.join("deep/sub").to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.matches("note:").count(),
        1,
        "exactly one note must fire, not the wrong ancestor-adoption note \
         plus the correct dir_override_note: {stderr}"
    );
    assert!(
        stderr.contains("does not apply") && stderr.contains("is in effect"),
        "the surviving note must be the correct one naming the adopted config file: {stderr}"
    );
    assert!(
        !stderr.contains("from a parent directory"),
        "the wrong ancestor-adoption note must not fire at all: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// H-4 — `hyalo config` tells the truth and emits runnable hints
// ---------------------------------------------------------------------------

#[test]
fn config_reports_the_config_path_under_a_redundant_dir() {
    let tmp = TempDir::new().unwrap();
    build_project(&tmp);

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["config", "--dir", "kb", "--format", "json"])
        .output()
        .unwrap();
    let json = envelope(&output.stdout, &output.stderr);
    let path = json["results"]["config_path"]
        .as_str()
        .unwrap_or_else(|| panic!("config_path must not be null while a config applies: {json}"));
    assert!(
        path.ends_with(".hyalo.toml"),
        "unexpected config_path: {path}"
    );
}

#[test]
fn config_hints_omit_dir_when_it_was_not_overridden() {
    let tmp = TempDir::new().unwrap();
    build_project(&tmp);

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["config", "--format", "json"])
        .output()
        .unwrap();
    let json = envelope(&output.stdout, &output.stderr);
    let hints = json["hints"].as_array().cloned().unwrap_or_default();
    assert!(!hints.is_empty(), "config must emit hints: {json}");
    for hint in &hints {
        let cmd = hint["cmd"].as_str().unwrap_or_default();
        assert!(
            !cmd.contains("--dir"),
            "a non-overridden config must not suggest --dir: {cmd}"
        );
        assert_eq!(
            hint["writes"].as_bool(),
            Some(false),
            "config drill-downs are read-only: {hint}"
        );
    }
}

#[test]
fn config_hints_run_and_return_non_degraded_results() {
    let tmp = TempDir::new().unwrap();
    build_project(&tmp);

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["config", "--format", "json"])
        .output()
        .unwrap();
    let json = envelope(&output.stdout, &output.stderr);
    for hint in json["hints"].as_array().cloned().unwrap_or_default() {
        let cmd = hint["cmd"].as_str().unwrap_or_default();
        let argv: Vec<&str> = cmd.split_whitespace().skip(1).collect();
        let run = hyalo()
            .current_dir(tmp.path())
            .args(&argv)
            .args(["--format", "json"])
            .output()
            .unwrap();
        let result = envelope(&run.stdout, &run.stderr);
        assert!(
            result.get("error").is_none(),
            "config hint `{cmd}` failed: {result}"
        );
        // "Non-degraded" concretely: `types list` must still see the schema.
        if cmd.contains("types list") {
            assert_eq!(
                result["total"].as_u64(),
                Some(1),
                "config hint `{cmd}` returned a config-less result: {result}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// M-2 — a malformed .hyalo.toml blocks writes and cannot be silenced
// ---------------------------------------------------------------------------

/// A project whose config has one unknown key — everything else is valid.
fn build_malformed_project(tmp: &TempDir) {
    fs::write(
        tmp.path().join(".hyalo.toml"),
        "dir = \"kb\"\nbogus_key = 1\n",
    )
    .unwrap();
    write_md(
        &tmp.path().join("kb"),
        "a.md",
        "---\ntitle: A\n---\n# A\n\n- [ ] open\n",
    );
}

#[test]
fn malformed_config_refuses_a_mutating_command_and_touches_nothing() {
    let tmp = TempDir::new().unwrap();
    build_malformed_project(&tmp);
    let note = tmp.path().join("kb").join("a.md");
    let before = fs::read(&note).unwrap();

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "set",
            "--property",
            "status=done",
            "--file",
            "a.md",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(1),
        "a writer on an unusable config must exit 1; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // User errors are emitted as a JSON object on stderr; the config warning
    // precedes it, so parse from the first `{`.
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let json: serde_json::Value = serde_json::from_str(
        &stderr[stderr
            .find('{')
            .unwrap_or_else(|| panic!("no JSON error on stderr: {stderr}"))..],
    )
    .unwrap_or_else(|e| panic!("stderr is not a JSON error ({e}): {stderr}"));
    assert!(
        json["error"]
            .as_str()
            .unwrap_or_default()
            .contains("unusable .hyalo.toml"),
        "unexpected error: {json}"
    );
    assert_eq!(
        fs::read(&note).unwrap(),
        before,
        "the refused command must not have written"
    );
}

#[test]
fn malformed_config_refuses_links_auto_apply_even_under_quiet() {
    let tmp = TempDir::new().unwrap();
    build_malformed_project(&tmp);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["links", "auto", "--apply", "-q", "--format", "json"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(1),
        "`links auto --apply -q` must not proceed on defaults; stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("malformed .hyalo.toml"),
        "the config diagnostic must survive -q; stderr: {stderr}"
    );
}

#[test]
fn malformed_config_still_warns_on_a_read_under_quiet() {
    let tmp = TempDir::new().unwrap();
    build_malformed_project(&tmp);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["summary", "-q", "--format", "json"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "reads still work on a malformed config"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("malformed .hyalo.toml"),
        "config-integrity warnings are not chatter; stderr: {stderr}"
    );
}

#[test]
fn malformed_config_allows_a_dry_run() {
    let tmp = TempDir::new().unwrap();
    build_malformed_project(&tmp);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "set",
            "--property",
            "status=done",
            "--file",
            "a.md",
            "--dry-run",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "--dry-run writes nothing, so it is not gated; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn malformed_config_keeps_the_configured_dir_for_reads() {
    // `dir` is salvaged from an otherwise-unusable file, so a read does not
    // silently re-root at the config directory and scan the whole repo.
    let tmp = TempDir::new().unwrap();
    build_malformed_project(&tmp);
    write_md(
        tmp.path(),
        "outside.md",
        "---\ntitle: Outside\n---\n# Outside\n",
    );

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["find", "--format", "json"])
        .output()
        .unwrap();
    let json = envelope(&output.stdout, &output.stderr);
    let text = json.to_string();
    assert!(
        !text.contains("outside.md"),
        "a salvaged `dir` must keep reads inside the vault: {text}"
    );
}

#[test]
fn malformed_config_reports_once_not_twice() {
    // The loader used to parse `.hyalo.toml` twice per invocation, so every run
    // ended with "1 additional identical warning(s) suppressed" (dogfood L-14).
    let tmp = TempDir::new().unwrap();
    build_malformed_project(&tmp);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["summary", "--format", "json"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.matches("malformed .hyalo.toml").count(),
        1,
        "the diagnostic must be emitted exactly once; stderr: {stderr}"
    );
    assert!(
        !stderr.contains("additional identical warning"),
        "no suppression notice means no double parse; stderr: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// iter-210 (UX-5): a malformed key names the command that fixes it
// ---------------------------------------------------------------------------

/// `[types.note]` is the recurring mis-spelling of `[schema.types.note]`. The
/// raw serde error lists `schema` among the accepted top-level fields, which
/// tells the reader they were wrong without telling them what to run.
#[test]
fn unknown_types_key_suggests_hyalo_types_set() {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join(".hyalo.toml"),
        "[types.note]\nrequired = [\"title\"]\n",
    )
    .unwrap();
    write_md(tmp.path(), "a.md", "---\ntitle: A\n---\n\nBody.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap(), "lint"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        stderr.contains("malformed .hyalo.toml"),
        "the malformed-config warning must survive: {stderr}"
    );
    assert!(
        stderr.contains("[schema.types.<name>]"),
        "the fix must name the real table: {stderr}"
    );
    assert!(
        stderr.contains("hyalo types set"),
        "the fix must name the command that creates it: {stderr}"
    );
}

/// The same treatment for a mis-placed lint rule override.
#[test]
fn unknown_rules_key_suggests_hyalo_lint_rules_set() {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join(".hyalo.toml"),
        "[rules.MD013]\nenabled = false\n",
    )
    .unwrap();
    write_md(tmp.path(), "a.md", "---\ntitle: A\n---\n\nBody.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap(), "lint"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        stderr.contains("hyalo lint-rules set"),
        "expected the lint-rules fix path: {stderr}"
    );
}

/// A malformed file with no known fix path must not grow a bogus one.
#[test]
fn unrecognised_malformed_config_gets_no_invented_fix() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".hyalo.toml"), "this is not = = toml\n").unwrap();
    write_md(tmp.path(), "a.md", "---\ntitle: A\n---\n\nBody.\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap(), "lint"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(stderr.contains("malformed .hyalo.toml"), "{stderr}");
    assert!(
        !stderr.contains("  fix: "),
        "no fix path should be invented: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// iter-213 UX-1 / DEC-079 — ancestor config discovery
// ---------------------------------------------------------------------------

/// The whole point of adoption: the settings that only exist in the parent
/// config must still shape a run started from inside the vault. `build_project`
/// pins `[lint] ignore` and a schema, so a run that lost the config lints
/// differently.
#[test]
fn ancestor_config_governs_a_run_started_inside_the_vault() {
    let tmp = TempDir::new().unwrap();
    build_project(&tmp);

    let from_root = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--format", "json"])
        .output()
        .unwrap();
    let from_vault = hyalo_no_hints()
        .current_dir(tmp.path().join("kb"))
        .args(["lint", "--format", "json"])
        .output()
        .unwrap();

    assert_eq!(
        String::from_utf8_lossy(&from_root.stdout),
        String::from_utf8_lossy(&from_vault.stdout),
        "lint from inside the vault must see the same config as from the root"
    );
}

/// `hyalo config` is the command people use to answer "which file applies?",
/// so it must name the adopted ancestor rather than reporting `(none)`.
#[test]
fn config_names_the_adopted_ancestor_file() {
    let tmp = TempDir::new().unwrap();
    build_project(&tmp);

    let output = hyalo_no_hints()
        .current_dir(tmp.path().join("kb"))
        .args(["config", "--format", "json"])
        .output()
        .unwrap();
    let json = envelope(&output.stdout, &output.stderr);
    let config_path = json["results"]["config_path"].as_str().unwrap_or_default();
    assert!(
        config_path.ends_with(".hyalo.toml"),
        "expected the ancestor config path, got: {json}"
    );
}

/// An ancestor whose configured vault does not contain CWD does not govern the
/// run — adopting it would silently widen the scope to an unrelated tree.
#[test]
fn ancestor_config_is_not_adopted_from_a_sibling_directory() {
    let tmp = TempDir::new().unwrap();
    build_project(&tmp);
    let sibling = tmp.path().join("elsewhere");
    fs::create_dir_all(&sibling).unwrap();

    let output = hyalo_no_hints()
        .current_dir(&sibling)
        .args(["config", "--format", "json"])
        .output()
        .unwrap();
    let json = envelope(&output.stdout, &output.stderr);
    assert!(
        json["results"]["config_path"].is_null(),
        "a config whose vault excludes CWD must not be adopted: {json}"
    );
}

// ---------------------------------------------------------------------------
// iter-213 UX-2 — a malformed config is detectable from `hyalo config` alone
// ---------------------------------------------------------------------------

#[test]
fn config_reports_a_malformed_file_in_its_own_output() {
    let tmp = TempDir::new().unwrap();
    build_malformed_project(&tmp);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["config", "--format", "json"])
        .output()
        .unwrap();
    let json = envelope(&output.stdout, &output.stderr);
    assert_eq!(
        json["results"]["malformed"].as_bool(),
        Some(true),
        "a JSON consumer must see the malformed state: {json}"
    );
    assert!(
        json["results"]["parse_error"]
            .as_str()
            .unwrap_or_default()
            .contains("malformed .hyalo.toml"),
        "expected the parse diagnostic in the payload: {json}"
    );

    let text = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["config", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&text.stdout);
    assert!(
        stdout.contains("malformed: true"),
        "the text rendering must lead with the integrity problem: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// iter-213 UX-5 — the stale diagnostic no longer precedes the note that
// contradicts it
// ---------------------------------------------------------------------------

/// `--dir` pointing at a different, healthy vault means the malformed CWD
/// config does not apply — so it must not be warned about at all.
#[test]
fn dir_to_a_healthy_vault_drops_the_malformed_cwd_warning() {
    let tmp = TempDir::new().unwrap();
    build_malformed_project(&tmp);
    let other = TempDir::new().unwrap();
    write_md(other.path(), "b.md", "---\ntitle: B\n---\n\nBody.\n");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["--dir", other.path().to_str().unwrap(), "lint"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("malformed .hyalo.toml"),
        "a config --dir switched away from must not be warned about: {stderr}"
    );
    assert!(
        stderr.contains("does not apply"),
        "the switch itself is still announced: {stderr}"
    );
}
