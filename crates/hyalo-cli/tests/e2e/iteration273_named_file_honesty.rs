//! Iteration 273 — index and named-file honesty.
//!
//! Every case here is one where hyalo used to answer a question about a file
//! the caller *named*, or a question answered from the snapshot index, with a
//! clean exit 0 and a wrong or empty result.
//!
//! - **Part A (NAMED-1..4).** A named path is a promise: an unparsable one is
//!   an error, one missing from the snapshot is read from disk, per-file
//!   `--broken-links` keeps its anchor verdict, and `lint --rule X` reports
//!   only rule X.
//! - **Part B (INDEX-1..3).** The snapshot tells the truth about in-place
//!   edits, `[scan] exclude` and invalid UTF-8.
//! - **Part C (MV-1..3).** `mv` uses the path you named, sweeps for split
//!   frontmatter links in batch mode too, and validates `--on-conflict`.

use assert_cmd::Command;
use tempfile::TempDir;

use crate::common::hyalo_no_hints;

fn hyalo(tmp: &TempDir) -> Command {
    let mut cmd = hyalo_no_hints();
    cmd.current_dir(tmp.path());
    cmd
}

fn write(tmp: &TempDir, rel: &str, body: &str) {
    let path = tmp.path().join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, body).unwrap();
}

/// Run a command and return `(exit_code, json, stderr)`.
///
/// Successful payloads land on stdout; the `{"error": …}` envelope lands on
/// stderr, so the parse falls back to stderr when stdout is not JSON.
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

fn run_ok(tmp: &TempDir, args: &[&str]) -> serde_json::Value {
    let (code, json, stderr) = run(tmp, args);
    assert_eq!(code, 0, "`hyalo {}` failed: {stderr}", args.join(" "));
    json
}

/// A vault with one file whose frontmatter declares `title` twice.
fn vault_with_unparsable_file() -> TempDir {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(
        &tmp,
        "dup.md",
        "---\ntitle: Dup\ntitle: Dup2\n---\n\nbody\n",
    );
    write(&tmp, "good.md", "---\ntitle: Good\n---\n\n# Good\n\nbody\n");
    tmp
}

// ---------------------------------------------------------------------------
// Part A — NAMED-1: a named file that will not parse is an error
// ---------------------------------------------------------------------------

#[test]
fn find_file_on_an_unparsable_note_exits_1_with_the_yaml_diagnostic() {
    let tmp = vault_with_unparsable_file();
    let (code, json, _) = run(&tmp, &["find", "--file", "dup.md"]);
    assert_eq!(code, 1, "a named unparsable file must fail the run: {json}");
    assert_eq!(json["error"], "dup.md: unparseable frontmatter", "{json}");
    assert!(
        json["cause"]
            .as_str()
            .unwrap_or_default()
            .contains("duplicate key"),
        "the YAML diagnostic must survive into `cause`: {json}"
    );
    assert!(
        json["hint"]
            .as_str()
            .unwrap_or_default()
            .contains("HYALO005"),
        "the hint must point at the rule that explains it: {json}"
    );
}

#[test]
fn positional_file_on_an_unparsable_note_exits_1_too() {
    let tmp = vault_with_unparsable_file();
    // `find`'s first positional is PATTERN, so the positional FILE form needs
    // a pattern in front of it.
    let (code, json, _) = run(&tmp, &["find", "body", "dup.md"]);
    assert_eq!(code, 1, "{json}");
    assert_eq!(json["error"], "dup.md: unparseable frontmatter", "{json}");
}

#[test]
fn files_from_keeps_batch_semantics_for_an_unparsable_note() {
    // DEC-284: a `--files-from` list is a batch, not a promise about one path.
    // The unusable entry is counted and the run still exits 0.
    let tmp = vault_with_unparsable_file();
    let mut cmd = hyalo(&tmp);
    let output = cmd
        .args(["find", "--files-from", "-", "--format", "json"])
        .write_stdin("dup.md\ngood.md\n")
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json["total"], 1,
        "only the parsable file is reported: {json}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unparsable frontmatter"),
        "the skip is still summarised on stderr: {stderr}"
    );
}

#[test]
fn a_glob_is_not_a_named_file() {
    // `--glob` selects; it does not promise that any particular file matched,
    // so an unparsable member stays a counted skip.
    let tmp = vault_with_unparsable_file();
    let json = run_ok(&tmp, &["find", "--glob", "*.md"]);
    assert_eq!(json["total"], 1, "{json}");
}

