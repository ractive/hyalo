mod common;

use common::hyalo;

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
        "hyalo properties summary",
        "hyalo properties list",
        "hyalo property read",
        "hyalo property set",
        "hyalo property remove",
        "hyalo property find",
        "hyalo property add-to-list",
        "hyalo property remove-from-list",
        "hyalo tags summary",
        "hyalo tags list",
        "hyalo tag find",
        "hyalo tag add",
        "hyalo tag remove",
        "hyalo links",
        "hyalo outline",
    ];

    for cmd in expected {
        assert!(stdout.contains(cmd), "COMMAND REFERENCE missing: {cmd}");
    }
}

#[test]
fn subcommand_help_unchanged_by_enriched_root() {
    // Subcommand --help should NOT contain root-level enriched sections
    let output = hyalo().args(["property", "--help"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(
        stdout.contains("Read, set, find, or remove frontmatter properties"),
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
