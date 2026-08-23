//! e2e: minimal-diff frontmatter writes (iter-214).
//!
//! Every frontmatter mutation must touch only the lines belonging to the keys
//! it changes. The regression these tests lock down: adding one property to a
//! real GitHub Docs `index.md` used to rewrite 116 of its 198 frontmatter
//! lines (long list items refolded into `>-` block scalars, `'` quote style
//! flipped to `"`), which made `hyalo set` unusable on any docs repo under
//! code review.

use super::common::{hyalo_no_hints, write_md};
use std::fs;
use tempfile::TempDir;

/// Frontmatter shaped like a real GitHub Docs `index.md`: single-quoted
/// version globs, `>-` folded intros, nested feature maps, long redirect and
/// child lists.
const GH_DOCS_FRONTMATTER: &str = "\
title: GitHub Actions documentation
shortTitle: GitHub Actions
intro: >-
  Automate, customize, and execute your software development workflows right
  in your repository with GitHub Actions. You can discover, create, and share
  actions to perform any job you'd like, including CI/CD.
allowTitleToDifferFromFilename: true
introLinks:
  quickstart: /actions/quickstart
  reference: /actions/reference
featuredLinks:
  startHere:
    - /actions/learn-github-actions/understanding-github-actions
    - /actions/learn-github-actions/finding-and-customizing-actions
    - /actions/examples/using-scripts-to-test-your-code-on-a-runner
  guideCards:
    - /actions/deployment/deploying-to-amazon-elastic-container-service
    - /actions/deployment/deploying-to-azure-app-service
  popular:
    - /actions/writing-workflows/workflow-syntax-for-github-actions
    - /actions/writing-workflows/events-that-trigger-workflows
changelog:
  label: actions
  prefix: 'GitHub Actions: '
redirect_from:
  - /articles/automating-your-workflow-with-github-actions
  - /articles/customizing-your-project-with-github-actions
  - /categories/automating-your-workflow-with-github-actions
layout: product-landing
versions:
  fpt: '*'
  ghes: '*'
  ghec: '*'
topics:
  - CI
  - CD
  - Developer
children:
  - /concepts
  - /tutorials
  - /how-tos
  - /reference
";

fn gh_docs_file(tmp: &TempDir) {
    write_md(
        tmp.path(),
        "index.md",
        &format!("---\n{GH_DOCS_FRONTMATTER}---\n\n# GitHub Actions\n\nBody text.\n"),
    );
}

/// Number of lines present in exactly one of the two texts — a coarse but
/// direction-agnostic stand-in for `git diff` line count.
fn diff_line_count(before: &str, after: &str) -> usize {
    let b: Vec<&str> = before.lines().collect();
    let a: Vec<&str> = after.lines().collect();
    a.iter().filter(|l| !b.contains(l)).count() + b.iter().filter(|l| !a.contains(l)).count()
}

