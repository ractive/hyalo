//! Ranked-search query language (DEC-333): OR precedence, parentheses,
//! prefix terms, multi-language stemming and did-you-mean. There are no
//! field terms (DEC-366): `name:value` is plain text.

use super::common::{hyalo, hyalo_no_hints, write_md};
use tempfile::TempDir;

fn vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "notes/rust-async.md",
        "---\ntitle: Rust async\ntags: [rust, project/async]\n---\n# Rust async\n\nrust async runtime\n\n## Install steps\n\nconfiguration here\n",
    );
    write_md(
        tmp.path(),
        "notes/rust-tokio.md",
        "---\ntitle: Tokio notes\ntags: [rust]\n---\nrust tokio executor\n",
    );
    write_md(
        tmp.path(),
        "notes/tokio-only.md",
        "---\ntitle: Tokio alone\n---\ntokio only here\n",
    );
    write_md(
        tmp.path(),
        "iterations/iteration-01-links.md",
        "---\ntitle: Iteration 01 links\ntags: [iteration]\n---\nlinking and links everywhere\n",
    );
    write_md(
        tmp.path(),
        "de/haus.md",
        "---\ntitle: Haus\nlanguage: de\n---\nDie Häuser stehen am See.\n",
    );
    tmp
}

fn run(tmp: &TempDir, args: &[&str]) -> (serde_json::Value, std::process::Output) {
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(args)
        .args(["--format", "json"])
        .output()
        .unwrap();
    let json = serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Null);
    (json, output)
}

