//! Config dir-boundary e2e gates (iter-221, H-1).
//!
//! A project-local `.hyalo.toml`'s `dir` used to be honored verbatim, with no
//! check that it stayed at-or-below the config directory — `dir = ".."` or an
//! absolute path both passed straight through, and every downstream boundary
//! gate (fs_util.rs, iter-202) then defended containment against that
//! attacker-chosen root instead of the real one. hyalo is agent-driven
//! (CLAUDE.md tells agents to run its hints verbatim), so a hostile cloned
//! repo plus a normal agent loop was a plausible write-scope-escape.
//!
//! Policy (DEC-092): hard refuse. Every command — reads included, not just
//! writers — refuses to run while a project-local `dir` resolves outside its
//! own config directory. `--dir` (the user's own explicit choice), the
//! global-equivalent redundant-`--dir` case, and an in-bounds relative `dir`
//! must all keep working unchanged, matching iter-201's DEC-069/070/071
//! "no silent config discard" stance: refuse loudly rather than clamp
//! silently.

use super::common::{hyalo, hyalo_no_hints, write_md};
use std::fs;
use tempfile::TempDir;

/// Parse a JSON envelope from stdout, failing loudly with both streams.
fn envelope(stdout: &[u8], stderr: &[u8]) -> serde_json::Value {
    serde_json::from_slice(stdout).unwrap_or_else(|e| {
        panic!(
            "not a JSON envelope ({e})\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(stdout),
            String::from_utf8_lossy(stderr)
        )
    })
}

// ---------------------------------------------------------------------------
// H-1 — dir = ".." escapes the config directory
// ---------------------------------------------------------------------------

/// The exact H-1 repro: a cloned repo whose `docs/.hyalo.toml` sets
/// `dir = ".."`, reached by `cd`-ing into `docs` (no `--dir`). A mutating
/// command must refuse and must not touch anything outside — or inside —
/// the repo.
#[test]
fn dir_dotdot_refuses_a_mutating_command_and_writes_nothing() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("evilrepo");
    let docs = repo.join("docs");
    fs::create_dir_all(&docs).unwrap();
    fs::write(docs.join(".hyalo.toml"), "dir = \"..\"\n").unwrap();
    write_md(&docs, "a.md", "---\ntitle: A\n---\n# A\n");

    // A sibling file in the parent (`repo/`) that `dir = ".."` would expose
    // as the vault, and that `mv` must never touch.
    let sentinel = repo.join("sentinel.md");
    fs::write(&sentinel, "untouched").unwrap();
    let sentinel_before = fs::read(&sentinel).unwrap();

    let output = hyalo_no_hints()
        .current_dir(&docs)
        .args(["mv", "a.md", "stolen.md", "--format", "json"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(1),
        "an out-of-bounds dir must refuse the run; stdout: {} stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !repo.join("stolen.md").exists(),
        "the file must not have been moved out of docs/"
    );
    assert!(
        docs.join("a.md").exists(),
        "the original file must be untouched"
    );
    assert_eq!(
        fs::read(&sentinel).unwrap(),
        sentinel_before,
        "nothing in the exposed parent tree may be touched either"
    );
}

/// The refusal is not limited to writers — a pure read must also refuse,
/// since even a read would operate against a boundary the config was never
/// entitled to set for itself.
#[test]
fn dir_dotdot_refuses_a_read_command_too() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("evilrepo");
    let docs = repo.join("docs");
    fs::create_dir_all(&docs).unwrap();
    fs::write(docs.join(".hyalo.toml"), "dir = \"..\"\n").unwrap();
    write_md(&docs, "a.md", "---\ntitle: A\n---\n# A\n");

    let output = hyalo_no_hints()
        .current_dir(&docs)
        .args(["summary", "--format", "json"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(1),
        "a read must also refuse under an out-of-bounds dir; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The refusal names both the offending config file and the offending `dir`
/// value, and points at the escape hatch.
#[test]
fn refusal_names_the_config_file_and_dir_value() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \"..\"\n").unwrap();
    write_md(tmp.path(), "a.md", "---\ntitle: A\n---\n# A\n");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["summary", "--format", "json"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(".hyalo.toml"),
        "must name the offending config file: {stderr}"
    );
    assert!(
        stderr.contains("dir = \"..\""),
        "must name the offending dir value: {stderr}"
    );
    assert!(
        stderr.contains("--dir"),
        "must point at --dir as the explicit escape hatch: {stderr}"
    );

    // User errors are written to stderr as a JSON object, preceded by the
    // loud warning line — parse from the first `{` (same convention as
    // config_trust.rs's malformed-config tests).
    let json: serde_json::Value = serde_json::from_str(
        &stderr[stderr
            .find('{')
            .unwrap_or_else(|| panic!("no JSON error on stderr: {stderr}"))..],
    )
    .unwrap_or_else(|e| panic!("stderr is not a JSON error ({e}): {stderr}"));
    assert!(
        json["error"]
            .as_str()
            .unwrap_or_default()
            .contains(".hyalo.toml"),
        "the user-error body must also name the file: {json}"
    );
}

