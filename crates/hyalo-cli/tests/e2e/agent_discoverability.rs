//! Iteration 251 — the two things an agent reads first: `-h` and an empty
//! result set.
//!
//! Measured against 0.21.0, `hyalo -h` was 7.7 KB and `hyalo find -h` 12.3 KB,
//! and a query that matched nothing printed `No results` with `hints: []`.
//! Both are load-bearing for whether an agent discovers that filters compose
//! at all, so both are pinned here: byte ceilings on every `-h` page (walked
//! from the binary's own command list, so a new subcommand cannot slip past
//! the gate), and the presence and shape of the zero-result hints.

use std::fs;

use super::common::{hyalo, hyalo_no_hints, shell_split, write_md};
use tempfile::TempDir;

/// `hyalo -h` ceiling (2.5 KiB).
const TOP_SHORT_HELP_MAX: usize = 2560;

/// `hyalo <cmd> -h` ceiling (3 KiB).
const SUB_SHORT_HELP_MAX: usize = 3072;

/// Every top-level subcommand name, read out of `hyalo help` rather than
/// hard-coded, so a command added later is covered without touching this file.
fn subcommand_names() -> Vec<String> {
    let output = hyalo_no_hints().arg("help").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    // The block between the `Commands:` heading and the `Options:` that
    // follows it — `hyalo help` prints the long help, whose COOKBOOK lines are
    // also two-space indented and would otherwise be read as command names.
    let commands = stdout
        .split_once("Commands:")
        .map_or(stdout.as_str(), |(_, rest)| rest);
    let commands = commands
        .split_once("\nOptions:")
        .map_or(commands, |(block, _)| block);
    let mut names = Vec::new();
    for line in commands.lines() {
        // Command rows are indented two spaces and start with the name; the
        // wrapped continuation of a description is indented much further.
        if !line.starts_with("  ") || line.starts_with("      ") {
            continue;
        }
        let Some(first) = line.split_whitespace().next() else {
            continue;
        };
        if first == "help" || !first.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
            continue;
        }
        names.push(first.to_owned());
    }
    assert!(
        names.len() >= 20,
        "expected the full subcommand list, parsed: {names:?}"
    );
    names
}

#[test]
fn top_level_short_help_is_under_its_ceiling() {
    let tmp = TempDir::new().unwrap();
    let output = hyalo_no_hints()
        .arg("-h")
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.len() <= TOP_SHORT_HELP_MAX,
        "`hyalo -h` is {} bytes, ceiling {TOP_SHORT_HELP_MAX}",
        stdout.len()
    );
}

