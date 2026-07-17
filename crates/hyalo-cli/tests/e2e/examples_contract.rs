/// NEW-7: Contract test — every subcommand's `--help` must contain an `EXAMPLES:` block.
///
/// This guards against future commands being added without examples.
/// The test runs `hyalo <subcommand> --help` (or `hyalo <parent> <sub> --help`)
/// and asserts the output contains the literal token `EXAMPLES:`.
use super::common::hyalo_no_hints;

/// All commands (including nested sub-actions) that must have EXAMPLES in `--help`.
///
/// Each entry is the argv slice passed after `hyalo` (e.g., `["task", "toggle"]`).
const SUBCOMMANDS: &[&[&str]] = &[
    // Top-level subcommands
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
    // Already had EXAMPLES (regression guard)
    &["lint"],
    &["mv"],
    &["new"],
    // Nested task sub-actions
    &["task", "read"],
    &["task", "toggle"],
    &["task", "set"],
    // changelog profile generators
    &["changelog"],
    &["changelog", "release"],
    &["changelog", "add"],
];

#[test]
fn every_subcommand_help_has_examples_block() {
    let mut failures: Vec<String> = Vec::new();

    for argv in SUBCOMMANDS {
        let mut cmd = hyalo_no_hints();
        for arg in *argv {
            cmd.arg(arg);
        }
        cmd.arg("--help");

        let output = cmd.output().expect("failed to spawn hyalo");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !stdout.contains("EXAMPLES:") {
            failures.push(format!(
                "hyalo {} --help: missing EXAMPLES: block\n  stdout: {}\n  stderr: {}",
                argv.join(" "),
                stdout.trim(),
                stderr.trim(),
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} subcommand(s) missing EXAMPLES: block:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