fn files(tmp: &TempDir, query: &str, extra: &[&str]) -> Vec<String> {
    let mut args = vec!["find", query];
    args.extend_from_slice(extra);
    let (json, output) = run(tmp, &args);
    assert!(output.status.success(), "{query}: {output:?}");
    let mut out: Vec<String> = json["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["file"].as_str().unwrap().to_owned())
        .collect();
    out.sort();
    out
}

#[test]
fn or_binds_tighter_than_and() {
    let tmp = vault();
    // a AND (b OR c): the old global OR returned every file mentioning any term.
    assert_eq!(
        files(&tmp, "rust async OR tokio", &[]),
        vec!["notes/rust-async.md", "notes/rust-tokio.md"]
    );
    assert_eq!(
        files(&tmp, "rust AND tokio", &[]),
        vec!["notes/rust-tokio.md"]
    );
}

#[test]
fn parentheses_group_and_negate() {
    let tmp = vault();
    assert_eq!(
        files(&tmp, "(rust OR tokio) -async", &[]),
        vec!["notes/rust-tokio.md", "notes/tokio-only.md"]
    );
    assert_eq!(
        files(&tmp, "tokio -(rust OR executor)", &[]),
        vec!["notes/tokio-only.md"]
    );
    assert_eq!(
        files(&tmp, "((rust OR tokio) (executor OR only))", &[]),
        vec!["notes/rust-tokio.md", "notes/tokio-only.md"]
    );
    // A parenthesis inside a word is literal.
    let got = files(&tmp, "main()", &[]);
    assert!(got.is_empty(), "expected empty, got {got:?}");
}

#[test]
fn malformed_queries_exit_1_with_json_envelope() {
    let tmp = vault();
    for query in ["(rust", "rust)", "()", "*", "rust (tokio OR"] {
        let (_, output) = run(&tmp, &["find", query]);
        assert_eq!(output.status.code(), Some(1), "{query}: {output:?}");
        let json: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
        let error = json["error"].as_str().unwrap();
        assert!(
            error.starts_with("invalid search query"),
            "{query}: {error}"
        );
        assert!(json["hint"].as_str().unwrap().contains("QUERY SYNTAX"));
    }
}

/// UX-6 / DEC-358: malformed-but-recoverable query input warns (`-q`-proof)
/// instead of being silently reinterpreted into a plausible-looking but
/// wrong answer.
#[test]
fn malformed_query_input_warns_even_with_quiet() {
    let tmp = vault();
    let cases: &[(&str, &str)] = &[
        ("rust OR", "dropped"),
        ("OR rust", "dropped"),
        ("rust \"tok", "never closed"),
        ("*rust", "literal character"),
        ("ru*t", "literal character"),
        (r#""rust async"~99"#, "clamped to 64"),
    ];
    for (query, needle) in cases {
        let output = hyalo_no_hints()
            .arg("--dir")
            .arg(tmp.path())
            .args(["-q", "find", "--format", "json", "--", query])
            .output()
            .unwrap();
        assert!(output.status.success(), "{query}: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(needle), "{query}: stderr={stderr}");
    }
}

/// Review fix (SHOULD-FIX 7): a query made entirely of operator keywords
/// (`find OR`) used to print BOTH the generic dangling-operator warning and
/// the more specific "was interpreted as a boolean operator" one -- the two
/// describe the exact same cause and must collapse to one.
#[test]
fn operator_only_query_warns_exactly_once() {
    let tmp = vault();
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "--format", "text", "--", "OR"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.matches("warning:").count(),
        1,
        "exactly one warning, not the dangling-operator one too: {stderr}"
    );
    assert!(
        stderr.contains("was interpreted as a boolean operator"),
        "{stderr}"
    );
    assert!(!stderr.contains("dropped"), "{stderr}");

    // A dangling OR that is NOT the whole query still warns -- this is not
    // the operator-only case, there is a real word to search for.
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "--format", "text", "--", "rust OR"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("dropped"), "{stderr}");
}

/// The companion error case: `"a b"~abc` is not a word trailing a phrase,
/// it is a malformed slop, and must be rejected rather than silently
/// reinterpreted.
#[test]
fn malformed_phrase_slop_exits_1() {
    let tmp = vault();
    let (_, output) = run(&tmp, &["find", r#""rust async"~abc"#]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .starts_with("invalid search query"),
        "{json}"
    );
}

#[test]
fn prefix_terms_expand_against_stems() {
    let tmp = vault();
    assert_eq!(
        files(&tmp, "tok*", &[]),
        vec!["notes/rust-tokio.md", "notes/tokio-only.md"]
    );
    // `configuration` is stemmed to `configur`: the prefix matches stems.
    assert_eq!(files(&tmp, "config*", &[]), vec!["notes/rust-async.md"]);
    let (json, _) = run(&tmp, &["find", "config*"]);
    let text = json["results"][0]["matches"][0]["text"].as_str().unwrap();
    assert_eq!(text, "configuration here");
}

#[test]
fn prefix_expansion_cap_warns_even_with_quiet() {
    let tmp = TempDir::new().unwrap();
    let words: Vec<String> = (0..300).map(|i| format!("zeta{i:03}")).collect();
    write_md(tmp.path(), "big.md", &format!("{}\n", words.join(" ")));
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["-q", "find", "zeta*", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("prefix 'zeta*' matches 300 terms"),
        "{stderr}"
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 1);
}

/// DEC-366: `title:`, `heading:`, `tag:` and `path:` are not field terms.
/// `title:tokio` is the two plain words `title` and `tokio` (no body here
/// says "title"), exactly like `foo:bar`; structure is selected with flags.
#[test]
fn name_value_tokens_are_plain_words_not_field_terms() {
    let tmp = vault();
    for query in [
        "title:tokio",
        "heading:install",
        "tag:project",
        "path:notes/",
        "foo:bar",
    ] {
        let got = files(&tmp, query, &[]);
        assert!(got.is_empty(), "{query}: expected no hits, got {got:?}");
    }
    // The flags are the way to select structure.
    assert_eq!(
        files(&tmp, "tokio", &["--title", "tokio"]),
        vec!["notes/rust-tokio.md", "notes/tokio-only.md"]
    );
    assert_eq!(
        files(
            &tmp,
            "link*",
            &["--title", "iteration", "--tag", "iteration"]
        ),
        vec!["iterations/iteration-01-links.md"]
    );
    // A word that does occur matches through a `name:value` token.
    write_md(
        tmp.path(),
        "notes/colon.md",
        "---\ntitle: Colon\n---\nthe page title tokio sits here\n",
    );
    assert_eq!(files(&tmp, "title:tokio", &[]), vec!["notes/colon.md"]);
    // A parenthesis glued to the word is literal, so the closing `)` of
    // `title:(rust OR tokio)` is unbalanced -- an ordinary syntax error.
    let (_, output) = run(&tmp, &["find", "title:(rust OR tokio)"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .starts_with("invalid search query"),
        "{json}"
    );
}

#[test]
fn german_note_found_by_german_inflection_with_english_default() {
    let tmp = vault();
    for extra in [&[][..], &["--index"][..]] {
        if !extra.is_empty() {
            let index = hyalo_no_hints()
                .arg("--dir")
                .arg(tmp.path())
                .arg("create-index")
                .output()
                .unwrap();
            assert!(index.status.success(), "{index:?}");
        }
        assert_eq!(
            files(&tmp, "Häusern", extra),
            vec!["de/haus.md"],
            "{extra:?}"
        );
        let (json, _) = run(&tmp, &[&["find", "Häusern"][..], extra].concat());
        assert_eq!(
            json["results"][0]["matches"][0]["text"],
            "Die Häuser stehen am See."
        );
    }
}

#[test]
fn zero_results_suggest_close_terms() {
    let tmp = vault();
    let (json, output) = run(&tmp, &["find", "toklo"]);
    assert!(output.status.success());
    assert_eq!(json["total"], 0);
    assert_eq!(
        json["suggestions"],
        serde_json::json!([{"term": "toklo", "candidates": [{"term": "tokio", "docs": 2}]}])
    );

    // Hints: corrected query and a terms listing.
    let output = hyalo()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "toklo", "--format", "json"])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let cmds: Vec<&str> = json["hints"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["cmd"].as_str().unwrap())
        .collect();
    assert!(
        cmds.iter()
            .any(|c| c.starts_with("hyalo find") && c.ends_with("-- tokio")),
        "{cmds:?}"
    );
    assert!(
        cmds.iter().any(|c| c.starts_with("hyalo terms tok")),
        "{cmds:?}"
    );

    // Text mode names the candidates in the zero-result notice.
    let output = hyalo()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "toklo", "--format", "text"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("'toklo' occurs in no document; closest indexed stems: tokio (2 docs)"),
        "{stderr}"
    );

    // Results present, or nothing close: no `suggestions` key.
    let (json, _) = run(&tmp, &["find", "tokio"]);
    assert!(json.get("suggestions").is_none());
    let (json, _) = run(&tmp, &["find", "qqqqxxxx"]);
    assert!(json.get("suggestions").is_none());
}