#[test]
fn the_vault_sweep_is_not_a_named_file() {
    let tmp = vault_with_unparsable_file();
    let json = run_ok(&tmp, &["find"]);
    assert_eq!(json["total"], 1, "{json}");
}

// ---------------------------------------------------------------------------
// Part A — NAMED-2: `--index --file` sees a file the snapshot never did
// ---------------------------------------------------------------------------

#[test]
fn find_index_file_reads_a_note_created_after_the_snapshot() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(&tmp, "old.md", "---\ntitle: Old\n---\n\nbody\n");
    run_ok(&tmp, &["create-index"]);
    write(&tmp, "brand-new.md", "---\ntitle: Brand New\n---\n\nbody\n");

    let json = run_ok(&tmp, &["find", "--index", "--file", "brand-new.md"]);
    assert_eq!(
        json["total"], 1,
        "a named file on disk must not be invisible just because the snapshot \
         predates it: {json}"
    );
    assert_eq!(json["results"][0]["title"], "Brand New", "{json}");
}

#[test]
fn find_index_file_still_refuses_a_path_in_neither_place() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(&tmp, "old.md", "---\ntitle: Old\n---\n\nbody\n");
    run_ok(&tmp, &["create-index"]);

    let (code, json, _) = run(&tmp, &["find", "--index", "--file", "ghost.md"]);
    assert_eq!(
        code, 1,
        "neither snapshot nor disk has it — the same refusal the non-index \
         path gives, never an empty success: {json}"
    );
    assert_eq!(json["error"], "file not found", "{json}");
}

// ---------------------------------------------------------------------------
// Part A — NAMED-3: per-file `--broken-links` keeps its anchor data
// ---------------------------------------------------------------------------

fn anchor_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(
        &tmp,
        "target.md",
        "---\ntitle: Target\n---\n\n## DEC-068: Snapshot index format\n",
    );
    write(
        &tmp,
        "source.md",
        "---\ntitle: Source\n---\n\nSee [[target#DEC-068]].\n",
    );
    tmp
}

/// `(broken_anchor, suggested_fragment)` for `source.md`'s only link.
fn anchor_verdict(tmp: &TempDir, args: &[&str]) -> (bool, Option<String>) {
    let json = run_ok(tmp, args);
    let link = json["results"]
        .as_array()
        .expect("results array")
        .iter()
        .find(|f| f["file"] == "source.md")
        .and_then(|f| f["links"].as_array())
        .and_then(|l| l.first())
        .unwrap_or_else(|| panic!("no link reported by `hyalo {}`", args.join(" ")));
    (
        link["broken_anchor"].as_bool().unwrap_or(false),
        link["suggested_fragment"].as_str().map(str::to_owned),
    )
}

#[test]
fn the_broken_anchor_verdict_does_not_depend_on_how_the_file_was_selected() {
    let tmp = anchor_vault();
    let sweep = anchor_verdict(&tmp, &["find", "--broken-links"]);
    assert_eq!(
        sweep,
        (true, Some("DEC-068: Snapshot index format".to_owned())),
        "baseline: the vault sweep sees the broken anchor and DEC-268's suggestion"
    );
    for args in [
        vec!["find", "--file", "source.md", "--broken-links"],
        vec!["find", "--glob", "source.md", "--broken-links"],
        vec!["find", "See", "source.md", "--broken-links"],
        vec!["find", "--file", "source.md", "--fields", "links"],
    ] {
        assert_eq!(
            anchor_verdict(&tmp, &args),
            sweep,
            "`hyalo {}` disagreed with the vault sweep",
            args.join(" ")
        );
    }
}

// ---------------------------------------------------------------------------
// Part A — NAMED-4: `lint --rule X` does not leak HYALO005
// ---------------------------------------------------------------------------

#[test]
fn lint_rule_filter_excludes_frontmatter_parse_errors() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(
        &tmp,
        "dup.md",
        "---\ntitle: Dup\ntitle: Dup2\n---\n\nbody\n",
    );

    let output = hyalo(&tmp)
        .args(["lint", "--rule", "MD018", "--count"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "0",
        "a parse error is not an MD018 hit"
    );
    assert_eq!(output.status.code(), Some(0), "…and must not fail the gate");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unparsable frontmatter"),
        "the file is still accounted for as a skip"
    );
}

