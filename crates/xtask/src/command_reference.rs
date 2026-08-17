//! Gate — COMMAND REFERENCE coverage.
//!
//! `hyalo --help` ends with a hand-written COMMAND REFERENCE block. Nothing
//! previously tied that block to the actual clap subcommand set, so eight
//! commands (`changelog`, `config`, `lint`, `lint-rules`, `madr`, `new`, `okf`,
//! `types`) shipped without ever appearing in it — the reference silently
//! claimed the CLI was smaller than it is (iter-192).
//!
//! This gate reads both halves out of a single `hyalo --help` run: the clap
//! `Commands:` listing, and the COMMAND REFERENCE section. Every command in the
//! former must have a section header in the latter, so a newly added subcommand
//! cannot skip documentation.

use anyhow::{Context, Result};
use std::process::Command;

use crate::workspace::workspace_root;

/// Subcommands exempt from needing a COMMAND REFERENCE entry.
///
/// `help` is clap's built-in and documents itself; `deinit` is documented as
/// part of the `init` story but still carries its own header, so it is not
/// listed here — only genuinely undocumentable commands belong.
const REFERENCE_EXEMPT: &[&str] = &["help"];

/// Extract the clap-declared subcommand names from `hyalo --help` output.
///
/// Reads the block introduced by a line equal to `Commands:` and ending at the
/// next unindented line (`Options:`). Names sit at indent 2; wrapped
/// description lines are indented further and are ignored.
pub fn extract_subcommands(help: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_block = false;
    for line in help.lines() {
        if line.trim_end() == "Commands:" {
            in_block = true;
            continue;
        }
        if !in_block {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        // An unindented line ends the Commands block (e.g. `Options:`).
        if !line.starts_with(' ') {
            break;
        }
        let indent = line.len() - line.trim_start().len();
        if indent != 2 {
            // Wrapped description continuation line.
            continue;
        }
        if let Some(name) = line.split_whitespace().next() {
            names.push(name.to_owned());
        }
    }
    names
}

/// The COMMAND REFERENCE section of `hyalo --help`, or `None` when absent.
pub fn reference_section(help: &str) -> Option<&str> {
    let start = help.find("COMMAND REFERENCE:")?;
    let rest = &help[start..];
    Some(match rest.find("\nCOOKBOOK:") {
        Some(end) => &rest[..end],
        None => rest,
    })
}

/// The section-header form a command must appear as inside COMMAND REFERENCE:
/// two-space indent, then the command name with its first letter uppercased
/// (`lint-rules` → `  Lint-rules `).
fn header_prefix(name: &str) -> String {
    let mut chars = name.chars();
    let head = chars.next().map(|c| c.to_ascii_uppercase());
    let mut out = String::from("  ");
    if let Some(c) = head {
        out.push(c);
    }
    out.push_str(chars.as_str());
    out.push(' ');
    out
}

/// Names present in `subcommands` but missing a COMMAND REFERENCE header.
pub fn missing_entries(subcommands: &[String], reference: &str) -> Vec<String> {
    subcommands
        .iter()
        .filter(|name| !REFERENCE_EXEMPT.contains(&name.as_str()))
        .filter(|name| {
            let prefix = header_prefix(name);
            !reference.lines().any(|line| line.starts_with(&prefix))
        })
        .cloned()
        .collect()
}

fn top_level_help(root: &std::path::Path) -> Result<String> {
    let out = Command::new("cargo")
        .args(["run", "-q", "-p", "hyalo-cli", "--", "--help"])
        .current_dir(root)
        .output()
        .context("running `cargo run -p hyalo-cli -- --help`")?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if stdout.trim().is_empty() {
        anyhow::bail!(
            "`hyalo --help` produced no stdout; stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(stdout)
}

pub fn run() -> Result<bool> {
    let root = workspace_root()?;
    run_with_root(&root)
}

pub fn run_with_root(root: &std::path::Path) -> Result<bool> {
    let help = top_level_help(root)?;
    let subcommands = extract_subcommands(&help);
    if subcommands.is_empty() {
        anyhow::bail!("could not parse any subcommands out of `hyalo --help`");
    }
    let Some(reference) = reference_section(&help) else {
        eprintln!("check-command-reference: `hyalo --help` has no COMMAND REFERENCE section.");
        return Ok(false);
    };

    let missing = missing_entries(&subcommands, reference);
    if missing.is_empty() {
        println!(
            "check-command-reference: all {} subcommands appear in COMMAND REFERENCE.",
            subcommands.len()
        );
        return Ok(true);
    }

    eprintln!("check-command-reference: FAILED");
    for name in &missing {
        eprintln!(
            "  - `hyalo {name}` has no COMMAND REFERENCE entry (expected a line starting with \"{}\")",
            header_prefix(name).trim_end()
        );
    }
    eprintln!(
        "\nAdd a section to HELP_LONG_TEMPLATE in crates/hyalo-cli/src/cli/help.rs for each command above."
    );
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
Hyalo — does things.

Usage: hyalo [OPTIONS] <COMMAND>

Commands:
  find          Search and filter markdown files — returns file objects with metadata,
                tasks, and links
  lint-rules    Manage markdown lint rule configuration
  config        Print the effective configuration
  help          Print this message

Options:
  -d, --dir <DIR>  Root directory

COMMAND REFERENCE:
  Find (search and filter, read-only):
    hyalo find [PATTERN]

  Lint-rules (manage the markdown lint rule catalog):
    hyalo lint-rules list

COOKBOOK:
  hyalo find --tag x
";

    #[test]
    fn extracts_names_ignoring_wrapped_descriptions() {
        assert_eq!(
            extract_subcommands(SAMPLE),
            vec!["find", "lint-rules", "config", "help"]
        );
    }

    #[test]
    fn reference_section_stops_before_cookbook() {
        let section = reference_section(SAMPLE).expect("reference section");
        assert!(section.contains("Lint-rules ("));
        assert!(!section.contains("COOKBOOK"));
    }

    #[test]
    fn missing_entries_reports_undocumented_command() {
        let subs = extract_subcommands(SAMPLE);
        let reference = reference_section(SAMPLE).expect("reference section");
        // `config` has no header; `help` is exempt; `find`/`lint-rules` are documented.
        assert_eq!(missing_entries(&subs, reference), vec!["config"]);
    }

    #[test]
    fn hyphenated_command_header_is_titlecased_on_first_letter_only() {
        assert_eq!(header_prefix("lint-rules"), "  Lint-rules ");
        assert_eq!(header_prefix("create-index"), "  Create-index ");
    }

    #[test]
    fn fully_documented_reference_has_no_missing_entries() {
        let documented = "\
COMMAND REFERENCE:
  Find (x):
  Lint-rules (x):
  Config (x):
";
        let subs = extract_subcommands(SAMPLE);
        assert!(missing_entries(&subs, documented).is_empty());
    }
}