#[test]
fn empty_negated_phrases_negate_nothing_and_never_overflow() {
    let tmp = vault();
    let expected = files(&tmp, "tokio", &[]);
    for query in [
        "-\"\" tokio".to_owned(),
        "-\"\" -\"\" tokio".to_owned(),
        // A few hundred repetitions prove the semantics end to end while
        // staying far under Windows' 32 767-character command line (each `"`
        // is escaped there). Depth is covered in-process by the core unit
        // tests `empty_negated_phrase_negates_nothing` (30 000 repeats) and
        // `consecutive_negations_are_bounded_and_collapse`.
        format!("{} tokio", "-\"\"".repeat(300)),
    ] {
        let output = hyalo_no_hints()
            .arg("--dir")
            .arg(tmp.path())
            .args(["find", "--format", "json", "--", &query])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0), "{}", query.len());
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let mut got: Vec<String> = json["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["file"].as_str().unwrap().to_owned())
            .collect();
        got.sort();
        assert_eq!(got, expected);
    }
}

#[test]
fn corrected_query_hint_survives_a_leading_dash() {
    let tmp = vault();
    let output = hyalo()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "--format", "json", "--", "-executor toklo"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let hint = json["hints"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["cmd"].as_str().unwrap())
        .find(|c| c.starts_with("hyalo find"))
        .expect("corrected-query hint");
    assert!(hint.ends_with("-- '-executor tokio'"), "{hint}");
    // The hint runs as written.
    let argv = super::common::shell_split(hint);
    let rerun = hyalo_no_hints().args(&argv[1..]).output().unwrap();
    assert!(rerun.status.success(), "{rerun:?}");
    let json: serde_json::Value = serde_json::from_slice(&rerun.stdout).unwrap();
    assert_eq!(json["results"][0]["file"], "notes/tokio-only.md");
}

