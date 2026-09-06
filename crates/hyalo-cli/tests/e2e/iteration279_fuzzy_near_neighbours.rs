//! Iteration 279 — the fuzzy scorer and near-neighbour stems (DEC-324).
//!
//! DEC-319 (iter-277) damps a fuzzy winner that only just outran a real
//! runner-up. Three wrong above-floor proposals from the post-batch-271-274
//! dogfood of the Obsidian Hub survived it untouched, because their runner-up
//! is not a near-tie — it is absent or far away, and the *absolute* score was
//! simply too generous. Each test below reproduces one of them at the shape it
//! has on the Hub, plus the correct match that must not move.

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

/// Every fuzzy plan produced for `src.md`, as `(new_target, confidence,
/// below_floor)`.
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

/// `[[Cat]]` → `CatMuse.md` scored 0.867 and was written by `--apply-fuzzy`.
/// `CatMuse` is one opaque token to a separator-only tokeniser, so `Cat` looks
/// like a typo of it; split at the camelCase boundary it is a name missing a
/// whole word.
#[test]
fn a_camel_case_note_missing_a_whole_word_is_not_a_typo() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "People/CatMuse.md", "x\n");
    write_md(tmp.path(), "src.md", "see [[Cat]]\n");

    for (target, confidence, below_floor) in fuzzy_plans(&tmp) {
        assert!(
            below_floor,
            "[[Cat]] -> {target} at {confidence} must not be applicable"
        );
    }
}

/// `[[paulbricman]]` → `paultreanor.md` scored 0.855: two different people
/// whose only common ground is a given name. Jaro-Winkler's prefix bonus is
/// what carried the pair over the token floor — plain Jaro is 0.758.
#[test]
fn a_shared_given_name_does_not_buy_token_identity() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "People/paultreanor.md", "x\n");
    write_md(tmp.path(), "src.md", "see [[paulbricman]]\n");

    for (target, confidence, below_floor) in fuzzy_plans(&tmp) {
        assert!(
            below_floor,
            "[[paulbricman]] -> {target} at {confidence} must not be applicable"
        );
    }
}

/// `[[obsidian-floating-toc-plugin]]` → `obsidian-plugin-toc.md` scored 0.857:
/// three of four tokens matched and recall was a perfect 1.0, so the harmonic
/// mean absorbed the one token — `floating`, the word that names the plugin
/// and a third of the target's characters.
#[test]
fn a_dropped_word_costs_its_character_share() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "Plugins/obsidian-plugin-toc.md", "x\n");
    write_md(
        tmp.path(),
        "src.md",
        "see [[obsidian-floating-toc-plugin]]\n",
    );

    for (target, confidence, below_floor) in fuzzy_plans(&tmp) {
        assert!(
            below_floor,
            "[[obsidian-floating-toc-plugin]] -> {target} at {confidence} must not be applicable"
        );
    }
}

/// The Hub's one *correct* above-floor fuzzy fix. It must keep its full
/// confidence even though camelCase tokenisation lifts `ObsidianPublisher.md`
/// — a genuinely similar name — from 0.596 to 0.978: DEC-319's contested
/// margin damps a guess, and an exact match is not a guess.
#[test]
fn an_exact_match_is_never_contested_by_a_worse_candidate() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "Concepts/Obsidian Publish.md", "x\n");
    write_md(tmp.path(), "People/ObsidianPublisher.md", "x\n");
    write_md(tmp.path(), "src.md", "see [[Obsidian Publish.]]\n");

    let plans = fuzzy_plans(&tmp);
    let winner = plans
        .iter()
        .find(|(target, _, _)| target.ends_with("Obsidian Publish.md"))
        .unwrap_or_else(|| panic!("the real match must still be proposed: {plans:?}"));
    assert!(
        (winner.1 - 1.0).abs() < 1e-9,
        "the real match must keep its 1.0, got {}",
        winner.1
    );
    assert!(!winner.2, "and must stay applicable: {winner:?}");
}

/// The tightening must not cost the case fuzzy matching exists for: a typo in
/// a stem, with the rest of the name intact.
#[test]
fn a_typo_is_still_fixed() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "Guides/configuration.md", "x\n");
    write_md(tmp.path(), "src.md", "see [[configuraton]]\n");

    let plans = fuzzy_plans(&tmp);
    assert_eq!(plans.len(), 1, "expected exactly one proposal: {plans:?}");
    assert!(
        !plans[0].2 && plans[0].1 > 0.9,
        "a typo fix must stay applicable at high confidence: {plans:?}"
    );
}

/// A basename that gains a whole word is a different document, so it is
/// reported for review rather than written by `--apply-fuzzy`. This is the
/// deliberate narrowing DEC-324 buys: `explained_mass` charges `archive` for
/// its seven of twenty-nine characters.
#[test]
fn a_basename_that_gains_a_word_is_reported_not_applied() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "decision-log-archive.md", "x\n");
    write_md(tmp.path(), "src.md", "see [[decision-log]]\n");

    let plans = fuzzy_plans(&tmp);
    assert_eq!(plans.len(), 1, "expected exactly one proposal: {plans:?}");
    assert!(
        plans[0].2 && plans[0].1 > 0.5,
        "still worth reporting, never applied by default: {plans:?}"
    );
}
