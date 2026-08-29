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

    if all_failures.is_empty() {
        println!(
            "check-help-drift: all subcommands have EXAMPLES blocks, no stale patterns, and \
             short help within its byte ceilings."
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