fn run(tmp: &TempDir, args: &[&str]) -> (bool, String) {
    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(args)
        .output()
        .unwrap();
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

// ---------------------------------------------------------------------------
// The headline acceptance criterion
// ---------------------------------------------------------------------------

#[test]
fn set_adds_one_property_to_github_docs_index_and_changes_one_line() {
    let tmp = TempDir::new().unwrap();
    gh_docs_file(&tmp);
    let before = fs::read_to_string(tmp.path().join("index.md")).unwrap();

    let (ok, stderr) = run(&tmp, &["set", "index.md", "--property", "status=reviewed"]);
    assert!(ok, "set failed: {stderr}");
    assert!(
        !stderr.contains("rewriting the entire frontmatter block"),
        "no fallback expected for plain GitHub Docs frontmatter: {stderr}"
    );

    let after = fs::read_to_string(tmp.path().join("index.md")).unwrap();
    assert_eq!(
        diff_line_count(&before, &after),
        1,
        "adding one property must change exactly one line\n--- before ---\n{before}\n--- after ---\n{after}"
    );
    assert!(after.contains("status: reviewed\n"));
    // Every original frontmatter line survives byte-identically.
    for line in GH_DOCS_FRONTMATTER.lines() {
        assert!(
            after.contains(&format!("{line}\n")),
            "original line lost or reformatted: {line:?}\n{after}"
        );
    }
}

#[test]
fn set_changing_one_value_leaves_the_rest_byte_identical() {
    let tmp = TempDir::new().unwrap();
    gh_docs_file(&tmp);
    let before = fs::read_to_string(tmp.path().join("index.md")).unwrap();

    let (ok, stderr) = run(&tmp, &["set", "index.md", "--property", "layout=landing"]);
    assert!(ok, "set failed: {stderr}");

    let after = fs::read_to_string(tmp.path().join("index.md")).unwrap();
    assert_eq!(
        diff_line_count(&before, &after),
        2,
        "changing one value must change exactly one line in each direction\n{after}"
    );
    assert!(after.contains("layout: landing\n"));
    assert!(!after.contains("layout: product-landing\n"));
}

#[test]
fn remove_drops_only_the_removed_key() {
    let tmp = TempDir::new().unwrap();
    gh_docs_file(&tmp);
    let before = fs::read_to_string(tmp.path().join("index.md")).unwrap();

    let (ok, stderr) = run(&tmp, &["remove", "index.md", "--property", "layout"]);
    assert!(ok, "remove failed: {stderr}");

    let after = fs::read_to_string(tmp.path().join("index.md")).unwrap();
    assert_eq!(diff_line_count(&before, &after), 1, "{after}");
    assert!(!after.contains("layout:"));
    assert!(after.contains("changelog:\n  label: actions\n"));
}

#[test]
fn append_to_a_list_rewrites_only_that_list() {
    let tmp = TempDir::new().unwrap();
    gh_docs_file(&tmp);
    let before = fs::read_to_string(tmp.path().join("index.md")).unwrap();

    let (ok, stderr) = run(
        &tmp,
        &["append", "index.md", "--property", "topics=Actions"],
    );
    assert!(ok, "append failed: {stderr}");

    let after = fs::read_to_string(tmp.path().join("index.md")).unwrap();
    assert!(after.contains("  - Actions\n"));
    // Only the appended item is new; the `intro`, `featuredLinks` and
    // `children` blocks must be untouched.
    assert_eq!(diff_line_count(&before, &after), 1, "{after}");
    assert!(after.contains("changelog:\n  label: actions\n  prefix: 'GitHub Actions: '\n"));
}

// ---------------------------------------------------------------------------
// Preservation corpus: quote styles, block scalars, indentation, CRLF
// ---------------------------------------------------------------------------

#[test]
fn quote_styles_and_block_scalars_survive_a_set() {
    let tmp = TempDir::new().unwrap();
    let fm = "\
double: \"a double quoted value\"
single: 'a single quoted value'
plain: an unquoted value
literal: |
  first literal line
  second literal line
folded: >-
  folded line one
  folded line two
flow: [alpha, beta, gamma]
flowMap: {a: 1, b: 2}
empty: ''
nullish: null
number: 42
boolish: true
";
    write_md(tmp.path(), "note.md", &format!("---\n{fm}---\n\nBody\n"));

    let (ok, stderr) = run(&tmp, &["set", "note.md", "--property", "added=1"]);
    assert!(ok, "set failed: {stderr}");
    let after = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert_eq!(
        after,
        format!("---\n{fm}added: 1\n---\n\nBody\n"),
        "every original line must be byte-identical"
    );
}

#[test]
fn compact_list_indentation_survives_a_set() {
    let tmp = TempDir::new().unwrap();
    let fm = "\
title: Compact
tags:
- one
- two
children:
- /a
- /b
";
    write_md(tmp.path(), "note.md", &format!("---\n{fm}---\n\nBody\n"));

    let (ok, stderr) = run(&tmp, &["set", "note.md", "--property", "status=draft"]);
    assert!(ok, "set failed: {stderr}");
    let after = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert_eq!(after, format!("---\n{fm}status: draft\n---\n\nBody\n"));
}

#[test]
fn unusual_indentation_and_nesting_survive_a_set() {
    let tmp = TempDir::new().unwrap();
    let fm = "\
versions:
    fpt: '*'
    ghes: '>=3.9'
    nested:
        deep:
            deeper: yes-value
title: Odd indentation
";
    write_md(tmp.path(), "note.md", &format!("---\n{fm}---\n\nBody\n"));

    let (ok, stderr) = run(&tmp, &["set", "note.md", "--property", "status=draft"]);
    assert!(ok, "set failed: {stderr}");
    let after = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert_eq!(after, format!("---\n{fm}status: draft\n---\n\nBody\n"));
}

#[test]
fn comments_are_preserved_and_travel_with_their_key() {
    let tmp = TempDir::new().unwrap();
    let fm = "\
# document-level header comment
title: Commented

# explains the status field
status: planned
tags:
  - a
# trailing note
";
    write_md(tmp.path(), "note.md", &format!("---\n{fm}---\n\nBody\n"));

    let (ok, stderr) = run(&tmp, &["set", "note.md", "--property", "status=completed"]);
    assert!(ok, "set failed: {stderr}");
    let after = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(after.contains("# document-level header comment\n"));
    assert!(after.contains("# explains the status field\nstatus: completed\n"));
    assert!(after.contains("# trailing note\n"));

    // Removing the key removes its explanatory comment with it.
    let (ok, stderr) = run(&tmp, &["remove", "note.md", "--property", "status"]);
    assert!(ok, "remove failed: {stderr}");
    let after = fs::read_to_string(tmp.path().join("note.md")).unwrap();
    assert!(!after.contains("# explains the status field"));
    assert!(after.contains("# document-level header comment\n"));
    assert!(after.contains("# trailing note\n"));
}

#[test]
fn crlf_files_stay_crlf_and_keep_untouched_lines() {
    let tmp = TempDir::new().unwrap();
    let content =
        "---\r\ntitle: 'CRLF note'\r\nstatus: planned\r\ntags:\r\n  - a\r\n---\r\n\r\nBody\r\n";
    write_md(tmp.path(), "crlf.md", content);

    let (ok, stderr) = run(&tmp, &["set", "crlf.md", "--property", "status=done"]);
    assert!(ok, "set failed: {stderr}");
    let after = fs::read_to_string(tmp.path().join("crlf.md")).unwrap();
    assert!(after.contains("title: 'CRLF note'\r\n"), "{after:?}");
    assert!(after.contains("status: done\r\n"), "{after:?}");
    assert!(after.contains("  - a\r\n"), "{after:?}");
    assert!(!after.contains("\n\n"), "no bare LF expected: {after:?}");
}

// ---------------------------------------------------------------------------
// Fallback behaviour: explicit, warned, never silent
// ---------------------------------------------------------------------------

#[test]
fn unmappable_frontmatter_warns_before_rewriting_the_block() {
    let tmp = TempDir::new().unwrap();
    // Explicit-key syntax (`? key` / `: value`) is deliberately not modelled by
    // the splicer, so the write falls back to a full re-serialization.
    let fm = "? complex key\n: a value\ntitle: Odd\n";
    write_md(tmp.path(), "odd.md", &format!("---\n{fm}---\n\nBody\n"));

    let (ok, stderr) = run(&tmp, &["set", "odd.md", "--property", "status=draft"]);
    assert!(ok, "set failed: {stderr}");
    assert!(
        stderr.contains("rewriting the entire frontmatter block"),
        "fallback must warn, never churn silently: {stderr}"
    );
    assert!(
        stderr.contains("cannot be mapped to per-key line spans"),
        "warning must say why: {stderr}"
    );

    let after = fs::read_to_string(tmp.path().join("odd.md")).unwrap();
    assert!(after.contains("status: draft"));
    assert!(after.contains("Body"));
}

#[test]
fn ordinary_writes_never_warn_about_a_full_rewrite() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "plain.md",
        "---\ntitle: Plain\nstatus: planned\n---\n\nBody\n",
    );
    let (ok, stderr) = run(&tmp, &["set", "plain.md", "--property", "status=done"]);
    assert!(ok, "set failed: {stderr}");
    assert!(
        !stderr.contains("rewriting the entire frontmatter block"),
        "unexpected fallback: {stderr}"
    );

    // A file with no frontmatter at all gains one without a warning.
    write_md(tmp.path(), "bare.md", "# No frontmatter\n");
    let (ok, stderr) = run(&tmp, &["set", "bare.md", "--property", "title=Added"]);
    assert!(ok, "set failed: {stderr}");
    assert!(
        !stderr.contains("rewriting the entire frontmatter block"),
        "creating frontmatter is not a rewrite: {stderr}"
    );

    // An empty frontmatter block has nothing to preserve — also no warning.
    write_md(tmp.path(), "emptyfm.md", "---\n---\n\nBody\n");
    let (ok, stderr) = run(&tmp, &["set", "emptyfm.md", "--property", "title=Added"]);
    assert!(ok, "set failed: {stderr}");
    assert!(
        !stderr.contains("rewriting the entire frontmatter block"),
        "empty block is not churn: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// The other frontmatter writers share the same path
// ---------------------------------------------------------------------------

#[test]
fn tags_rename_preserves_surrounding_formatting() {
    let tmp = TempDir::new().unwrap();
    gh_docs_file(&tmp);
    // `topics` is the GitHub Docs list; give the file a `tags` list too so the
    // tag rewriter has something to touch.
    let (ok, stderr) = run(&tmp, &["append", "index.md", "--property", "tags=ci"]);
    assert!(ok, "append failed: {stderr}");
    let before = fs::read_to_string(tmp.path().join("index.md")).unwrap();

    let (ok, stderr) = run(&tmp, &["tags", "rename", "--from", "ci", "--to", "actions"]);
    assert!(ok, "tags rename failed: {stderr}");
    assert!(
        !stderr.contains("rewriting the entire frontmatter block"),
        "unexpected fallback: {stderr}"
    );
    let after = fs::read_to_string(tmp.path().join("index.md")).unwrap();
    assert!(after.contains("  - actions\n"), "{after}");
    // Only the renamed tag line differs, in each direction.
    assert_eq!(diff_line_count(&before, &after), 2, "{after}");
}

#[test]
fn properties_rename_preserves_surrounding_formatting() {
    let tmp = TempDir::new().unwrap();
    gh_docs_file(&tmp);
    let before = fs::read_to_string(tmp.path().join("index.md")).unwrap();

    let (ok, stderr) = run(
        &tmp,
        &[
            "properties",
            "rename",
            "--from",
            "layout",
            "--to",
            "pageLayout",
        ],
    );
    assert!(ok, "properties rename failed: {stderr}");
    assert!(
        !stderr.contains("rewriting the entire frontmatter block"),
        "unexpected fallback: {stderr}"
    );
    let after = fs::read_to_string(tmp.path().join("index.md")).unwrap();
    assert!(after.contains("pageLayout: product-landing\n"), "{after}");
    assert_eq!(diff_line_count(&before, &after), 2, "{after}");
}