#[test]
fn every_subcommand_short_help_is_under_its_ceiling() {
    let tmp = TempDir::new().unwrap();
    let mut oversized: Vec<(String, usize)> = Vec::new();
    for name in subcommand_names() {
        let output = hyalo_no_hints()
            .args([name.as_str(), "-h"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        assert!(output.status.success(), "`hyalo {name} -h` failed");
        let len = output.stdout.len();
        if len > SUB_SHORT_HELP_MAX {
            oversized.push((name, len));
        }
    }
    assert!(
        oversized.is_empty(),
        "subcommand -h pages over the {SUB_SHORT_HELP_MAX}-byte ceiling: {oversized:?}"
    );
}

#[test]
fn subcommand_short_help_replaces_the_global_block_with_one_line() {
    let tmp = TempDir::new().unwrap();
    for name in subcommand_names() {
        let output = hyalo_no_hints()
            .args([name.as_str(), "-h"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(
            stdout.contains("Global: "),
            "`hyalo {name} -h` lost the global-options pointer:\n{stdout}"
        );
        // The pointer stands in for the block, so the block's own prose must
        // be gone — `--jq`'s LIMITS paragraph is the bulkiest part of it.
        assert!(
            !stdout.contains("LIMITS: a filter is given"),
            "`hyalo {name} -h` still prints the --jq limits paragraph"
        );
    }
}

#[test]
fn long_help_still_carries_the_full_global_block() {
    let tmp = TempDir::new().unwrap();
    let output = hyalo_no_hints()
        .args(["find", "--help"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("LIMITS: a filter is given"),
        "--help must keep every word the short page dropped"
    );
    assert!(
        stdout.contains("--site-prefix"),
        "--help must keep the global flags"
    );
}

#[test]
fn find_short_help_names_the_operator_sort_and_field_vocabularies() {
    let tmp = TempDir::new().unwrap();
    let output = hyalo_no_hints()
        .args(["find", "-h"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    for needle in [
        "Filters:",
        "Output:",
        "K~=/re/i",   // the property-operator line
        "dot-path",   // the dot-path note
        "backlinks",  // a --fields value
        "property:K", // a --sort key
        "EXAMPLES",
        "hyalo find --help",
    ] {
        assert!(
            stdout.contains(needle),
            "`find -h` must still name {needle:?}:\n{stdout}"
        );
    }
}

// ---------------------------------------------------------------------------
// Zero-result hints
// ---------------------------------------------------------------------------

fn vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        "---\nstatus: draft\ntitle: A\ntags: [research]\n---\n\nbody\n",
    );
    write_md(
        tmp.path(),
        "b.md",
        "---\nstatus: completed\ntitle: B\n---\n\nbody\n",
    );
    tmp
}

#[test]
fn text_mode_echoes_the_filters_that_matched_nothing() {
    let tmp = vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "find",
            "--property",
            "status=nonexistent",
            "--tag",
            "research",
            "--format",
            "text",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("No results for --property status=nonexistent --tag research"),
        "the empty state must name the query it ran:\n{stderr}"
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("-> hyalo "),
        "text mode must still render the zero-result hints:\n{stdout}"
    );
}

#[test]
fn json_mode_carries_non_empty_zero_result_hints() {
    let tmp = vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "find",
            "--property",
            "status=nonexistent",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(parsed["total"], 0);
    let hints = parsed["hints"].as_array().expect("hints array");
    assert!(
        !hints.is_empty() && hints.len() <= 3,
        "expected 1-3 zero-result hints: {parsed}"
    );
    for hint in hints {
        assert_eq!(
            hint["writes"], false,
            "zero-result hints never suggest a mutation: {hint}"
        );
    }
    assert!(
        hints.iter().any(|h| h["description"]
            .as_str()
            .is_some_and(|d| d.contains("draft"))),
        "the hints must name the values `status` really has: {parsed}"
    );
}

#[test]
fn did_you_mean_fires_for_a_one_character_typo() {
    let tmp = vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--property", "status=draf", "--format", "json"])
        .output()
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let hints = parsed["hints"].as_array().unwrap();
    let suggestion = hints
        .iter()
        .find(|h| {
            h["description"]
                .as_str()
                .is_some_and(|d| d.starts_with("Did you mean"))
        })
        .unwrap_or_else(|| panic!("no did-you-mean hint: {parsed}"));
    assert_eq!(suggestion["description"], "Did you mean status=draft?");
    assert!(
        suggestion["cmd"]
            .as_str()
            .unwrap()
            .contains("--property status=draft")
    );
}

#[test]
fn did_you_mean_stays_silent_for_an_unrelated_value() {
    let tmp = vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "find",
            "--property",
            "status=nonexistent",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let hints = parsed["hints"].as_array().unwrap();
    assert!(
        !hints.iter().any(|h| h["description"]
            .as_str()
            .is_some_and(|d| d.starts_with("Did you mean"))),
        "an unrelated value is not a typo: {parsed}"
    );
}

#[test]
fn a_second_filter_earns_a_drop_the_most_selective_hint() {
    let tmp = vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "find",
            "--property",
            "status=archived",
            "--tag",
            "research",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let hints = parsed["hints"].as_array().unwrap();
    let drop = hints
        .iter()
        .find(|h| {
            h["description"]
                .as_str()
                .is_some_and(|d| d.starts_with("Drop the most selective"))
        })
        .unwrap_or_else(|| panic!("no drop-filter hint: {parsed}"));
    let cmd = drop["cmd"].as_str().unwrap();
    assert!(
        !cmd.contains("status=archived") && cmd.contains("--tag research"),
        "the drop hint removes exactly one filter: {cmd}"
    );
}

#[test]
fn a_zero_result_hint_is_a_runnable_command() {
    let tmp = vault();
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--property", "status=draf", "--format", "json"])
        .output()
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let cmd = parsed["hints"][0]["cmd"].as_str().unwrap().to_owned();
    // Re-run the suggested command verbatim, undoing HintBuilder's own
    // single-quote escaping with `shell_split` rather than a bare
    // `split_whitespace()`: on Windows the `--dir` value is a temp path
    // containing `\`, which `shell_quote` wraps in single quotes even though
    // it has no space, and a naive whitespace split leaves the literal quote
    // characters in the token, so the path hyalo receives never exists.
    let argv: Vec<String> = shell_split(&cmd).into_iter().skip(1).collect();
    let rerun = hyalo().args(&argv).output().unwrap();
    assert!(
        rerun.status.success(),
        "hint `{cmd}` did not run: {}",
        String::from_utf8_lossy(&rerun.stderr)
    );
}

#[test]
fn an_empty_vault_still_gets_a_next_step() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("empty")).unwrap();
    let output = hyalo()
        .args(["--dir", tmp.path().join("empty").to_str().unwrap()])
        .args(["find", "--format", "json"])
        .output()
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let hints = parsed["hints"].as_array().unwrap();
    assert!(
        !hints.is_empty(),
        "even a filter-less empty query names a next step: {parsed}"
    );
}

// ---------------------------------------------------------------------------
// Iteration 254 — no short-help entry ends mid-sentence
// ---------------------------------------------------------------------------
//
// The iter-251 split moved the detail of each doc comment into a second
// paragraph. Sixteen of them were cut at a line break rather than a sentence
// boundary, so `-h` shipped lines like "…; reject writes that would". Nothing
// failed, which is exactly why it needs a test as well as the xtask gate.

/// Words a short-help line must not end on.
const DANGLING_WORDS: &[&str] = &[
    "and", "or", "by", "if", "to", "a", "the", "rather", "would", "(no",
];

/// Maximum rendered lines one short-help entry may occupy.
const MAX_SHORT_HELP_LINES: usize = 2;

/// Column at or beyond which a `-h` line is a wrapped continuation.
const CONTINUATION_INDENT: usize = 10;

/// One flag/argument entry parsed out of a rendered `-h` page: the flag
/// column, the unwrapped help column, and the rendered line count.
fn short_help_entries(help: &str) -> Vec<(String, String, usize)> {
    let starts_entry = |line: &str| {
        let indent = line.len() - line.trim_start().len();
        if !(2..CONTINUATION_INDENT).contains(&indent) {
            return false;
        }
        let rest = line.trim_start();
        rest.starts_with('-') || rest.starts_with('[') || rest.starts_with('<')
    };

    let mut entries: Vec<(String, String, usize)> = Vec::new();
    let mut current: Option<(String, Vec<String>)> = None;
    let flush = |cur: &mut Option<(String, Vec<String>)>,
                 out: &mut Vec<(String, String, usize)>| {
        if let Some((flag, parts)) = cur.take() {
            let n = parts.len();
            out.push((flag, parts.join(" "), n));
        }
    };
    for line in help.lines() {
        if starts_entry(line) {
            flush(&mut current, &mut entries);
            let trimmed = line.trim_end().trim_start();
            let (flag, help_col) = trimmed
                .split_once("  ")
                .map_or((trimmed, ""), |(f, h)| (f.trim(), h.trim()));
            let parts = if help_col.is_empty() {
                Vec::new()
            } else {
                vec![help_col.to_owned()]
            };
            current = Some((flag.to_owned(), parts));
        } else if let Some((_, parts)) = current.as_mut() {
            let indent = line.len() - line.trim_start().len();
            if line.trim().is_empty() || indent < CONTINUATION_INDENT {
                flush(&mut current, &mut entries);
            } else {
                parts.push(line.trim().to_owned());
            }
        }
    }
    flush(&mut current, &mut entries);
    entries
}

/// Drop clap's trailing `[default: …]` / `[aliases: …]` annotation, which is
/// appended after the sentence rather than being part of it.
fn without_clap_suffix(text: &str) -> &str {
    let trimmed = text.trim_end();
    let Some(open) = trimmed.rfind('[') else {
        return trimmed;
    };
    if !trimmed.ends_with(']') {
        return trimmed;
    }
    let inside = &trimmed[open + 1..trimmed.len() - 1];
    if ["default:", "aliases:", "alias:", "possible values:"]
        .iter()
        .any(|tag| inside.starts_with(tag))
    {
        trimmed[..open].trim_end()
    } else {
        trimmed
    }
}

/// Every gate-3e problem in one rendered `-h` page, as `(flag, reason)`.
fn short_help_fragments(help: &str) -> Vec<(String, String)> {
    let mut issues = Vec::new();
    for (flag, text, lines) in short_help_entries(help) {
        if text.is_empty() {
            continue;
        }
        let text = without_clap_suffix(&text);
        if text.ends_with([',', ';', ':']) {
            issues.push((flag.clone(), format!("ends mid-sentence: {text:?}")));
        } else if let Some(last) = text.split_whitespace().next_back()
            && DANGLING_WORDS.contains(&last.trim_end_matches('.').to_lowercase().as_str())
        {
            issues.push((flag.clone(), format!("dangling {last:?}: {text:?}")));
        }
        if lines > MAX_SHORT_HELP_LINES {
            issues.push((flag, format!("{lines} rendered lines: {text:?}")));
        }
    }
    issues
}

/// Every `-h` page, discovered from the binary: the root, each subcommand, and
/// each nested sub-action a subcommand's own `Commands:` block names.
fn every_short_help_page(dir: &std::path::Path) -> Vec<Vec<String>> {
    fn nested(dir: &std::path::Path, path: &[String]) -> Vec<String> {
        let mut cmd = hyalo_no_hints();
        cmd.args(path.iter().map(String::as_str));
        let out = cmd.arg("-h").current_dir(dir).output().unwrap();
        let stdout = String::from_utf8(out.stdout).unwrap();
        let Some((_, rest)) = stdout.split_once("\nCommands:\n") else {
            return Vec::new();
        };
        let block = rest.split_once("\n\n").map_or(rest, |(b, _)| b);
        block
            .lines()
            .filter(|l| l.starts_with("  ") && !l.starts_with("      "))
            .filter_map(|l| l.split_whitespace().next())
            .filter(|n| *n != "help")
            .map(str::to_owned)
            .collect()
    }

    let mut pages: Vec<Vec<String>> = vec![Vec::new()];
    for name in subcommand_names() {
        let path = vec![name];
        for sub in nested(dir, &path) {
            let mut child = path.clone();
            child.push(sub);
            pages.push(child);
        }
        pages.push(path);
    }
    pages
}

#[test]
fn no_short_help_entry_ends_mid_sentence() {
    let tmp = TempDir::new().unwrap();
    let mut failures: Vec<String> = Vec::new();
    for page in every_short_help_page(tmp.path()) {
        let mut cmd = hyalo_no_hints();
        cmd.args(page.iter().map(String::as_str));
        let out = cmd.arg("-h").current_dir(tmp.path()).output().unwrap();
        assert!(out.status.success(), "`hyalo {} -h` failed", page.join(" "));
        let stdout = String::from_utf8(out.stdout).unwrap();
        for (flag, reason) in short_help_fragments(&stdout) {
            failures.push(format!("hyalo {} -h — {flag}: {reason}", page.join(" ")));
        }
    }
    assert!(
        failures.is_empty(),
        "short help must read as whole sentences:\n{}",
        failures.join("\n")
    );
}

#[test]
fn the_fragment_guard_itself_catches_a_reintroduced_fragment() {
    // Mutation test: the guard is only worth having if it fails on the exact
    // shapes iteration 254 removed. A clean page must stay clean.
    let clean = "Options:\n  -f, --file <FILE>  Target file(s), repeatable (excludes --glob)\n";
    assert!(short_help_fragments(clean).is_empty(), "{clean}");

    let cut_at_a_semicolon =
        "Options:\n      --section <H>  Extract section(s) by substring match;\n";
    assert_eq!(short_help_fragments(cut_at_a_semicolon).len(), 1);

    let dangling_word =
        "Options:\n      --profile <P>  Scaffold a preset vault flavour (okf, madr) by\n";
    assert_eq!(short_help_fragments(dangling_word).len(), 1);

    let dangling_before_a_default = "Options:\n      --threshold <N>  Similarity for a file to be considered a [default: 0.8]\n";
    assert_eq!(short_help_fragments(dangling_before_a_default).len(), 1);

    let three_lines = "Options:\n      --wordy  one two three\n               four five six\n               seven eight nine\n";
    assert_eq!(short_help_fragments(three_lines).len(), 1);
}

// ---------------------------------------------------------------------------
// Iteration 254 — every documented example actually parses
// ---------------------------------------------------------------------------

/// The `hyalo …` lines inside every EXAMPLES / COOKBOOK block of a help page.
///
/// A block runs from its heading to the first line that is neither indented
/// nor blank. Only lines whose first word is `hyalo` are collected: a synopsis
/// row in COMMAND REFERENCE (`hyalo types remove <TYPE>`) is a grammar, not a
/// runnable command, and a shell pipeline (`git diff … | hyalo …`) is not one
/// argv.
fn example_command_lines(help: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_block = false;
    for line in help.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("EXAMPLES") || trimmed.starts_with("COOKBOOK") {
            in_block = true;
            continue;
        }
        if !in_block {
            continue;
        }
        if !line.starts_with(' ')
            && !line.starts_with('\u{00a0}')
            && !trimmed.is_empty()
            && !trimmed.starts_with("hyalo ")
            && !trimmed.starts_with('#')
        {
            in_block = false;
            continue;
        }
        if !trimmed.starts_with("hyalo ") {
            continue;
        }
        // Strip a trailing `  # explanation` comment.
        let cmd = trimmed.split_once("  #").map_or(trimmed, |(c, _)| c).trim();
        // A shell pipeline is not one argv.
        if cmd.contains(" | ") {
            continue;
        }
        // A `cmd-a && cmd-b` chain is not one argv either, but unlike a
        // pipeline both halves are whole hyalo invocations — so check each
        // one rather than dropping the example (iter-255: `new --help`
        // documents the scaffold-then-`set` chain).
        for part in cmd.split(" && ") {
            let part = part.trim();
            if !part.starts_with("hyalo ") {
                continue;
            }
            // A wrapped continuation would parse as a separate command; clap's
            // wrap column is the only thing that could split one, and no example
            // is allowed to be that long, so an unbalanced quote means the line
            // was cut and the test would be checking a fragment.
            if part.matches('\'').count() % 2 != 0 || part.matches('"').count() % 2 != 0 {
                continue;
            }
            out.push(part.to_owned());
        }
    }
    out
}

/// Split a documented example into argv, honouring both quote styles.
///
/// `common::shell_split` handles only single quotes; several examples use
/// double quotes (`--message "New export format"`), and splitting those on
/// whitespace would hand clap a fragment and fail the test for the wrong
/// reason.
fn split_argv(cmd: &str) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    for c in cmd.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => current.push(c),
            None if c == '\'' || c == '"' => {
                quote = Some(c);
                started = true;
            }
            None if c.is_whitespace() => {
                if started {
                    args.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            None => {
                current.push(c);
                started = true;
            }
        }
    }
    if started {
        args.push(current);
    }
    args
}

