//! Iteration 324: a search suggestion leads to results, hints are runnable and
//! worth running, and the small leftovers of iteration 323.

use super::common::hyalo;
use std::fs;
use std::path::Path;
use std::process::Output;
use tempfile::TempDir;

fn vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \"kb\"\n").unwrap();
    fs::create_dir(tmp.path().join("kb")).unwrap();
    tmp
}

fn note(tmp: &TempDir, name: &str, text: &str) {
    fs::write(tmp.path().join("kb").join(name), text).unwrap();
}

fn run(root: &Path, args: &[&str]) -> Output {
    hyalo().current_dir(root).args(args).output().unwrap()
}

fn json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "not JSON ({e}): {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

#[test]
fn a_did_you_mean_candidate_finds_documents() {
    let tmp = vault();
    note(&tmp, "a.md", "# A\n\nThe response arrived.\n");
    note(
        &tmp,
        "b.md",
        "# B\n\nAnother response, and a promise kept.\n",
    );
    for typo in ["responce", "promse"] {
        let zero = json(&run(
            tmp.path(),
            &["find", typo, "--format", "json", "--no-hints"],
        ));
        assert_eq!(zero["total"], 0, "{typo}");
        let with_hints = json(&run(tmp.path(), &["find", typo, "--format", "json"]));
        let suggestions = with_hints["suggestions"].as_array().expect("suggestions");
        assert!(!suggestions.is_empty(), "{typo}: {with_hints}");
        for suggestion in suggestions {
            for candidate in suggestion["candidates"].as_array().unwrap() {
                let term = candidate["term"].as_str().unwrap();
                let found = json(&run(tmp.path(), &["find", "--format", "json", "--", term]));
                assert!(
                    found["total"].as_u64().unwrap() > 0,
                    "candidate {term} of {typo} finds nothing"
                );
                let listed = json(&run(tmp.path(), &["terms", term, "--format", "json"]));
                assert!(
                    listed["results"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|t| t["term"] == term),
                    "terms does not list {term}"
                );
            }
        }
    }
}

#[test]
fn find_and_terms_agree_on_a_dictionary_term() {
    let tmp = vault();
    note(&tmp, "a.md", "# A\n\nThe response arrived.\n");
    let listed = json(&run(tmp.path(), &["terms", "respons", "--format", "json"]));
    assert_eq!(listed["results"][0]["term"], "respons");
    let found = json(&run(
        tmp.path(),
        &["find", "respons", "--format", "json", "--no-hints"],
    ));
    assert_eq!(found["total"], 1);
}

#[test]
fn an_orphan_listing_does_not_hint_at_its_empty_backlinks() {
    let tmp = vault();
    note(&tmp, "lonely.md", "# Lonely\n\nNo links.\n");
    let out = json(&run(tmp.path(), &["find", "--orphan", "--format", "json"]));
    let hints = out["hints"].as_array().unwrap();
    assert!(
        hints
            .iter()
            .all(|h| !h["cmd"].as_str().unwrap().starts_with("hyalo backlinks")),
        "{out}"
    );
}

#[test]
fn a_failed_slop_phrase_offers_either_order_not_a_loop() {
    let tmp = vault();
    note(
        &tmp,
        "a.md",
        "# A\n\nalpha one two three four five six beta\n",
    );
    let out = json(&run(
        tmp.path(),
        &["find", "\"beta alpha\"~2", "--format", "json"],
    ));
    let cmds: Vec<&str> = out["hints"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|h| h["cmd"].as_str())
        .collect();
    assert!(
        cmds.iter().any(|c| c.contains("OR")),
        "no either-order hint: {cmds:?}"
    );
}

#[test]
fn advice_only_hints_are_not_printed_with_a_command_arrow() {
    let tmp = vault();
    note(&tmp, "a.md", "# A\n\n[x](/missing/page)\n");
    let out = run(tmp.path(), &["find", "--broken-links", "--format", "text"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.lines()
            .filter(|l| l.contains("site-absolute"))
            .all(|l| l.trim_start().starts_with("note: ")),
        "{text}"
    );
}

#[test]
fn config_hints_keep_the_explicit_format() {
    let tmp = vault();
    let out = json(&run(tmp.path(), &["config", "--format", "json"]));
    for hint in out["hints"].as_array().unwrap() {
        assert!(
            hint["cmd"].as_str().unwrap().contains("--format json"),
            "{hint}"
        );
    }
}

#[test]
fn a_section_excerpt_drops_the_files_bom() {
    let tmp = vault();
    fs::write(
        tmp.path().join("kb/bom.md"),
        "\u{feff}# Title\n\nbody\n\n## Sub\n\ntext\n",
    )
    .unwrap();
    let out = run(
        tmp.path(),
        &["read", "bom.md", "--section", "Title", "--format", "text"],
    );
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains('\u{feff}'),
        "section excerpt carries the BOM"
    );
    let whole = run(tmp.path(), &["read", "bom.md", "--format", "text"]);
    assert!(String::from_utf8_lossy(&whole.stdout).contains('\u{feff}'));
}