#[test]
fn whole_word_prefix_uses_its_stem() {
    let tmp = vault();
    assert_eq!(
        files(&tmp, "configuration*", &[]),
        vec!["notes/rust-async.md"]
    );
}

// ---------------------------------------------------------------------------
// BUG-5 / DEC-356 (review fix): dash-swallowed PATTERN hint, keyed on the
// real argv shape rather than on "no PATTERN" alone.
// ---------------------------------------------------------------------------

fn dash_hint_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    // `find --section` unions every matching heading *within a file*
    // (DEC-333) -- "ambiguous" means one file with 2+ matches, not two
    // files with one match each. `a.md` has two headings containing
    // "config" (ambiguous for `-sconfig`) but only one containing "section"
    // (a clean single match for `-ssection`); `b.md` is unrelated noise.
    write_md(
        tmp.path(),
        "a.md",
        "# A\n\n## Config section\n\ntext\n\n## Config setup\n\nmore text\n",
    );
    write_md(tmp.path(), "b.md", "# B\n\n## Other\n\ntext\n");
    tmp
}

/// `hyalo find -sconfig`: clap reads `-s` + "config" as `--section config`,
/// matching two headings in the same file (`Config section`, `Config
/// setup`) -- the ambiguous case already warned about, now also naming the
/// dash fix.
#[test]
fn dash_swallowed_short_section_flag_ambiguous_match_gets_the_hint() {
    let tmp = dash_hint_vault();
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "-sconfig"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("matched more than one heading")
            && stderr.contains("hyalo find -- '-term'"),
        "{stderr}"
    );
}

/// The common case the dash-swallow bug actually produces: exactly one
/// heading matches (`-ssection` -> `--section section`, only `Config
/// section` contains "section"), which never set `ambiguous_section_files`
/// and so used to return silently with no warning at all.
#[test]
fn dash_swallowed_short_section_flag_single_match_gets_the_hint() {
    let tmp = dash_hint_vault();
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "-ssection"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no PATTERN was given") && stderr.contains("hyalo find -- '-term'"),
        "{stderr}"
    );
}

/// `hyalo find -tag:iteration`: clap reads `-t` + "ag:iteration" as `--tag
/// 'ag:iteration'`, an invalid tag name (contains ':') -- the resulting
/// error names the dash fix.
#[test]
fn dash_swallowed_short_tag_flag_gets_the_hint() {
    let tmp = dash_hint_vault();
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "-tag:iteration"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("invalid character ':'"),
        "{json}"
    );
    assert_eq!(
        json["hint"].as_str().unwrap(),
        "to search for a term starting with '-', write `hyalo find -- '-term'`"
    );
}