#[test]
fn lint_rule_hyalo005_still_reports_the_parse_error() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(
        &tmp,
        "dup.md",
        "---\ntitle: Dup\ntitle: Dup2\n---\n\nbody\n",
    );

    for filter in [
        vec!["lint", "--rule", "HYALO005", "--count"],
        vec!["lint", "--rule-prefix", "HYALO", "--count"],
    ] {
        let output = hyalo(&tmp).args(&filter).output().unwrap();
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "1",
            "`hyalo {}` must still see the parse error",
            filter.join(" ")
        );
    }
}

// ---------------------------------------------------------------------------
// Part B — INDEX-1: the stale probe sees an in-place overwrite
// ---------------------------------------------------------------------------

#[test]
fn an_in_place_overwrite_makes_the_next_index_read_warn() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(
        &tmp,
        "n1.md",
        "---\ntitle: N1\nstatus: final\n---\n\nbody\n",
    );
    write(
        &tmp,
        "n2.md",
        "---\ntitle: N2\nstatus: final\n---\n\nbody\n",
    );
    run_ok(&tmp, &["create-index"]);

    // The probe compares whole seconds with a one-second tolerance, so the
    // overwrite has to land in a later second to be detectable at all.
    std::thread::sleep(std::time::Duration::from_millis(2100));
    write(
        &tmp,
        "n2.md",
        "---\ntitle: N2 rewritten\nstatus: draft\n---\n\nnew\n",
    );

    let (code, _, stderr) = run(&tmp, &["find", "--index", "--property", "status=final"]);
    assert_eq!(code, 0, "warn-but-serve: the read still answers");
    assert!(
        stderr.contains("index older than vault") && stderr.contains("n2.md"),
        "the directory-mtime probe cannot see an in-place overwrite; the \
         per-file probe must, and must name the witness: {stderr}"
    );
}

#[test]
fn a_clean_index_does_not_warn() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(&tmp, "n1.md", "---\ntitle: N1\n---\n\nbody\n");
    run_ok(&tmp, &["create-index"]);

    let (_, _, stderr) = run(&tmp, &["find", "--index"]);
    assert!(
        !stderr.contains("index older than vault"),
        "an untouched vault must stay quiet: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Part B — INDEX-2: `summary --index` reports the excluded count
// ---------------------------------------------------------------------------

#[test]
fn summary_agrees_with_the_disk_scan_about_excluded_files() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(
        tmp.path().join(".hyalo.toml"),
        "dir = \".\"\n[scan]\nexclude = [\"Templates/**\"]\n",
    )
    .unwrap();
    for i in 0..3 {
        write(&tmp, &format!("n{i}.md"), "---\ntitle: N\n---\n\nbody\n");
    }
    for i in 0..2 {
        write(
            &tmp,
            &format!("Templates/t{i}.md"),
            "---\ntitle: T\n---\n\nbody\n",
        );
    }

    let disk = run_ok(&tmp, &["summary"])["results"]["files"].clone();
    run_ok(&tmp, &["create-index"]);
    let indexed = run_ok(&tmp, &["summary", "--index"])["results"]["files"].clone();

    assert_eq!(disk["excluded"], 2, "baseline off disk: {disk}");
    assert_eq!(
        indexed["excluded"], disk["excluded"],
        "the snapshot records what exclusion dropped when it was built, so \
         `--index` cannot report 0: disk={disk} index={indexed}"
    );
    assert_eq!(indexed["total"], disk["total"], "{indexed}");
    assert_eq!(indexed["skipped"], disk["skipped"], "{indexed}");
}

// ---------------------------------------------------------------------------
// Part B — INDEX-3: invalid UTF-8 answers the same off disk and from the index
// ---------------------------------------------------------------------------

#[test]
fn invalid_utf8_notes_answer_identically_on_both_paths() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(&tmp, "good.md", "---\ntitle: Good\n---\n\nzebra herd\n");
    let mut bytes = b"---\ntitle: Bad\n---\n\nzebra ".to_vec();
    bytes.extend_from_slice(&[0xff, 0xfe]);
    bytes.extend_from_slice(b" trailing\n");
    std::fs::write(tmp.path().join("bad.md"), bytes).unwrap();
    run_ok(&tmp, &["create-index"]);

    // BM25 drops the file on both paths (it is out of the corpus).
    let files = |v: &serde_json::Value| -> Vec<String> {
        v["results"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|f| f["file"].as_str().unwrap_or_default().to_owned())
                    .collect()
            })
            .unwrap_or_default()
    };
    assert_eq!(
        files(&run_ok(&tmp, &["find", "zebra"])),
        files(&run_ok(&tmp, &["find", "zebra", "--index"])),
        "BM25 must agree"
    );
    // `find -e` matches it lossily on both paths.
    assert_eq!(
        files(&run_ok(&tmp, &["find", "-e", "zebra"])),
        files(&run_ok(&tmp, &["find", "-e", "zebra", "--index"])),
        "regex search must agree"
    );
    // And naming it directly returns it on both paths.
    assert_eq!(
        files(&run_ok(&tmp, &["find", "--file", "bad.md"])),
        files(&run_ok(&tmp, &["find", "--index", "--file", "bad.md"])),
        "a named invalid-UTF-8 note must answer the same either way"
    );
}

