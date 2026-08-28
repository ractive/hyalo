//! Mechanical guard for the hand-maintained COMMAND REFERENCE and COOKBOOK
//! blocks in `hyalo --help` (iter-246, F-7).
//!
//! `help.rs` holds a hand-written string template, so it can (and did — F-1,
//! F-2, F-4) drift from the real clap surface: a synopsis names a flag the
//! subcommand does not have (`summary [--limit N]`) or omits the real
//! invocation form (`changelog add <CATEGORY> <TEXT>` for a flags-only
//! subcommand). This test extracts every `hyalo ...` synopsis from the COMMAND
//! REFERENCE section of the *actual* `hyalo --help` output, compiles it into
//! concrete argv (placeholders → benign values, synopsis-only notation
//! stripped), and runs each against a throwaway vault. Every executable
//! `hyalo ...` line in COOKBOOK is run the same way.
//!
//! It asserts parse-level acceptance only — a command "passes" when the CLI
//! did not reject it as a parse error (`unexpected argument`, `required
//! arguments`, `invalid value`). Runtime user errors (`file not found`, an
//! unknown rule id, dry-run drift exiting non-zero) are fine: they prove the
//! command was *understood*, which is exactly what the reference must not get
//! wrong.
//!
//! Robustness notes:
//! - clap wraps long help lines at the terminal width, so synopsis entries
//!   span physical lines unpredictably. Entries are re-joined with the rules
//!   in [`accumulate_entry`] (see its docs).
//! - Lines that embed shell/jq pipes (`hyalo ... | xargs ...`, jq filters
//!   containing `|`) are skipped in COOKBOOK: they are shell, not hyalo
//!   grammar, and the pipe characters live inside quoted jq strings that a
//!   line-level parser cannot reliably strip.

use super::common::{hyalo_no_hints, shell_split};
use std::process::Command as StdCommand;
use tempfile::TempDir;

/// Run `hyalo --help` once and return its full stdout.
fn long_help() -> String {
    let output = hyalo_no_hints().arg("--help").output().unwrap();
    assert!(
        output.status.success(),
        "hyalo --help failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// Cut a section (from its `HEADING:` marker to the next section marker).
fn section<'a>(help: &'a str, start: &str, end: &str) -> &'a str {
    let from = help
        .find(start)
        .unwrap_or_else(|| panic!("--help output missing section marker {start:?}"));
    let rest = &help[from..];
    match rest.find(end) {
        Some(to) => &rest[..to],
        None => rest,
    }
}

// ---------------------------------------------------------------------------
// Synopsis extraction
// ---------------------------------------------------------------------------

/// Subcommand and verb words that legitimately appear in a synopsis. A token
/// outside this vocabulary that isn't a flag/placeholder ends the synopsis
/// (it is trailing prose like "Flag form; ...").
const SUBCOMMAND_WORDS: &[&str] = &[
    "find",
    "read",
    "set",
    "remove",
    "append",
    "properties",
    "tags",
    "summary",
    "task",
    "toggle",
    "backlinks",
    "links",
    "fix",
    "auto",
    "mv",
    "views",
    "list",
    "lint",
    "lint-rules",
    "show",
    "types",
    "new",
    "madr",
    "toc",
    "changelog",
    "add",
    "release",
    "okf",
    "index",
    "log",
    "config",
    "init",
    "deinit",
    "create-index",
    "drop-index",
    "completions",
    "rename",
    // Prose lines that are part of the reference block but not commands
    // (`--line accepts comma-separated lists; ...`) start with `--`, which
    // is covered by the flag branch — no vocab entry needed.
];

/// Is `tok` a token that can legitimately appear in a synopsis (as opposed to
/// trailing prose like `Flag form; ...`)?  Deliberately permissive — a token
/// that *looks* structural keeps the line; prose ends it.
fn is_synopsis_token(tok: &str) -> bool {
    if tok == "|" {
        return true;
    }
    // Strip synopsis brackets, then classify the inner text.
    let bare = tok.trim_start_matches('[').trim_end_matches(']');
    if bare.starts_with('-') || tok.starts_with('<') {
        return true;
    }
    if bare == "..." || tok.ends_with("...") {
        return true;
    }
    // Inline alternatives enumerations (`okf|madr|skills|changelog]`) are
    // synopsis notation, not prose.
    if bare.contains('|') {
        return true;
    }
    // A wrapped `]` fragment (`STATUS]`, `FILTER ...]`) is synopsis material.
    if bare.is_empty() {
        return true;
    }
    if bare.ends_with("...") {
        return true;
    }
    if is_placeholder(bare) {
        return true;
    }
    SUBCOMMAND_WORDS.contains(&bare)
}