/// A deliberate long-form `--section config` with no PATTERN is not the
/// dash-swallow bug -- clap read exactly what was typed -- so its ambiguous
/// multi-heading warning must stay plain.
#[test]
fn explicit_long_form_section_with_no_pattern_does_not_get_the_hint() {
    let tmp = dash_hint_vault();
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "--section", "config"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("matched more than one heading"), "{stderr}");
    assert!(
        !stderr.contains("hyalo find -- '-term'"),
        "a deliberate --section Task must not get the dash hint: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// UX-1 / DEC-357 (review fix): corrected_query fixes every suggested term,
// not just the first, and the hint description names all of them.
// ---------------------------------------------------------------------------

#[test]
fn corrected_query_hint_fixes_every_misspelled_term() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "ostrich kangaroo\n");
    write_md(tmp.path(), "b.md", "ostrich only\n");
    write_md(tmp.path(), "c.md", "kangaroo only\n");
    let output = hyalo()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "ostrch kangroo", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let cmds: Vec<&str> = json["hints"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["cmd"].as_str().unwrap())
        .collect();
    assert!(
        cmds.iter().any(|c| c.ends_with("-- 'ostrich kangaroo'")),
        "{cmds:?}"
    );

    // Running the hinted, fully-corrected query actually returns a result.
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "--format", "json", "--", "ostrich kangaroo"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 1);
    assert_eq!(json["results"][0]["file"], "a.md");
}

/// UX-1 / DEC-357 (review fix, SHOULD-FIX 5): "Try OR instead of AND" must
/// not be offered when every word in the query has zero postings --
/// `search_suggestions` alone undercounts this, because `suggest()` drops a
/// word silently when it has no close dictionary candidate either.
#[test]
fn try_or_hint_is_withheld_when_no_word_has_any_postings() {
    let tmp = vault();
    let output = hyalo()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "qqqzzz wwwxxx", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 0);
    let descriptions: Vec<&str> = json["hints"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["description"].as_str().unwrap())
        .collect();
    assert!(
        !descriptions
            .iter()
            .any(|d| d.contains("Try OR instead of AND")),
        "neither word has any postings at all, OR cannot help: {descriptions:?}"
    );

    // Sanity check the opposite: when one word does have postings, the
    // hint is still offered.
    let output = hyalo()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "rust qqqzzz", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let descriptions: Vec<&str> = json["hints"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["description"].as_str().unwrap())
        .collect();
    assert!(
        descriptions
            .iter()
            .any(|d| d.contains("Try OR instead of AND")),
        "{descriptions:?}"
    );
}

/// The invalid-query envelope's hint names a `find --help` section; that
/// header must exist (check-help-drift 3g guards every such hint).
#[test]
fn invalid_query_hint_names_a_real_help_section() {
    let tmp = vault();
    let (_, output) = run(&tmp, &["find", "(rust"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    let hint = json["hint"].as_str().unwrap();
    let section = hint
        .split("see ")
        .nth(1)
        .and_then(|rest| rest.split(" in `hyalo find --help`").next())
        .unwrap_or_else(|| panic!("no section named in {hint}"));
    let help = hyalo_no_hints().args(["find", "--help"]).output().unwrap();
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(
        help.lines().any(|line| line
            .trim_start()
            .strip_prefix(section)
            .is_some_and(|r| r.starts_with(':') || r.starts_with(" ("))),
        "find --help has no '{section}' header"
    );
}

/// DEC-366 migration: a zero-result query holding a `title:x`-shaped word
/// says field terms are gone and hints the flag, carrying the rest of the
/// query.
#[test]
fn zero_result_field_shaped_word_hints_the_flag() {
    let tmp = vault();
    let output = hyalo()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "rust tag:zzqmissing", "--format", "text"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        text.contains("'tag:zzqmissing' is not a field term")
            && text.contains("--tag 'zzqmissing'"),
        "{text}"
    );
    let output = hyalo()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "rust title:zzqmissing", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let hints = json["hints"].as_array().unwrap();
    let hint = hints
        .iter()
        .find(|h| {
            h["description"]
                .as_str()
                .unwrap()
                .contains("Field terms are not part of the search grammar")
        })
        .unwrap_or_else(|| panic!("{hints:?}"));
    let cmd = hint["cmd"].as_str().unwrap();
    assert!(
        cmd.starts_with("hyalo find --title zzqmissing") && cmd.ends_with("-- rust"),
        "{cmd}"
    );
    // No "Try OR" rewrite of the field-shaped word.
    assert!(
        !hints
            .iter()
            .any(|h| h["description"].as_str().unwrap().contains("Try OR")),
        "{hints:?}"
    );
}