// ---------------------------------------------------------------------------
// Part C — MV-1: the destination is normalised like the source
// ---------------------------------------------------------------------------

/// A project root holding `.hyalo.toml` with `dir = "kb"`, so commands run from
/// the root name files as `kb/<name>.md` — the shape that produced `kb/kb/...`.
fn nested_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \"kb\"\n").unwrap();
    for name in ["a", "b", "c", "d"] {
        write(
            &tmp,
            &format!("kb/{name}.md"),
            &format!("---\ntitle: {name}\nstatus: draft\n---\n\n# {name}\n"),
        );
    }
    std::fs::create_dir_all(tmp.path().join("kb/sub")).unwrap();
    tmp
}

#[test]
fn every_destination_form_lands_inside_the_vault_not_below_a_second_copy_of_it() {
    let tmp = nested_vault();

    run_ok(&tmp, &["mv", "kb/a.md", "kb/sub/a.md"]);
    run_ok(&tmp, &["mv", "--file", "kb/b.md", "--to", "kb/sub/b.md"]);
    run_ok(
        &tmp,
        &["mv", "--glob", "c.md", "--to", "kb/sub/", "--apply"],
    );

    for name in ["a", "b", "c"] {
        assert!(
            tmp.path().join(format!("kb/sub/{name}.md")).is_file(),
            "{name}.md should be at kb/sub/{name}.md"
        );
        assert!(
            !tmp.path().join(format!("kb/kb/sub/{name}.md")).exists(),
            "{name}.md must not land under a second `kb/` segment"
        );
    }
    assert!(
        !tmp.path().join("kb/kb").exists(),
        "no `kb/kb` directory should ever be created"
    );
}

#[test]
fn a_vault_relative_destination_still_works_from_inside_the_vault() {
    let tmp = nested_vault();
    let output = hyalo_no_hints()
        .current_dir(tmp.path().join("kb"))
        .args(["mv", "d.md", "sub/d.md", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(tmp.path().join("kb/sub/d.md").is_file());
}

#[test]
fn a_trailing_slash_destination_that_does_not_exist_says_so() {
    let tmp = nested_vault();
    let (code, json, _) = run(&tmp, &["mv", "--file", "kb/a.md", "--to", "kb/nope/"]);
    assert_eq!(code, 1, "{json}");
    assert_eq!(
        json["error"], "destination directory does not exist",
        "{json}"
    );
    let hint = json["hint"].as_str().unwrap_or_default();
    assert!(
        !hint.contains("/.md"),
        "a trailing slash is directory syntax, never a filename stem: {hint}"
    );
    assert!(hint.contains("nope/a.md"), "{hint}");
}

// ---------------------------------------------------------------------------
// Part C — MV-2: batch `mv` sweeps for split frontmatter links too
// ---------------------------------------------------------------------------

#[test]
fn batch_mv_reports_a_split_frontmatter_link_the_graph_never_saw() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(
        &tmp,
        "Categories/Books.md",
        "---\ntags:\n  - categories\n---\n\n# Books\n",
    );
    write(
        &tmp,
        "References/Folded.md",
        "---\nsummary: >\n  points at [[Categories/\n  Books]] somehow\n---\n\nNo other link.\n",
    );

    let json = run_ok(
        &tmp,
        &["mv", "--glob", "Categories/*.md", "--to", "Archive/"],
    );
    let moves = json["results"]["moves"].as_array().expect("moves array");
    assert_eq!(moves.len(), 1, "{json}");
    let skipped = moves[0]["frontmatter_links_skipped"]
        .as_array()
        .expect("per-move split-link report");
    assert_eq!(
        skipped.len(),
        1,
        "batch mode must run the same sweep single-file `mv` does: {json}"
    );
    assert_eq!(skipped[0]["source"], "References/Folded.md", "{json}");
    assert_eq!(skipped[0]["line"], 3, "{json}");
}

#[test]
fn batch_mv_without_split_links_reports_none() {
    let tmp = nested_vault();
    let json = run_ok(&tmp, &["mv", "--glob", "a.md", "--to", "sub/"]);
    let moves = json["results"]["moves"].as_array().expect("moves array");
    assert!(
        moves[0].get("frontmatter_links_skipped").is_none(),
        "the key stays out of the ordinary result shape: {json}"
    );
}

// ---------------------------------------------------------------------------
// Part C — MV-3: `--on-conflict` is validated and honoured
// ---------------------------------------------------------------------------

#[test]
fn an_unknown_on_conflict_policy_is_a_usage_error() {
    let tmp = nested_vault();
    let output = hyalo(&tmp)
        .args(["mv", "--file", "kb/a.md", "--to", "kb/sub/a.md"])
        .args(["--on-conflict", "overwrite"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(2),
        "a bogus policy must not parse and then silently behave as `error`"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("possible values: error, skip"), "{stderr}");
}

#[test]
fn single_file_mv_honours_on_conflict_skip() {
    let tmp = nested_vault();
    write(
        &tmp,
        "kb/sub/a.md",
        "---\ntitle: existing\n---\n\nkeep me\n",
    );

    let json = run_ok(
        &tmp,
        &[
            "mv",
            "--file",
            "kb/a.md",
            "--to",
            "kb/sub/a.md",
            "--on-conflict",
            "skip",
        ],
    );
    assert_eq!(json["results"]["skipped"][0], "a.md", "{json}");
    assert_eq!(json["results"]["total_files_updated"], 0, "{json}");
    assert!(tmp.path().join("kb/a.md").is_file(), "the source stays put");
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("kb/sub/a.md")).unwrap(),
        "---\ntitle: existing\n---\n\nkeep me\n",
        "the destination is untouched"
    );
}

