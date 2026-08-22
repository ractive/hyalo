//! Execution-based hint gate (iter-192).
//!
//! Every other hint test asserts on *substrings* of a hint's command — e.g.
//! `assert!(hint.cmd.contains("--limit 0"))`. That is exactly the assertion the
//! broken `hyalo tags --limit 0` hint satisfied while failing to run: `tags`
//! takes no `--limit`, only `tags summary` does. Substring assertions cannot
//! tell a runnable command from a plausible-looking one.
//!
//! This module closes that gap by *running* what the CLI tells users to run.
//! It sweeps a fixture vault with a broad set of seed commands, harvests every
//! hint they emit, executes each harvested command against a pristine copy of
//! that vault, and fails on any invocation the CLI rejects.
//!
//! Each hint runs against its own fresh vault so mutating hints (`links fix
//! --apply`, `lint --fix`, …) cannot perturb their neighbours or make the
//! outcome order-dependent.

use super::common::{hyalo, md, shell_split, write_md};
use std::path::Path;
use tempfile::TempDir;

/// Build the fixture vault that seed commands are run against.
///
/// Deliberately shaped so that as many hint branches as possible fire:
/// - **60 notes** — past the default 50-result cap, so "Show all N" hints fire
///   for `find`, and enough distinct tags/properties to truncate the `tags
///   summary` / `properties summary` aggregates too.
/// - **broken + valid links**, orphans and dead-ends for the link hints.
/// - **open and done tasks** for the task hints.
/// - **a schema type with a required property that some files violate**, so
///   `lint` reports findings and its fix/drill-down hints fire.
fn build_fixture(root: &Path) {
    // A schema so `lint`, `types`, and `new` all have something to work with.
    std::fs::write(
        root.join(".hyalo.toml"),
        md!(r#"
[schema.types.note]
required = ["title", "status"]

[schema.types.iteration]
required = ["title", "status", "date"]
"#),
    )
    .unwrap();

    for i in 0..60 {
        // Every fifth note omits `status`, producing lint findings.
        let status_line = if i % 5 == 0 {
            String::new()
        } else {
            format!("status: {}\n", ["draft", "in-progress", "completed"][i % 3])
        };
        // Distinct tags per file plus a shared one, so the tag summary exceeds
        // the default cap and the "narrow by tag" hints have material.
        let body_link = if i % 7 == 0 {
            format!("See [[missing-target-{i}]] for context.\n")
        } else {
            format!("See [[note-{}]] for context.\n", (i + 1) % 60)
        };
        write_md(
            root,
            &format!("notes/note-{i}.md"),
            &format!(
                "---\ntitle: Note {i}\ntype: note\n{status_line}tags:\n  - shared\n  - topic-{i}\nowner: person-{}\n---\n# Note {i}\n\n- [ ] Open task {i}\n- [x] Done task {i}\n\n{body_link}",
                i % 4
            ),
        );
    }

    // An orphan (nothing links to it, it links nowhere) and a dead-end.
    write_md(
        root,
        "notes/orphan.md",
        md!(r"
---
title: Orphan
type: note
status: draft
tags:
  - shared
---
# Orphan

Nothing here links anywhere.
"),
    );

    write_md(
        root,
        "decision-log.md",
        md!(r"
---
title: Decision Log
type: note
status: draft
tags:
  - shared
---
# Decision Log

Linked from [[note-0]] and links to [[note-1]].
"),
    );
}

/// Seed commands whose hints are harvested. Each is argv *after* `hyalo`.
///
/// Covers every read-only `HintSource` reachable without side effects; the
/// mutating sources (`set`/`remove`/`append`/`mv`/`task`) are included too
/// because their hints are the ones most likely to name a wrong subcommand,
/// and each runs in a throwaway vault.
const SEED_COMMANDS: &[&[&str]] = &[
    &["summary"],
    &["find"],
    &["find", "--orphan"],
    &["find", "--dead-end"],
    &["find", "--broken-links"],
    &["find", "--task", "todo"],
    &["find", "--property", "status=draft"],
    &["find", "--tag", "shared"],
    // Two view-serializable filter dimensions on a single-file result set, so
    // the "Save this query as a view" hint fires without being crowded out of
    // the MAX_HINTS budget — that hint is the one that writes `.hyalo.toml`
    // (iter-201, M-7).
    &["find", "--property", "status=draft", "--tag", "topic-3"],
    &["tags", "summary"],
    &["properties", "summary"],
    &["backlinks", "notes/note-1.md"],
    &["read", "decision-log.md"],
    &["lint"],
    &["lint", "--detailed"],
    &["types", "list"],
    &["types", "show", "note"],
    &["views", "list"],
    &["lint-rules", "list"],
    &["links", "fix"],
    &["links", "auto"],
    &["config"],
    &["create-index"],
    &["task", "read", "--file", "notes/note-1.md", "--line", "12"],
    &[
        "task",
        "toggle",
        "--file",
        "notes/note-1.md",
        "--line",
        "12",
    ],
    &[
        "set",
        "--property",
        "reviewed=true",
        "--file",
        "notes/note-1.md",
    ],
    &["remove", "--property", "owner", "--file", "notes/note-2.md"],
    &[
        "append",
        "--property",
        "aliases=Alt",
        "--file",
        "notes/note-3.md",
    ],
    &["mv", "notes/note-4.md", "notes/moved-4.md"],
];

/// A harvested hint: the seed command that produced it, its description, and
/// the command string the CLI told the user to run.
#[derive(Debug, Clone)]
struct Harvested {
    seed: String,
    description: String,
    cmd: String,
    /// The `writes` marker the CLI attached to the hint (iter-201, M-7).
    writes: bool,
}

/// Run one seed command and collect the hints from its JSON envelope.
fn harvest(seed: &[&str]) -> Vec<Harvested> {
    let tmp = TempDir::new().unwrap();
    build_fixture(tmp.path());
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--format", "json", "--hints"])
        .args(seed)
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let seed_label = seed.join(" ");
    let envelope: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "seed command `hyalo {seed_label}` did not emit a JSON envelope ({e}).\nstdout: {stdout}\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });

    envelope["hints"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|h| {
                    let cmd = h.get("cmd")?.as_str()?;
                    // Advice-only hints carry an empty command by design.
                    if cmd.trim().is_empty() {
                        return None;
                    }
                    Some(Harvested {
                        seed: seed_label.clone(),
                        description: h
                            .get("description")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        cmd: cmd.to_owned(),
                        // Absent `writes` is a bug in the envelope, not a
                        // read-only hint: default to `true` so a missing marker
                        // surfaces as an unexpected mutation rather than a
                        // silently skipped check.
                        writes: h
                            .get("writes")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(true),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Rewrite a harvested command into argv runnable against `vault`.
///
/// Drops the leading `hyalo` and repoints any `--dir` at the fresh vault (the
/// hint carries the *harvest* vault's path, which is already deleted).
fn to_argv(cmd: &str, vault: &Path) -> Vec<String> {
    let tokens = shell_split(cmd);
    let mut argv: Vec<String> = Vec::with_capacity(tokens.len() + 2);
    let mut skip_next = false;
    let mut saw_dir = false;
    for (i, tok) in tokens.iter().enumerate() {
        if i == 0 && tok == "hyalo" {
            continue;
        }
        if skip_next {
            skip_next = false;
            continue;
        }
        if tok == "--dir" || tok == "-d" {
            skip_next = true;
            saw_dir = true;
            argv.push("--dir".to_owned());
            argv.push(vault.to_string_lossy().into_owned());
            continue;
        }
        argv.push(tok.clone());
    }
    if !saw_dir {
        argv.insert(0, vault.to_string_lossy().into_owned());
        argv.insert(0, "--dir".to_owned());
    }
    argv
}

/// Why a hint execution counts as a failure, or `None` when it ran cleanly.
///
/// Exit code alone is too blunt: `lint --strict` legitimately exits 1 when it
/// finds violations, and that is the command working, not failing. What must
/// never happen is the CLI *rejecting* the command — a clap parse error, an
/// unknown subcommand, or a user error in the JSON envelope.
fn rejection_reason(exit_code: Option<i32>, stdout: &str, stderr: &str) -> Option<String> {
    // clap parse failures and other refusals go to stderr with an `error:` prefix.
    for line in stderr.lines() {
        let t = line.trim_start();
        if t.starts_with("error:") || t.starts_with("Error:") {
            return Some(format!("CLI rejected the command: {}", t.trim()));
        }
    }
    // JSON-mode user errors surface as an `error` key on stdout.
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(stdout)
        && let Some(err) = v.get("error").and_then(serde_json::Value::as_str)
    {
        return Some(format!("command returned a user error: {err}"));
    }
    // Internal errors (exit 2) are always a failure.
    if exit_code == Some(2) {
        return Some("command exited 2 (internal error)".to_owned());
    }
    if exit_code.is_none() {
        return Some("command terminated by signal".to_owned());
    }
    None
}

/// The gate: every hint the CLI emits must be a command the CLI accepts.
#[test]
fn every_emitted_hint_executes_cleanly() {
    let harvested: Vec<Harvested> = SEED_COMMANDS.iter().flat_map(|s| harvest(s)).collect();

    assert!(
        harvested.len() >= 25,
        "harvested only {} hints — the fixture stopped provoking them, which would \
         make this gate vacuous",
        harvested.len()
    );

    let mut failures: Vec<String> = Vec::new();
    for h in &harvested {
        let tmp = TempDir::new().unwrap();
        build_fixture(tmp.path());
        let argv = to_argv(&h.cmd, tmp.path());
        let output = hyalo().args(&argv).output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if let Some(reason) = rejection_reason(output.status.code(), &stdout, &stderr) {
            failures.push(format!(
                "  hint from `hyalo {}`\n    description: {}\n    command:     {}\n    problem:     {reason}",
                h.seed, h.description, h.cmd
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} emitted hints do not run:\n{}",
        failures.len(),
        harvested.len(),
        failures.join("\n")
    );
}

/// The gate must actually reject a broken command — the property that makes the
/// sweep above meaningful rather than decorative.
#[test]
fn gate_rejects_a_hint_that_does_not_parse() {
    let tmp = TempDir::new().unwrap();
    build_fixture(tmp.path());
    // The exact pre-iter-192 bug: `--limit` belongs to `tags summary`, not `tags`.
    let argv = to_argv("hyalo tags --limit 0", tmp.path());
    let output = hyalo().args(&argv).output().unwrap();
    let reason = rejection_reason(
        output.status.code(),
        &String::from_utf8_lossy(&output.stdout),
        &String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        reason.is_some(),
        "`hyalo tags --limit 0` must be classified as a rejection"
    );
}

/// A command that exits non-zero for a *domain* reason (lint found violations)
/// is not a rejection — otherwise the gate would force hints to avoid lint.
#[test]
fn gate_accepts_lint_strict_exiting_nonzero() {
    let tmp = TempDir::new().unwrap();
    build_fixture(tmp.path());
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["lint", "--strict", "--format", "json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_eq!(
        rejection_reason(output.status.code(), &stdout, &stderr),
        None,
        "lint --strict finding violations is not a rejection (stdout: {stdout}, stderr: {stderr})"
    );
}

#[test]
fn to_argv_repoints_dir_at_the_fresh_vault() {
    let vault = Path::new("/tmp/fresh-vault");
    assert_eq!(
        to_argv("hyalo --dir /tmp/old find --orphan", vault),
        vec!["--dir", "/tmp/fresh-vault", "find", "--orphan"]
    );
}

#[test]
fn to_argv_injects_dir_when_the_hint_omits_it() {
    let vault = Path::new("/tmp/fresh-vault");
    assert_eq!(
        to_argv("hyalo tags summary --limit 0", vault),
        vec![
            "--dir",
            "/tmp/fresh-vault",
            "tags",
            "summary",
            "--limit",
            "0"
        ]
    );
}

// ---------------------------------------------------------------------------
// Side-effect gate (iter-201, M-7)
// ---------------------------------------------------------------------------
//
// The gate above proves every hint *runs*. It says nothing about what running
// it does. `hyalo find --property … --tag …` used to suggest a `views set …`
// command that rewrites `.hyalo.toml`, rendered in the same `-> hyalo …` list
// as read-only drill-downs — so "run the hints to explore" was not safe advice
// to give an agent. Hints now carry a `writes` flag; this gate holds the flag
// honest by running every hint marked read-only and diffing the vault.

/// A content fingerprint of every file under `root`, including `.hyalo.toml`.
///
/// Sorted vault-relative paths paired with their bytes, so the comparison
/// catches creations, deletions, and in-place edits alike.
fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, root, out);
            } else if let Ok(bytes) = std::fs::read(&path) {
                let rel = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((rel, bytes));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Describe how two snapshots differ, or `None` when they are identical.
fn describe_diff(before: &[(String, Vec<u8>)], after: &[(String, Vec<u8>)]) -> Option<String> {
    let mut changes: Vec<String> = Vec::new();
    for (path, bytes) in after {
        match before.iter().find(|(p, _)| p == path) {
            None => changes.push(format!("created {path}")),
            Some((_, old)) if old != bytes => changes.push(format!("modified {path}")),
            Some(_) => {}
        }
    }
    for (path, _) in before {
        if !after.iter().any(|(p, _)| p == path) {
            changes.push(format!("deleted {path}"));
        }
    }
    (!changes.is_empty()).then(|| changes.join(", "))
}

/// Every hint the CLI presents as read-only must leave the vault byte-identical.
#[test]
fn unmarked_hints_have_no_side_effects() {
    let harvested: Vec<Harvested> = SEED_COMMANDS.iter().flat_map(|s| harvest(s)).collect();
    let read_only: Vec<&Harvested> = harvested.iter().filter(|h| !h.writes).collect();

    assert!(
        read_only.len() >= 20,
        "only {} read-only hints harvested — the gate would be near-vacuous",
        read_only.len()
    );

    let mut failures: Vec<String> = Vec::new();
    for h in read_only {
        let tmp = TempDir::new().unwrap();
        build_fixture(tmp.path());
        let before = snapshot(tmp.path());
        let argv = to_argv(&h.cmd, tmp.path());
        let _ = hyalo().args(&argv).output().unwrap();
        let after = snapshot(tmp.path());
        if let Some(diff) = describe_diff(&before, &after) {
            failures.push(format!(
                "  hint from `hyalo {}`\n    description: {}\n    command:     {}\n    changed:     {diff}",
                h.seed, h.description, h.cmd
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} hint(s) presented as read-only modified the vault:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// At least one hint must actually be *marked* as writing, otherwise
/// `unmarked_hints_have_no_side_effects` could pass by the marker being applied
/// to everything.
#[test]
fn the_views_set_hint_is_marked_as_writing() {
    let harvested: Vec<Harvested> =
        harvest(&["find", "--property", "status=draft", "--tag", "topic-3"]);
    let views_set = harvested
        .iter()
        .find(|h| h.cmd.contains("views set"))
        .unwrap_or_else(|| {
            panic!(
                "the save-as-view hint stopped firing; harvested: {:?}",
                harvested.iter().map(|h| &h.cmd).collect::<Vec<_>>()
            )
        });
    assert!(
        views_set.writes,
        "`{}` writes .hyalo.toml but is not marked",
        views_set.cmd
    );
}

/// The marker must reach text output too — JSON consumers are not the only
/// audience, and the plain `->` list is what a human reads.
#[test]
fn text_output_distinguishes_writing_hints() {
    let tmp = TempDir::new().unwrap();
    build_fixture(tmp.path());
    let output = hyalo()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "find",
            "--property",
            "status=draft",
            "--tag",
            "topic-3",
            "--format",
            "text",
            "--hints",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let writing_line = stdout
        .lines()
        .find(|l| l.contains("views set"))
        .unwrap_or_else(|| panic!("no views-set hint in text output:\n{stdout}"));
    assert!(
        writing_line.trim_start().starts_with("=>"),
        "writing hint must use the `=>` arrow, got: {writing_line}"
    );
    assert!(
        writing_line.ends_with("[writes]"),
        "writing hint must be tagged [writes], got: {writing_line}"
    );
    assert!(
        stdout.lines().any(|l| l.trim_start().starts_with("-> ")),
        "read-only hints must keep the `->` arrow:\n{stdout}"
    );
}

/// The snapshot helper must notice a change, or the gate above is decorative.
#[test]
fn snapshot_diff_detects_a_write() {
    let tmp = TempDir::new().unwrap();
    build_fixture(tmp.path());
    let before = snapshot(tmp.path());
    let argv = to_argv(
        "hyalo set --property touched=true --file notes/note-1.md",
        tmp.path(),
    );
    let _ = hyalo().args(&argv).output().unwrap();
    let after = snapshot(tmp.path());
    let diff = describe_diff(&before, &after).expect("a write must show up as a diff");
    assert!(
        diff.contains("modified notes/note-1.md"),
        "unexpected diff: {diff}"
    );
}

// ---------------------------------------------------------------------------
// L-9 (iter-204): a custom index path propagates into BOTH follow-up hints
// ---------------------------------------------------------------------------

/// `create-index -o <custom>` emitted a bare `drop-index` hint, which targets
/// `<vault>/.hyalo-index` — a different file. Following it left the custom
/// index on disk (and, in a read-only vault, failed outright).
///
/// The seed sweep above cannot cover this: its commands are static argv, while
/// a custom index path only exists relative to a temp vault. So this test runs
/// the pairing end-to-end — create at a custom path, harvest the hints, execute
/// the drop hint verbatim, and check which file actually disappeared.
#[test]
fn custom_index_path_drop_hint_targets_that_index() {
    let tmp = TempDir::new().unwrap();
    build_fixture(tmp.path());
    let vault = tmp.path();
    let custom = vault.join("my-index");
    let default_index = vault.join(".hyalo-index");

    // A default index also exists, so a bare `drop-index` hint would "work"
    // while deleting the wrong file — the failure mode this test pins down.
    let seed = hyalo()
        .args(["--dir", vault.to_str().unwrap()])
        .args(["--format", "json", "--no-hints", "create-index"])
        .output()
        .unwrap();
    assert!(
        seed.status.success(),
        "default create-index failed: {seed:?}"
    );
    assert!(default_index.exists());

    let output = hyalo()
        .args(["--dir", vault.to_str().unwrap()])
        .args(["--format", "json", "--hints"])
        .args(["create-index", "-o", custom.to_str().unwrap()])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let envelope: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("no JSON envelope ({e}): {stdout}"));
    assert!(custom.exists(), "custom index was not created");

    let drop_hint = envelope["hints"]
        .as_array()
        .expect("hints array")
        .iter()
        .find_map(|h| {
            let cmd = h.get("cmd")?.as_str()?;
            cmd.contains("drop-index").then(|| cmd.to_owned())
        })
        .unwrap_or_else(|| panic!("no drop-index hint in {envelope}"));
    assert!(
        drop_hint.contains(custom.to_str().unwrap()),
        "drop hint must name the custom index: {drop_hint}"
    );

    // Run the hint exactly as printed.
    let argv: Vec<String> = shell_split(&drop_hint)
        .into_iter()
        .skip(1) // leading `hyalo`
        .collect();
    let run = hyalo().args(&argv).output().unwrap();
    assert!(
        run.status.success(),
        "the drop hint did not run: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        !custom.exists(),
        "the custom index should have been deleted"
    );
    assert!(
        default_index.exists(),
        "the default index must be left alone"
    );
}
