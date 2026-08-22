//! Vault-boundary refusals across the whole write path (iteration 202).
//!
//! Iteration 191 gave the `set`/`append`/`remove`/`task`/`mv` family a
//! boundary-checked write. The v0.21.0-pre dogfood then found three writers it
//! had missed — `madr toc` (H-3), `changelog add`/`release` (M-3) and
//! `new --file` (M-4) — plus an inconsistent exit code and message shape
//! (L-16). This suite pins all of that down:
//!
//! - every escape vector refuses at **exit 1**, never 0 and never 2;
//! - nothing outside the vault is created or modified;
//! - every refusal carries the same two-path wording — the path the user typed
//!   in the error's `path` field, and where it really resolves in the message.
//!
//! Unix only: creating a symlink on Windows needs developer mode or
//! elevation. The `../` traversal vectors need no symlink, but they live here
//! with the rest of the family rather than being split across two files.
#![cfg(unix)]

use super::common::{hyalo_no_hints, write_md};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Run `hyalo --dir <vault> <args...>` and return `(exit code, stdout+stderr)`.
fn run(vault: &Path, args: &[&str]) -> (i32, String) {
    let out = hyalo_no_hints()
        .args(["--dir", vault.to_str().unwrap()])
        .args(args)
        .output()
        .unwrap();
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code().unwrap_or(-1), combined)
}

/// Assert a boundary refusal: exit 1 and the shared wording.
///
/// Exit 1 is the whole point of L-16 — `okf log` used to escape via an
/// `anyhow` bail and surface as exit 2, the documented "internal error" class,
/// which tells a caller to file a bug rather than to fix its path.
fn assert_boundary_refusal(code: i32, combined: &str, context: &str) {
    assert_eq!(code, 1, "{context}: expected exit 1, got {code}: {combined}");
    assert!(
        combined.contains("resolves outside vault boundary"),
        "{context}: expected the shared boundary wording, got: {combined}"
    );
}

// ---------------------------------------------------------------------------
// H-3 — `madr toc` built `<adr-dir>/README.md` from unvalidated user input
// ---------------------------------------------------------------------------

#[test]
fn madr_toc_refuses_parent_traversal_adr_dir() {
    let root = TempDir::new().unwrap();
    let vault = root.path().join("vault");
    let outside = root.path().join("outside");
    fs::create_dir_all(vault.join("docs")).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("README.md"), "PRECIOUS\n").unwrap();

    let (code, combined) = run(&vault, &["madr", "toc", "../outside", "--apply"]);
    assert_boundary_refusal(code, &combined, "madr toc ../outside");
    assert_eq!(
        fs::read_to_string(outside.join("README.md")).unwrap(),
        "PRECIOUS\n",
        "the out-of-vault README must be byte-for-byte untouched"
    );
}

#[test]
fn madr_toc_refuses_symlinked_adr_dir() {
    let root = TempDir::new().unwrap();
    let vault = root.path().join("vault");
    let outside = root.path().join("outside");
    fs::create_dir_all(&vault).unwrap();
    fs::create_dir_all(&outside).unwrap();
    write_md(
        &outside,
        "0001-thing.md",
        "---\ntitle: Thing\ntype: adr\n---\n# Thing\n",
    );
    std::os::unix::fs::symlink(&outside, vault.join("decisions")).unwrap();

    let (code, combined) = run(&vault, &["madr", "toc", "decisions", "--apply"]);
    assert_boundary_refusal(code, &combined, "madr toc via symlinked dir");
    assert!(
        !outside.join("README.md").exists(),
        "no README.md may be fabricated outside the vault"
    );
}

/// Dry-run and apply must agree. A dry-run that cheerfully reports the TOC it
/// would write, followed by an apply that refuses, is the disagreement
/// iteration 191 spent its time eliminating elsewhere.
#[test]
fn madr_toc_refuses_escaping_adr_dir_in_dry_run_too() {
    let root = TempDir::new().unwrap();
    let vault = root.path().join("vault");
    let outside = root.path().join("outside");
    fs::create_dir_all(&vault).unwrap();
    fs::create_dir_all(&outside).unwrap();

    let (code, combined) = run(&vault, &["madr", "toc", "../outside", "--dry-run"]);
    assert_boundary_refusal(code, &combined, "madr toc --dry-run");
}

