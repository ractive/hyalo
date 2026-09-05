//! Gate 3 — Help-text drift check.
//!
//! 3a. Every subcommand listed in the SUBCOMMANDS constant must have an
//!     `EXAMPLES:` section in its `--help` output containing at least 2
//!     example lines.
//!
//! 3b. No `--help` output for any listed command may contain any phrase in
//!     `crates/xtask/stale-help-patterns.toml`.
//!
//! 3c. (iter-251) Short help stays short: `hyalo -h` under
//!     [`TOP_SHORT_HELP_MAX`] bytes and every `hyalo <cmd> -h` under
//!     [`SUB_SHORT_HELP_MAX`]. `-h` is the page an agent reads first, and it
//!     regressed to 7.7 KB (12.3 KB for `find`) purely by accretion — one
//!     unsplit doc comment at a time, plus the global-options block repeated
//!     on all 27 subcommands. A ceiling is the only thing that keeps it short.
//!
//! 3d. (iter-251) Every subcommand's `-h` ends with the one-line global-options
//!     pointer instead of reprinting the block.
//!
//! 3e. (iter-254) No short-help entry on any `-h` page ends mid-sentence, and
//!     none spans more than two rendered lines. The iter-251 split moved the
//!     detail of each doc comment into a second paragraph, but sixteen of them
//!     were cut at a line break rather than at a sentence boundary, so `-h`
//!     shipped lines like "…; reject writes that would" and "Scaffold a preset
//!     vault flavour (okf, madr, skills, changelog) by". Nothing failed, which
//!     is exactly why it needs a gate.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::process::Command;

use crate::workspace::workspace_root;

/// Commands (and nested sub-actions) whose `--help` must have an EXAMPLES block.
///
/// Each entry is the argv slice passed after `hyalo` (mirrors examples_contract.rs).
const SUBCOMMANDS: &[&[&str]] = &[
    &["find"],
    &["read"],
    &["set"],
    &["remove"],
    &["append"],
    &["summary"],
    &["backlinks"],
    &["task"],
    &["properties"],
    &["tags"],
    &["links"],
    &["views"],
    &["init"],
    &["create-index"],
    &["lint-rules"],
    &["types"],
    &["lint"],
    &["mv"],
    &["new"],
    &["task", "read"],
    &["task", "toggle"],
    &["task", "set"],
    // iter-192: commands that reached the COMMAND REFERENCE but had never been
    // held to the EXAMPLES / stale-wording bar.
    &["changelog"],
    &["config"],
    &["madr"],
    &["okf"],
];

/// Commands scanned for stale wording (3b) on top of [`SUBCOMMANDS`].
///
/// The empty argv is `hyalo --help` itself. It carries the OUTPUT paragraph,
/// the global-flags block, and the OUTPUT SHAPES note — the three places
/// iter-192 found contradicting the binary — but has no `EXAMPLES:` header of
/// its own (its examples live in the `-h` short help), so it is excluded from
/// the 3a check rather than allowlisted out of it.
const STALE_ONLY_COMMANDS: &[&[&str]] = &[&[]];

/// Commands allowed to skip the EXAMPLES requirement (no-op / meta commands).
const EXAMPLES_ALLOWLIST: &[&str] = &["help", "completions"];

/// Byte ceiling for `hyalo -h` (2.5 KiB — iter-251 acceptance criterion).
const TOP_SHORT_HELP_MAX: usize = 2560;

/// Byte ceiling for every `hyalo <cmd> -h` (3 KiB — iter-251 acceptance
/// criterion). `find` is the one that lives closest to it.
const SUB_SHORT_HELP_MAX: usize = 3072;

/// The literal every subcommand's `-h` must carry in place of the global
/// options block. Kept in sync with `cli::help::global_pointer` by this gate.
const GLOBAL_POINTER_PREFIX: &str = "Global: ";

#[derive(Debug, Deserialize)]
pub struct StalePatternFile {
    #[serde(default)]
    pub patterns: Vec<StalePattern>,
}

#[derive(Debug, Deserialize)]
pub struct StalePattern {
    pub pattern: String,
    pub reason: String,
}

fn load_stale_patterns(workspace_root: &std::path::Path) -> Result<Vec<StalePattern>> {
    let path = workspace_root
        .join("crates")
        .join("xtask")
        .join("stale-help-patterns.toml");
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("reading stale patterns at {path:?}"))?;
    let file: StalePatternFile =
        toml::from_str(&content).with_context(|| "parsing stale-help-patterns.toml")?;
    Ok(file.patterns)
}

