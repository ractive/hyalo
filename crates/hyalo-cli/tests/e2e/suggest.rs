use super::common::{hyalo_no_hints, write_md};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// e2e tests for subcommand-flag suggestion
// ---------------------------------------------------------------------------
//
// These tests verify that when the user passes a subcommand name as a
// `--flag` (e.g. `--toggle` instead of `toggle`), the CLI prints a
// "did you mean" tip to stderr and exits with code 2.

fn setup_file(tmp: &tempfile::TempDir) {
    write_md(
        tmp.path(),
        "tasks.md",
        "---\ntitle: Test\n---\n- [ ] First task\n",
    );
}

// ---------------------------------------------------------------------------
// task subcommand misplacement
// ---------------------------------------------------------------------------

#[test]
fn suggest_task_toggle_as_flag() {
    let tmp = tempfile::tempdir().unwrap();
    setup_file(&tmp);

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["task", "--toggle", "--file", "tasks.md", "--line", "4"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("did you mean"),
        "expected 'did you mean' in stderr; got: {stderr}"
    );
    assert!(
        stderr.contains("task toggle"),
        "expected corrected command 'task toggle' in stderr; got: {stderr}"
    );
}

#[test]
fn suggest_properties_rename_as_flag() {
    let tmp = tempfile::tempdir().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["properties", "--rename", "--from", "old", "--to", "new"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("did you mean"),
        "expected 'did you mean' in stderr; got: {stderr}"
    );
    assert!(
        stderr.contains("properties rename"),
        "expected 'properties rename' in stderr; got: {stderr}"
    );
}

