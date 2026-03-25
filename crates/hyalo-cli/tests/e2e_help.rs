mod common;

use std::fs;

use common::hyalo;
use tempfile::TempDir;

#[test]
fn short_help_is_compact() {
    let output = hyalo().arg("-h").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    // -h should contain the basic sections
    assert!(stdout.contains("Usage: hyalo"));
    assert!(stdout.contains("Commands:"));
    assert!(stdout.contains("Options:"));
    assert!(stdout.contains("EXAMPLES:"));

    // -h should NOT contain the enriched sections
    assert!(
        !stdout.contains("COMMAND REFERENCE:"),
        "-h must not include COMMAND REFERENCE"
    );
    assert!(
        !stdout.contains("COOKBOOK:"),
        "-h must not include COOKBOOK"
    );
    assert!(
        !stdout.contains("OUTPUT SHAPES"),
        "-h must not include OUTPUT SHAPES"
    );
}

#[test]
fn long_help_contains_enriched_sections() {
    let output = hyalo().arg("--help").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    // --help should contain all enriched sections
    assert!(
        stdout.contains("COMMAND REFERENCE:"),
        "--help must include COMMAND REFERENCE"
    );
    assert!(stdout.contains("COOKBOOK:"), "--help must include COOKBOOK");
    assert!(
        stdout.contains("OUTPUT SHAPES"),
        "--help must include OUTPUT SHAPES"
    );
}

#[test]
fn long_help_command_reference_lists_all_commands() {
    let output = hyalo().arg("--help").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Verify every command/subcommand appears in the COMMAND REFERENCE
    let expected = [
        "hyalo find",
        "hyalo read",
        "hyalo set",
        "hyalo remove",
        "hyalo append",
        "hyalo properties",
        "hyalo tags",
        "hyalo summary",
        "hyalo task read",
        "hyalo task toggle",
        "hyalo task set-status",
        "hyalo init",
    ];

    for cmd in expected {
        assert!(stdout.contains(cmd), "COMMAND REFERENCE missing: {cmd}");
    }
}

#[test]
fn subcommand_help_unchanged_by_enriched_root() {
    // Subcommand --help should NOT contain root-level enriched sections
    let output = hyalo().args(["task", "--help"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(
        stdout.contains("Read, toggle, or set status on a single task"),
        "subcommand --help should contain its own long_about"
    );
    assert!(
        !stdout.contains("COMMAND REFERENCE:"),
        "subcommand --help must not include root COMMAND REFERENCE"
    );
    assert!(
        !stdout.contains("COOKBOOK:"),
        "subcommand --help must not include root COOKBOOK"
    );
}

// ---------------------------------------------------------------------------
// Config-aware help: hide flags that are already set in .hyalo.toml
// ---------------------------------------------------------------------------

/// Parse the `Options:` block out of `--help` output.
///
/// Returns the text from the `Options:` heading up to (but not including) the
/// next empty line followed by an all-caps section heading.  This isolates the
/// generated options table from the static `after_long_help` prose, which may
/// also mention `--dir` as part of examples.
fn options_block(help: &str) -> &str {
    let start = help
        .find("\nOptions:\n")
        .expect("no Options: block in help");
    // Advance past the leading newline so the slice starts at 'O'.
    let start = start + 1;
    // Find the next blank line that precedes a section we know ends the options.
    // The heading after Options is always "COMMAND REFERENCE:" in long help or
    // "Commands:" in short help — either way there is a blank line before it.
    // We search for "\n\n" after our start position.
    let end = help[start..]
        .find("\n\n")
        .map(|off| start + off)
        .unwrap_or(help.len());
    &help[start..end]
}

#[test]
fn help_hides_dir_when_config_sets_it() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \"notes\"\n").unwrap();

    let output = hyalo()
        .arg("--help")
        .current_dir(tmp.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let opts = options_block(&stdout);

    assert!(
        !opts.contains("--dir"),
        "--dir should be hidden in Options: when config sets dir:\n{opts}"
    );
}

#[test]
fn help_hides_format_when_config_sets_it() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".hyalo.toml"), "format = \"text\"\n").unwrap();

    let output = hyalo()
        .arg("--help")
        .current_dir(tmp.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let opts = options_block(&stdout);

    assert!(
        !opts.contains("--format"),
        "--format should be hidden in Options: when config sets format:\n{opts}"
    );
}

#[test]
fn help_shows_dir_without_config() {
    let tmp = TempDir::new().unwrap();
    // No .hyalo.toml — dir defaults to "."

    let output = hyalo()
        .arg("--help")
        .current_dir(tmp.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let opts = options_block(&stdout);

    assert!(
        opts.contains("--dir"),
        "--dir should be visible in Options: when no config is present:\n{opts}"
    );
}
