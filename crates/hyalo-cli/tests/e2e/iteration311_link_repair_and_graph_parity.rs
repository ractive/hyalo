//! Iteration 311: link repair and graph parity.
//!
//! Regression coverage for the dogfood findings fixed in this iteration:
//! BUG-1 (anchor fragment form by link kind), BUG-2 (`summary.links.broken`
//! vs. `find --broken-links` / HYALO006 parity on alias links), UX-7 (one
//! anchor-suggestion chooser, `Fixable:` counts anchor fixes) and the Hints
//! task (alias-only fixes get an `--apply` hint; `lint --rule-prefix HYALO`
//! hints `find --broken-links`).
use super::common::{hyalo, hyalo_no_hints, write_md};
use serde_json::Value;
use tempfile::TempDir;

fn links_fix(dir: &std::path::Path, extra: &[&str]) -> Value {
    let mut cmd = hyalo_no_hints();
    cmd.arg("--dir")
        .arg(dir)
        .args(["links", "fix", "--format", "json"])
        .args(extra);
    let output = cmd.assert().success().get_output().stdout.clone();
    let value: Value = serde_json::from_slice(&output).unwrap();
    value["results"].clone()
}

fn find_links(dir: &std::path::Path, file: &str) -> Value {
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(dir)
        .args([
            "find", "--file", file, "--fields", "links", "--format", "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&output).unwrap();
    value["results"][0]["links"].clone()
}

// ---------------------------------------------------------------------------
// BUG-1 (DEC-351): a wikilink anchor fix writes the heading TEXT; a markdown
// link anchor fix keeps the GFM slug. Obsidian matches a wikilink fragment
// against heading text and never resolves a slug there.
// ---------------------------------------------------------------------------

#[test]
fn anchor_fix_writes_heading_text_into_a_wikilink_and_slug_into_a_markdown_link() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "source.md",
        "[[target#Deploy Steps]]\n[md](target.md#deploy-steps)\n",
    );
    write_md(tmp.path(), "target.md", "## 3. Deploy Steps\n");

    let dry = links_fix(tmp.path(), &["--dry-run"]);
    assert_eq!(dry["broken_anchors"], 2);
    assert_eq!(dry["anchor_fixable"], 2);
    let fixes = dry["anchor_fixes"].as_array().unwrap();
    let wikilink_fix = fixes.iter().find(|f| f["line"] == 1).unwrap();
    assert_eq!(wikilink_fix["new_fragment"], "3. Deploy Steps");
    let markdown_fix = fixes.iter().find(|f| f["line"] == 2).unwrap();
    assert_eq!(markdown_fix["new_fragment"], "3-deploy-steps");

    let applied = links_fix(tmp.path(), &["--apply"]);
    assert_eq!(applied["anchors_applied"], 2);
    assert_eq!(
        applied["deferred_anchor_fixes"].as_array().unwrap().len(),
        0,
        "the publication pass must recognise the same kind-dependent\
         fragment the planner chose -- not re-derive the slug for a\
         wikilink and defer on a false mismatch: {applied:?}"
    );

    let content = std::fs::read_to_string(tmp.path().join("source.md")).unwrap();
    assert_eq!(
        content,
        "[[target#3. Deploy Steps]]\n[md](target.md#3-deploy-steps)\n"
    );

    // Both links now resolve -- Obsidian's rule is satisfied for the
    // wikilink (heading text) and the markdown link (GFM slug) alike.
    let links = find_links(tmp.path(), "source.md");
    for link in links.as_array().unwrap() {
        assert_ne!(
            link["broken_anchor"].as_bool(),
            Some(true),
            "{link:?} should resolve after the fix"
        );
    }

    // A second apply is a no-op: both anchors are already correct.
    let again = links_fix(tmp.path(), &["--apply"]);
    assert_eq!(again["broken_anchors"], 0);
    assert_eq!(again["anchors_applied"], 0);
}

// ---------------------------------------------------------------------------
// BUG-2 (amends DEC-318): `summary.links.broken` must count a bare
// `[[alias]]` link exactly as `find --broken-links` and HYALO006 do.
// ---------------------------------------------------------------------------

fn summary_broken(dir: &std::path::Path) -> i64 {
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(dir)
        .args(["summary", "--format", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&output).unwrap();
    value["results"]["links"]["broken"].as_i64().unwrap()
}

fn hyalo006_violations(dir: &std::path::Path) -> i64 {
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(dir)
        .args(["lint", "--rule", "HYALO006", "--format", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&output).unwrap();
    value["results"]["violations"].as_i64().unwrap()
}

#[test]
fn summary_broken_count_includes_bare_alias_links_like_find_and_hyalo006() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "lf.md",
        "---\naliases: [Leah]\n---\n# Leah Ferguson\n",
    );
    write_md(tmp.path(), "talk.md", "[[Leah]] and [[Nobody]]\n");

    // Two broken links: the bare alias `[[Leah]]` (DEC-308: Obsidian does
    // not resolve it) and `[[Nobody]]` (no file, no alias).
    assert_eq!(summary_broken(tmp.path()), 2);
    assert_eq!(hyalo006_violations(tmp.path()), 2);

    let dry = links_fix(tmp.path(), &["--dry-run"]);
    assert_eq!(dry["alias_fixes"], 1);
    assert_eq!(dry["broken"], 1, "Nobody only -- Leah is in alias_fixes");
    // summary's broken total folds alias_fixes back in so all three surfaces
    // agree: broken (1, Nobody) + alias_fixes (1, Leah) = 2.
    assert_eq!(
        dry["broken"].as_i64().unwrap() + dry["alias_fixes"].as_i64().unwrap(),
        2
    );
}

