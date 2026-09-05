//! Iteration 267 — the help, hint and text-output findings left over from the
//! v0.22.0 Obsidian-vault dogfood run.
//!
//! One test per finding: the stale `find --help` COMMON MISTAKES claims and
//! the NBSP indent guards (BUG-25), `summary -h` naming no result keys
//! (HELP-14), an unquoted second positional dying as `file not found` (UX-3),
//! the zero-result notice printing *after* the hint that explains it (COH-17),
//! the `hyalo index` / `types list` empty states (UX-13), the built-in
//! `links auto` common-title stop-list (UX-9, DEC-286), `new --dry-run` plus
//! the honest placeholders (UX-17, DEC-285), named files overriding
//! `[lint] ignore` (DEC-284, covered in `lint.rs`), and the `config` /
//! `lint --format github` / clap-prefix wording fixes (UX-18).

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

/// `hyalo <cmd> --help` / `-h` text.
fn help(args: &[&str]) -> String {
    let output = hyalo_no_hints()
        .args(args)
        .output()
        .expect("hyalo help should run");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "alpha.md",
        "---\ntitle: Alpha\nstatus: draft\n---\n\n# Alpha\n\nProse about dataview plugins.\n",
    );
    write_md(
        tmp.path(),
        "plain note.md",
        "---\nstatus: draft\n---\n\nNo title property and no H1 here.\n",
    );
    tmp
}

// ---------------------------------------------------------------------------
// HELP-1 — stale help text, result keys, wording (BUG-25, HELP-14, UX-18)
// ---------------------------------------------------------------------------

/// The NBSP indent guards are gone from every help page: they defeated
/// `grep`, survived copy-paste into a shell, and rendered as a stray byte in
/// any log that was not UTF-8 aware.
#[test]
fn help_pages_contain_no_non_breaking_spaces() {
    for page in [
        vec!["find", "--help"],
        vec!["--help"],
        vec!["links", "auto", "--help"],
        vec!["lint", "--help"],
    ] {
        let text = help(&page);
        assert!(
            !text.contains('\u{00a0}'),
            "{page:?} still contains a non-breaking space"
        );
    }
}

/// BUG-25: both COMMON MISTAKES claims now match the parser. `=~` is a hard
/// error (iteration 264), and `--property title~=` matches the *promoted*
/// title, not just the frontmatter property (DEC-283).
#[test]
fn find_help_common_mistakes_match_the_parser() {
    let text = help(&["find", "--help"]);
    assert!(
        text.contains("is a hard"),
        "the `=~` entry must say it is an error, not silently accepted"
    );
    assert!(
        !text.contains("--property title~= only searches frontmatter"),
        "the frontmatter-only claim about title~= is false since DEC-283"
    );
    assert!(
        text.contains("SAME promoted title"),
        "COMMON MISTAKES should say both filters read the promoted title"
    );

    // And the claim is true of the binary, not just the page.
    let tmp = vault();
    let (code, _, stderr) = run(tmp.path(), &["find", "--property", "title=~/Alpha/"]);
    assert_eq!(code, 1, "`=~` must be a user error: {stderr}");
    assert!(
        stderr.contains("use '~=' for a regex match"),
        "the error should name the right operator: {stderr}"
    );
    // `title~=` reaches a file whose title comes from the filename alone.
    let (code, stdout, stderr) = run(
        tmp.path(),
        &["find", "--property", "title~=/plain/", "--filenames-only"],
    );
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "plain note.md");
}

/// HELP-14: the short page names the JSON result keys, so a caller reaching
/// for `--jq` no longer has to run the command once to discover them.
#[test]
fn summary_help_lists_the_result_keys() {
    for page in [vec!["summary", "-h"], vec!["summary", "--help"]] {
        let text = help(&page);
        for key in [
            "files.total",
            "files.skipped",
            "files.excluded",
            "links.broken",
            "links.broken_anchors",
            "orphans",
            "properties",
            "tags",
        ] {
            assert!(text.contains(key), "{page:?} should name `{key}`");
        }
        assert!(
            text.contains("--jq '.results.links.broken'"),
            "{page:?} should carry one worked --jq example"
        );
    }
}

