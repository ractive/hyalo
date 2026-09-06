//! Iteration 280 — the fuzzy candidacy gate normalises the stem (DEC-325).
//!
//! Iteration 279 taught the *scorer* that a camelCase run is a sequence of
//! words, so `MyLongNote` and `my-long-note` are the same name at confidence
//! 1.0. The pair never reached it: `LinkMatcher::fuzzy_shortlist`'s candidacy
//! gate — the cheap prefilter that decides which files are worth scoring —
//! compared the raw, case-sensitive stems, where Jaro-Winkler puts the same
//! pair at 0.53, well under the 0.8 `--threshold`. The gate now compares
//! `link_score::gate_key` normal forms; a plain lowercase-hyphen slug is its
//! own key, so nothing about a documentation corpus changes.

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

fn hyalo_no_hints() -> Command {
    crate::common::hyalo_no_hints()
}

fn write_md(dir: &std::path::Path, rel: &str, body: &str) {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, body).unwrap();
}

fn json(tmp: &TempDir, args: &[&str]) -> Value {
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path().to_str().unwrap())
        .args(args)
        .args(["--format", "json"])
        .output()
        .unwrap();
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "not JSON: {e}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

/// Every fuzzy plan in the vault, as `(new_target, confidence, below_floor)`.
fn fuzzy_plans(tmp: &TempDir) -> Vec<(String, f64, bool)> {
    let fix = json(tmp, &["links", "fix", "--dry-run"]);
    fix["results"]["fuzzy_fixes"]
        .as_array()
        .expect("fuzzy_fixes is always an array")
        .iter()
        .map(|p| {
            (
                p["new_target"].as_str().unwrap_or_default().to_string(),
                p["confidence"].as_f64().unwrap_or_default(),
                p["below_floor"].as_bool().unwrap_or(false),
            )
        })
        .collect()
}

/// The gap the iteration exists to close. `[[my-long-note]]` in an Obsidian
/// vault whose notes are named in prose case must reach `MyLongNote.md`, and at
/// the perfect confidence the scorer already assigned it — so `--apply-fuzzy`
/// writes it rather than leaving the link unfixable.
#[test]
fn a_separator_spelling_reaches_its_camel_case_note() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "Notes/MyLongNote.md", "x\n");
    write_md(tmp.path(), "src.md", "see [[my-long-note]]\n");

    let plans = fuzzy_plans(&tmp);
    assert_eq!(plans.len(), 1, "expected exactly one proposal: {plans:?}");
    assert!(
        plans[0].0.ends_with("MyLongNote.md"),
        "wrong candidate: {plans:?}"
    );
    assert!(
        (plans[0].1 - 1.0).abs() < 1e-9,
        "the scorer rates this pair 1.0: {plans:?}"
    );
    assert!(!plans[0].2, "and it must be applicable: {plans:?}");
}

/// The same for an acronym head, where the camelCase boundary sits at the last
/// uppercase of the run (`HTMLParser` → `html` + `parser`).
#[test]
fn an_acronym_head_reaches_its_slug_spelling() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "Notes/HTMLParser.md", "x\n");
    write_md(tmp.path(), "src.md", "see [[html-parser]]\n");

    let plans = fuzzy_plans(&tmp);
    assert_eq!(plans.len(), 1, "expected exactly one proposal: {plans:?}");
    assert!(
        plans[0].0.ends_with("HTMLParser.md"),
        "wrong candidate: {plans:?}"
    );
    assert!(!plans[0].2 && plans[0].1 > 0.99, "{plans:?}");
}

/// `--apply-fuzzy` actually writes it: the point of clearing the gate is that
/// the link stops being unfixable, and the rewrite round-trips (it resolves).
#[test]
fn apply_fuzzy_writes_the_camel_case_repair() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "Notes/MyLongNote.md", "x\n");
    write_md(tmp.path(), "src.md", "see [[my-long-note]]\n");

    let applied = json(&tmp, &["links", "fix", "--apply", "--apply-fuzzy"]);
    assert_eq!(applied["results"]["fuzzy"].as_u64(), Some(1), "{applied}");
    assert_eq!(applied["results"]["fuzzy_below_floor"].as_u64(), Some(0));
    assert_eq!(applied["results"]["unapplied"].as_u64(), Some(0));

    let body = std::fs::read_to_string(tmp.path().join("src.md")).unwrap();
    assert!(body.contains("MyLongNote"), "not rewritten: {body}");

    // Nothing is broken any more.
    let after = json(&tmp, &["links", "fix", "--dry-run"]);
    assert_eq!(after["results"]["broken"].as_u64(), Some(0), "{after}");
}

/// Widening candidacy is not removing it: two genuinely different names still
/// fail the gate, and a name that merely *gains a word* is still reported
/// rather than applied (DEC-324's charge survives the wider gate).
#[test]
fn the_gate_still_gates() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "Notes/CompletelyUnrelated.md", "x\n");
    write_md(tmp.path(), "src.md", "see [[xyz-abc-notexist]]\n");
    assert!(fuzzy_plans(&tmp).is_empty());

    let tmp2 = TempDir::new().unwrap();
    write_md(tmp2.path(), "Notes/CatMuse.md", "x\n");
    write_md(tmp2.path(), "src.md", "see [[Cat]]\n");
    for (target, confidence, below_floor) in fuzzy_plans(&tmp2) {
        assert!(
            below_floor,
            "[[Cat]] -> {target} at {confidence} must still not be applicable"
        );
    }
}

/// The regression guard for the corpora the gate was tuned on: a
/// lowercase-hyphen slug is its own gate key, so a documentation-shaped vault
/// sees exactly the proposals and confidences it saw before iteration 280.
#[test]
fn a_documentation_slug_vault_is_unchanged() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "guides/configuration.md", "x\n");
    write_md(tmp.path(), "graphql/reference/actions.md", "x\n");
    write_md(
        tmp.path(),
        "src.md",
        "[a](guides/configuraton)\n[b](/actions)\n",
    );

    let mut plans = fuzzy_plans(&tmp);
    plans.sort_by(|a, b| b.1.total_cmp(&a.1));
    assert_eq!(plans.len(), 2, "{plans:?}");
    // The same-directory typo stays the applicable one …
    assert!(plans[0].0.ends_with("guides/configuration.md"), "{plans:?}");
    assert!(!plans[0].2 && plans[0].1 >= 0.8, "{plans:?}");
    // … and the cross-tree same-name substitution stays at exactly the 0.7
    // basename weight, below the floor.
    assert!(plans[1].0.ends_with("actions.md"), "{plans:?}");
    assert!((plans[1].1 - 0.7).abs() < 1e-9 && plans[1].2, "{plans:?}");
}
