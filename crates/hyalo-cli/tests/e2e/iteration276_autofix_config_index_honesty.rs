//! Iteration 276 — autofix, config and index honesty.
//!
//! Every fixture here comes verbatim from the dogfood v0.22.0 report
//! (post-batch 271–274), one test per numbered item:
//!
//! - **LINT.** `disable-next-line` protects the *next* line (BUG-4);
//!   list-indented fences are code (BUG-5); an unknown directive id warns
//!   (BUG-43); `--max-per-rule 0` is unlimited (BUG-19).
//! - **SCHEMA.** A typo'd key is a config error (BUG-20); `required` is not a
//!   type (BUG-42, DEC-312).
//! - **INDEX.** A named `--index-file` is a promise (BUG-11); the snapshot
//!   carries a format version (BUG-12, G4).
//! - **PATHS.** A CWD-shadowed bare path warns (BUG-21); `--dir` redundancy
//!   is worded honestly (BUG-28).
//! - **WRITE.** Fence bytes round-trip (BUG-33/34); the unparsable-file error
//!   is one envelope (BUG-35); flow lists stay flow (BUG-38); ordered and
//!   wide-gap tasks are tasks (BUG-40); `skipped_detail` names the reason
//!   (UX-1).
//! - **HELP.** No `lint-rules set` hint for a non-configurable rule (BUG-27).

use assert_cmd::Command;
use tempfile::TempDir;

use crate::common::hyalo_no_hints;

fn hyalo(tmp: &TempDir) -> Command {
    let mut cmd = hyalo_no_hints();
    cmd.current_dir(tmp.path());
    cmd
}

fn vault(config: &str) -> TempDir {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), config).unwrap();
    tmp
}

fn write(tmp: &TempDir, rel: &str, body: &str) {
    let path = tmp.path().join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, body).unwrap();
}

fn read(tmp: &TempDir, rel: &str) -> String {
    std::fs::read_to_string(tmp.path().join(rel)).unwrap()
}