/// `K=V`, `K,K`, `N`, `F`, `SHELL`, `K|K=V`, `DIR/` — all-caps synopsis
/// placeholders (alternatives and path separators allowed inside).
fn is_placeholder(tok: &str) -> bool {
    let bare = tok.trim_end_matches('/');
    !bare.is_empty()
        && bare
            .chars()
            .all(|c| c.is_ascii_uppercase() || c == '=' || c == ',' || c == '|' || c == '_')
        && bare.chars().any(|c| c.is_ascii_uppercase())
}

/// Concrete benign value for a synopsis placeholder. Values are chosen so the
/// instantiated command reaches (or fails *past*) clap parsing:
/// numeric/enum-ish placeholders get parser-valid values; everything else is
/// an arbitrary token.
fn placeholder_value(tok: &str) -> String {
    match tok {
        "COLS" => "80".to_owned(),
        "DATE" => "2026-01-01".to_owned(),
        "VERSION" => "1.2.3".to_owned(),
        "SHELL" => "bash".to_owned(),
        "PROFILE" => "okf".to_owned(),
        "POLICY" => "skip".to_owned(),
        "RANGE" => "1:10".to_owned(),
        "K=V" => "status=x".to_owned(),
        "CAT" => "Added".to_owned(),
        // `N` serves numeric counts/limits; `T` serves `--threshold T`
        // (0.0..=1.0) and `-t/--tag T` (any string).
        "N" | "T" => "1".to_owned(),
        // `F` serves both `--file F` (any path parses) and `--min-confidence
        // F` (threshold parser demands 0.0..=1.0).
        "F" => "0.9".to_owned(),
        "BOOL" => "true".to_owned(),
        "RULE_ID" => "MD013".to_owned(),
        _ => "x".to_owned(),
    }
}