/// Get the `--help` output for a given argv (the args passed after `hyalo`).
fn help_text(workspace_root: &std::path::Path, argv: &[&str]) -> Option<String> {
    let mut args = vec!["run", "-q", "-p", "hyalo-cli", "--"];
    args.extend_from_slice(argv);
    args.push("--help");

    let out = Command::new("cargo")
        .args(&args)
        .current_dir(workspace_root)
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if stdout.trim().is_empty() {
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        if stderr.trim().is_empty() {
            None
        } else {
            Some(stderr)
        }
    } else {
        Some(stdout)
    }
}

/// Count example lines in a help text.
///
/// A line is counted as an example when, after trimming, it:
/// - starts with `hyalo ` or `$ hyalo`, OR
/// - is inside a fenced code block and contains `hyalo `.
pub fn count_examples(help: &str) -> usize {
    let mut count = 0;
    let mut in_fence = false;
    for line in help.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if trimmed.starts_with("hyalo ")
            || trimmed.starts_with("$ hyalo")
            || (in_fence && trimmed.contains("hyalo "))
        {
            count += 1;
        }
    }
    count
}

/// Get the `-h` (short help) output for a given argv.
fn short_help_text(workspace_root: &std::path::Path, argv: &[&str]) -> Option<String> {
    let mut args = vec!["run", "-q", "-p", "hyalo-cli", "--"];
    args.extend_from_slice(argv);
    args.push("-h");

    let out = Command::new("cargo")
        .args(&args)
        .current_dir(workspace_root)
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if stdout.trim().is_empty() {
        None
    } else {
        Some(stdout)
    }
}

/// 3c + 3d: short help stays short, and stands in one line for the globals.
fn check_short_help(root: &std::path::Path) -> Vec<String> {
    let mut failures = Vec::new();

    match short_help_text(root, &[]) {
        Some(help) => {
            if help.len() > TOP_SHORT_HELP_MAX {
                failures.push(format!(
                    "Help drift (3c): 'hyalo -h' is {} bytes; ceiling is {TOP_SHORT_HELP_MAX}. \
                     Move detail into the second paragraph of the doc comment (clap long_help) \
                     rather than growing the short page.",
                    help.len()
                ));
            }
            if !help.contains("COMMANDS") {
                failures.push(
                    "Help drift (3c): 'hyalo -h' no longer renders the grouped COMMANDS block."
                        .to_owned(),
                );
            }
        }
        None => failures.push("Help drift (3c): could not get 'hyalo -h' output".to_owned()),
    }

    for argv in SUBCOMMANDS {
        let cmd_label = argv.join(" ");
        let Some(help) = short_help_text(root, argv) else {
            failures.push(format!(
                "Help drift (3c): could not get help output for 'hyalo {cmd_label} -h'"
            ));
            continue;
        };
        if help.len() > SUB_SHORT_HELP_MAX {
            failures.push(format!(
                "Help drift (3c): 'hyalo {cmd_label} -h' is {} bytes; ceiling is \
                 {SUB_SHORT_HELP_MAX}.",
                help.len()
            ));
        }
        if !help.contains(GLOBAL_POINTER_PREFIX) {
            failures.push(format!(
                "Help drift (3d): 'hyalo {cmd_label} -h' is missing the \
                 \"{GLOBAL_POINTER_PREFIX}…\" pointer line that stands in for the global \
                 options block."
            ));
        }
    }
    failures
}

/// 3a: Check EXAMPLES blocks.
fn check_examples(root: &std::path::Path) -> Vec<String> {
    let mut failures = Vec::new();
    for argv in SUBCOMMANDS {
        // Skip allowlisted commands.
        if argv.iter().any(|a| EXAMPLES_ALLOWLIST.contains(a)) {
            continue;
        }

        let cmd_label = argv.join(" ");
        let Some(help) = help_text(root, argv) else {
            failures.push(format!(
                "Help drift (3a): could not get help output for 'hyalo {cmd_label} --help'"
            ));
            continue;
        };

        if !help.contains("EXAMPLES:") && !help.contains("Examples:") {
            failures.push(format!(
                "Help drift (3a): 'hyalo {cmd_label} --help' has no EXAMPLES block."
            ));
            continue;
        }

        let n = count_examples(&help);
        if n < 2 {
            failures.push(format!(
                "Help drift (3a): 'hyalo {cmd_label} --help' EXAMPLES block has {n} example(s); need at least 2."
            ));
        }
    }
    failures
}