#[test]
fn suggest_tags_summary_as_flag() {
    let tmp = tempfile::tempdir().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["tags", "--summary"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("did you mean"),
        "expected 'did you mean' in stderr; got: {stderr}"
    );
    assert!(
        stderr.contains("tags summary"),
        "expected 'tags summary' in stderr; got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// No suggestion for valid commands
// ---------------------------------------------------------------------------

#[test]
fn no_suggestion_for_valid_task_toggle() {
    let tmp = tempfile::tempdir().unwrap();
    setup_file(&tmp);

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["task", "toggle", "--file", "tasks.md", "--line", "4"])
        .output()
        .unwrap();

    // Should succeed (or fail with exit 1 for a content error, not 2)
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("did you mean"),
        "unexpected suggestion for a valid command; stderr: {stderr}"
    );
    assert_ne!(
        output.status.code(),
        Some(2),
        "exit code 2 indicates a clap error was hit; stderr: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// --filter typo → suggest --property (not --file)
// ---------------------------------------------------------------------------

#[test]
fn suggest_property_when_filter_used() {
    let tmp = tempfile::tempdir().unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--filter", "status=draft"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--property"),
        "expected '--property' suggestion in stderr; got: {stderr}"
    );
    assert!(
        !stderr.contains("--file"),
        "unexpected '--file' suggestion in stderr; got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Bug 7 — bare-word typos that resemble top-level flags should suggest them
// ---------------------------------------------------------------------------

#[test]
fn suggest_version_for_typo() {
    let output = hyalo_no_hints().arg("versio").output().unwrap();

    assert!(
        !output.status.success(),
        "expected failure for unknown subcommand 'versio'"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--version"),
        "expected '--version' suggestion in stderr; got: {stderr}"
    );
}

#[test]
fn suggest_help_for_typo() {
    let output = hyalo_no_hints().arg("hep").output().unwrap();

    assert!(
        !output.status.success(),
        "expected failure for unknown subcommand 'hep'"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--help"),
        "expected '--help' suggestion in stderr; got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// NEW-UX-1: `append --tag` → friendly hint, not clap's unknown-arg error
// ---------------------------------------------------------------------------

#[test]
fn append_tag_shows_friendly_hint() {
    let tmp = tempfile::tempdir().unwrap();
    setup_file(&tmp);

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["append", "tasks.md", "--tag", "foo"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("`hyalo append` does not accept --tag"),
        "expected friendly hint in stderr; got: {stderr}"
    );
    assert!(
        stderr.contains("hyalo set"),
        "expected `hyalo set` recommendation in stderr; got: {stderr}"
    );
}

#[test]
fn append_tag_hint_only_fires_for_real_append_subcommand() {
    // CodeRabbit review finding: the hint previously matched any argv element
    // equal to "append", so commands like `hyalo find append --tag foo` also
    // got the `hyalo append`-specific message. Gate on the resolved top-level
    // subcommand instead.
    let tmp = tempfile::tempdir().unwrap();
    setup_file(&tmp);

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "append", "--tag", "foo"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("`hyalo append` does not accept --tag"),
        "append-specific hint must not fire when subcommand is `find`; got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Cross-group verb aliases (iter-192)
// ---------------------------------------------------------------------------

/// `summary` and `list` are the same read verb wearing two names across the
/// five subcommand groups. Before iter-192 each group accepted only one of
/// them, so a verb learned in one group failed in the next.
#[test]
fn read_verb_aliases_work_in_every_subcommand_group() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        "---\ntitle: A\ntags:\n  - x\n---\n# A\n",
    );
    let dir = tmp.path().to_str().unwrap().to_owned();

    for argv in [
        // `list` is the native verb here; `summary` is the alias.
        vec!["types", "summary"],
        vec!["views", "summary"],
        vec!["lint-rules", "summary"],
        // `summary` is the native verb here; `list` is the alias.
        vec!["tags", "list"],
        vec!["properties", "list"],
    ] {
        let label = argv.join(" ");
        let output = hyalo_no_hints()
            .args(["--dir", &dir])
            .args(&argv)
            .args(["--format", "json"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "`hyalo {label}` should be accepted; stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let envelope: serde_json::Value = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|e| panic!("`hyalo {label}` did not emit JSON: {e}"));
        assert!(
            envelope.get("total").is_some(),
            "`hyalo {label}` should behave exactly like its canonical spelling: {envelope}"
        );
    }
}

/// Aliases must produce byte-identical output to the verb they alias — they are
/// alternative spellings, not variant behaviour.
#[test]
fn alias_output_matches_canonical_verb() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        "---\ntitle: A\ntags:\n  - x\n---\n# A\n",
    );
    let dir = tmp.path().to_str().unwrap().to_owned();

    for (canonical, alias) in [
        (vec!["tags", "summary"], vec!["tags", "list"]),
        (vec!["properties", "summary"], vec!["properties", "list"]),
        (vec!["types", "list"], vec!["types", "summary"]),
        (vec!["views", "list"], vec!["views", "summary"]),
    ] {
        let run = |argv: &Vec<&str>| {
            let out = hyalo_no_hints()
                .args(["--dir", &dir])
                .args(argv)
                .args(["--format", "json"])
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout).into_owned()
        };
        assert_eq!(
            run(&canonical),
            run(&alias),
            "`{}` and `{}` must produce identical output",
            canonical.join(" "),
            alias.join(" ")
        );
    }
}

// ---------------------------------------------------------------------------
// unknown --<property> flag → suggest --property K=V
// ---------------------------------------------------------------------------
//
// Models and users reach for natural-language flags (`hyalo find --status
// planned`) even though the help teaches `--property status=planned`. When
// the unknown long flag names a property declared in the effective schema,
// the CLI says so; when it doesn't, clap's normal error stays untouched.

/// Helper: run `hyalo <args>` with CWD set to a temp vault holding `config`.
/// CWD-based (not `--dir`) because config discovery is CWD-anchored — this
/// mirrors the real scenario (`cd vault && hyalo find --status …`).
fn run_with_config(config: &str, args: &[&str]) -> (Option<i32>, String) {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), config).unwrap();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(args)
        .output()
        .unwrap();
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

const SCHEMA_WITH_STATUS: &str = "\
[schema.default]
required = [\"title\", \"type\"]

[schema.types.note.properties.status]
type = \"string\"
";

#[test]
fn unknown_flag_naming_a_schema_property_suggests_property_flag() {
    let (code, stderr) = run_with_config(SCHEMA_WITH_STATUS, &["find", "--status", "planned"]);
    assert_eq!(code, Some(2));
    assert!(
        stderr.contains("'status' is a frontmatter property"),
        "expected the property hint; got: {stderr}"
    );
    assert!(
        stderr.contains("--property status="),
        "expected the corrected flag form; got: {stderr}"
    );
    // The misleading clap tip must be gone.
    assert!(
        !stderr.contains("as a value"),
        "clap's '--status as a value' tip must be replaced; got: {stderr}"
    );
}

#[test]
fn unknown_flag_with_equals_form_also_gets_the_hint() {
    let (code, stderr) = run_with_config(SCHEMA_WITH_STATUS, &["find", "--status=planned"]);
    assert_eq!(code, Some(2));
    assert!(
        stderr.contains("--property status="),
        "the --flag=value form should hit the same hint; got: {stderr}"
    );
}

#[test]
fn unknown_flag_for_undeclared_property_keeps_clap_error() {
    // No [schema.types.note.properties.banana] — no hint, clap's normal error.
    let (code, stderr) = run_with_config(SCHEMA_WITH_STATUS, &["find", "--banana"]);
    assert_eq!(code, Some(2));
    assert!(
        !stderr.contains("frontmatter property"),
        "no property hint for an undeclared name; got: {stderr}"
    );
}

#[test]
fn unknown_flag_without_any_schema_types_keeps_clap_error() {
    // Config with no [schema] section at all: `--status` is just unknown.
    let (code, stderr) = run_with_config("dir = \".\"\n", &["find", "--status", "planned"]);
    assert_eq!(code, Some(2));
    assert!(
        !stderr.contains("frontmatter property"),
        "no schema → no property hint; got: {stderr}"
    );
}

#[test]
fn required_and_default_properties_also_trigger_the_hint() {
    // `milestone` is only in [schema.default].required — still a known
    // property. (Not `title`/`type`: `--title` is a real find flag, so it
    // never reaches the unknown-arg path.)
    let config = "\
[schema.default]
required = [\"title\", \"milestone\"]

[schema.types.note.properties.status]
type = \"string\"
";
    let (code, stderr) = run_with_config(config, &["find", "--milestone"]);
    assert_eq!(code, Some(2));
    assert!(
        stderr.contains("'milestone' is a frontmatter property"),
        "required properties should trigger the hint too; got: {stderr}"
    );
}