#[test]
fn every_documented_example_parses_without_a_clap_usage_error() {
    let pages: Vec<Vec<String>> = {
        let probe = TempDir::new().unwrap();
        every_short_help_page(probe.path())
    };

    let mut collected: Vec<(String, String)> = Vec::new();
    for page in &pages {
        let probe = TempDir::new().unwrap();
        let mut cmd = hyalo_no_hints();
        cmd.args(page.iter().map(String::as_str));
        let out = cmd
            .arg("--help")
            .current_dir(probe.path())
            .output()
            .unwrap();
        let help = String::from_utf8_lossy(&out.stdout).into_owned();
        let label = format!("hyalo {}", page.join(" "));
        for line in example_command_lines(&help) {
            collected.push((label.clone(), line));
        }
    }
    assert!(
        collected.len() >= 100,
        "expected the documented examples, collected {}",
        collected.len()
    );

    let mut failures: Vec<String> = Vec::new();
    for (page, line) in collected {
        // Each example gets a fresh vault: several of them write .hyalo.toml
        // or mutate files, and one example must not change how the next parses.
        let tmp = TempDir::new().unwrap();
        write_md(
            tmp.path(),
            "note.md",
            "---\ntitle: Note\n---\n\n# Note\n\n- [ ] a\n",
        );
        let argv: Vec<String> = split_argv(&line).into_iter().skip(1).collect();
        let out = hyalo_no_hints()
            .args(&argv)
            .current_dir(tmp.path())
            .output()
            .unwrap();
        // clap exits 2 on a usage error and prints its own `Usage:` block;
        // hyalo's own runtime errors exit 1 (a missing file, an empty vault)
        // and are fine — the example is being checked for *parseability*.
        let stderr = String::from_utf8_lossy(&out.stderr);
        if out.status.code() == Some(2) && stderr.contains("Usage: hyalo") {
            failures.push(format!(
                "{page} --help: `{line}` is not accepted by clap:\n    {}",
                stderr.lines().next().unwrap_or_default()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "documented examples that do not parse:\n{}",
        failures.join("\n")
    );
}