#[test]
fn madr_toc_still_works_inside_the_vault() {
    let root = TempDir::new().unwrap();
    let vault = root.path().join("vault");
    fs::create_dir_all(&vault).unwrap();
    write_md(
        &vault,
        "docs/decisions/0001-thing.md",
        "---\ntitle: Thing\ntype: adr\nstatus: accepted\n---\n# Thing\n",
    );

    let (code, combined) = run(&vault, &["madr", "toc", "--apply"]);
    assert_eq!(code, 0, "in-vault TOC generation must succeed: {combined}");
    let toc = fs::read_to_string(vault.join("docs/decisions/README.md")).unwrap();
    assert!(toc.contains("Thing"), "TOC should list the ADR: {toc}");
}

// ---------------------------------------------------------------------------
// M-3 — `changelog` followed a CHANGELOG.md symlink out of the vault
// ---------------------------------------------------------------------------

/// Vault whose `CHANGELOG.md` is a symlink to a changelog outside it.
fn vault_with_escaping_changelog() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
    let root = TempDir::new().unwrap();
    let vault = root.path().join("vault");
    let outside = root.path().join("outside");
    fs::create_dir_all(&vault).unwrap();
    fs::create_dir_all(&outside).unwrap();
    let escapee = outside.join("CHANGELOG.md");
    fs::write(&escapee, "# Changelog\n\n## [Unreleased]\n").unwrap();
    std::os::unix::fs::symlink(&escapee, vault.join("CHANGELOG.md")).unwrap();
    (root, vault, escapee)
}

#[test]
fn changelog_add_refuses_symlink_escaping_vault() {
    let (_root, vault, escapee) = vault_with_escaping_changelog();

    let (code, combined) = run(
        &vault,
        &[
            "changelog",
            "add",
            "--category",
            "Added",
            "--message",
            "injected",
            "--apply",
        ],
    );
    assert_boundary_refusal(code, &combined, "changelog add");
    assert!(
        !fs::read_to_string(&escapee).unwrap().contains("injected"),
        "the out-of-vault changelog must be untouched"
    );
}

#[test]
fn changelog_release_refuses_symlink_escaping_vault() {
    let (_root, vault, escapee) = vault_with_escaping_changelog();

    let (code, combined) = run(&vault, &["changelog", "release", "1.0.0", "--apply"]);
    assert_boundary_refusal(code, &combined, "changelog release");
    assert!(
        !fs::read_to_string(&escapee).unwrap().contains("1.0.0"),
        "the out-of-vault changelog must be untouched"
    );
}

/// An *intentional* out-of-vault changelog — configured via `[changelog] path`
/// for the common repo-root-changelog / docs-subdir-vault layout — is a
/// documented setup, not an escape. Only the silent symlink hop is refused.
#[test]
fn changelog_add_allows_configured_repo_root_path() {
    let root = TempDir::new().unwrap();
    let vault = root.path().join("docs");
    fs::create_dir_all(&vault).unwrap();
    fs::write(
        root.path().join(".hyalo.toml"),
        "dir = \"docs\"\n\n[changelog]\npath = \"CHANGELOG.md\"\n",
    )
    .unwrap();
    fs::write(
        root.path().join("CHANGELOG.md"),
        "# Changelog\n\n## [Unreleased]\n",
    )
    .unwrap();

    let out = hyalo_no_hints()
        .current_dir(root.path())
        .args([
            "changelog",
            "add",
            "--category",
            "Added",
            "--message",
            "intentional",
            "--apply",
        ])
        .output()
        .unwrap();
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "a configured repo-root changelog must stay allowed: {combined}"
    );
    assert!(
        fs::read_to_string(root.path().join("CHANGELOG.md"))
            .unwrap()
            .contains("intentional"),
        "the configured changelog should have been written"
    );
}

// ---------------------------------------------------------------------------
// M-4 — `new --file` validated lexically but never resolved
// ---------------------------------------------------------------------------