#[test]
fn summary_broken_count_matches_find_and_hyalo006_with_aliases_enabled() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "lf.md",
        "---\naliases: [Leah]\n---\n# Leah Ferguson\n",
    );
    write_md(tmp.path(), "talk.md", "[[Leah]] and [[Nobody]]\n");
    write_md(tmp.path(), ".hyalo.toml", "[links]\naliases = true\n");

    // With `aliases = true` (iteration 272 resolution restored), `[[Leah]]`
    // resolves and only `[[Nobody]]` is broken.
    assert_eq!(summary_broken(tmp.path()), 1);
    assert_eq!(hyalo006_violations(tmp.path()), 1);
}

// ---------------------------------------------------------------------------
// UX-7: `find --broken-links` and `links fix` share one chooser
// (`anchor::unique_heading_by_prefix`) for anchor suggestions, a deferred
// anchor fix carries that suggestion plus its own reason, and the `Fixable:`
// text line counts anchor fixes so it cannot read 0 next to an "Apply N
// fixes" hint.
// ---------------------------------------------------------------------------

#[test]
fn deferred_anchor_fix_carries_the_same_suggestion_find_shows() {
    let tmp = TempDir::new().unwrap();
    // "Intro" is broken (no heading slugs to it exactly), has no numbered
    // heading to repair from, but is the unique prefix of "Introduction" --
    // the same fragment `find --broken-links` would suggest.
    write_md(tmp.path(), "note.md", "[[note#Intro]]\n## Introduction\n");

    let fix = links_fix(tmp.path(), &["--dry-run"]);
    assert_eq!(fix["anchor_fixable"], 0);
    assert_eq!(fix["anchors_deferred"], 1);
    let deferred = &fix["deferred_anchor_fixes"][0];
    assert_eq!(deferred["suggested_fragment"], "Introduction");
    assert!(
        deferred["reason"].as_str().unwrap().contains("numbered"),
        "{deferred:?}"
    );

    let links = find_links(tmp.path(), "note.md");
    assert_eq!(
        links[0]["suggested_fragment"], "Introduction",
        "find --broken-links must offer the identical suggestion: {links:?}"
    );
}

#[test]
fn fixable_text_line_counts_anchor_fixes() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "source.md", "[[target#Deploy Steps]]\n");
    write_md(tmp.path(), "target.md", "## 3. Deploy Steps\n");

    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["links", "fix", "--format", "text"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    assert!(
        text.contains("Fixable: 1"),
        "an anchor-only vault must not read \"Fixable: 0\": {text}"
    );
}

#[test]
fn alias_only_fixes_get_an_apply_hint() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "lf.md",
        "---\naliases: [Leah]\n---\n# Leah Ferguson\n",
    );
    write_md(tmp.path(), "talk.md", "[[Leah]]\n");

    let output = hyalo()
        .arg("--dir")
        .arg(tmp.path())
        .args(["links", "fix", "--format", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&output).unwrap();
    let hints = value["hints"].as_array().unwrap();
    assert!(
        hints
            .iter()
            .any(|h| h["cmd"].as_str().unwrap_or("").contains("--apply")
                && !h["cmd"].as_str().unwrap_or("").contains("--apply-fuzzy")),
        "an alias-only dry run must still offer an --apply hint: {hints:?}"
    );
}

#[test]
fn lint_rule_prefix_hyalo_hints_find_broken_links() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "[[missing]]\n");

    let output = hyalo()
        .arg("--dir")
        .arg(tmp.path())
        .args(["lint", "--rule-prefix", "HYALO", "--format", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&output).unwrap();
    let hints = value["hints"].as_array().unwrap();
    assert!(
        hints
            .iter()
            .any(|h| h["cmd"].as_str().unwrap_or("").contains("--broken-links")),
        "lint --rule-prefix HYALO must hint find --broken-links: {hints:?}"
    );
}

// ---------------------------------------------------------------------------
// BUG-17: `links fix`'s case-mismatch (and relocation/fuzzy/certain) text
// rendering must show the byte-truthful `emitted_target`, not the
// vault-relative `new_target` a wikilink write never puts on disk verbatim
// (a wikilink always drops `.md`).
// ---------------------------------------------------------------------------