/// Compile one COMMAND REFERENCE synopsis line into runnable argv.
fn synopsis_to_argv(entry: &str) -> Vec<String> {
    // Strip trailing hand-written comments (`# bash, zsh, fish, ...`).
    let line = entry.split_once(" # ").map_or(entry, |(l, _)| l);

    let tokens: Vec<&str> = line.split_whitespace().collect();
    // Drop trailing prose: everything from the first non-synopsis token on.
    let end = tokens[2..]
        .iter()
        .position(|t| !is_synopsis_token(t))
        .map_or(tokens.len(), |i| i + 2);
    let tokens = &tokens[..end];

    let mut argv: Vec<String> = Vec::new();
    // Raw tokens of the bracket group currently being collected (`[a | b]`,
    // `[--flag P ...]`, `[...]`). A group is compiled and flushed when its
    // `]` closes it. `pipe_at` records where a `|` alternatives separator
    // sits so the flush can keep only the first (complete) alternative;
    // `prose` marks word-only groups (`[find filters...]`) — synopsis prose,
    // not argv.
    let mut group: Option<(Vec<String>, Option<usize>, bool)> = None;

    let flush = |argv: &mut Vec<String>, group: &mut Option<(Vec<String>, Option<usize>, bool)>| {
        let Some((group_toks, pipe_at, prose)) = group.take() else {
            return;
        };
        // `[find filters...]`-style word-only groups name a concept, not args.
        if prose {
            return;
        }
        // Compile each raw token now that the group's shape is known.
        let mut compiled: Vec<String> = Vec::new();
        for tok in group_toks {
            if tok == "..." || tok == "|" {
                continue;
            }
            // `-f/--file` shorthand: emit the long form so the value that
            // follows pairs with it.
            let tok = match tok.strip_prefix('-') {
                Some(rest) => match rest.strip_prefix(|c: char| c.is_ascii_alphabetic()) {
                    Some(body) if body.starts_with("/--") => format!("--{}", &body[3..]),
                    _ => tok.clone(),
                },
                None => tok.clone(),
            };
            // Inline alternatives (`okf|madr|skills|changelog`) resolve to
            // their first alternative.
            let tok = tok.split('|').next().unwrap_or(&tok).to_owned();
            let had_slash = tok.ends_with('/');
            let stem = tok.trim_end_matches('/').trim_matches(['<', '>']);
            if is_placeholder(stem) {
                compiled.push(format!(
                    "{}{}",
                    placeholder_value(stem),
                    if had_slash { "/" } else { "" }
                ));
            } else if !tok.is_empty() {
                compiled.push(tok);
            }
        }
        match pipe_at {
            // `[a | b]` alternatives group: keep only the first alternative,
            // complete with its value(s) (the second is usually its negation
            // and would conflict with it).
            Some(pipe) => argv.extend(compiled.into_iter().take(pipe)),
            None => argv.extend(compiled),
        }
    };

    for tok in tokens {
        let in_group = group.is_some();
        let opens = tok.starts_with('[') && !tok.ends_with(']');
        let closes = tok.ends_with(']');

        let clean = tok.trim_start_matches('[').trim_end_matches(']');

        if clean == "..." || clean == "|" {
            if clean == "|"
                && let Some((group_toks, pipe_at, _)) = group.as_mut()
            {
                *pipe_at = Some(group_toks.len());
            }
            continue;
        }

        let (compiled, prose) = {
            // `-f/--file` shorthand outside a bracket group: pick the long
            // form (same rule as `flush` applies inside groups) so a
            // following value pairs with it.
            let shorthand_resolved = match clean.strip_prefix('-') {
                Some(rest) => match rest.strip_prefix(|c: char| c.is_ascii_alphabetic()) {
                    Some(body) if body.starts_with("/--") => format!("--{}", &body[3..]),
                    _ => clean.to_owned(),
                },
                None => clean.to_owned(),
            };
            let first_alt = shorthand_resolved
                .split('|')
                .next()
                .unwrap_or(&shorthand_resolved);
            let stem = first_alt.trim_end_matches('/').trim_matches(['<', '>']);
            if is_placeholder(stem) {
                (
                    format!(
                        "{}{}",
                        placeholder_value(stem),
                        if first_alt.ends_with('/') { "/" } else { "" }
                    ),
                    false,
                )
            } else if !first_alt.is_empty() {
                (first_alt.to_owned(), !structural_word(first_alt))
            } else {
                continue;
            }
        };

        if opens {
            if in_group {
                flush(&mut argv, &mut group);
            }
            group = Some((vec![compiled], None, prose));
        } else if in_group {
            let g = group.as_mut().unwrap();
            g.0.push(compiled);
            g.2 |= prose;
            if closes {
                flush(&mut argv, &mut group);
            }
        } else {
            argv.push(compiled);
        }
    }
    // Defensive: flush a group left open by an unbalanced synopsis.
    flush(&mut argv, &mut group);

    // The synopsis's own `[-d/--dir DIR]` would fight the runner's injected
    // `--dir` — drop any `--dir VALUE` pair from the compiled argv.
    let mut out = Vec::with_capacity(argv.len());
    let mut skip = false;
    for tok in argv {
        if skip {
            skip = false;
            continue;
        }
        if tok == "--dir" || tok == "-d" {
            skip = true;
            continue;
        }
        out.push(tok);
    }
    out
}

/// A bare word that names an argument concept rather than a flag/value.
/// Subcommand words are structural; lowercase prose words are not.
fn structural_word(tok: &str) -> bool {
    tok.starts_with('-') || is_placeholder(tok) || tok == "..."
}
// ---------------------------------------------------------------------------
// Guards
// ---------------------------------------------------------------------------

/// Parse errors that mean "the reference names a command the CLI does not
/// accept". Runtime user errors are fine (see module docs).
fn clap_rejection(stderr: &str) -> Option<String> {
    for line in stderr.lines() {
        let t = line.trim_start();
        if t.starts_with("error:")
            && (t.contains("unexpected argument")
                || t.contains("required arguments")
                || t.contains("invalid value")
                || t.contains("cannot be used multiple times")
                || t.contains("unrecognized subcommand"))
        {
            return Some(t.to_owned());
        }
    }
    None
}

/// Run one compiled argv against a fresh vault; return the clap rejection if
/// any (see [`clap_rejection`]).
fn run_against_fresh_vault(argv: &[String]) -> Result<(), String> {
    let tmp = TempDir::new().unwrap();
    let mut cmd = StdCommand::new(env!("CARGO_BIN_EXE_hyalo"));
    cmd.arg("--dir")
        .arg(tmp.path())
        .arg("--format")
        .arg("json")
        .stdin(std::process::Stdio::null())
        .args(argv);
    let output = cmd.output().unwrap();
    clap_rejection(&String::from_utf8_lossy(&output.stderr)).map_or(Ok(()), Err)
}