#[test]
fn new_refuses_symlinked_output_directory() {
    let root = TempDir::new().unwrap();
    let vault = root.path().join("vault");
    let outside = root.path().join("outside");
    fs::create_dir_all(&vault).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(
        vault.join(".hyalo.toml"),
        "[schema.types.note]\nrequired = [\"title\"]\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(&outside, vault.join("outdir")).unwrap();

    let (code, combined) = run(
        &vault,
        &["new", "--type", "note", "--file", "outdir/planted.md"],
    );
    assert_boundary_refusal(code, &combined, "new --file through symlinked dir");
    assert!(
        !outside.join("planted.md").exists(),
        "no file may be created outside the vault"
    );
}

/// The symlink may sit several levels above the new file: the check anchors on
/// the nearest ancestor that exists, so directories that would be *created*
/// below an escaping ancestor are refused before `create_dir_all` runs.
#[test]
fn new_refuses_nested_path_below_symlinked_directory() {
    let root = TempDir::new().unwrap();
    let vault = root.path().join("vault");
    let outside = root.path().join("outside");
    fs::create_dir_all(&vault).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(
        vault.join(".hyalo.toml"),
        "[schema.types.note]\nrequired = [\"title\"]\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(&outside, vault.join("outdir")).unwrap();

    let (code, combined) = run(
        &vault,
        &["new", "--type", "note", "--file", "outdir/a/b/planted.md"],
    );
    assert_boundary_refusal(code, &combined, "new --file nested below symlink");
    assert!(
        !outside.join("a").exists(),
        "create_dir_all must not fabricate directories outside the vault"
    );
}

#[test]
fn new_still_creates_files_inside_the_vault() {
    let root = TempDir::new().unwrap();
    let vault = root.path().join("vault");
    fs::create_dir_all(&vault).unwrap();
    fs::write(
        vault.join(".hyalo.toml"),
        "[schema.types.note]\nrequired = [\"title\"]\n",
    )
    .unwrap();

    let (code, combined) = run(
        &vault,
        &["new", "--type", "note", "--file", "nested/fine.md"],
    );
    assert_eq!(code, 0, "in-vault creation must succeed: {combined}");
    assert!(vault.join("nested/fine.md").is_file());
}

// ---------------------------------------------------------------------------
// L-16 — one exit code and one message shape for the whole family
// ---------------------------------------------------------------------------

/// `okf log` used to refuse correctly but with exit 2, because the refusal
/// came from `atomic_write_within`'s `anyhow` bail rather than a user error.
#[test]
fn okf_log_refuses_symlink_escaping_vault_at_exit_one() {
    let root = TempDir::new().unwrap();
    let vault = root.path().join("vault");
    let outside = root.path().join("outside");
    fs::create_dir_all(&vault).unwrap();
    fs::create_dir_all(&outside).unwrap();
    let escapee = outside.join("log.md");
    fs::write(&escapee, "# Log\n").unwrap();
    std::os::unix::fs::symlink(&escapee, vault.join("log.md")).unwrap();

    let (code, combined) = run(&vault, &["okf", "log", "--message", "injected", "--apply"]);
    assert_boundary_refusal(code, &combined, "okf log");
    assert!(
        !fs::read_to_string(&escapee).unwrap().contains("injected"),
        "the out-of-vault log must be untouched"
    );
}

/// The refusals must not just each be exit 1 — they must read the same way.
/// Every one of them names where the path *resolves to*, so a caller that has
/// been handed a symlinked vault can see what actually happened.
#[test]
fn boundary_refusals_share_exit_code_and_message_shape() {
    let root = TempDir::new().unwrap();
    let vault = root.path().join("vault");
    let outside = root.path().join("outside");
    fs::create_dir_all(&vault).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(
        vault.join(".hyalo.toml"),
        "[schema.types.note]\nrequired = [\"title\"]\n",
    )
    .unwrap();
    fs::write(outside.join("secret.md"), "---\ntitle: S\n---\nBody\n").unwrap();
    fs::write(outside.join("CHANGELOG.md"), "# Changelog\n\n## [Unreleased]\n").unwrap();
    fs::write(outside.join("log.md"), "# Log\n").unwrap();
    std::os::unix::fs::symlink(outside.join("secret.md"), vault.join("escape.md")).unwrap();
    std::os::unix::fs::symlink(outside.join("CHANGELOG.md"), vault.join("CHANGELOG.md")).unwrap();
    std::os::unix::fs::symlink(outside.join("log.md"), vault.join("log.md")).unwrap();
    std::os::unix::fs::symlink(&outside, vault.join("outdir")).unwrap();

    let escape_target = fs::canonicalize(outside.join("secret.md")).unwrap();
    let vectors: Vec<(&str, Vec<&str>)> = vec![
        (
            "set",
            vec!["set", "--file", "escape.md", "--property", "status=done"],
        ),
        (
            "append",
            vec!["append", "--file", "escape.md", "--property", "tags=x"],
        ),
        (
            "remove",
            vec!["remove", "--file", "escape.md", "--property", "title"],
        ),
        (
            "changelog add",
            vec![
                "changelog", "add", "--category", "Added", "--message", "x", "--apply",
            ],
        ),
        (
            "okf log",
            vec!["okf", "log", "--message", "x", "--apply"],
        ),
        (
            "new",
            vec!["new", "--type", "note", "--file", "outdir/x.md"],
        ),
        ("madr toc", vec!["madr", "toc", "../outside", "--apply"]),
    ];

    for (name, args) in vectors {
        let (code, combined) = run(&vault, &args);
        assert_boundary_refusal(code, &combined, name);
    }

    // The `set` family now names the resolved target as well as the typed
    // path — the two-path form `okf log` had and iteration 191's writers did
    // not (L-16).
    let (_, combined) = run(
        &vault,
        &["set", "--file", "escape.md", "--property", "status=done"],
    );
    assert!(
        combined.contains(&escape_target.display().to_string()),
        "the refusal must name the resolved target, got: {combined}"
    );
    assert!(
        combined.contains("escape.md"),
        "the refusal must also name the path the user typed, got: {combined}"
    );
    assert!(
        fs::read_to_string(outside.join("secret.md"))
            .unwrap()
            .contains("Body"),
        "nothing outside the vault may be modified"
    );
}

// ---------------------------------------------------------------------------
// M-5 — an in-vault symlink and its target are one file, not two
// ---------------------------------------------------------------------------

/// Vault holding one real note plus an in-vault symlink to it.
fn vault_with_intra_vault_alias() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "target.md", "---\ntitle: Target\n---\nx\n");
    write_md(
        tmp.path(),
        "hub.md",
        "---\ntitle: Hub\n---\nSee [[Target]].\n",
    );
    std::os::unix::fs::symlink("hub.md", tmp.path().join("alias.md")).unwrap();
    tmp
}

