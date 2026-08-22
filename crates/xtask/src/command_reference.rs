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

// ---------------------------------------------------------------------------
// Flag-accuracy gate (M-9, iter-204)
// ---------------------------------------------------------------------------

/// Long flags every subcommand inherits, documented once under GLOBAL FLAGS.
///
/// Repeating them in each COMMAND REFERENCE entry would triple the block's
/// length and teach nothing, so they never count as missing.
const GLOBAL_FLAGS: &[&str] = &[
    "--dir",
    "--format",
    "--jq",
    "--hints",
    "--no-hints",
    "--quiet",
    "--site-prefix",
    "--index",
    "--index-file",
    "--count",
    "--files-from",
    "--help",
    "--version",
    "--config-dir",
];

/// Per-command flags deliberately left out of COMMAND REFERENCE.
///
/// Each entry is `(subcommand, flag, reason)`. The reason is what keeps this
/// list from becoming a dumping ground: an omission has to be justifiable in a
/// sentence, and a *new* flag cannot be added to the CLI without either
/// documenting it in the reference or arguing for it here.
const REFERENCE_FLAG_EXEMPT: &[(&str, &str, &str)] = &[
    (
        "find",
        "--view",
        "documented in the Views entry, where saved views are explained",
    ),
    (
        "find",
        "--desc",
        "undocumented alias of --reverse, which is listed",
    ),
    (
        "find",
        "--language",
        "BM25 stemmer tuning; belongs with the language table in `find --help`",
    ),
    (
        "find",
        "--stemmer",
        "alias of --language, exempt for the same reason",
    ),
    (
        "read",
        "--glob",
        "shared input-selection flag; `read` resolves to a single file, so the entry shows the FILE forms",
    ),
    (
        "backlinks",
        "--glob",
        "shared input-selection flag; the entry shows the FILE forms",
    ),
];

fn is_exempt_flag(command: &str, flag: &str) -> bool {
    REFERENCE_FLAG_EXEMPT
        .iter()
        .any(|(cmd, f, _)| *cmd == command && *f == flag)
}

/// Extract the long flags a subcommand accepts from its own `--help` output.
///
/// Scans only the text after `Options:` so flags merely *mentioned* in prose
/// (e.g. a precedence note) are not mistaken for accepted ones.
pub fn extract_long_flags(sub_help: &str) -> Vec<String> {
    let options = match sub_help.rfind("Options:") {
        Some(i) => &sub_help[i..],
        None => return Vec::new(),
    };
    let mut flags: Vec<String> = Vec::new();
    for line in options.lines() {
        let trimmed = line.trim_start();
        // clap renders a flag declaration at indent 2 (`-s, --long`) or 6
        // (`      --long`, no short form). Its wrapped help prose starts at
        // indent 10 and routinely *mentions* other flags — matching there is
        // what made an earlier draft of this gate report `--apply)`.
        let indent = line.len() - trimmed.len();
        if indent != 2 && indent != 6 {
            continue;
        }
        let candidate = if trimmed.starts_with("--") {
            trimmed
        } else if let Some((_, rest)) = trimmed.split_once(", ") {
            rest
        } else {
            continue;
        };
        let Some(name) = candidate.split([' ', '=', '<']).next() else {
            continue;
        };
        // A declaration is exactly `--name`; anything carrying punctuation came
        // from prose that happened to wrap onto a shallow line.
        let is_declaration = name.len() > 2
            && name.starts_with("--")
            && name[2..]
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if is_declaration {
            flags.push(name.to_owned());
        }
    }
    flags.sort_unstable();
    flags.dedup();
    flags
}

/// Split COMMAND REFERENCE into `(command, entry-body)` pairs.
///
/// Entry headers are `  Xxx (…):` lines, i.e. the command name with its first
/// letter uppercased — the same shape [`header_prefix`] builds.
pub fn reference_entries(reference: &str) -> Vec<(String, String)> {
    let mut entries: Vec<(String, String)> = Vec::new();
    for line in reference.lines() {
        let is_header = line.starts_with("  ")
            && !line.starts_with("   ")
            && line.contains(" (")
            && line
                .trim_start()
                .chars()
                .next()
                .is_some_and(char::is_uppercase);
        if is_header {
            let name = line
                .trim_start()
                .split(' ')
                .next()
                .unwrap_or("")
                .to_string();
            entries.push((name.to_lowercase(), String::new()));
        } else if let Some(last) = entries.last_mut() {
            last.1.push('\n');
            last.1.push_str(line);
        }
    }
    entries
}