/// Doc-comment indentation that leaked into a rendered `--help` body.
///
/// Gate 3f (iter-274, UX-7). A `#[command(long_about = "…")]` written as one
/// long single-line literal has to spell its newlines `\n` — and every one of
/// the `types` subcommands had the source file's own 12-space continuation
/// indentation baked in after each of them, so `hyalo types set --help`
/// rendered a wall of text indented 12 columns under an unindented first
/// paragraph. Nothing failed; it just looked broken. The healthy spelling is
/// clap's line-continuation form (`\n\` at end of source line), where Rust
/// eats the indentation.
///
/// Only the prose body is scanned — everything before clap's `Usage:` line —
/// because clap legitimately indents the `Arguments:` and `Options:` blocks it
/// renders below it.
const MAX_HELP_BODY_INDENT: usize = 7;

/// 3f: no rendered `--help` body line carries leaked source indentation.
fn check_help_body_indentation(root: &std::path::Path) -> Vec<String> {
    let mut failures = Vec::new();
    for argv in SHORT_HELP_PAGES {
        let cmd_label = if argv.is_empty() {
            "hyalo".to_owned()
        } else {
            format!("hyalo {}", argv.join(" "))
        };
        let Some(help) = help_text(root, argv) else {
            continue;
        };
        for line in help.lines() {
            if line.starts_with("Usage:") {
                break;
            }
            let indent = line.len() - line.trim_start_matches(' ').len();
            if indent > MAX_HELP_BODY_INDENT && !line.trim().is_empty() {
                failures.push(format!(
                    "Help drift (3f): '{cmd_label} --help' body line is indented {indent} \
                     columns — doc-comment indentation leaked into the rendered page. Use the \
                     `\\n\\` line-continuation form in the long_about literal.\n    {}",
                    line.trim_end()
                ));
                break;
            }
        }
    }
    failures
}

/// 3b: Check for stale wording patterns.
fn check_stale_patterns(root: &std::path::Path, patterns: &[StalePattern]) -> Vec<String> {
    if patterns.is_empty() {
        return Vec::new();
    }

    let mut failures = Vec::new();
    for argv in SUBCOMMANDS.iter().chain(STALE_ONLY_COMMANDS.iter()) {
        let cmd_label = argv.join(" ");
        let Some(help) = help_text(root, argv) else {
            continue;
        };

        // Collapse whitespace so a pattern still matches when clap wraps the
        // help at a column that happens to fall inside the phrase.
        let help_lower = help
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        for sp in patterns {
            if help_lower.contains(sp.pattern.to_lowercase().as_str()) {
                failures.push(format!(
                    "Help drift (3b): 'hyalo {cmd_label} --help' contains stale phrase \"{}\". Reason: {}",
                    sp.pattern, sp.reason
                ));
            }
        }
    }
    failures
}

/// Every `-h` page gate 3e walks — the root, every subcommand, and every
/// nested sub-action.
///
/// Deliberately separate from [`SUBCOMMANDS`]: a nested action like
/// `links fix` is not held to the EXAMPLES bar (its parent's `--help` carries
/// them) but its short help still has to read as sentences.
const SHORT_HELP_PAGES: &[&[&str]] = &[
    &[],
    &["find"],
    &["read"],
    &["properties"],
    &["properties", "summary"],
    &["properties", "rename"],
    &["tags"],
    &["tags", "summary"],
    &["tags", "rename"],
    &["task"],
    &["task", "read"],
    &["task", "toggle"],
    &["task", "set"],
    &["summary"],
    &["backlinks"],
    &["mv"],
    &["set"],
    &["remove"],
    &["append"],
    &["init"],
    &["deinit"],
    &["create-index"],
    &["drop-index"],
    &["views"],
    &["views", "list"],
    &["views", "set"],
    &["views", "remove"],
    &["views", "run"],
    &["links"],
    &["links", "fix"],
    &["links", "auto"],
    &["lint"],
    &["lint-rules"],
    &["lint-rules", "list"],
    &["lint-rules", "show"],
    &["lint-rules", "set"],
    &["lint-rules", "remove"],
    &["types"],
    &["types", "list"],
    &["types", "show"],
    &["types", "set"],
    &["types", "remove"],
    &["new"],
    &["okf"],
    &["okf", "index"],
    &["okf", "log"],
    &["madr"],
    &["madr", "toc"],
    &["changelog"],
    &["changelog", "release"],
    &["changelog", "add"],
    &["config"],
    &["completions"],
];

