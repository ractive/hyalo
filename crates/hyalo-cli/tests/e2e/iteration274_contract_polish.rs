//! Iteration 274 — the hints, help and contract findings left over from the
//! post-batch dogfood of v0.22.0.
//!
//! One test per user-visible contract: the exit-code taxonomy (DEC-307) and
//! the four paths that used to print bare text and exit 2 under `--format
//! json` (BUG-25), `find a b` (UX-1), `deinit --dir <nonexistent>` (UX-18),
//! `okf index --dry-run` exiting 0 (UX-20), the `--property` operand
//! rejections (UX-21), `lint --rule SCHEMA` (UX-5), the empty `--files-from`
//! warning (UX-9), `--sort title` collation and H1-comment stripping (UX-15,
//! UX-16), the CRLF task text (UX-17), and the zero-result hint that
//! distinguishes a key the vault does not have from one whose values simply do
//! not match (BUG-17).

use super::common::{hyalo_no_hints, write_md};
use tempfile::TempDir;

/// Run hyalo in `dir`, returning `(exit code, stdout, stderr)`.
fn run(dir: &std::path::Path, args: &[&str]) -> (i32, String, String) {
    let output = hyalo_no_hints()
        .current_dir(dir)
        .args(args)
        .output()
        .expect("hyalo should run");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// The JSON error envelope a refused run printed, parsed. Panics when the run
/// printed something that is not one — which is the failure these tests exist
/// to catch.
fn error_envelope(stdout: &str, stderr: &str) -> serde_json::Value {
    for stream in [stdout, stderr] {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(stream.trim())
            && v.get("error").is_some()
        {
            return v;
        }
    }
    panic!("no JSON error envelope in stdout {stdout:?} / stderr {stderr:?}");
}

fn vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "alpha.md",
        "---\ntitle: Alpha\nstatus: draft\n---\n\n# Alpha\n\nProse.\n",
    );
    tmp
}

// ---------------------------------------------------------------------------
// DEC-307 / BUG-25 / UX-1 — every hyalo-own user error is an envelope + exit 1
// ---------------------------------------------------------------------------

/// A `--glob` that will not compile is the caller's mistake, not hyalo's.
#[test]
fn bad_glob_is_an_envelope_at_exit_one() {
    let tmp = vault();
    let (code, stdout, stderr) = run(
        tmp.path(),
        &["find", "--dir", ".", "--glob", "[", "--format", "json"],
    );
    assert_eq!(code, 1, "a bad glob is a user error: {stderr}");
    let v = error_envelope(&stdout, &stderr);
    assert!(
        v["error"]
            .as_str()
            .is_some_and(|e| e.contains("invalid glob")),
        "{v}"
    );
    assert!(
        v["hint"].is_string(),
        "the refusal must say what to do: {v}"
    );
}

