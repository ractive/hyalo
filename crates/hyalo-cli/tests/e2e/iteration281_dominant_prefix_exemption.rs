//! Iteration 281 — the dominant-prefix exemption is a prefix *relationship*
//! (DEC-326).
//!
//! Iteration 280 widened the fuzzy candidacy gate and, in doing so, exposed a
//! scorer defect it deliberately left alone: on the Obsidian Hub `[[Mathjax]]`
//! — no such note exists — offered `Plugins/mathpad.md` at 0.886, over the 0.8
//! apply floor. `mathjax` and `mathpad` are one token each and plain Jaro rates
//! them 0.810, *below* `TOKEN_MATCH_FLOOR`; DEC-324's `shares_dominant_prefix`
//! admitted them anyway because the shared `math` covers four of seven
//! characters on both sides, clearing its "at least half" bar. Half is not a
//! prefix relationship, so the exemption now asks whether the prefix *spends*
//! the shorter token — empty remainder (`get`/`getting`) or the lone `e` of
//! `create`/`creating`, not `jax` against `pad`.

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

/// The carry-over this iteration exists to close, reproduced in two files.
#[test]
fn mathjax_does_not_fuzzy_match_mathpad_above_the_floor() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "Plugins/mathpad.md", "x\n");
    write_md(tmp.path(), "src.md", "see [[Mathjax]]\n");

    for (target, confidence, below_floor) in fuzzy_plans(&tmp) {
        assert!(
            below_floor,
            "[[Mathjax]] -> {target} at {confidence} must not be applicable"
        );
    }
}

/// And `--apply-fuzzy` leaves the link alone: the file is not rewritten, and
/// the broken link is still reported as broken rather than quietly repaired
/// into the wrong note.
#[test]
fn apply_fuzzy_does_not_write_the_mathpad_substitution() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "Plugins/mathpad.md", "x\n");
    write_md(tmp.path(), "src.md", "see [[Mathjax]]\n");

    let applied = json(&tmp, &["links", "fix", "--apply", "--apply-fuzzy"]);
    assert_eq!(applied["results"]["fuzzy"].as_u64(), Some(0), "{applied}");

    let body = std::fs::read_to_string(tmp.path().join("src.md")).unwrap();
    assert!(
        body.contains("[[Mathjax]]") && !body.contains("mathpad"),
        "the link was rewritten: {body}"
    );
    let after = json(&tmp, &["links", "fix", "--dry-run"]);
    assert_eq!(after["results"]["broken"].as_u64(), Some(1), "{after}");
}

/// The exemption's own reason for existing survives untouched: the
/// gerund-to-imperative slug rename documentation sites do wholesale is still
/// applied at full confidence.
#[test]
fn the_morphological_rename_is_still_applied() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "actions/how-tos/create-a-composite-action.md",
        "x\n",
    );
    write_md(
        tmp.path(),
        "src.md",
        "[a](actions/how-tos/creating-a-composite-action)\n",
    );

    let plans = fuzzy_plans(&tmp);
    assert_eq!(plans.len(), 1, "expected one proposal: {plans:?}");
    assert!(
        plans[0].0.ends_with("create-a-composite-action.md"),
        "{plans:?}"
    );
    assert!(
        !plans[0].2 && plans[0].1 >= 0.8,
        "the rename must stay applicable: {plans:?}"
    );
}

/// `get-started` / `getting-started`, the other iteration-279 fixture that
/// rides on the exemption — a shared prefix that consumes the shorter token
/// entirely.
#[test]
fn a_fully_consumed_token_is_still_a_prefix_relationship() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "guides/getting-started.md", "x\n");
    write_md(tmp.path(), "src.md", "[a](guides/get-started)\n");

    let plans = fuzzy_plans(&tmp);
    assert_eq!(plans.len(), 1, "expected one proposal: {plans:?}");
    assert!(plans[0].0.ends_with("getting-started.md"), "{plans:?}");
    assert!(!plans[0].2 && plans[0].1 >= 0.8, "{plans:?}");
}

/// A typo never needed the exemption — it clears the floor on plain Jaro — so
/// the narrower rule cannot cost it anything.
#[test]
fn a_typo_is_unaffected() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "guides/configuration.md", "x\n");
    write_md(tmp.path(), "src.md", "[a](guides/configuraton)\n");

    let plans = fuzzy_plans(&tmp);
    assert_eq!(plans.len(), 1, "{plans:?}");
    assert!(plans[0].0.ends_with("guides/configuration.md"), "{plans:?}");
    assert!(!plans[0].2 && plans[0].1 >= 0.8, "{plans:?}");
}

/// Iteration 280's own regression guard, re-run: the documentation-slug corpus
/// the scorer was tuned on sees exactly the proposals and confidences it saw
/// before this iteration, because none of them rested on a half-shared prefix.
#[test]
fn a_documentation_slug_vault_is_still_unchanged() {
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
    assert!(plans[0].0.ends_with("guides/configuration.md"), "{plans:?}");
    assert!(!plans[0].2 && plans[0].1 >= 0.8, "{plans:?}");
    assert!(plans[1].0.ends_with("actions.md"), "{plans:?}");
    assert!((plans[1].1 - 0.7).abs() < 1e-9 && plans[1].2, "{plans:?}");
}