/// Words a short-help line must not end on: cutting a doc comment at a line
/// break rather than a sentence boundary almost always leaves one of these
/// dangling (iter-254, gate 3e).
const DANGLING_WORDS: &[&str] = &[
    "and", "or", "by", "if", "to", "a", "the", "rather", "would", "(no",
];

/// Maximum rendered lines one short-help entry may occupy (iter-254, gate 3e).
///
/// Three or more means the text belongs in the `--help` paragraph, not on the
/// page an agent reads first.
const MAX_SHORT_HELP_LINES: usize = 2;

/// One flag/argument entry parsed out of a rendered `-h` page.
#[derive(Debug, PartialEq, Eq)]
pub struct ShortHelpEntry {
    /// The flag or argument column, e.g. `-f, --file <FILE>`.
    pub flag: String,
    /// The help column, with clap's wrapping undone.
    pub text: String,
    /// How many rendered lines the entry occupies.
    pub lines: usize,
}

/// Split a rendered `-h` page into its flag/argument entries.
///
/// clap renders an entry as `  <flag column>  <help column>`, continuing the
/// help column on following lines indented past the column start. Anything
/// less indented (a section header, a blank line, the trailing global-options
/// pointer) ends the entry.
#[must_use]
pub fn parse_short_help_entries(help: &str) -> Vec<ShortHelpEntry> {
    /// Column at or beyond which a line is a wrapped continuation rather than
    /// a new entry. clap indents help-column continuations well past this.
    const CONTINUATION_INDENT: usize = 10;

    let starts_entry = |line: &str| {
        let indent = line.len() - line.trim_start().len();
        if !(2..CONTINUATION_INDENT).contains(&indent) {
            return false;
        }
        let rest = line.trim_start();
        rest.starts_with('-') || rest.starts_with('[') || rest.starts_with('<')
    };

    let mut entries = Vec::new();
    let mut current: Option<(String, Vec<String>)> = None;

    let flush = |current: &mut Option<(String, Vec<String>)>, entries: &mut Vec<ShortHelpEntry>| {
        if let Some((flag, parts)) = current.take() {
            entries.push(ShortHelpEntry {
                flag,
                text: parts.join(" "),
                lines: parts.len(),
            });
        }
    };

    for line in help.lines() {
        if starts_entry(line) {
            flush(&mut current, &mut entries);
            // Split the flag column from the help column on the run of two or
            // more spaces clap puts between them. A flag with no help text on
            // the same line (clap's wide layout) starts with an empty help.
            let trimmed = line.trim_end();
            match trimmed.trim_start().split_once("  ") {
                Some((flag, help_col)) => {
                    let help_col = help_col.trim().to_owned();
                    let parts = if help_col.is_empty() {
                        Vec::new()
                    } else {
                        vec![help_col]
                    };
                    current = Some((flag.trim().to_owned(), parts));
                }
                None => current = Some((trimmed.trim().to_owned(), Vec::new())),
            }
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

/// Report every gate-3e problem in one rendered `-h` page.
///
/// Returns `(flag, reason)` pairs so the caller can prefix them with the
/// command they came from.
#[must_use]
pub fn dangling_short_help(help: &str) -> Vec<(String, String)> {
    let mut issues = Vec::new();
    for entry in parse_short_help_entries(help) {
        if entry.text.is_empty() {
            continue;
        }
        // clap appends `[default: …]` / `[aliases: …]` after the help text;
        // the sentence ends before them.
        let text = strip_clap_suffixes(&entry.text);
        if text.ends_with([',', ';', ':']) {
            issues.push((
                entry.flag.clone(),
                format!("short help ends mid-sentence: \"…{}\"", tail(text)),
            ));
        } else if let Some(last) = text.split_whitespace().next_back()
            && DANGLING_WORDS.contains(&last.trim_end_matches('.').to_lowercase().as_str())
        {
            issues.push((
                entry.flag.clone(),
                format!(
                    "short help ends on the dangling word {last:?}: \"…{}\"",
                    tail(text)
                ),
            ));
        }
        if entry.lines > MAX_SHORT_HELP_LINES {
            issues.push((
                entry.flag.clone(),
                format!(
                    "short help spans {} rendered lines (max {MAX_SHORT_HELP_LINES}); move the \
                     detail into the second paragraph of the doc comment",
                    entry.lines
                ),
            ));
        }
    }
    issues
}

/// Drop clap's trailing `[default: …]` / `[aliases: …]` annotation.
fn strip_clap_suffixes(text: &str) -> &str {
    let trimmed = text.trim_end();
    let Some(open) = trimmed.rfind('[') else {
        return trimmed;
    };
    if !trimmed.ends_with(']') {
        return trimmed;
    }
    let inside = &trimmed[open + 1..trimmed.len() - 1];
    for tag in ["default:", "aliases:", "alias:", "possible values:"] {
        if inside.starts_with(tag) {
            return trimmed[..open].trim_end();
        }
    }
    trimmed
}

/// The last few words of `text`, for a diagnostic that fits on one line.
fn tail(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    words[words.len().saturating_sub(6)..].join(" ")
}

/// 3e: no short-help entry ends mid-sentence or overflows two rendered lines.
fn check_short_help_fragments(root: &std::path::Path) -> Vec<String> {
    let mut failures = Vec::new();
    for argv in SHORT_HELP_PAGES {
        let cmd_label = if argv.is_empty() {
            "hyalo".to_owned()
        } else {
            format!("hyalo {}", argv.join(" "))
        };
        let Some(help) = short_help_text(root, argv) else {
            continue;
        };
        for (flag, reason) in dangling_short_help(&help) {
            failures.push(format!(
                "Help drift (3e): '{cmd_label} -h' {flag}: {reason}"
            ));
        }
    }
    failures
}

pub fn run() -> Result<bool> {
    let root = workspace_root()?;
    run_with_root(&root)
}

pub fn run_with_root(root: &std::path::Path) -> Result<bool> {
    let stale_patterns = load_stale_patterns(root)?;

    let mut all_failures: Vec<String> = Vec::new();

    let examples_failures = check_examples(root);
    all_failures.extend(examples_failures);

    let stale_failures = check_stale_patterns(root, &stale_patterns);
    all_failures.extend(stale_failures);

    let short_help_failures = check_short_help(root);
    all_failures.extend(short_help_failures);

    let fragment_failures = check_short_help_fragments(root);
    all_failures.extend(fragment_failures);

    let indent_failures = check_help_body_indentation(root);
    all_failures.extend(indent_failures);

    if all_failures.is_empty() {
        println!(
            "check-help-drift: all subcommands have EXAMPLES blocks, no stale patterns, \
             short help within its byte ceilings with no sentence fragments, and no leaked \
             doc-comment indentation."
        );
        Ok(true)
    } else {
        eprintln!(
            "check-help-drift: {} issue(s):\n\n{}",
            all_failures.len(),
            all_failures.join("\n\n")
        );
        Ok(false)
    }
}

// ---------------------------------------------------------------------------
// Unit tests (parser logic only — no subprocess calls)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_examples_plain_lines() {
        let help = r#"
EXAMPLES:
  hyalo find "rust"
  hyalo find --property status=planned
"#;
        assert_eq!(count_examples(help), 2);
    }

    #[test]
    fn count_examples_dollar_prefix() {
        let help = r#"
EXAMPLES:
  $ hyalo set note.md --property foo=bar
  $ hyalo remove note.md --property foo
"#;
        assert_eq!(count_examples(help), 2);
    }

    #[test]
    fn count_examples_fenced_block() {
        let help = r#"
EXAMPLES:
```
hyalo find "rust"
hyalo find --property status=done
```
"#;
        assert_eq!(count_examples(help), 2);
    }

    #[test]
    fn count_examples_zero_when_no_block() {
        let help = "No examples here.";
        assert_eq!(count_examples(help), 0);
    }

    #[test]
    fn count_examples_one_line_fails_threshold() {
        let help = r#"
EXAMPLES:
  hyalo find "rust"
"#;
        assert_eq!(count_examples(help), 1);
    }

    // --- 3e: short-help fragment guard ---

    /// A rendered `-h` page in clap's narrow layout, with one clean entry, one
    /// entry cut mid-sentence, one ending on a dangling word, and one wrapped
    /// across three lines.
    const SAMPLE_HELP: &str = "\
Do a thing

Usage: hyalo thing [OPTIONS]

Options:
  -f, --file <FILE>    Target file(s), repeatable (excludes --glob)
      --section <H>    Extract section(s) by substring match;
      --profile <P>    Scaffold a preset vault flavour (okf, madr, skills) by
      --threshold <N>  Minimum stem similarity for a fuzzy candidate [default: 0.8]
      --wordy          One two three four five six seven eight nine ten
                       eleven twelve thirteen fourteen fifteen sixteen
                       seventeen eighteen nineteen twenty
  -h, --help           Print help

Global: --format -q — see `hyalo -h`
";

    #[test]
    fn parses_entries_and_unwraps_continuations() {
        let entries = parse_short_help_entries(SAMPLE_HELP);
        let flags: Vec<&str> = entries.iter().map(|e| e.flag.as_str()).collect();
        assert_eq!(
            flags,
            vec![
                "-f, --file <FILE>",
                "--section <H>",
                "--profile <P>",
                "--threshold <N>",
                "--wordy",
                "-h, --help",
            ]
        );
        let wordy = entries.iter().find(|e| e.flag == "--wordy").unwrap();
        assert_eq!(wordy.lines, 3);
        assert!(wordy.text.contains("ten eleven"), "{}", wordy.text);
    }

    #[test]
    fn flags_trailing_punctuation_dangling_word_and_overflow() {
        let issues = dangling_short_help(SAMPLE_HELP);
        let flagged: Vec<&str> = issues.iter().map(|(f, _)| f.as_str()).collect();
        assert!(flagged.contains(&"--section <H>"), "{issues:?}");
        assert!(flagged.contains(&"--profile <P>"), "{issues:?}");
        assert!(flagged.contains(&"--wordy"), "{issues:?}");
        assert!(!flagged.contains(&"-f, --file <FILE>"), "{issues:?}");
        assert!(!flagged.contains(&"-h, --help"), "{issues:?}");
    }

    #[test]
    fn clap_default_suffix_does_not_hide_the_sentence_end() {
        // `--threshold` is clean; the `[default: 0.8]` must not be mistaken
        // for the end of the sentence, nor mask a dangling word before it.
        let issues = dangling_short_help(SAMPLE_HELP);
        assert!(
            !issues.iter().any(|(f, _)| f == "--threshold <N>"),
            "{issues:?}"
        );
        let cut = "Options:\n      --threshold <N>  Similarity for a file to be considered a [default: 0.8]\n";
        assert_eq!(dangling_short_help(cut).len(), 1, "{cut}");
    }

    #[test]
    fn strip_clap_suffixes_leaves_a_real_bracket_alone() {
        assert_eq!(strip_clap_suffixes("Text [default: 3]"), "Text");
        assert_eq!(
            strip_clap_suffixes("Matches 'Tasks [4/4]'"),
            "Matches 'Tasks [4/4]'"
        );
        assert_eq!(strip_clap_suffixes("Plain text"), "Plain text");
    }

    #[test]
    fn parse_stale_pattern_file() {
        let toml_str = r#"
[[patterns]]
pattern = "parent must exist"
reason = "iter-140 fixed via create_dir_all"
"#;
        let file: StalePatternFile = toml::from_str(toml_str).unwrap();
        assert_eq!(file.patterns.len(), 1);
        assert_eq!(file.patterns[0].pattern, "parent must exist");
        assert!(file.patterns[0].reason.contains("iter-140"));
    }

    #[test]
    fn parse_stale_pattern_empty_file() {
        let file: StalePatternFile = toml::from_str("").unwrap();
        assert!(file.patterns.is_empty());
    }

    #[test]
    fn stale_pattern_detected_case_insensitive() {
        let help = "This flag assumes the parent must exist in the filesystem.";
        let lower = help.to_lowercase();
        assert!(lower.contains("parent must exist"));
    }

    #[test]
    fn stale_pattern_not_detected_when_absent() {
        let help = "This flag creates parent directories automatically.";
        let lower = help.to_lowercase();
        assert!(!lower.contains("parent must exist"));
    }
}