/// UX-18: `config` reports the format this run actually resolved to, and the
/// effective `hints` boolean, instead of `null` for both.
#[test]
fn config_reports_effective_format_and_hints() {
    let tmp = vault();
    let (code, stdout, stderr) = run(
        tmp.path(),
        &["config", "--format", "json", "--jq", ".results"],
    );
    assert_eq!(code, 0, "stderr: {stderr}");
    let results: serde_json::Value = serde_json::from_str(&stdout).expect("jq output is JSON");
    assert!(
        results["hints"].is_boolean(),
        "results.hints must be the effective boolean: {results}"
    );
    assert!(
        results["format"].is_string(),
        "results.format must be the effective format: {results}"
    );
    assert_eq!(
        results["format_source"].as_str(),
        Some("flag"),
        "an explicit --format is the source here: {results}"
    );
    // Nothing pinned in .hyalo.toml, so the raw configured value stays null.
    assert!(results["format_configured"].is_null(), "{results}");

    // Text mode says the same thing in one line.
    let (_, text, _) = run(tmp.path(), &["config", "--format", "text"]);
    assert!(
        text.contains("format: text (--format)"),
        "text report should name the effective format and its source:\n{text}"
    );
}

/// UX-18: the `--format github` summary carries the same `files_checked`
/// denominator `--format text` prints, so "0 errors, 0 warnings" can no
/// longer read the same for a clean run and a run that linted nothing.
#[test]
fn lint_github_summary_counts_files_checked() {
    let tmp = vault();
    let (code, stdout, stderr) = run(tmp.path(), &["lint", "--format", "github"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let summary = stdout
        .lines()
        .last()
        .expect("github output ends with a summary line");
    assert!(
        summary.contains("of 2 files checked"),
        "summary should name the denominator, got: {summary}"
    );
}

/// UX-18: hyalo's own text errors use the lowercase `error:` prefix clap and
/// anyhow already use, so one session's scrollback does not mix both spellings.
#[test]
fn text_errors_use_a_lowercase_error_prefix() {
    let tmp = vault();
    let (code, _, stderr) = run(tmp.path(), &["read", "missing.md", "--format", "text"]);
    assert_eq!(code, 1);
    assert!(
        stderr.starts_with("error:"),
        "expected a lowercase prefix, got: {stderr}"
    );
    // clap's own errors already agree.
    let (_, _, clap_stderr) = run(tmp.path(), &["find", "--definitely-not-a-flag"]);
    assert!(
        clap_stderr.starts_with("error:"),
        "clap prefix drifted: {clap_stderr}"
    );
}

// ---------------------------------------------------------------------------
// HINT-1 — second positional, stream order, empty states (UX-3, UX-13, COH-17)
// ---------------------------------------------------------------------------

/// UX-3: `hyalo find dataview plugin` is an unquoted two-word body search.
/// clap gives the second word to FILE, and the run used to die with a bare
/// `file not found: plugin`.
#[test]
fn unquoted_second_positional_suggests_quoting_the_query() {
    let tmp = vault();
    let (code, _, stderr) = run(tmp.path(), &["find", "dataview", "plugin"]);
    // iter-274 (UX-1, DEC-307): hyalo's own did-you-mean is a user error, so it
    // exits 1 like `--sort nope` — 2 is reserved for clap usage and internal
    // errors.
    assert_eq!(code, 1, "a hyalo-own user error exits 1: {stderr}");
    assert!(
        stderr.contains("'plugin' is not a file"),
        "the message should name the offending word: {stderr}"
    );
    assert!(
        stderr.contains("hyalo find 'dataview plugin'"),
        "the message should hand back the quoted command: {stderr}"
    );
    assert!(
        !stderr.contains("file not found"),
        "the generic not-found envelope must not be what a quoting slip gets: {stderr}"
    );
}

/// A real file target still behaves like a file target, and a `.md` path is
/// never mistaken for a stray query word.
#[test]
fn a_real_second_positional_is_still_a_file_target() {
    let tmp = vault();
    let (code, stdout, stderr) = run(
        tmp.path(),
        &["find", "prose", "alpha.md", "--filenames-only"],
    );
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "alpha.md");

    // A missing `.md` path keeps the file-not-found envelope: it is a path,
    // not a quoting accident.
    let (code, _, stderr) = run(tmp.path(), &["find", "prose", "nope.md"]);
    assert_eq!(code, 1, "stderr: {stderr}");
    assert!(
        stderr.contains("file not found"),
        "a genuine missing path keeps its own error: {stderr}"
    );
}

/// The reverse direction: a PATTERN that is itself an existing file is a body
/// search for that literal text. The results are legitimate, so this is a
/// hint rather than an error.
#[test]
fn a_pattern_naming_a_file_hints_at_the_file_flag() {
    let tmp = vault();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["find", "alpha.md", "--format", "json"])
        .env_remove("HYALO_NO_HINTS")
        .args(["--hints"])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let hints = json["hints"].as_array().expect("hints array");
    assert!(
        hints.iter().any(|h| h["cmd"]
            .as_str()
            .is_some_and(|c| c.starts_with("hyalo find --file alpha.md"))),
        "expected a --file hint, got: {hints:?}"
    );
}