#[test]
fn summary_counts_an_aliased_note_once() {
    let tmp = vault_with_intra_vault_alias();
    let out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["summary", "--format", "json"])
        .output()
        .unwrap();
    let val: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        val["results"]["files"]["total"], 2,
        "hub.md and target.md are two files; the alias must not make three: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn find_count_counts_an_aliased_note_once() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "real.md", "---\ntitle: Real\n---\nx\n");
    std::os::unix::fs::symlink("real.md", tmp.path().join("alias.md")).unwrap();

    let out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["find", "--count", "--format", "json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        stdout.trim(),
        "1",
        "one file reachable under two spellings is still one file: {stdout}"
    );
}

/// The regression that made this a HIGH-value fix: `links fix --apply` rewrote
/// the same note once per spelling, and the second write saw the mtime the
/// first one had just changed — "modified by another process", exit 1, even
/// though the fix had landed.
#[test]
fn links_fix_apply_rewrites_an_aliased_note_once() {
    let tmp = vault_with_intra_vault_alias();
    let out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["links", "fix", "--apply"])
        .output()
        .unwrap();
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "the double write used to trip the concurrency guard: {combined}"
    );
    assert!(
        !combined.contains("modified by another process"),
        "no concurrent-modification false positive: {combined}"
    );
    let hub = fs::read_to_string(tmp.path().join("hub.md")).unwrap();
    assert_eq!(
        hub.matches("[[target]]").count(),
        1,
        "the note must be rewritten exactly once: {hub}"
    );
    assert!(
        fs::symlink_metadata(tmp.path().join("alias.md"))
            .unwrap()
            .file_type()
            .is_symlink(),
        "the alias must survive as a symlink"
    );
}

/// The skip warning for a symlink pointing *out* of the vault is emitted from
/// the walker, and a single CLI run walks the vault more than once. It must
/// still be reported once.
#[test]
fn out_of_vault_symlink_warning_is_printed_once() {
    let root = TempDir::new().unwrap();
    let vault = root.path().join("vault");
    let outside = root.path().join("outside");
    fs::create_dir_all(&vault).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("secret.md"), "---\ntitle: S\n---\nx\n").unwrap();
    write_md(&vault, "in.md", "---\ntitle: In\n---\nx\n");
    std::os::unix::fs::symlink(outside.join("secret.md"), vault.join("escape.md")).unwrap();

    let out = hyalo_no_hints()
        .args(["--dir", vault.to_str().unwrap()])
        .args(["summary", "--format", "json"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        stderr.matches("symlink target resolves outside vault").count(),
        1,
        "the skip warning must be emitted once per run, got: {stderr}"
    );
}