fn run(tmp: &TempDir, args: &[&str]) -> (i32, serde_json::Value, String) {
    let output = hyalo(tmp)
        .args(args)
        .args(["--format", "json"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let json = serde_json::from_slice(&output.stdout)
        .or_else(|_| serde_json::from_str(&stderr))
        .unwrap_or(serde_json::Value::Null);
    (output.status.code().unwrap_or(-1), json, stderr)
}

// ---------------------------------------------------------------------------
// LINT-1 (BUG-4) — `nl.md`, the report's fixture, verbatim
// ---------------------------------------------------------------------------

/// The comment's own line must still be linted; the line *after* it must not.
/// Lines 8, 13 and 16 are protected (id form, alias form, trailing form) and
/// must survive `--fix` byte for byte; line 15 carries the directive *and* a
/// violation, so it is rewritten. Line 10 is the unprotected control.
#[test]
fn disable_next_line_protects_the_following_line_only() {
    let tmp = vault("dir = \".\"\n");
    let before = "---\ntitle: nl\n---\n\n# Heading\n\n\
                  <!-- markdownlint-disable-next-line MD019 -->\n\
                  #   L8 id next-line (silent)\n\n\
                  #   L10 unprotected\n\n\
                  <!-- markdownlint-disable-next-line no-multiple-space-atx -->\n\
                  #   L13 alias next-line (silent)\n\n\
                  #   L15 trailing comment <!-- markdownlint-disable-next-line MD019 -->\n\
                  #   L16 line after trailing comment (silent)\n";
    write(&tmp, "nl.md", before);

    // Read-only: only the two unprotected headings fire.
    let (_, json, _) = run(&tmp, &["lint", "--rule", "MD019", "--detailed"]);
    let lines: Vec<u64> = json["results"]["files"][0]["rule_groups"][0]["violations"]
        .as_array()
        .expect("MD019 violations")
        .iter()
        .filter_map(|v| v["line"].as_u64())
        .collect();
    assert_eq!(lines, vec![10, 15], "{json}");

    let (code, _, _) = run(&tmp, &["lint", "--fix"]);
    assert_eq!(code, 0);
    let fixed = read(&tmp, "nl.md");
    let after: Vec<&str> = fixed.lines().collect();
    let orig: Vec<&str> = before.lines().collect();
    for protected in [8usize, 13, 16] {
        assert_eq!(
            after[protected - 1],
            orig[protected - 1],
            "line {protected} must be byte-identical"
        );
    }
    assert_eq!(after[9], "# L10 unprotected");
    assert!(
        after[14].starts_with("# L15 trailing comment <!--"),
        "line 15 fires and is fixed: {:?}",
        after[14]
    );
}

/// The corrupting half of BUG-4: MD022's "surround headings with blank lines"
/// fix inserted a blank line *between* the directive and the heading it
/// guarded, disarming it so the next pass rewrote the protected line. The
/// finding is dropped, not merely un-fixed — inserting the blank by hand
/// breaks the directive just the same.
#[test]
fn no_autofix_splits_a_directive_from_the_line_it_guards() {
    let tmp = vault("dir = \".\"\n");
    write(
        &tmp,
        "g.md",
        "---\ntitle: g\n---\n\n# T\n\n<!-- markdownlint-disable-next-line MD019 -->\n#   x\n",
    );
    let (_, json, _) = run(&tmp, &["lint", "--rule", "MD022", "--detailed"]);
    assert_eq!(
        json["results"]["violations"], 0,
        "MD022 must not object to the directive line: {json}"
    );
}

// ---------------------------------------------------------------------------
// LINT-2 (BUG-5) — fences indented inside list items
// ---------------------------------------------------------------------------

/// The GitHub Docs shape (`removing-dependabot-access-to-public-registries.md`
/// line 224): a 4-space-indented `1.` whose fence sits at five columns.
/// `lint --fix` wrapped the URL inside it in angle brackets, corrupting the
/// YAML sample.
#[test]
fn md034_does_not_wrap_a_url_inside_a_list_indented_fence() {
    let tmp = vault("dir = \".\"\n");
    let before = "---\ntitle: lf\n---\n\n# T\n\n\
                  1. First step\n\
                  \x20   1. Add the registry to a `.yarnrc.yml` file\n\
                  \x20    ```\n\
                  \x20    npmRegistryServer: \"https://private_registry_url\"\n\
                  \x20    ```\n\n\
                  - bullet\n\
                  \x20 ```\n\
                  \x20 see https://example.com/raw\n\
                  \x20 ```\n\n\
                  > - quoted\n\
                  >   ```\n\
                  >   see https://example.com/quoted\n\
                  >   ```\n";
    write(&tmp, "lf.md", before);

    let (_, json, _) = run(&tmp, &["lint", "--rule", "MD034", "--detailed"]);
    assert_eq!(
        json["results"]["violations"], 0,
        "no bare-URL finding inside a list-indented fence: {json}"
    );

    let (_, _, _) = run(&tmp, &["lint", "--fix", "--fix-rule", "MD034"]);
    assert_eq!(read(&tmp, "lf.md"), before, "the samples are untouched");
}

// ---------------------------------------------------------------------------
// LINT-3 (BUG-43) / LINT-4 (BUG-19)
// ---------------------------------------------------------------------------

#[test]
fn an_unknown_directive_rule_id_warns_even_under_quiet() {
    let tmp = vault("dir = \".\"\n");
    write(
        &tmp,
        "u.md",
        "---\ntitle: u\n---\n\n# T\n\n<!-- markdownlint-disable no-hard-tab -->\n\ntext\n",
    );
    let (_, _, stderr) = run(&tmp, &["lint", "-q"]);
    assert!(
        stderr.contains("unknown rule `no-hard-tab`"),
        "-q must not hide it: {stderr:?}"
    );
    assert!(
        stderr.contains("u.md:7"),
        "names the comment's line: {stderr:?}"
    );

    // A known id inside a fenced sample is not a directive at all.
    write(
        &tmp,
        "u.md",
        "---\ntitle: u\n---\n\n# T\n\n```md\n<!-- markdownlint-disable no-such-rule -->\n```\n",
    );
    let (_, _, stderr) = run(&tmp, &["lint", "-q"]);
    assert!(
        !stderr.contains("unknown rule"),
        "a sample is not a directive: {stderr:?}"
    );
}

#[test]
fn max_per_rule_zero_means_unlimited() {
    let tmp = vault("dir = \".\"\n");
    let mut body = String::from("---\ntitle: m\n---\n\n# T\n");
    for i in 0..5 {
        use std::fmt::Write as _;
        writeln!(body, "\n#   heading {i}").unwrap();
    }
    write(&tmp, "m.md", &body);
    let (_, json, _) = run(
        &tmp,
        &[
            "lint",
            "--rule",
            "MD019",
            "--detailed",
            "--max-per-rule",
            "0",
        ],
    );
    let group = &json["results"]["files"][0]["rule_groups"][0];
    assert_eq!(group["truncated"], false, "{json}");
    assert_eq!(
        group["violations"].as_array().map(Vec::len),
        Some(5),
        "every violation is shown: {json}"
    );
}

// ---------------------------------------------------------------------------
// SCHEMA-1 (BUG-20) / SCHEMA-2 (BUG-42, DEC-312)
// ---------------------------------------------------------------------------

#[test]
fn a_typo_in_a_type_schema_is_a_config_error() {
    let tmp = vault("dir = \".\"\n\n[schema.types.note]\nrequried = [\"title\", \"status\"]\n");
    write(&tmp, "a.md", "---\ntype: note\n---\n\n# A\n");

    let (_, json, _) = run(&tmp, &["config"]);
    assert_eq!(json["results"]["malformed"], true, "{json}");
    let err = json["results"]["schema_error"]
        .as_str()
        .expect("schema_error");
    assert!(err.contains("unknown field `requried`"), "{err}");
    assert!(err.contains("expected one of"), "{err}");

    // DEC-290: the gate refuses rather than validating against an empty schema.
    let (code, _, _) = run(&tmp, &["lint", "--strict"]);
    assert_eq!(
        code, 1,
        "lint --strict refuses a schema that validates nothing"
    );
}

#[test]
fn a_mis_nested_type_table_names_the_command_that_creates_it() {
    let tmp = vault("dir = \".\"\n\n[schema.note]\nrequired = [\"title\"]\n");
    let (_, json, _) = run(&tmp, &["config"]);
    let err = json["results"]["schema_error"]
        .as_str()
        .expect("schema_error");
    assert!(err.contains("[schema.types.note]"), "{err}");
    assert!(err.contains("hyalo types set note"), "{err}");
}

/// DEC-312: `required` says present-and-non-empty, nothing about the type.
#[test]
fn required_without_a_property_block_imposes_no_type() {
    let tmp = vault("dir = \".\"\n\n[schema.types.note]\nrequired = [\"title\"]\n");
    write(&tmp, "n.md", "---\ntype: note\ntitle: 2024\n---\n\n# N\n");

    let (code, json, _) = run(&tmp, &["lint", "--rule", "SCHEMA", "--detailed"]);
    assert_eq!(code, 0, "{json}");
    assert_eq!(json["results"]["violations"], 0, "{json}");

    let (code, _, _) = run(
        &tmp,
        &[
            "set",
            "n.md",
            "--property",
            "title=2024",
            "--validate",
            "--dry-run",
        ],
    );
    assert_eq!(
        code, 0,
        "set --validate accepts a non-string required value"
    );

    // An *empty* required property is still an error — presence still matters.
    write(&tmp, "e.md", "---\ntype: note\ntitle:\n---\n\n# E\n");
    let (_, json, _) = run(
        &tmp,
        &["lint", "--rule", "SCHEMA", "--detailed", "--file", "e.md"],
    );
    assert!(
        json["results"]["violations"].as_u64().unwrap_or(0) >= 1,
        "{json}"
    );
}

// ---------------------------------------------------------------------------
// INDEX-1 (BUG-11) / INDEX-2 (BUG-12, G4)
// ---------------------------------------------------------------------------

#[test]
fn a_named_index_file_that_cannot_be_read_is_an_error() {
    let tmp = vault("dir = \".\"\n");
    write(&tmp, "a.md", "---\ntitle: a\n---\n");

    let (code, json, _) = run(&tmp, &["find", "--index-file", "definitely-not-here"]);
    assert_eq!(code, 1, "{json}");
    assert!(
        json["error"]
            .as_str()
            .is_some_and(|e| e.contains("could not read index file")),
        "{json}"
    );

    // Bare `--index` with no snapshot keeps the fallback, but says so where
    // `-q` cannot hide it.
    let (code, _, stderr) = run(&tmp, &["find", "--index", "-q"]);
    assert_eq!(code, 0);
    assert!(
        stderr.contains("falling back to disk scan"),
        "-q must not hide the fallback: {stderr:?}"
    );
}

#[test]
fn the_snapshot_carries_a_format_version_that_config_and_summary_expose() {
    let tmp = vault("dir = \".\"\n");
    write(&tmp, "a.md", "---\ntitle: a\n---\n\n# A\n");
    let (code, _, _) = run(&tmp, &["create-index"]);
    assert_eq!(code, 0);

    let (_, cfg, _) = run(&tmp, &["config"]);
    let binary_version = cfg["results"]["snapshot_format_version"]
        .as_u64()
        .expect("snapshot_format_version");
    assert!(binary_version >= 1, "{cfg}");

    let (_, json, _) = run(&tmp, &["summary", "--index"]);
    assert_eq!(
        json["results"]["index_format_version"].as_u64(),
        Some(binary_version),
        "a fresh index reports the binary's own version: {json}"
    );

    // A disk scan has no snapshot, so the key is absent rather than a lie.
    let (_, json, _) = run(&tmp, &["summary"]);
    assert!(json["results"]["index_format_version"].is_null(), "{json}");
}

// ---------------------------------------------------------------------------
// PATH-1 (BUG-21) / PATH-3 (BUG-28)
// ---------------------------------------------------------------------------

#[test]
fn a_cwd_shadowed_bare_path_names_both_candidates() {
    let tmp = vault("dir = \"kb\"\n");
    write(&tmp, "kb/a.md", "---\ntitle: root\n---\n");
    write(&tmp, "kb/sub/a.md", "---\ntitle: sub\n---\n");

    let mut cmd = hyalo_no_hints();
    cmd.current_dir(tmp.path().join("kb").join("sub"));
    let out = cmd
        .args(["set", "a.md", "--property", "x=1", "--dry-run", "-q"])
        .args(["--format", "json"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("resolved against the vault root"),
        "-q must not hide it: {stderr:?}"
    );
    assert!(
        stderr.contains("sub"),
        "names the CWD candidate too: {stderr:?}"
    );

    // From the vault root the two candidates are the same file: silence.
    let mut cmd = hyalo_no_hints();
    cmd.current_dir(tmp.path());
    let out = cmd
        .args(["set", "kb/a.md", "--property", "x=1", "--dry-run"])
        .args(["--format", "json"])
        .output()
        .unwrap();
    assert!(
        !String::from_utf8_lossy(&out.stderr).contains("resolved against the vault root"),
        "no shadow at the vault root"
    );
}

#[test]
fn mv_refuses_a_destination_containing_dot_dot() {
    let tmp = vault("dir = \".\"\n");
    write(&tmp, "a.md", "---\ntitle: a\n---\n");
    let (code, json, _) = run(&tmp, &["mv", "a.md", "--to", "../deep/", "--dry-run"]);
    assert_eq!(code, 1, "{json}");
    assert!(
        json["error"]
            .as_str()
            .is_some_and(|e| e.contains("path contains '..'")),
        "same wording as the source check: {json}"
    );
}

#[test]
fn the_redundant_dir_note_does_not_claim_an_unset_dir() {
    let tmp = vault("");
    write(&tmp, "a.md", "---\ntitle: a\n---\n");
    let (_, _, stderr) = run(&tmp, &["find", "--dir", ".", "--limit", "1"]);
    assert!(
        stderr.contains("sets no dir") && stderr.contains("the default is `.`"),
        "{stderr:?}"
    );
}

// ---------------------------------------------------------------------------
// WRITE-1/2 (BUG-33, BUG-34) — the fences are bytes nobody addressed
// ---------------------------------------------------------------------------

#[test]
fn frontmatter_fences_with_trailing_whitespace_round_trip() {
    let tmp = vault("dir = \".\"\n");
    write(&tmp, "close.md", "---\ntitle: t\n--- \n\nbody\n");
    let (code, _, _) = run(&tmp, &["set", "close.md", "--property", "x=1"]);
    assert_eq!(code, 0);
    assert_eq!(
        read(&tmp, "close.md"),
        "---\ntitle: t\nx: 1\n--- \n\nbody\n"
    );

    write(&tmp, "open.md", "--- \ntitle: t\n---\n\nbody\n");
    let (code, _, _) = run(&tmp, &["set", "open.md", "--property", "x=1"]);
    assert_eq!(code, 0);
    assert_eq!(
        read(&tmp, "open.md"),
        "--- \ntitle: t\nx: 1\n---\n\nbody\n",
        "`--- ` opens frontmatter; no second block is prepended"
    );
}

// ---------------------------------------------------------------------------
// WRITE-3 (BUG-35) — one envelope, no bare `error:` line
// ---------------------------------------------------------------------------

#[test]
fn a_write_against_an_unparsable_file_is_a_single_json_envelope() {
    for cmd in ["set", "append", "remove"] {
        let tmp = vault("dir = \".\"\n");
        write(&tmp, "bad.md", "---\ntitle: [unclosed\n---\n\nbody\n");
        let arg = if cmd == "remove" { "title" } else { "x=1" };
        let output = hyalo(&tmp)
            .args([cmd, "bad.md", "--property", arg, "--format", "json"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{cmd}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.trim_start().starts_with('{'),
            "{cmd}: stderr must be only the envelope, got {stderr:?}"
        );
        let json: serde_json::Value = serde_json::from_str(&stderr).unwrap();
        assert!(
            json["cause"]
                .as_str()
                .is_some_and(|c| c.contains("unclosed bracket")),
            "{cmd}: the diagnostic rides in `cause`: {json}"
        );
        assert!(
            !json["hint"]
                .as_str()
                .unwrap_or_default()
                .contains("error above"),
            "{cmd}: the hint no longer points at a line that is gone"
        );
    }
}

// ---------------------------------------------------------------------------
// WRITE-4 (BUG-38) / WRITE-5 (BUG-40) / WRITE-6 (UX-1)
// ---------------------------------------------------------------------------

#[test]
fn tags_rename_keeps_a_flow_list_in_flow_style() {
    let tmp = vault("dir = \".\"\n");
    write(&tmp, "flow.md", "---\ntags: [a, b]\n---\n\nbody\n");
    write(&tmp, "block.md", "---\ntags:\n  - a\n  - b\n---\n\nbody\n");
    let (code, _, _) = run(&tmp, &["tags", "rename", "--from", "a", "--to", "c"]);
    assert_eq!(code, 0);
    assert_eq!(read(&tmp, "flow.md"), "---\ntags: [c, b]\n---\n\nbody\n");
    assert_eq!(
        read(&tmp, "block.md"),
        "---\ntags:\n  - c\n  - b\n---\n\nbody\n",
        "a block list keeps its own indentation too"
    );
}

#[test]
fn ordered_and_wide_gap_list_markers_are_tasks() {
    let tmp = vault("dir = \".\"\n");
    write(
        &tmp,
        "t.md",
        "---\ntitle: t\n---\n\n# T\n\n\
         - [ ] one space\n\
         -  [ ] two spaces\n\
         1. [ ] ordered dot\n\
         2) [x] ordered paren\n\
         1.5 not a task\n\
         -fish not a task\n",
    );
    let (_, json, _) = run(&tmp, &["find", "--file", "t.md", "--fields", "tasks"]);
    let tasks = json["results"][0]["tasks"].as_array().expect("tasks");
    let lines: Vec<u64> = tasks.iter().filter_map(|t| t["line"].as_u64()).collect();
    assert_eq!(lines, vec![7, 8, 9, 10], "{json}");
    assert_eq!(tasks[3]["status"], "x");

    // The write side agrees: toggling keeps the marker's own spacing.
    let (code, _, _) = run(&tmp, &["task", "toggle", "t.md", "--line", "8"]);
    assert_eq!(code, 0);
    assert!(
        read(&tmp, "t.md").contains("-  [x] two spaces"),
        "{}",
        read(&tmp, "t.md")
    );
}

#[test]
fn bulk_writes_name_why_each_file_was_skipped() {
    let tmp = vault("dir = \".\"\n");
    write(&tmp, "same.md", "---\ntitle: a\nstatus: draft\n---\n");
    write(&tmp, "other.md", "---\ntitle: b\n---\n");
    write(&tmp, "bad.md", "---\ntitle: [unclosed\n---\n");

    let (code, json, _) = run(
        &tmp,
        &[
            "set",
            "--glob",
            "*.md",
            "--property",
            "status=draft",
            "--dry-run",
        ],
    );
    assert_eq!(code, 0, "{json}");
    let detail = json["results"]["skipped_detail"]
        .as_array()
        .expect("skipped_detail");
    let mut pairs: Vec<(String, String)> = detail
        .iter()
        .map(|d| {
            (
                d["file"].as_str().unwrap_or_default().to_owned(),
                d["reason"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    pairs.sort();
    assert_eq!(
        pairs,
        vec![
            ("bad.md".to_owned(), "unparsable".to_owned()),
            ("same.md".to_owned(), "unchanged".to_owned()),
        ],
        "{json}"
    );
}

// ---------------------------------------------------------------------------
// WRITE-7 (BUG-44) / HELP-1 (BUG-27)
// ---------------------------------------------------------------------------

#[test]
fn the_near_duplicate_warning_needs_a_real_similarity_and_a_real_sample() {
    // Twelve files, values sharing no letters: the report's false positive.
    let tmp = vault("dir = \".\"\n");
    for i in 0..11 {
        write(&tmp, &format!("n{i}.md"), "---\ntitle: n\nkind: aa\n---\n");
    }
    write(&tmp, "odd.md", "---\ntitle: odd\nkind: zz\n---\n");
    let (_, _, stderr) = run(&tmp, &["summary"]);
    assert!(
        !stderr.contains("did you mean"),
        "unrelated short values are not typos: {stderr:?}"
    );

    // A genuine typo in a genuine distribution still speaks.
    let tmp = vault("dir = \".\"\n");
    for i in 0..11 {
        write(
            &tmp,
            &format!("n{i}.md"),
            "---\ntitle: n\nstatus: completed\n---\n",
        );
    }
    write(&tmp, "typo.md", "---\ntitle: t\nstatus: complated\n---\n");
    let (_, _, stderr) = run(&tmp, &["summary"]);
    assert!(stderr.contains("did you mean \"completed\""), "{stderr:?}");
}

#[test]
fn lint_rules_show_offers_no_set_hint_for_a_non_configurable_rule() {
    let tmp = vault("dir = \".\"\n");
    let output = hyalo(&tmp)
        .args(["lint-rules", "show", "SCHEMA", "--format", "text"])
        .env_remove("HYALO_NO_HINTS")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        !text.contains("lint-rules set SCHEMA"),
        "the hint used to fail with `no such rule`: {text}"
    );
}