/// COH-17: the zero-result notice goes to stderr and the hints to stdout, so
/// emission order is what a `2>&1` reader sees. The reason must come first.
#[test]
fn zero_result_notice_precedes_the_hints() {
    let tmp = vault();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "find",
            "--property",
            "status=nonexistent",
            "--format",
            "text",
            "--hints",
        ])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stderr.contains("No results for --property status=nonexistent"),
        "the notice should echo the filters: {stderr}"
    );
    // The hint block must not open with blank lines any more (UX-13): with no
    // body to separate from, the first stdout line IS a hint.
    assert!(
        stdout.starts_with("  ->") || stdout.starts_with("  =>"),
        "hints should not be preceded by blank lines: {stdout:?}"
    );
}

/// UX-13: `hyalo index` is the name people reach for first; clap's nearest
/// match by edit distance was `find`, which has nothing to do with snapshots.
#[test]
fn index_did_you_mean_points_at_create_index() {
    let tmp = vault();
    let (code, _, stderr) = run(tmp.path(), &["index"]);
    assert_eq!(code, 2);
    let mentions = stderr
        .lines()
        .filter(|l| l.contains("create-index"))
        .count();
    assert_eq!(
        mentions, 1,
        "exactly one line should name create-index: {stderr}"
    );
    assert!(
        stderr.contains("--index"),
        "the hint should also mention how reads opt in: {stderr}"
    );
}

/// UX-13: an unconfigured vault is not a failed query, and its empty state
/// says so — with no blank line between the notice and the hint.
#[test]
fn types_list_empty_state_names_the_missing_configuration() {
    let tmp = TempDir::new().unwrap();
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["types", "list", "--format", "text", "--hints"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("No types configured"),
        "expected the types-specific empty state, got: {stderr}"
    );
    assert!(
        !stderr.contains("No results"),
        "the generic sentence should be gone: {stderr}"
    );
    assert!(
        !stdout.starts_with('\n'),
        "no blank line before the hint: {stdout:?}"
    );
}