/// Append a physical help line to the synopsis currently being accumulated.
///
/// clap wraps long help lines at the terminal width (breaking at spaces), so
/// a synopsis may span several physical lines whose wrap points are not under
/// our control. A line belongs to the current entry when:
/// - it starts with `[` — a bracket group was pushed to the next line, or
/// - the accumulated entry has an unbalanced `[` — the wrap split a group
///   (e.g. `[--task` / `STATUS]`), or
/// - the entry ends with a `-x/--long` shorthand whose value was wrapped to
///   the next line.
///
/// Description prose (`Flag form; ...`, `Unique property names, ...`) never
/// satisfies any rule: the entries' descriptions are balanced and don't start
/// with `[`, and the shorthand rule only fires directly after a flag token.
fn accumulate_entry(entry: &mut String, line: &str) {
    let trimmed = line.trim();
    let unbalanced = entry.matches('[').count() > entry.matches(']').count();
    let dangling_flag = entry
        .split_whitespace()
        .last()
        .is_some_and(|last| last.starts_with('-') && last.contains("/--"));
    if trimmed.starts_with('[') || unbalanced || dangling_flag {
        entry.push(' ');
        entry.push_str(trimmed);
    }
}

/// Every `hyalo ...` synopsis in COMMAND REFERENCE (wrapped lines joined) must
/// compile to argv the CLI accepts.
#[test]
fn command_reference_synopses_parse() {
    let help = long_help();
    let reference = section(&help, "COMMAND REFERENCE:", "COOKBOOK:");

    let mut entries: Vec<String> = Vec::new();
    for line in reference.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("hyalo ") {
            entries.push(trimmed.to_owned());
        } else if let Some(cur) = entries.last_mut() {
            accumulate_entry(cur, line);
        }
    }
    assert!(
        entries.len() >= 30,
        "only {} `hyalo ...` entries extracted from COMMAND REFERENCE — \
         the extraction heuristic broke",
        entries.len()
    );

    let mut failures = Vec::new();
    for entry in &entries {
        let argv = synopsis_to_argv(entry);
        // The compiled argv carries the leading `hyalo` word for display;
        // strip it before invoking the binary.
        if let Err(rejection) = run_against_fresh_vault(&argv[1..]) {
            failures.push(format!(
                "  {entry}\n    compiled: {argv:?}\n    rejected: {rejection}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} COMMAND REFERENCE synopses do not parse:\n{}",
        failures.len(),
        entries.len(),
        failures.join("\n")
    );
}

/// Every executable `hyalo ...` line in COOKBOOK must run (pipes skipped —
/// see module docs).
#[test]
fn cookbook_lines_parse() {
    let help = long_help();
    let cookbook = section(&help, "COOKBOOK:", "OUTPUT SHAPES");

    let mut checked = 0;
    let mut failures = Vec::new();
    for line in cookbook.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("hyalo ") || trimmed.contains('|') {
            continue;
        }
        checked += 1;
        // Cookbook lines are shell command lines: quoted args (jq filters,
        // globs with spaces) must survive as single argv tokens.
        let argv: Vec<String> = shell_split(trimmed)
            .into_iter()
            .skip(1) // drop the leading `hyalo` word
            .collect();
        if let Err(rejection) = run_against_fresh_vault(&argv) {
            failures.push(format!("  {trimmed}\n    rejected: {rejection}"));
        }
    }
    assert!(
        checked >= 30,
        "only {checked} cookbook lines checked — extraction heuristic broke"
    );
    assert!(
        failures.is_empty(),
        "{} cookbook lines do not parse:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Pin the specific drift findings this guard exists for (F-1, F-2, F-4), so
/// a regression in the extraction heuristics above cannot silently un-cover
/// them.
#[test]
fn known_drift_findings_stay_fixed() {
    let help = long_help();

    // F-1: summary has no --limit.
    assert!(
        !help.contains("summary [-g/--glob G] [-n/--recent N] [--depth N] [--limit N]"),
        "summary synopsis must not advertise a phantom --limit"
    );
    // F-2: changelog add is flags-only, not positional.
    assert!(
        !help.contains("hyalo changelog add <CATEGORY> <TEXT>"),
        "changelog add synopsis must show --category/--message, not positionals"
    );
    // F-3: okf log takes --message, not a bare TEXT positional.
    assert!(
        !help.contains("hyalo okf log <TEXT>"),
        "okf log synopsis must show --message"
    );
    // F-4: links fix names its gating flags.
    let fix_block = section(&help, "hyalo links fix", "Persist the exclusions");
    for flag in [
        "--apply-fuzzy",
        "--min-confidence",
        "--case-insensitive",
        "--expand-short-form",
    ] {
        assert!(
            fix_block.contains(flag),
            "links fix synopsis missing {flag}"
        );
    }
    // And the real invocation works end-to-end (AC).
    let tmp = TempDir::new().unwrap();
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args([
            "changelog",
            "add",
            "--category",
            "Added",
            "--message",
            "x",
            "--dry-run",
        ])
        .output()
        .unwrap();
    assert!(
        clap_rejection(&String::from_utf8_lossy(&output.stderr)).is_none(),
        "changelog add --category/--message rejected"
    );
}

/// `hyalo summary --limit N` must still be rejected (F-1 regression check on
/// the CLI surface itself, not just the prose).
#[test]
fn summary_still_has_no_limit_flag() {
    let tmp = TempDir::new().unwrap();
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["summary", "--limit", "5"])
        .output()
        .unwrap();
    assert!(!output.status.success(), "summary must reject --limit");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unexpected argument"),
        "expected an unexpected-argument error for summary --limit"
    );
}

// ---------------------------------------------------------------------------
// Unit tests for the extraction helpers themselves
// ---------------------------------------------------------------------------

#[test]
fn compiles_shorthand_flags_from_bracketed_groups() {
    let argv = synopsis_to_argv("hyalo summary [-g/--glob G] [-n/--recent N] [--depth N]");
    assert_eq!(
        argv,
        vec![
            "hyalo", "summary", "--glob", "x", "--recent", "1", "--depth", "1"
        ]
    );
}

#[test]
fn compiles_alternatives_group_to_first_alternative() {
    let argv =
        synopsis_to_argv("hyalo links auto [--first-only | --no-first-only] [--min-length N]");
    // The first alternative survives complete; the negation is dropped
    // (conflicts with the first and both parse identically alone).
    assert_eq!(
        argv,
        vec![
            "hyalo",
            "links",
            "auto",
            "--first-only",
            "--min-length",
            "1"
        ]
    );
}

#[test]
fn compiles_repeatable_group_with_sample_value() {
    let argv =
        synopsis_to_argv("hyalo set  -p/--property K=V [-p ...] [-t/--tag T ...] [--dry-run]");
    // Repeat markers emit the flag once (a second occurrence would just
    // repeat the same parse); a placeholder-only sample value is dropped.
    assert_eq!(
        argv,
        vec![
            "hyalo",
            "set",
            "--property",
            "status=x",
            "-p",
            "--tag",
            "1",
            "--dry-run"
        ]
    );
}

#[test]
fn compiles_bare_positionals_and_keeps_subcommand_words() {
    let argv = synopsis_to_argv(
        "hyalo task set        -f/--file F -l/--line N -s/--status C           Set a custom",
    );
    assert_eq!(
        argv,
        vec![
            "hyalo", "task", "set", "--file", "0.9", "--line", "1", "--status", "x"
        ]
    );
}

#[test]
fn joins_wrapped_group_split_across_lines() {
    // Reproduces the real wrapping of the find synopsis (`[--task` / `STATUS]`).
    let mut entry =
        "hyalo find [PATTERN | -e/--regexp REGEX] [-p/--property K=V ...] [-t/--tag T ...] [--task"
            .to_owned();
    accumulate_entry(&mut entry, "    STATUS]");
    assert!(
        entry.ends_with("[--task STATUS]"),
        "wrapped fragment must rejoin: {entry}"
    );
    let argv = synopsis_to_argv(&entry);
    assert!(argv.contains(&"--task".to_owned()));
    assert!(argv.contains(&"x".to_owned()));
}

#[test]
fn does_not_join_description_prose() {
    let mut entry =
        "hyalo properties summary [-g/--glob G] [-n/--limit N]         Unique property names, types, and"
            .to_owned();
    accumulate_entry(&mut entry, "    file counts (read-only) [alias: list]");
    assert!(
        !entry.contains("alias"),
        "description prose must not be joined: {entry}"
    );
}