#[test]
fn lint_rules_set_reports_a_rewrite_that_changes_no_effective_state() {
    let tmp = vault();
    fs::write(
        tmp.path().join(".hyalo.toml"),
        "dir = \"kb\"\n\n[lint.rules]\nMD013 = { severity = \"eror\" }\n",
    )
    .unwrap();
    let out = run(
        tmp.path(),
        &[
            "lint-rules",
            "set",
            "MD013",
            "--severity",
            "warn",
            "--format",
            "text",
        ],
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(!text.contains("(no change)"), "{text}");
    assert!(text.contains("rewritten"), "{text}");
    let after = fs::read_to_string(tmp.path().join(".hyalo.toml")).unwrap();
    assert!(!after.contains("eror"), "{after}");
}

#[test]
fn a_multi_line_flow_list_comment_loss_is_warned() {
    let tmp = vault();
    note(&tmp, "a.md", "---\ntags: [a,\n  b] # note\n---\nbody\n");
    let out = run(
        tmp.path(),
        &[
            "tags", "rename", "--from", "a", "--to", "c", "--format", "text",
        ],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("comment is lost"), "{stderr}");
}

#[test]
fn set_without_a_target_names_the_positional_form() {
    let tmp = vault();
    let out = run(
        tmp.path(),
        &["set", "--property", "k=1", "--format", "json"],
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("hyalo set FILE"), "{stderr}");
}

#[test]
fn a_correction_inside_quotes_runs() {
    let tmp = vault();
    note(&tmp, "a.md", "# A\n\nThe response time was long.\n");
    for typo in ["\"responce time\"", "\"responce\"", "responce time"] {
        let out = json(&run(tmp.path(), &["find", typo, "--format", "json"]));
        let fixed = out["hints"]
            .as_array()
            .unwrap()
            .iter()
            .find(|h| {
                h["description"]
                    .as_str()
                    .unwrap()
                    .starts_with("Did you mean")
            })
            .unwrap_or_else(|| panic!("{typo}: no correction in {out}"));
        let cmd = fixed["cmd"].as_str().unwrap();
        // The hint is `hyalo find ... -- '<query>'`; run it through the shell-free parser.
        let query = cmd.rsplit(" -- ").next().unwrap().trim_matches('\'');
        let found = json(&run(tmp.path(), &["find", "--format", "json", "--", query]));
        assert!(found["total"].as_u64().unwrap() > 0, "{typo}: {cmd}");
    }
}

#[test]
fn a_comment_loss_warning_names_the_file() {
    let tmp = vault();
    note(&tmp, "a.md", "---\ntags: [it's,\n  a] # note\n---\nbody\n");
    let out = run(tmp.path(), &["tags", "rename", "--from", "a", "--to", "c"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("a.md") && stderr.contains("comment is lost"),
        "{stderr}"
    );
}

#[test]
fn a_dry_run_says_the_override_would_be_rewritten() {
    let tmp = vault();
    fs::write(
        tmp.path().join(".hyalo.toml"),
        "dir = \"kb\"\n\n[lint.rules]\nMD013 = { severity = \"eror\" }\n",
    )
    .unwrap();
    let out = run(
        tmp.path(),
        &[
            "lint-rules",
            "set",
            "MD013",
            "--severity",
            "warn",
            "--dry-run",
            "--format",
            "text",
        ],
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("would be rewritten") && !text.contains("was rewritten"),
        "{text}"
    );
}

#[test]
fn a_missing_index_file_is_not_threaded_into_hints() {
    let tmp = vault();
    let out = json(&run(
        tmp.path(),
        &[
            "--index-file",
            "missing.idx",
            "types",
            "list",
            "--format",
            "json",
        ],
    ));
    for hint in out["hints"].as_array().unwrap() {
        assert!(
            !hint["cmd"].as_str().unwrap().contains("missing.idx"),
            "{hint}"
        );
    }
}

#[test]
fn a_refused_custom_index_is_rebuilt_where_it_lives() {
    let tmp = vault();
    for i in 0..501 {
        note(&tmp, &format!("n{i}.md"), "# N\n\nbody\n");
    }
    let custom = tmp.path().join("kb/custom.idx");
    let custom_arg = custom.to_str().unwrap();
    let built = run(
        tmp.path(),
        &["create-index", "-o", custom_arg, "--format", "json"],
    );
    assert!(built.status.success());
    // Make this binary refuse it: the header's `format_version` becomes 7.
    let mut bytes = fs::read(&custom).unwrap();
    let key = b"format_version";
    let at = bytes.windows(key.len()).position(|w| w == key).unwrap() + key.len();
    assert_eq!(
        u32::from(bytes[at]),
        hyalo_core::index::SNAPSHOT_FORMAT_VERSION
    );
    bytes[at] = 7;
    fs::write(&custom, &bytes).unwrap();

    let refused = run(
        tmp.path(),
        &["summary", "--index-file", custom_arg, "--format", "json"],
    );
    let out = json(&refused);
    let first = out["hints"][0]["cmd"].as_str().unwrap().to_owned();
    assert!(first.starts_with("hyalo create-index"), "{first}");
    assert!(
        first.contains("--output") && first.contains("custom.idx"),
        "{first}"
    );
    assert!(!tmp.path().join("kb/.hyalo-index").exists());

    let argv: Vec<&str> = first.split_whitespace().skip(1).collect();
    let rebuilt = run(tmp.path(), &argv);
    assert!(
        rebuilt.status.success(),
        "{}",
        String::from_utf8_lossy(&rebuilt.stderr)
    );
    let again = json(&run(
        tmp.path(),
        &["summary", "--index-file", custom_arg, "--format", "json"],
    ));
    assert_eq!(again["results"]["source"], "index", "{again}");
}