/// UX-18: `types remove note` used to answer `type 'note' not found` while
/// `hyalo lint` was busy reporting schema errors for files whose frontmatter
/// said `type: note` — a flat contradiction from the user's side. The error
/// now separates the two and names what actually changes the outcome.
#[test]
fn types_remove_explains_an_undeclared_type_in_use() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(
        tmp.path().join(".hyalo.toml"),
        "dir = \".\"\n[schema.default]\nrequired = [\"title\", \"date\"]\n",
    )
    .unwrap();
    write_md(tmp.path(), "n.md", "---\ntype: note\n---\n\n# N\n");

    // Precondition: lint does complain about the file, so the user has every
    // reason to think the type exists.
    let (_, lint_stdout, _) = run(tmp.path(), &["lint", "--format", "text"]);
    assert!(
        lint_stdout.contains("n.md"),
        "precondition: lint should report the file: {lint_stdout}"
    );

    let (code, _, stderr) = run(tmp.path(), &["types", "remove", "note"]);
    assert_eq!(code, 1);
    assert!(
        stderr.contains("no [schema.types.note] block in .hyalo.toml"),
        "the error should say what is actually missing, got: {stderr}"
    );
    assert!(
        stderr.contains("[schema.default]") && stderr.contains("types set note"),
        "the hint should name where lint's errors come from and how to declare the type, \
         got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// TITLE-1 — filename stem fallback (UX-5, DEC-283)
// ---------------------------------------------------------------------------

/// A vault whose notes carry neither a `title` property nor an H1 used to
/// print `title: (none)` for every file. The stem is what Obsidian shows.
#[test]
fn text_output_no_longer_reports_a_missing_title() {
    let tmp = vault();
    let (code, stdout, stderr) = run(tmp.path(), &["find", "--format", "text"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        !stdout.contains("title: (none)"),
        "no file should report a missing title:\n{stdout}"
    );
    assert!(
        stdout.contains("title: plain note"),
        "the stem should be shown for the property-less file:\n{stdout}"
    );
}

/// `--sort title` orders by the promoted value, so stem-titled files sort
/// among the others instead of collecting in one null bucket.
#[test]
fn sort_title_uses_the_promoted_value() {
    let tmp = vault();
    let (code, stdout, stderr) = run(tmp.path(), &["find", "--sort", "title", "--filenames-only"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let order: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        order,
        vec!["alpha.md", "plain note.md"],
        "Alpha before `plain note` (its stem), not nulls-last"
    );
}

// ---------------------------------------------------------------------------
// TEXT-1 — links auto stop-list (UX-9, DEC-286)
// ---------------------------------------------------------------------------

/// The plan's fixture: notes titled `github`, `links`, `Markdown` and
/// `Dataview`, all mentioned once in prose. Only `Dataview` survives — the
/// three platform/word titles are held back and named in the report.
#[test]
fn links_auto_stop_list_holds_back_common_word_titles() {
    let tmp = TempDir::new().unwrap();
    for title in ["github", "links", "Markdown", "Dataview"] {
        write_md(
            tmp.path(),
            &format!("{title}.md"),
            &format!("---\ntitle: {title}\n---\n\n# {title}\n"),
        );
    }
    write_md(
        tmp.path(),
        "prose.md",
        "---\ntitle: Prose\n---\n\n# Prose\n\nSee github and links and Markdown and Dataview.\n",
    );

    let (code, stdout, stderr) = run(
        tmp.path(),
        &["links", "auto", "--dry-run", "--format", "json"],
    );
    assert_eq!(code, 0, "stderr: {stderr}");
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let results = &json["results"];

    let matched: Vec<&str> = results["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["matched_text"].as_str().unwrap())
        .collect();
    assert_eq!(
        matched,
        vec!["Dataview"],
        "only the domain noun should be proposed: {results}"
    );

    let excluded: Vec<&str> = results["default_excluded_titles"]
        .as_array()
        .expect("default_excluded_titles is always present")
        .iter()
        .map(|t| t.as_str().unwrap())
        .collect();
    for held in ["github", "links", "markdown"] {
        assert!(
            excluded.contains(&held),
            "`{held}` should be named as held back: {excluded:?}"
        );
    }
    assert!(
        !excluded.contains(&"dataview"),
        "the proposed title must not also be listed as excluded: {excluded:?}"
    );
    assert_eq!(
        results["default_excluded_mentions"].as_u64(),
        Some(3),
        "three mentions were held back: {results}"
    );

    // The opt-out restores every candidate.
    let (_, stdout, _) = run(
        tmp.path(),
        &[
            "links",
            "auto",
            "--dry-run",
            "--format",
            "json",
            "--no-warn-common-titles",
        ],
    );
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["results"]["matched"].as_u64(), Some(4));
    assert_eq!(
        json["results"]["default_excluded_titles"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

/// A configured `[links.auto] exclude_titles` replaces the built-in list
/// rather than composing with it — the user's judgment wins outright.
#[test]
fn configured_exclude_titles_replace_the_built_in_stop_list() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(
        tmp.path().join(".hyalo.toml"),
        "dir = \".\"\n[links.auto]\nexclude_titles = [\"Dataview\"]\n",
    )
    .unwrap();
    for title in ["github", "Dataview"] {
        write_md(
            tmp.path(),
            &format!("{title}.md"),
            &format!("---\ntitle: {title}\n---\n\n# {title}\n"),
        );
    }
    write_md(
        tmp.path(),
        "prose.md",
        "---\ntitle: Prose\n---\n\n# Prose\n\nSee github and Dataview.\n",
    );

    let (code, stdout, stderr) = run(
        tmp.path(),
        &["links", "auto", "--dry-run", "--format", "json"],
    );
    assert_eq!(code, 0, "stderr: {stderr}");
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let matched: Vec<&str> = json["results"]["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["matched_text"].as_str().unwrap())
        .collect();
    assert_eq!(
        matched,
        vec!["github"],
        "the config excludes Dataview and re-admits github: {}",
        json["results"]
    );
    assert_eq!(
        json["results"]["default_excluded_titles"]
            .as_array()
            .unwrap()
            .len(),
        0,
        "the built-in list steps aside entirely: {}",
        json["results"]
    );
}

// ---------------------------------------------------------------------------
// NEW-1 — `new --dry-run` and honest placeholders (UX-17, DEC-285)
// ---------------------------------------------------------------------------

fn scaffold_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    std::fs::write(
        tmp.path().join(".hyalo.toml"),
        "dir = \".\"\n\
         [schema.types.thing]\n\
         required = [\"title\", \"type\", \"rating\"]\n\
         [schema.types.thing.properties.rating]\n\
         type = \"number\"\n",
    )
    .unwrap();
    tmp
}

/// `--dry-run` returns the scaffold and writes nothing — not the file, and
/// not its parent directory.
#[test]
fn new_dry_run_writes_nothing_and_returns_the_scaffold() {
    let tmp = scaffold_vault();
    let (code, stdout, stderr) = run(
        tmp.path(),
        &[
            "new",
            "--type",
            "thing",
            "--file",
            "notes/x.md",
            "--dry-run",
            "--format",
            "json",
        ],
    );
    assert_eq!(code, 0, "stderr: {stderr}");
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let results = &json["results"];
    assert_eq!(results["dry_run"].as_bool(), Some(true), "{results}");
    assert_eq!(results["created"].as_bool(), Some(false), "{results}");
    assert!(
        results["content"]
            .as_str()
            .is_some_and(|c| c.starts_with("---\ntype: thing")),
        "the preview should carry the scaffold: {results}"
    );
    assert!(
        !tmp.path().join("notes/x.md").exists(),
        "--dry-run must not create the file"
    );
    assert!(
        !tmp.path().join("notes").exists(),
        "--dry-run must not create the parent directory either"
    );

    // Text mode says which it was.
    let (_, text, _) = run(
        tmp.path(),
        &[
            "new",
            "--type",
            "thing",
            "--file",
            "notes/x.md",
            "--dry-run",
            "--format",
            "text",
        ],
    );
    assert!(
        text.starts_with("[dry-run] would create notes/x.md"),
        "got: {text}"
    );
}

/// DEC-285: a required number with no schema default is scaffolded EMPTY, and
/// `lint` reports it — instead of a plausible `0` that lint would accept.
#[test]
fn required_number_scaffolds_empty_and_lint_reports_it() {
    let tmp = scaffold_vault();
    let (code, _, stderr) = run(tmp.path(), &["new", "--type", "thing", "--file", "x.md"]);
    assert_eq!(code, 0, "stderr: {stderr}");

    let content = std::fs::read_to_string(tmp.path().join("x.md")).unwrap();
    assert!(
        content.lines().any(|l| l.trim_end() == "rating:"),
        "rating should be empty, not 0:\n{content}"
    );
    assert!(
        !content.contains("rating: 0"),
        "a fabricated 0 is what DEC-285 removed:\n{content}"
    );
    assert!(
        content.contains("title: TBD"),
        "a required string keeps its TBD placeholder:\n{content}"
    );

    let (_, stdout, _) = run(
        tmp.path(),
        &["lint", "--file", "x.md", "--format", "text", "--detailed"],
    );
    assert!(
        stdout.contains("\"rating\" must not be empty"),
        "lint should name the un-filled field:\n{stdout}"
    );
}