#[test]
fn single_file_mv_without_skip_still_refuses_and_says_how_to_skip() {
    let tmp = nested_vault();
    write(
        &tmp,
        "kb/sub/a.md",
        "---\ntitle: existing\n---\n\nkeep me\n",
    );

    let (code, json, _) = run(&tmp, &["mv", "--file", "kb/a.md", "--to", "kb/sub/a.md"]);
    assert_eq!(code, 1, "{json}");
    assert_eq!(json["error"], "target file already exists", "{json}");
    assert!(
        json["hint"]
            .as_str()
            .unwrap_or_default()
            .contains("--on-conflict skip"),
        "{json}"
    );
}

#[test]
fn a_pre_existing_destination_is_not_reported_as_two_sources_clashing() {
    let tmp = nested_vault();
    write(
        &tmp,
        "kb/sub/a.md",
        "---\ntitle: existing\n---\n\nkeep me\n",
    );

    let (code, json, _) = run(&tmp, &["mv", "--glob", "a.md", "--to", "sub/", "--apply"]);
    assert_eq!(code, 1, "{json}");
    assert_eq!(
        json["error"], "destination collision: a file already exists at the destination",
        "one source colliding with an existing note is not a clash between \
         sources, and the two have different fixes: {json}"
    );
}

#[test]
fn two_sources_mapping_to_one_destination_still_says_so() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(&tmp, "one/note.md", "---\ntitle: One\n---\n\nbody\n");
    write(&tmp, "two/note.md", "---\ntitle: Two\n---\n\nbody\n");

    // iter-275 (MV-5, BUG-25): a *dry run* lists the collision instead of
    // refusing to answer; `--apply` still refuses.
    let (code, json, _) = run(&tmp, &["mv", "--glob", "**/note.md", "--to", "flat/"]);
    assert_eq!(code, 0, "{json}");
    let collisions = json["results"]["collisions"].as_array().unwrap();
    assert_eq!(collisions.len(), 2, "{json}");
    assert_eq!(collisions[0]["destination"], "flat/note.md", "{json}");
    assert_eq!(collisions[0]["source"], "one/note.md", "{json}");
    assert_eq!(collisions[1]["source"], "two/note.md", "{json}");
    assert_eq!(json["results"]["moves"].as_array().unwrap().len(), 0);

    let (code, json, _) = run(
        &tmp,
        &["mv", "--glob", "**/note.md", "--to", "flat/", "--apply"],
    );
    assert_eq!(code, 1, "{json}");
    assert_eq!(
        json["error"], "destination collision: multiple sources map to the same destination",
        "{json}"
    );
}