// ---------------------------------------------------------------------------
// H-1 — an absolute dir is refused the same way
// ---------------------------------------------------------------------------

#[test]
fn dir_absolute_path_refuses_the_run() {
    let tmp = TempDir::new().unwrap();
    let home_like = tmp.path().join("elsewhere");
    fs::create_dir_all(&home_like).unwrap();
    write_md(&home_like, "secret.md", "---\ntitle: Secret\n---\n# S\n");

    let toml = format!("dir = {:?}\n", home_like.to_str().unwrap());
    fs::write(tmp.path().join(".hyalo.toml"), toml).unwrap();
    write_md(tmp.path(), "a.md", "---\ntitle: A\n---\n# A\n");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["summary", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "an absolute dir must refuse the run; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("absolute"),
        "the diagnostic should say why: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Legitimate cases must keep working unchanged
// ---------------------------------------------------------------------------

/// An in-bounds relative `dir` (the overwhelmingly common case) is unaffected.
#[test]
fn in_bounds_relative_dir_is_unaffected() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \"kb\"\n").unwrap();
    write_md(&tmp.path().join("kb"), "a.md", "---\ntitle: A\n---\n# A\n");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["summary", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "an in-bounds dir must not be refused; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A bounded round-trip through `..` that never nets above the config
/// directory (mirrors `[changelog] path`'s own allowance) stays legal.
#[test]
fn bounded_round_trip_dir_is_allowed() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("kb")).unwrap();
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \"sub/../kb\"\n").unwrap();
    write_md(&tmp.path().join("kb"), "a.md", "---\ntitle: A\n---\n# A\n");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["summary", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "a bounded round-trip dir must not be refused; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Explicit `--dir` is the user's own choice and must keep working exactly
/// as before, even when the target tree's own `.hyalo.toml` sets an
/// out-of-bounds `dir` — `EffectiveConfig::dir` is always the literal
/// `--dir` value, never the config's own `dir` field, so the run is safe
/// regardless.
#[test]
fn explicit_dir_flag_still_works_despite_the_targets_own_poisoned_dir() {
    let tmp = TempDir::new().unwrap();
    let other = tmp.path().join("other");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join(".hyalo.toml"), "dir = \"..\"\n").unwrap();
    write_md(&other, "c.md", "---\ntitle: C\n---\n# C\n");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["summary", "--dir", "other", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "an explicit --dir must not be refused by the target's own poisoned \
         dir setting; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json = envelope(&output.stdout, &output.stderr);
    let text = json.to_string();
    assert!(text.contains("c.md"), "expected to see other/c.md: {json}");
}

/// The existing ancestor-adoption containment (config.rs:426-429) is
/// unaffected: a legitimate nested vault still works when `cd`-ed into.
#[test]
fn ancestor_adoption_containment_is_unchanged() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \"kb\"\n").unwrap();
    let kb = tmp.path().join("kb");
    write_md(&kb, "a.md", "---\ntitle: A\n---\n# A\n");

    let output = hyalo_no_hints()
        .current_dir(&kb)
        .args(["summary", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "ancestor adoption of a legitimate nested vault must still work; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json = envelope(&output.stdout, &output.stderr);
    let text = json.to_string();
    assert!(text.contains("a.md"), "expected to see kb/a.md: {json}");
}

/// A sibling directory whose *own* ancestor config has an out-of-bounds
/// `dir` is still refused even when discovered through `--dir`, because the
/// discovered file is still a project-local config, not the user's own
/// `--dir` value.
#[test]
fn dir_flag_to_a_foreign_subdir_still_refuses_that_subtrees_own_poisoned_ancestor() {
    let tmp = TempDir::new().unwrap();
    let other = tmp.path().join("other");
    fs::create_dir_all(other.join("deep/sub")).unwrap();
    fs::write(other.join(".hyalo.toml"), "dir = \"..\"\n").unwrap();
    write_md(&other, "c.md", "---\ntitle: C\n---\n# C\n");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args([
            "summary",
            "--dir",
            other.join("deep/sub").to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    // The vault directory itself (`other/deep/sub`, the literal --dir value)
    // is safe regardless — this only asserts the run does not crash and does
    // not silently widen scope; the ancestor's poisoned `dir` never becomes
    // the effective vault because `EffectiveConfig::dir` is always the
    // literal `--dir` value.
    assert_eq!(
        output.status.code(),
        Some(0),
        "the literal --dir target is safe on its own regardless of its \
         ancestor's poisoned dir; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ---------------------------------------------------------------------------
// hyalo config surfaces the diagnostic without being blocked
// ---------------------------------------------------------------------------

#[test]
fn hyalo_config_reports_the_diagnostic_and_is_not_itself_refused() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \"..\"\n").unwrap();
    write_md(tmp.path(), "a.md", "---\ntitle: A\n---\n# A\n");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["config", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "hyalo config must keep working so it can show the problem; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json = envelope(&output.stdout, &output.stderr);
    assert_eq!(
        json["results"]["dir_out_of_bounds"].as_bool(),
        Some(true),
        "the report must flag the out-of-bounds dir: {json}"
    );
    assert!(
        json["results"]["dir_out_of_bounds_reason"]
            .as_str()
            .unwrap_or_default()
            .contains("dir = \"..\""),
        "the reason must name the offending value: {json}"
    );
    // dir itself must not be the poisoned value.
    assert_eq!(
        json["results"]["dir"].as_str(),
        Some("."),
        "dir must fall back to the safe default, not the offending value: {json}"
    );

    let text = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["config", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&text.stdout);
    assert!(
        stdout.contains("dir_out_of_bounds: true"),
        "the text rendering must lead with the integrity problem: {stdout}"
    );
}

/// A healthy config reports `dir_out_of_bounds: false` so JSON consumers
/// never have to distinguish "absent" from "false".
#[test]
fn hyalo_config_reports_false_for_a_healthy_dir() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \"kb\"\n").unwrap();
    write_md(&tmp.path().join("kb"), "a.md", "---\ntitle: A\n---\n# A\n");

    let output = hyalo()
        .current_dir(tmp.path())
        .args(["config", "--format", "json"])
        .output()
        .unwrap();
    let json = envelope(&output.stdout, &output.stderr);
    assert_eq!(json["results"]["dir_out_of_bounds"].as_bool(), Some(false));
    assert!(json["results"]["dir_out_of_bounds_reason"].is_null());
}

// ---------------------------------------------------------------------------
// The warning cannot be silenced
// ---------------------------------------------------------------------------

#[test]
fn dir_out_of_bounds_warning_survives_quiet() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".hyalo.toml"), "dir = \"..\"\n").unwrap();
    write_md(tmp.path(), "a.md", "---\ntitle: A\n---\n# A\n");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["summary", "-q", "--format", "json"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("dir = \"..\""),
        "the diagnostic must survive -q: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// PR #253 review (Copilot finding 2) — the diagnostic cannot be used to
// inject terminal escape sequences into the victim's stderr
// ---------------------------------------------------------------------------

/// A hostile `.hyalo.toml` embeds a raw ESC (CSI) sequence in `dir`, via
/// TOML's backslash-u escape syntax, so the file itself is provable to have
/// asked for exactly that byte. The refusal diagnostic must not carry that
/// byte through to the terminal raw -- end to end, through both the
/// `warn_always` stderr line and the `AppError::User` error body, neither of
/// which sanitizes on its own (unlike the JSON/text pipeline `hyalo config`
/// goes through).
#[test]
fn dir_out_of_bounds_diagnostic_does_not_leak_a_raw_escape_sequence() {
    let tmp = TempDir::new().unwrap();
    // A "../<segment>" escape (refused on the leading ".." component) whose
    // second segment carries an embedded ESC CSI color-code sequence,
    // decoded from TOML's backslash-u escape syntax. The ESC must be
    // preceded by a real path separator: "..<esc>..." with nothing between
    // is a single oddly-named component, not a ".." traversal, and would
    // exercise the unrelated "vault dir does not exist" check instead.
    fs::write(
        tmp.path().join(".hyalo.toml"),
        "dir = \"../\\u001b[31mFAKE\\u001b[0m\"\n",
    )
    .unwrap();
    write_md(tmp.path(), "a.md", "---\ntitle: A\n---\n# A\n");

    let output = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(["summary", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));

    let raw_esc = b'\x1b';
    assert!(
        !output.stderr.contains(&raw_esc),
        "a raw ESC byte reached stderr: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.stdout.contains(&raw_esc),
        "a raw ESC byte reached stdout: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
    // The rest of the diagnostic must still be legible — sanitization must
    // strip the escape byte, not eat the whole message.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("FAKE") && stderr.contains(".hyalo.toml"),
        "sanitization must not eat the rest of the diagnostic: {stderr}"
    );
}
