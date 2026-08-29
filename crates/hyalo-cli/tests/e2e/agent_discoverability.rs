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