/// Flags a subcommand accepts that its COMMAND REFERENCE entry never mentions.
///
/// This is the accuracy half of the gate. Presence-only checking is what let
/// `mv`'s entry go stale: it named `--file`/`--to` while the command had grown
/// a positional form, batch mode, `--apply`, `--on-conflict` and
/// `--allow-ambiguous` (M-9).
pub fn undocumented_flags(command: &str, sub_help: &str, entry: &str) -> Vec<String> {
    extract_long_flags(sub_help)
        .into_iter()
        .filter(|f| !GLOBAL_FLAGS.contains(&f.as_str()))
        .filter(|f| !is_exempt_flag(command, f))
        .filter(|f| !entry.contains(f.as_str()))
        .collect()
}

fn subcommand_help(root: &std::path::Path, name: &str) -> Result<String> {
    let out = Command::new("cargo")
        .args(["run", "-q", "-p", "hyalo-cli", "--", name, "--help"])
        .current_dir(root)
        .output()
        .with_context(|| format!("running `cargo run -p hyalo-cli -- {name} --help`"))?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
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
    if !missing.is_empty() {
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
        return Ok(false);
    }

    // Accuracy pass (M-9): an entry that exists is not necessarily current.
    let entries = reference_entries(reference);
    let mut stale: Vec<(String, Vec<String>)> = Vec::new();
    for name in &subcommands {
        if REFERENCE_EXEMPT.contains(&name.as_str()) {
            continue;
        }
        let Some((_, entry)) = entries.iter().find(|(cmd, _)| cmd == name) else {
            continue;
        };
        let sub_help = subcommand_help(root, name)?;
        let undocumented = undocumented_flags(name, &sub_help, entry);
        if !undocumented.is_empty() {
            stale.push((name.clone(), undocumented));
        }
    }

    if !stale.is_empty() {
        eprintln!("check-command-reference: FAILED (stale entries)");
        for (name, flags) in &stale {
            eprintln!("  - `hyalo {name}` accepts flags its entry never mentions: {flags:?}");
        }
        eprintln!(
            "\nAdd them to the command's COMMAND REFERENCE entry in \
             crates/hyalo-cli/src/cli/help.rs, or — if the omission is deliberate — \
             record it with a reason in REFERENCE_FLAG_EXEMPT \
             (crates/xtask/src/command_reference.rs)."
        );
        return Ok(false);
    }

    println!(
        "check-command-reference: all {} subcommands appear in COMMAND REFERENCE, \
         and every non-global flag they accept is documented there.",
        subcommands.len()
    );
    Ok(true)
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

    const SUB_HELP: &str = "\
Move or rename a markdown file.

Usage: hyalo mv [OPTIONS] [FILE] [DEST]

Arguments:
  [FILE]
          Source file

Options:
  -f, --file <FILE>
          Source file to move

      --to <TO>
          Destination path

      --on-conflict <POLICY>
          What to do when DEST exists (rejected without --apply)

      --allow-ambiguous
          Rewrite ambiguous [[stem]] links

  -h, --help
          Print help
";

    #[test]
    fn extract_long_flags_reads_declarations_only() {
        let flags = extract_long_flags(SUB_HELP);
        assert_eq!(
            flags,
            vec![
                "--allow-ambiguous",
                "--file",
                "--help",
                "--on-conflict",
                "--to"
            ]
        );
        // `--apply` is mentioned in wrapped prose, never declared.
        assert!(!flags.iter().any(|f| f == "--apply"));
    }

    #[test]
    fn undocumented_flags_reports_a_stale_entry() {
        let entry = "    hyalo mv FILE DEST [--allow-ambiguous]";
        assert_eq!(
            undocumented_flags("mv", SUB_HELP, entry),
            vec!["--file", "--on-conflict", "--to"]
        );
    }

    #[test]
    fn undocumented_flags_is_empty_for_a_current_entry() {
        let entry = "    hyalo mv FILE DEST [--on-conflict P] [--allow-ambiguous]\n\
                     hyalo mv --file F --to DEST";
        assert!(undocumented_flags("mv", SUB_HELP, entry).is_empty());
    }

    #[test]
    fn global_and_exempt_flags_never_count_as_missing() {
        // `--help` is global; `--view` is exempt on `find`.
        let help = "Options:\n  -h, --help\n      --view <NAME>\n";
        assert!(undocumented_flags("find", help, "").is_empty());
        // The same flag on another command is NOT exempt.
        assert_eq!(undocumented_flags("mv", help, ""), vec!["--view"]);
    }

    #[test]
    fn reference_entries_splits_on_headers() {
        let reference = reference_section(SAMPLE).expect("reference section");
        let entries = reference_entries(reference);
        let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["find", "lint-rules"]);
        assert!(entries[0].1.contains("hyalo find [PATTERN]"));
        assert!(!entries[0].1.contains("lint-rules"));
    }

    #[test]
    fn every_flag_exemption_carries_a_reason() {
        for (cmd, flag, reason) in REFERENCE_FLAG_EXEMPT {
            assert!(
                reason.len() > 20,
                "{cmd} {flag}: exemption needs a real justification, got {reason:?}"
            );
        }
    }
}