#[test]
fn case_mismatch_text_shows_the_truthful_emitted_target_not_new_target() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "src.md", "[[sub/Note.MD]]\n");
    write_md(tmp.path(), "sub/note.md", "# Note\n");

    let dry = links_fix(tmp.path(), &["--dry-run"]);
    let plan = &dry["case_mismatch_fixes"][0];
    assert_eq!(plan["new_target"], "sub/note.md");
    assert_eq!(
        plan["emitted_target"], "sub/note",
        "the write layer always drops the wikilink's .md suffix: {plan:?}"
    );

    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["links", "fix", "--format", "text"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    assert!(
        text.contains(r#""sub/Note" → "sub/note""#),
        "text rendering must show the truthful emitted_target, not \
         \"sub/note.md\" (which --apply never writes): {text}"
    );
}

// ---------------------------------------------------------------------------
// BUG-15: a site-absolute bare `/` resolves to the vault-root `index.md`
// when it exists, consistent with `/dir` resolving to `dir/index.md`.
// ---------------------------------------------------------------------------

#[test]
fn bare_root_slash_resolves_to_vault_root_index() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "index.md", "# Root\n");
    write_md(tmp.path(), "a.md", "[root](/)\n");

    let links = find_links(tmp.path(), "a.md");
    assert_eq!(links[0]["path"], "index.md", "{links:?}");
}

#[test]
fn site_absolute_target_equal_to_the_configured_prefix_resolves_to_root_index() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "index.md", "# Root\n");
    write_md(tmp.path(), "a.md", "[root](/docs/)\n");
    write_md(tmp.path(), ".hyalo.toml", "site_prefix = \"docs\"\n");

    let links = find_links(tmp.path(), "a.md");
    assert_eq!(links[0]["path"], "index.md", "{links:?}");
}

// ---------------------------------------------------------------------------
// UX-3: `--apply-fuzzy` (or `--min-confidence`) without `--apply` must not
// claim anything was written. `fuzzy_applied` is false on every dry run, a
// separate `fuzzy_requested` lets the text renderer distinguish "never
// opted in" from "opted in but this is a dry run", and the two text lines
// read "not written/not applied — pass --apply" in the latter case.
// ---------------------------------------------------------------------------

#[test]
fn apply_fuzzy_without_apply_reports_not_written_and_pass_apply() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "target.md", "# Target\n");
    // "trget" is a one-edit typo of "target" -- a fuzzy match well above the
    // default 0.8 confidence floor.
    write_md(tmp.path(), "source.md", "[[trget]]\n");

    let dry = links_fix(tmp.path(), &["--apply-fuzzy"]);
    assert_eq!(dry["dry_run"], true);
    assert_eq!(dry["applied"], false);
    assert_eq!(
        dry["fuzzy_applied"], false,
        "nothing was written without --apply: {dry:?}"
    );
    assert_eq!(dry["fuzzy_requested"], true);

    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["links", "fix", "--apply-fuzzy", "--format", "text"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    assert!(
        text.contains("not written — pass --apply") && !text.contains("pass --apply-fuzzy"),
        "{text}"
    );

    // Nothing was actually written to disk.
    let content = std::fs::read_to_string(tmp.path().join("source.md")).unwrap();
    assert_eq!(content, "[[trget]]\n");

    // A plain dry run (never opted in at all) keeps the original wording.
    let plain = links_fix(tmp.path(), &[]);
    assert_eq!(plain["fuzzy_requested"], false);
    let plain_text = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["links", "fix", "--format", "text"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let plain_text = String::from_utf8(plain_text).unwrap();
    assert!(plain_text.contains("not written — pass --apply-fuzzy"));
}

// ---------------------------------------------------------------------------
// Hints task: `[[<placeholder>]]` angle-bracket targets go to the templated
// bucket with `{{…}}`, never to fuzzy candidates -- a plugin-manifest
// placeholder is exactly as unknowable as a template expression, and at a
// low enough --min-confidence it used to fuzzy-match a real file.
// ---------------------------------------------------------------------------

#[test]
fn angle_bracket_placeholder_is_templated_not_fuzzy() {
    let tmp = TempDir::new().unwrap();
    // A plausible fuzzy-match decoy: a note whose basename a low
    // --min-confidence would otherwise happily match the placeholder against.
    write_md(tmp.path(), "plugin-id.md", "# Plugin\n");
    write_md(tmp.path(), "manifest.md", "[[<plugin-id>]]\n");

    let fix = links_fix(tmp.path(), &["--min-confidence", "0.0"]);
    assert_eq!(fix["templated"], 1, "{fix:?}");
    let templated = fix["templated_links"].as_array().unwrap();
    assert!(
        templated.iter().any(|l| l["target"] == "<plugin-id>"),
        "{templated:?}"
    );
    assert!(
        fix["fuzzy_fixes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["old_target"] != "<plugin-id>"),
        "a placeholder target must never be offered as a fuzzy candidate: {fix:?}"
    );
}