/// An unreadable `--files-from` list used to print bare text and exit 2.
#[test]
fn unreadable_files_from_is_an_envelope_at_exit_one() {
    let tmp = vault();
    let missing = tmp.path().join("no-such-list.txt");
    let (code, stdout, stderr) = run(
        tmp.path(),
        &[
            "find",
            "--dir",
            ".",
            "--files-from",
            missing.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert_eq!(code, 1, "stderr: {stderr}");
    let v = error_envelope(&stdout, &stderr);
    assert!(
        v["error"]
            .as_str()
            .is_some_and(|e| e.contains("--files-from")),
        "{v}"
    );
}

/// `create-index --output` into a directory that does not exist.
#[test]
fn create_index_into_a_missing_directory_is_an_envelope_at_exit_one() {
    let tmp = vault();
    let (code, stdout, stderr) = run(
        tmp.path(),
        &[
            "create-index",
            "--dir",
            ".",
            "--output",
            "nope/index.bin",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, 1, "stderr: {stderr}");
    let v = error_envelope(&stdout, &stderr);
    assert!(
        v["error"]
            .as_str()
            .is_some_and(|e| e.contains("does not exist")),
        "{v}"
    );
}

/// An unknown `init --profile` used to print bare text and exit 2.
#[test]
fn unknown_init_profile_is_an_envelope_at_exit_one() {
    let tmp = TempDir::new().unwrap();
    let (code, stdout, stderr) = run(
        tmp.path(),
        &["init", "--profile", "nope", "--format", "json"],
    );
    assert_eq!(code, 1, "stderr: {stderr}");
    let v = error_envelope(&stdout, &stderr);
    assert!(
        v["error"]
            .as_str()
            .is_some_and(|e| e.contains("unknown profile")),
        "{v}"
    );
}

/// UX-1: hyalo's own did-you-mean-quotes error is a user error (exit 1), the
/// same as `--sort nope` — 2 is reserved for clap usage and internal errors.
#[test]
fn unquoted_multiword_query_exits_one_like_every_other_user_error() {
    let tmp = vault();
    let (quotes_code, _, quotes_err) = run(tmp.path(), &["find", "--dir", ".", "prose", "about"]);
    let (sort_code, _, _) = run(tmp.path(), &["find", "--dir", ".", "--sort", "nope"]);
    assert_eq!(quotes_code, 1, "stderr: {quotes_err}");
    assert_eq!(
        quotes_code, sort_code,
        "both are hyalo-own user errors and must share an exit code"
    );
}

/// UX-18: a `deinit --dir` naming a directory that is not there is a mistyped
/// path, not an already-clean tree.
#[test]
fn deinit_on_a_missing_directory_exits_one() {
    let tmp = TempDir::new().unwrap();
    let missing = tmp.path().join("not-here");
    let (code, _, stderr) = run(tmp.path(), &["deinit", "--dir", missing.to_str().unwrap()]);
    assert_eq!(code, 1, "stderr: {stderr}");
}

// ---------------------------------------------------------------------------
// UX-21 — a filter operand the caller left empty is a typo, not a query
// ---------------------------------------------------------------------------

#[test]
fn empty_and_ambiguous_property_operands_are_rejected() {
    let tmp = vault();
    for filter in ["status=", "status>=", "status>", "a=b=c", "=draft"] {
        let (code, stdout, stderr) = run(
            tmp.path(),
            &[
                "find",
                "--dir",
                ".",
                "--property",
                filter,
                "--format",
                "json",
            ],
        );
        assert_eq!(code, 1, "`{filter}` must be rejected: {stdout}{stderr}");
    }
    // The spellings that DO express those intents keep working.
    for filter in ["status", "status=draft", "!missing"] {
        let (code, _, stderr) = run(
            tmp.path(),
            &[
                "find",
                "--dir",
                ".",
                "--property",
                filter,
                "--format",
                "json",
            ],
        );
        assert_eq!(code, 0, "`{filter}` must still work: {stderr}");
    }
}

// ---------------------------------------------------------------------------
// UX-5 — the schema pass is selectable, listed and inspectable
// ---------------------------------------------------------------------------

#[test]
fn schema_is_a_selectable_and_listed_rule() {
    let tmp = vault();
    let (code, _, stderr) = run(
        tmp.path(),
        &["lint", "--dir", ".", "--rule", "SCHEMA", "--format", "json"],
    );
    assert_eq!(code, 0, "--rule SCHEMA must run the schema pass: {stderr}");

    let (code, _, stderr) = run(
        tmp.path(),
        &[
            "lint",
            "--dir",
            ".",
            "--rule-prefix",
            "SCHEMA",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, 0, "--rule-prefix SCHEMA must select it too: {stderr}");

    let (code, stdout, _) = run(
        tmp.path(),
        &[
            "lint-rules",
            "list",
            "--rule-prefix",
            "SCHEMA",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, 0);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("json");
    let rows = v["results"].as_array().expect("results array");
    assert_eq!(rows.len(), 1, "exactly the SCHEMA row: {stdout}");
    assert_eq!(rows[0]["id"], "SCHEMA");
    assert_eq!(
        rows[0]["configurable"], false,
        "the row must say it is not configurable: {stdout}"
    );

    let (code, stdout, _) = run(
        tmp.path(),
        &["lint-rules", "show", "SCHEMA", "--format", "json"],
    );
    assert_eq!(code, 0, "a listed rule must be inspectable: {stdout}");
}

/// UX-14: an override that pins only the severity reports only the severity —
/// `enabled: null` read as "unknown" rather than "not overridden".
#[test]
fn lint_rules_show_omits_the_dimensions_an_override_does_not_set() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(
        tmp.path().join(".hyalo.toml"),
        "dir = \".\"\n[lint.rules.MD013]\nseverity = \"error\"\n",
    )
    .unwrap();
    let (code, stdout, _) = run(
        tmp.path(),
        &["lint-rules", "show", "MD013", "--format", "json"],
    );
    assert_eq!(code, 0);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("json");
    let over = &v["results"]["override"];
    assert_eq!(over["severity"], "error", "{stdout}");
    assert!(
        over.get("enabled").is_none(),
        "an unset dimension is absent, never null: {stdout}"
    );
    assert!(
        v["results"]["effective_enabled"].is_boolean(),
        "the effective value is where the answer lives: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// UX-9 — an empty `--files-from` list examines nothing, and says so
// ---------------------------------------------------------------------------

#[test]
fn an_empty_files_from_list_warns_even_under_quiet() {
    let tmp = vault();
    let list = tmp.path().join("empty.txt");
    std::fs::write(&list, "").unwrap();
    for extra in [vec![], vec!["-q"]] {
        let mut args = vec![
            "lint",
            "--dir",
            ".",
            "--files-from",
            list.to_str().unwrap(),
            "--format",
            "json",
        ];
        args.extend(extra.iter().copied());
        let (code, _, stderr) = run(tmp.path(), &args);
        assert_eq!(code, 0, "an empty list is not itself an error");
        assert!(
            stderr.contains("listed no paths"),
            "the warning must survive -q: {stderr}"
        );
    }
}

// ---------------------------------------------------------------------------
// UX-15 / UX-16 — title collation and the H1 fallback
// ---------------------------------------------------------------------------

#[test]
fn sort_title_collates_and_the_h1_fallback_strips_comments() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "z.md", "# Zebra\n");
    write_md(tmp.path(), "a.md", "# apple\n");
    write_md(
        tmp.path(),
        "r.md",
        "# Release notes <!-- markdownlint-disable-line MD013 -->\n",
    );
    let (code, stdout, stderr) = run(
        tmp.path(),
        &[
            "find", "--dir", ".", "--fields", "title", "--sort", "title", "--format", "json",
        ],
    );
    assert_eq!(code, 0, "stderr: {stderr}");
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("json");
    let titles: Vec<&str> = v["results"]
        .as_array()
        .expect("results")
        .iter()
        .map(|r| r["title"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        titles,
        vec!["apple", "Release notes", "Zebra"],
        "case-folded order, and the HTML comment stripped: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// UX-17 — a CRLF file's task text has no trailing carriage return
// ---------------------------------------------------------------------------

#[test]
fn task_text_strips_the_crlf_carriage_return() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(
        tmp.path().join("t.md"),
        "---\r\ntitle: T\r\n---\r\n\r\n# T\r\n\r\n- [ ] one\r\n",
    )
    .unwrap();
    let (code, stdout, stderr) = run(
        tmp.path(),
        &[
            "task", "read", "t.md", "--dir", ".", "--all", "--format", "json",
        ],
    );
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.contains("\"one\""),
        "the task text must not carry the \\r: {stdout}"
    );
    assert!(!stdout.contains("one\\r"), "{stdout}");
}

// ---------------------------------------------------------------------------
// BUG-17 — key-absent and value-absent are different diagnoses
// ---------------------------------------------------------------------------

#[test]
fn a_zero_result_names_the_values_a_present_key_actually_has() {
    let tmp = vault();
    let (code, _, stderr) = run(
        tmp.path(),
        &[
            "find",
            "--dir",
            ".",
            "--property",
            "status=nonexistent",
            "--hints",
            "--format",
            "text",
        ],
    );
    assert_eq!(code, 0, "a zero result is not an error");
    let _ = stderr;

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "find",
            "--dir",
            ".",
            "--property",
            "status=nonexistent",
            "--format",
            "json",
            "--hints",
        ])
        .output()
        .expect("hyalo should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("json");
    let hints = v["hints"].as_array().cloned().unwrap_or_default();
    let descriptions: Vec<&str> = hints
        .iter()
        .filter_map(|h| h["description"].as_str())
        .collect();
    assert!(
        descriptions
            .iter()
            .any(|d| d.contains("`status` is set in") && d.contains("draft (1)")),
        "the key exists — name its values with counts: {descriptions:?}"
    );
    assert!(
        !descriptions
            .iter()
            .any(|d| d.contains("No file has a `status`")),
        "and never claim the key is absent: {descriptions:?}"
    );
}

/// A key nothing in the vault declares still reads as key-absent.
#[test]
fn a_zero_result_on_an_unknown_key_still_says_no_file_has_it() {
    let tmp = vault();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "find",
            "--dir",
            ".",
            "--property",
            "nosuchkey=1",
            "--format",
            "json",
            "--hints",
        ])
        .output()
        .expect("hyalo should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("json");
    let descriptions: Vec<String> = v["hints"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|h| h["description"].as_str().map(str::to_owned))
        .collect();
    assert!(
        descriptions
            .iter()
            .any(|d| d.contains("No file has a `nosuchkey` property")),
        "{descriptions:?}"
    );
}

// ---------------------------------------------------------------------------
// UX-2 — an indexed run's hints stay indexed
// ---------------------------------------------------------------------------

#[test]
fn hints_on_an_indexed_run_carry_the_index_flag() {
    let tmp = vault();
    let (code, _, stderr) = run(tmp.path(), &["create-index", "--dir", "."]);
    assert_eq!(code, 0, "stderr: {stderr}");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "summary", "--dir", ".", "--index", "--format", "json", "--hints",
        ])
        .output()
        .expect("hyalo should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("json");
    let hints = v["hints"].as_array().cloned().unwrap_or_default();
    assert!(!hints.is_empty(), "summary must emit hints: {stdout}");
    for hint in &hints {
        let cmd = hint["cmd"].as_str().unwrap_or_default();
        // Only commands that accept the flag carry it; `create-index` writes a
        // snapshot rather than reading one and is left alone.
        if cmd.contains("hyalo create-index") || cmd.contains("hyalo drop-index") {
            assert!(!cmd.contains("--index "), "a writer must not read: {cmd}");
            continue;
        }
        assert!(
            cmd.contains("--index"),
            "an indexed run's hints must stay indexed: {cmd}"
        );
    }
}
