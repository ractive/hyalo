//! Iteration 316 (DEC-367): search discoverability for agents. Teaching
//! hints for section mode, OR and the `"phrase"~N` form; a runnable
//! `hyalo <cmd> --help` twin on every "see X in `hyalo <cmd> --help`"
//! envelope; and the FIND 101 block that opens `find --help`.

use super::common::{hyalo, hyalo_no_hints, shell_split, write_md};
use tempfile::TempDir;

/// `n` notes that all contain "alpha beta gamma", each under a `## Tasks`
/// heading, plus one note with "solo" and one with "lonely".
fn vault(n: usize) -> TempDir {
    let tmp = TempDir::new().unwrap();
    for i in 0..n {
        write_md(
            tmp.path(),
            &format!("notes/n{i:03}.md"),
            &format!(
                "---\ntitle: Note {i}\n---\n# Note {i}\n\n## Tasks\n\n- [ ] alpha beta gamma \
                 item {i}\n\n## Other\n\ndelta text\n"
            ),
        );
    }
    write_md(tmp.path(), "solo.md", "---\ntitle: Solo\n---\nsolo here\n");
    write_md(
        tmp.path(),
        "lonely.md",
        "---\ntitle: Lonely\n---\nlonely there\n",
    );
    tmp
}

/// `(description, cmd)` of every hint of a JSON `find` run against `tmp`.
fn hints(tmp: &TempDir, args: &[&str]) -> Vec<(String, String)> {
    let output = hyalo()
        .arg("--dir")
        .arg(tmp.path())
        .args(args)
        .args(["--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{args:?}: {output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    json["hints"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| {
            (
                h["description"].as_str().unwrap().to_owned(),
                h["cmd"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

/// Run a hint's command line and return its JSON stdout; the command must
/// exit 0.
fn run_hint(cmd: &str) -> serde_json::Value {
    let argv = shell_split(cmd);
    assert_eq!(argv[0], "hyalo", "{cmd}");
    let output = hyalo_no_hints().args(&argv[1..]).output().unwrap();
    assert!(output.status.success(), "{cmd}: {output:?}");
    serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Null)
}

#[test]
fn many_file_hits_for_three_words_hint_section_mode() {
    let tmp = vault(105);
    let hints = hints(&tmp, &["find", "alpha beta gamma"]);
    let (description, cmd) = hints
        .iter()
        .find(|(_, cmd)| cmd.contains("--granularity section"))
        .unwrap_or_else(|| panic!("no section-mode hint: {hints:?}"));
    assert!(
        description.contains("the paragraph, not the file") && description.contains("105 files"),
        "{description}"
    );
    assert!(cmd.ends_with("-- 'alpha beta gamma'"), "{cmd}");
    assert!(cmd.contains("--format json"), "{cmd}");
    let json = run_hint(cmd);
    assert_eq!(json["results"][0]["section"]["heading"], "Tasks", "{json}");
}

#[test]
fn section_mode_hint_needs_three_words_and_a_hundred_files() {
    // Two words: no hint, however many files.
    let tmp = vault(105);
    let two = hints(&tmp, &["find", "alpha beta"]);
    assert!(
        !two.iter().any(|(_, c)| c.contains("--granularity")),
        "{two:?}"
    );
    // Three words, too few files: no hint.
    let small = vault(20);
    let few = hints(&small, &["find", "alpha beta gamma"]);
    assert!(
        !few.iter().any(|(_, c)| c.contains("--granularity")),
        "{few:?}"
    );
    // A flag section mode refuses: no hint (it would exit 1).
    let sorted = hints(&tmp, &["find", "alpha beta gamma", "--sort", "title"]);
    assert!(
        !sorted.iter().any(|(_, c)| c.contains("--granularity")),
        "{sorted:?}"
    );
}

#[test]
fn section_filter_matching_many_files_hints_section_mode_with_heading_words() {
    let tmp = vault(12);
    let hints = hints(&tmp, &["find", "--section", "## Tasks"]);
    let (description, cmd) = hints
        .iter()
        .find(|(_, cmd)| cmd.contains("--granularity section"))
        .unwrap_or_else(|| panic!("no section-mode hint: {hints:?}"));
    assert!(
        description.contains("12 files") && description.contains("--section scope is dropped"),
        "{description}"
    );
    assert!(cmd.ends_with("-- Tasks"), "{cmd}");
    assert!(!cmd.contains("--section"), "{cmd}");
    let json = run_hint(cmd);
    assert_eq!(json["total"], 12, "{json}");
    // A `/regex/` filter has no words to search: no hint.
    let regex = self::hints(&tmp, &["find", "--section", "/^Tasks$/"]);
    assert!(
        !regex.iter().any(|(_, c)| c.contains("--granularity")),
        "{regex:?}"
    );
}

#[test]
fn zero_result_and_of_existing_words_hints_or_and_says_why() {
    let tmp = vault(3);
    let hints = hints(&tmp, &["find", "solo lonely"]);
    let (description, cmd) = hints
        .iter()
        .find(|(d, _)| d.starts_with("Try OR instead of AND"))
        .unwrap_or_else(|| panic!("no OR hint: {hints:?}"));
    assert!(
        description.contains("every word occurs, but no file holds them all"),
        "{description}"
    );
    let json = run_hint(cmd);
    assert_eq!(json["total"], 2, "{json}");
    // One word missing: the plain OR hint, without the claim.
    let partial = self::hints(&tmp, &["find", "solo zzqqmissing"]);
    let (description, _) = partial
        .iter()
        .find(|(d, _)| d.starts_with("Try OR instead of AND"))
        .unwrap_or_else(|| panic!("no OR hint: {partial:?}"));
    assert!(!description.contains("every word occurs"), "{description}");
    // A filter may be why nothing matched: no claim either.
    let filtered = self::hints(&tmp, &["find", "solo lonely", "--tag", "zzq"]);
    assert!(
        !filtered
            .iter()
            .any(|(d, _)| d.contains("every word occurs")),
        "{filtered:?}"
    );
}

#[test]
fn many_results_hint_the_slop_phrase_form() {
    let tmp = vault(12);
    let hints = hints(&tmp, &["find", "alpha gamma"]);
    let (description, cmd) = hints
        .iter()
        .find(|(_, c)| c.contains("~5"))
        .unwrap_or_else(|| panic!("no slop-phrase hint: {hints:?}"));
    assert!(description.contains("exact phrase"), "{description}");
    let json = run_hint(cmd);
    assert_eq!(json["total"], 12, "{json}");
}

#[test]
fn help_pointer_errors_carry_a_runnable_help_hint() {
    let tmp = vault(1);
    for (args, section) in [
        (vec!["find", "(alpha"], "QUERY SYNTAX"),
        (vec!["find", "--facet", "bogus"], "FACETS"),
        (
            vec![
                "find",
                "alpha",
                "--granularity",
                "section",
                "--sort",
                "title",
            ],
            "SEARCH MODES",
        ),
    ] {
        let output = hyalo()
            .arg("--dir")
            .arg(tmp.path())
            .args(&args)
            .args(["--format", "json"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{args:?}: {output:?}");
        let json: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
        assert!(
            json["hint"]
                .as_str()
                .unwrap()
                .contains(&format!("see {section} in `hyalo find --help`")),
            "{json}"
        );
        assert_eq!(json["hints"][0]["cmd"], "hyalo find --help", "{json}");
        assert_eq!(json["hints"][0]["description"], section, "{json}");
        let help = hyalo_no_hints().args(["find", "--help"]).output().unwrap();
        assert!(help.status.success());
        assert!(
            String::from_utf8_lossy(&help.stdout).contains(&format!("{section}:"))
                || String::from_utf8_lossy(&help.stdout).contains(&format!("{section} (")),
            "{section}"
        );

        let text = hyalo()
            .arg("--dir")
            .arg(tmp.path())
            .args(&args)
            .args(["--format", "text"])
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&text.stderr);
        assert!(
            stderr.contains(&format!("  -> hyalo find --help  # {section}")),
            "{stderr}"
        );
    }
}

#[test]
fn find_help_opens_with_the_find_101_block() {
    let help = hyalo_no_hints().args(["find", "--help"]).output().unwrap();
    let help = String::from_utf8_lossy(&help.stdout);
    let lines: Vec<&str> = help.lines().collect();
    assert!(lines[2].starts_with("FIND 101 "), "{}", lines[2]);
    let block: Vec<&str> = lines[2..]
        .iter()
        .take_while(|l| !l.trim().is_empty())
        .copied()
        .collect();
    assert!(block.len() <= 10, "{block:?}");
    let joined = block.join("\n");
    for needle in [
        "implicit AND",
        "OR binds tighter",
        "\"a b\"~3",
        "config*",
        "-term excludes",
        "( ) groups",
        "--granularity section",
        "--facet",
        "hyalo terms config",
        "structure is selected with flags: --title --tag --glob --section --property",
    ] {
        assert!(joined.contains(needle), "{needle} missing from:\n{joined}");
    }
}

#[test]
fn top_level_help_names_the_grammar_and_the_terms_recipe() {
    let help = hyalo_no_hints().arg("--help").output().unwrap();
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(
        help.lines()
            .any(|l| l.contains("PATTERN is a query language")
                && l.contains("implicit AND")
                && l.contains("OR")
                && l.contains("\"phrase\"~N")
                && l.contains("prefix*")
                && l.contains("-term")
                && l.contains("groups")),
        "no grammar line in hyalo --help"
    );
    assert!(
        help.lines().any(|l| l.trim() == "hyalo terms config"),
        "no `hyalo terms config` cookbook line"
    );
}

#[test]
fn help_pointer_hints_obey_no_hints_and_jq() {
    let tmp = vault(1);
    let json_no_hints = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "(x", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(json_no_hints.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&json_no_hints.stderr).unwrap();
    assert!(
        json["hint"].as_str().unwrap().contains("QUERY SYNTAX"),
        "{json}"
    );
    assert!(
        json["hints"].as_array().is_none_or(Vec::is_empty),
        "--no-hints still printed hints: {json}"
    );

    let with_jq = hyalo()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "(x", "--jq", ".total"])
        .output()
        .unwrap();
    assert_eq!(with_jq.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&with_jq.stderr).unwrap();
    assert!(
        json["hint"].as_str().unwrap().contains("QUERY SYNTAX"),
        "{json}"
    );
    assert!(
        json["hints"].as_array().is_none_or(Vec::is_empty),
        "--jq still printed hints: {json}"
    );

    let text = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "(x", "--format", "text"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&text.stderr);
    assert!(stderr.contains("hint: "), "{stderr}");
    assert!(!stderr.contains("->"), "{stderr}");
}

#[test]
fn all_words_claim_is_decided_from_the_compiled_query() {
    let tmp = vault(3);
    write_md(
        tmp.path(),
        "words.md",
        "---\ntitle: Words\n---\nfoo bar getUserName gardening tomato\n",
    );
    for query in [
        "solo zzqx*",
        "solo foo.zzqx",
        "solo getZzqxName",
        "solo gardening-zzqx",
    ] {
        let hints = hints(&tmp, &["find", query]);
        assert!(
            !hints.iter().any(|(d, _)| d.contains("every word occurs")),
            "{query}: {hints:?}"
        );
    }
    // Compiled parts that all exist still earn the claim.
    let hints = hints(&tmp, &["find", "solo getUserName"]);
    assert!(
        hints.iter().any(|(d, _)| d.contains("every word occurs")),
        "{hints:?}"
    );
}

#[test]
fn slop_phrase_hint_is_withheld_for_prefix_or_and_negation() {
    let tmp = vault(12);
    for query in ["alp* gamma", "alpha OR gamma", "alpha gamma -delta"] {
        let hints = hints(&tmp, &["find", query]);
        assert!(
            !hints.iter().any(|(_, c)| c.contains("~5")),
            "{query}: {hints:?}"
        );
    }
    // Ranked ahead of the generic drill-downs, so it survives the budget.
    let hints = hints(&tmp, &["find", "alpha gamma"]);
    assert!(
        hints.iter().take(2).any(|(_, c)| c.contains("~5")),
        "{hints:?}"
    );
}

#[test]
fn section_hint_needs_a_significant_heading_word() {
    let tmp = TempDir::new().unwrap();
    for i in 0..12 {
        write_md(
            tmp.path(),
            &format!("n{i}.md"),
            "---\ntitle: N\n---\n## The\n\nx\n\n## 1. Go\n\ny\n",
        );
    }
    for filter in ["The", "1. Go", "1."] {
        let hints = hints(&tmp, &["find", "--section", filter]);
        assert!(
            !hints.iter().any(|(_, c)| c.contains("--granularity")),
            "{filter}: {hints:?}"
        );
    }
}
