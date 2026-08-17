use super::common::{hyalo_no_hints, write_md, write_tagged};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn setup_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "a.md", &["rust", "cli"]);
    write_tagged(tmp.path(), "b.md", &["rust", "iteration"]);
    write_md(tmp.path(), "c.md", "No frontmatter.\n");
    tmp
}

// ---------------------------------------------------------------------------
// Basic --count usage
// ---------------------------------------------------------------------------

#[test]
fn count_find_all_files() {
    let tmp = setup_vault();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--count"])
        .args(["find"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "3");
}

#[test]
fn count_find_filtered_by_tag() {
    let tmp = setup_vault();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--count"])
        .args(["find", "--tag", "rust"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "2");
}

#[test]
fn count_tags_summary() {
    // 4 files but only 2 unique tags — ensures we count tags, not files.
    let tmp = TempDir::new().unwrap();
    write_tagged(tmp.path(), "a.md", &["rust"]);
    write_tagged(tmp.path(), "b.md", &["rust"]);
    write_tagged(tmp.path(), "c.md", &["cli"]);
    write_tagged(tmp.path(), "d.md", &["cli"]);

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--count"])
        .args(["tags", "summary"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "2");
}

#[test]
fn count_zero_results() {
    let tmp = setup_vault();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--count"])
        .args(["find", "--tag", "nonexistent"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "0");
}

// ---------------------------------------------------------------------------
// --count with --format (output is always bare integer)
// ---------------------------------------------------------------------------

#[test]
fn count_with_format_text() {
    let tmp = setup_vault();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "text"])
        .args(["--count"])
        .args(["find"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "3");
}

#[test]
fn count_with_format_json() {
    let tmp = setup_vault();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "json"])
        .args(["--count"])
        .args(["find"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "3");
}

// ---------------------------------------------------------------------------
// --count with properties summary
// ---------------------------------------------------------------------------

#[test]
fn count_properties_summary() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "note.md",
        "---\ntitle: Hello\nstatus: draft\npriority: 1\n---\n# Body\n",
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--count"])
        .args(["properties", "summary"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    // 3 unique properties: title, status, priority
    assert_eq!(stdout.trim(), "3");
}

// ---------------------------------------------------------------------------
// --count with zero results and --format text (no spurious stderr)
// ---------------------------------------------------------------------------

#[test]
fn count_zero_results_format_text_no_stderr_notice() {
    let tmp = setup_vault();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "text"])
        .args(["--count"])
        .args(["find", "--tag", "nonexistent"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "0");
    // --count short-circuits before the "No files matched" notice
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.is_empty(), "expected no stderr, got: {stderr}");
}

// ---------------------------------------------------------------------------
// Conflict: --count + --jq
// ---------------------------------------------------------------------------

#[test]
fn count_with_jq_errors() {
    let tmp = setup_vault();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--count"])
        .args(["--jq", ".total"])
        .args(["find"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    // iter-181 task 2: conflicting user flags exit 1 (user error), not 2.
    assert_eq!(
        output.status.code(),
        Some(1),
        "expected exit code 1 (user error)"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--count cannot be combined with --jq"),
        "expected conflict error, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// --count on non-list command
// ---------------------------------------------------------------------------

#[test]
fn count_on_read_command_errors() {
    let tmp = setup_vault();
    write_md(tmp.path(), "note.md", "---\ntitle: Test\n---\nBody\n");

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--count"])
        .args(["read", "--file", "note.md"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    // iter-181 task 2: unsupported flag for this command exits 1 (user error), not 2.
    assert_eq!(
        output.status.code(),
        Some(1),
        "expected exit code 1 (user error)"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--count is only supported for list commands"),
        "expected unsupported error, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// LIST_COMMANDS is one source of truth (iter-192)
// ---------------------------------------------------------------------------

/// Collapse every run of whitespace to a single space so assertions are immune
/// to clap's line wrapping (the same phrase wraps at different columns in
/// different help sections).
fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The list-command phrase the binary itself reports, read out of the `--count`
/// runtime error. Everything else in this module is checked against it, so the
/// test never restates the list — restating it is the bug being prevented.
fn declared_list_commands() -> Vec<String> {
    let tmp = TempDir::new().unwrap();
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["summary", "--count", "--format", "text"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let start = stderr
        .find("list commands (")
        .unwrap_or_else(|| panic!("no --count error to parse: {stderr}"))
        + "list commands (".len();
    let rest = &stderr[start..];
    let end = rest.find(')').expect("unterminated command list");
    rest[..end]
        .split(", ")
        .map(str::trim)
        .map(str::to_owned)
        .collect()
}

#[test]
fn list_commands_phrase_is_identical_in_every_help_section() {
    let commands = declared_list_commands();
    assert!(
        commands.len() >= 5,
        "parsed a suspiciously short list: {commands:?}"
    );
    let phrase = commands.join(", ");

    let output = hyalo_no_hints().arg("--help").output().unwrap();
    let help = squash(&String::from_utf8_lossy(&output.stdout));
    let occurrences = help.matches(&phrase).count();

    // Four call sites render the list: the top-level OUTPUT paragraph, the
    // --count flag's long help, the "Default output limits" block, and the
    // OUTPUT SHAPES note. All four read from LIST_COMMANDS, so all four must
    // agree with the runtime error verbatim.
    assert_eq!(
        occurrences, 4,
        "expected the list-command phrase \"{phrase}\" in all 4 help sections, found {occurrences}"
    );
}

#[test]
fn every_declared_list_command_emits_total_and_accepts_count() {
    let tmp = setup_vault();
    let dir = tmp.path().to_str().unwrap().to_owned();

    for cmd in declared_list_commands() {
        let mut argv: Vec<&str> = cmd.split(' ').collect();
        // `backlinks` is the one list command with a required operand.
        if argv.first() == Some(&"backlinks") {
            argv.push("a.md");
        }

        // --count must print a bare integer.
        let counted = hyalo_no_hints()
            .args(["--dir", &dir])
            .args(&argv)
            .arg("--count")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&counted.stdout).into_owned();
        assert!(
            stdout.trim().parse::<u64>().is_ok(),
            "`hyalo {cmd} --count` did not print a bare integer (stdout: {stdout:?}, stderr: {})",
            String::from_utf8_lossy(&counted.stderr)
        );

        // The JSON envelope must carry the `total` that makes --count possible.
        let json_out = hyalo_no_hints()
            .args(["--dir", &dir])
            .args(&argv)
            .args(["--format", "json"])
            .output()
            .unwrap();
        let envelope: serde_json::Value = serde_json::from_slice(&json_out.stdout)
            .unwrap_or_else(|e| panic!("`hyalo {cmd} --format json` is not JSON: {e}"));
        assert!(
            envelope.get("total").is_some(),
            "`hyalo {cmd}` is declared a list command but its envelope has no `total`: {envelope}"
        );
    }
}

#[test]
fn known_non_list_commands_reject_count() {
    let tmp = setup_vault();
    let dir = tmp.path().to_str().unwrap().to_owned();
    let declared = declared_list_commands();

    // Commands whose payload is a single object, not a countable collection.
    for cmd in [
        vec!["summary"],
        vec!["read", "a.md"],
        vec!["links", "fix"],
        vec!["config"],
    ] {
        let label = cmd.join(" ");
        assert!(
            !declared.contains(&label),
            "{label} is declared a list command; this test's premise is stale"
        );
        let output = hyalo_no_hints()
            .args(["--dir", &dir])
            .args(&cmd)
            .args(["--count", "--format", "text"])
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "`hyalo {label} --count` should be rejected, not silently succeed"
        );
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert!(
            stderr.contains("--count is only supported for list commands"),
            "`hyalo {label} --count` gave an unexpected error: {stderr}"
        );
    }
}
