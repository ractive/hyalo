//! Iteration 263 — `lint --fix` must never corrupt Obsidian content.
//!
//! Each test pins one of the destructive autofixes the v0.22.0 Obsidian-vault
//! dogfood run found (BUG-3, BUG-9, UX-10) plus the conflict explanation the
//! same run asked for (UX-16). The shared shape is deliberate: run the fix
//! for real and compare the file byte-for-byte, because the failure mode
//! being guarded against is a *silent* rewrite, not a wrong exit code.

use super::common::{hyalo_no_hints, typed_results, write_md};
use hyalo_cli::commands::lint::{ExtLintFixOutput, ExtLintOutput};
use tempfile::TempDir;

/// A vault with no schema requirements, so only body rules speak.
fn vault(files: &[(&str, &str)]) -> TempDir {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    for (name, content) in files {
        write_md(tmp.path(), name, content);
    }
    tmp
}

// ---------------------------------------------------------------------------
// BUG-3 — MD018 vs Obsidian tags
// ---------------------------------------------------------------------------

#[test]
fn lint_fix_md018_leaves_an_obsidian_tag_line_byte_identical() {
    let body = "---\ntitle: Daily Log\n---\n\nWoke up late.\n\n#todo\n\nCalled the vet.\n\n#todo/next follow up\n";
    let tmp = vault(&[("log.md", body)]);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--fix", "--rule", "MD018", "--format", "json"])
        .output()
        .unwrap();

    let after = std::fs::read_to_string(tmp.path().join("log.md")).unwrap();
    assert_eq!(after, body, "a tag line must survive `--fix` untouched");

    let results: ExtLintFixOutput = typed_results(&output.stdout);
    assert_eq!(
        results.total_fixed, 0,
        "no MD018 fix may be reported for tag lines"
    );
}

#[test]
fn lint_fix_md018_still_fixes_a_real_heading_typo() {
    let tmp = vault(&[(
        "note.md",
        "---\ntitle: Note\n---\n\n#Heading typo\n\nprose\n",
    )]);

    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--fix", "--rule", "MD018"])
        .output()
        .unwrap();

    let after = std::fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(
        after.contains("# Heading typo"),
        "a capitalized word followed by prose is still a heading typo: {after:?}"
    );
}

// ---------------------------------------------------------------------------
// BUG-9 — MD034 / MD042 vs image-as-link-text
// ---------------------------------------------------------------------------

/// The report's fixture: a badge whose link text is an image and whose
/// destination MD034 wrapped in angle brackets.
const BADGE: &str =
    "---\ntitle: Embed Adjustments\n---\n\n[![](img/badge.png)](https://example.com/snippet.png)\n";

#[test]
fn lint_fix_md034_leaves_a_link_destination_alone() {
    let tmp = vault(&[("badge.md", BADGE)]);

    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--fix", "--fix-rule", "MD034"])
        .output()
        .unwrap();

    let after = std::fs::read_to_string(tmp.path().join("badge.md")).unwrap();
    assert_eq!(
        after, BADGE,
        "a URL that is already a link destination must not be wrapped"
    );
}

#[test]
fn lint_md042_accepts_an_image_as_link_text() {
    let tmp = vault(&[("badge.md", BADGE)]);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--rule", "MD042", "--format", "json"])
        .output()
        .unwrap();

    let results: ExtLintOutput = typed_results(&output.stdout);
    let hits: usize = results
        .files
        .iter()
        .flat_map(|f| &f.rule_groups)
        .filter(|g| g.rule == "MD042")
        .map(|g| g.count)
        .sum();
    assert_eq!(hits, 0, "the badge idiom is not an empty link: {results:?}");
}

#[test]
fn lint_md042_still_reports_a_genuinely_empty_link() {
    let tmp = vault(&[(
        "empty.md",
        "---\ntitle: Empty\n---\n\nsee [](https://example.com/) here\n",
    )]);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--rule", "MD042", "--format", "json"])
        .output()
        .unwrap();

    let results: ExtLintOutput = typed_results(&output.stdout);
    let hits: usize = results
        .files
        .iter()
        .flat_map(|f| &f.rule_groups)
        .filter(|g| g.rule == "MD042")
        .map(|g| g.count)
        .sum();
    assert_eq!(hits, 1, "`[](url)` is still empty: {results:?}");
}

// ---------------------------------------------------------------------------
// UX-10 — MD001 reports, never rewrites
// ---------------------------------------------------------------------------

#[test]
fn lint_md001_warns_but_proposes_no_fix() {
    let body = "---\ntitle: Snippet\n---\n\n## Section\n\nprose\n\n###### Caption\n";
    let tmp = vault(&[("snippet.md", body)]);

    // Read-only lint still reports the skipped level.
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--rule", "MD001", "--format", "json"])
        .output()
        .unwrap();
    let results: ExtLintOutput = typed_results(&output.stdout);
    assert!(
        results
            .files
            .iter()
            .flat_map(|f| &f.rule_groups)
            .any(|g| g.rule == "MD001"),
        "MD001 must still warn: {results:?}"
    );

    // `--fix --dry-run` proposes nothing, and `--fix` changes nothing.
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "lint",
            "--fix",
            "--dry-run",
            "--rule",
            "MD001",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let fix_results: ExtLintFixOutput = typed_results(&output.stdout);
    assert_eq!(
        fix_results.total_fixed, 0,
        "MD001 is report-only (DEC-272): {fix_results:?}"
    );

    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--fix", "--rule", "MD001"])
        .output()
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("snippet.md")).unwrap(),
        body,
        "a deliberate `######` caption must survive `--fix`"
    );
}

#[test]
fn lint_rules_list_shows_md001_as_not_autofixable() {
    let tmp = vault(&[]);
    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "lint-rules",
            "list",
            "--format",
            "json",
            "--jq",
            r#".results[] | select(.id=="MD001") | .autofixable"#,
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        "false",
        "MD001 must advertise AUTOFIX no: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// UX-16 — `conflicts N` is explained
// ---------------------------------------------------------------------------

#[test]
fn lint_fix_text_explains_each_conflict() {
    // MD012 (collapse multiple blanks) and MD047 (single trailing newline)
    // both want the run of blank lines at EOF, so one of them loses.
    let tmp = vault(&[("conflict.md", "---\ntitle: C\n---\n\nbody\n\n\n\n")]);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--fix", "--dry-run", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("conflicts 1"),
        "the fixture must actually produce an overlap: {stdout}"
    );
    // Before iteration 263 this printed `conflicts 1` and nothing else: the
    // per-file conflict line existed but was suppressed because MD047 also
    // appeared under `would fix`, and it carried no line to tell the two
    // violations apart.
    assert!(
        stdout.contains("conflict  MD047  line 5: range overlap with MD012"),
        "a conflict must name its rule, line and blocker: {stdout}"
    );
}

#[test]
fn lint_fix_json_carries_the_conflict_line() {
    let tmp = vault(&[("conflict.md", "---\ntitle: C\n---\n\nbody\n\n\n\n")]);

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["lint", "--fix", "--dry-run", "--format", "json"])
        .output()
        .unwrap();
    let results: ExtLintFixOutput = typed_results(&output.stdout);
    for file in &results.files {
        assert_eq!(
            file.conflicts_total,
            file.conflicts.len(),
            "an uncapped run reports every conflict it listed"
        );
        for conflict in &file.conflicts {
            assert!(
                conflict.line > 0,
                "every conflict carries a 1-based line: {conflict:?}"
            );
        }
    }
}
