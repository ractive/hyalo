use std::fs;

use super::common::hyalo_no_hints;
use tempfile::TempDir;

#[test]
fn short_help_is_compact() {
    let output = hyalo_no_hints().arg("-h").output().unwrap();
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
    let output = hyalo_no_hints().arg("--help").output().unwrap();
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
    let output = hyalo_no_hints().arg("--help").output().unwrap();
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
        "hyalo task set",
        "hyalo init",
        "hyalo deinit",
        "hyalo views list",
        "hyalo views set",
        "hyalo views remove",
    ];

    for cmd in expected {
        assert!(stdout.contains(cmd), "COMMAND REFERENCE missing: {cmd}");
    }
}

#[test]
fn subcommand_help_unchanged_by_enriched_root() {
    // Subcommand --help should NOT contain root-level enriched sections
    let output = hyalo_no_hints().args(["task", "--help"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(
        stdout.contains("Read, toggle, or set status on task checkboxes"),
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
        .map_or(help.len(), |off| start + off);
    &help[start..end]
}

#[test]
fn help_hides_dir_when_config_sets_it() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \"notes\"\n").unwrap();

    let output = hyalo_no_hints()
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

    let output = hyalo_no_hints()
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

    let output = hyalo_no_hints()
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

// ---------------------------------------------------------------------------
// Config-aware help: examples and cookbook filtering
// ---------------------------------------------------------------------------

/// Extract the EXAMPLES block from `-h` output (short help).
///
/// Returns the text from the `EXAMPLES:` heading to the end of the output.
fn examples_block(help: &str) -> &str {
    let start = help.find("EXAMPLES:").expect("no EXAMPLES: block in help");
    &help[start..]
}

/// Extract the COOKBOOK block from `--help` output (long help).
///
/// Returns the text from the `COOKBOOK:` heading to the next major section
/// (`OUTPUT SHAPES`) or end of output.
fn cookbook_block(help: &str) -> &str {
    let start = help.find("COOKBOOK:").expect("no COOKBOOK: block in help");
    // Trim to just the cookbook section (end at the next section heading).
    let slice = &help[start..];
    if let Some(end_offset) = slice.find("\nOUTPUT SHAPES") {
        &slice[..end_offset]
    } else {
        slice
    }
}

#[test]
fn examples_omit_format_when_config_sets_it() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".hyalo.toml"), "format = \"text\"\n").unwrap();

    let output = hyalo_no_hints()
        .arg("-h")
        .current_dir(tmp.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let examples = examples_block(&stdout);

    assert!(
        !examples.contains("--format"),
        "EXAMPLES should omit --format when config sets format:\n{examples}"
    );
}

#[test]
fn examples_show_format_without_config() {
    let tmp = TempDir::new().unwrap();
    // No .hyalo.toml

    let output = hyalo_no_hints()
        .arg("-h")
        .current_dir(tmp.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let examples = examples_block(&stdout);

    assert!(
        examples.contains("--format"),
        "EXAMPLES should show --format when no config is present:\n{examples}"
    );
}

#[test]
fn cookbook_omits_format_when_config_sets_it() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".hyalo.toml"), "format = \"text\"\n").unwrap();

    let output = hyalo_no_hints()
        .arg("--help")
        .current_dir(tmp.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let cookbook = cookbook_block(&stdout);

    assert!(
        !cookbook.contains("--format"),
        "COOKBOOK should omit --format when config sets format:\n{cookbook}"
    );
}

#[test]
fn cookbook_shows_format_without_config() {
    let tmp = TempDir::new().unwrap();
    // No .hyalo.toml

    let output = hyalo_no_hints()
        .arg("--help")
        .current_dir(tmp.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let cookbook = cookbook_block(&stdout);

    assert!(
        cookbook.contains("--format"),
        "COOKBOOK should show --format when no config is present:\n{cookbook}"
    );
}

/// Extract the "Global flags" sub-section from a `--help` output string.
///
/// Returns text from the `Global flags` heading to the next blank line
/// that follows (i.e., just the flag table rows).
fn global_flags_block(help: &str) -> &str {
    let start = help
        .find("Global flags (apply to all commands):")
        .expect("no 'Global flags' section in --help");
    let slice = &help[start..];
    // The block ends at the first blank line after the heading.
    if let Some(end_offset) = slice.find("\n\n") {
        &slice[..end_offset]
    } else {
        slice
    }
}

#[test]
fn command_reference_omits_dir_flag_when_config_sets_it() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \"notes\"\n").unwrap();

    let output = hyalo_no_hints()
        .arg("--help")
        .current_dir(tmp.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    // The global flags table row for -d/--dir should be absent.
    let flags = global_flags_block(&stdout);
    assert!(
        !flags.contains("-d/--dir"),
        "Global flags table should omit -d/--dir when config sets dir:\n{flags}"
    );
}

#[test]
fn command_reference_shows_dir_flag_without_config() {
    let tmp = TempDir::new().unwrap();
    // No .hyalo.toml

    let output = hyalo_no_hints()
        .arg("--help")
        .current_dir(tmp.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    let flags = global_flags_block(&stdout);
    assert!(
        flags.contains("-d/--dir"),
        "Global flags table should show -d/--dir when no config is present:\n{flags}"
    );
}
