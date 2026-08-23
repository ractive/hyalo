---
title: "Iteration 222 — security & robustness batch (UTF-8 lint, symlink policy, Windows paths, error leaks)"
type: iteration
date: 2026-08-23
status: planned
branch: iter-222/security-robustness-batch
tags: [iteration, security, lint, write-path, cross-platform]
related:
  - "[[reviews/adversarial-review-2026-08-23]]"
  - "[[reviews/deep-analysis-2-2026-08-23]]"
  - "[[iterations/iteration-202-boundary-completion]]"
---

# Iteration 222 — security & robustness batch

## Goal

Clear the MEDIUM/LOW security findings and the ADVISORY items from
[[reviews/adversarial-review-2026-08-23]] that don't belong to the H-1
config-dir iteration: the invalid-UTF-8 lint abort, the `create-index -o`
symlink-policy divergence, the Windows drive-relative/ADS gap, and the
parser-internal error leaks.

## Context

- **M-1 (MEDIUM, VERIFIED)** — `crates/hyalo-cli/src/commands/lint.rs:2226`:
  `std::fs::read_to_string(full_path).with_context(...)?` propagates out of
  the per-file loop, so one invalid-UTF-8 file aborts the whole `lint` /
  `lint --fix` run (exit 2). Other commands already skip+warn per file
  (`scanner/mod.rs:99-104`). One corrupt file disables all autofix.
- **L-1 (LOW, VERIFIED)** — `crates/hyalo-core/src/index.rs:925-929`:
  `write_snapshot` uses raw `NamedTempFile::new_in` + `persist`, bypassing
  `fs_util`'s `resolve_write_target` and so replacing a symlinked
  `.hyalo-index` instead of following it (DEC-062 says the target is
  replaced, the symlink stays). Safety-neutral but a silent policy
  divergence: the `-o` path and the frontmatter write path give different
  answers for the same input.
- **M-2 (MEDIUM, SUSPECTED — needs Windows)** — `index.rs:659-676`
  (SEC-1) and the lexical `resolve_file` gate accept `C:foo`
  (drive-relative: `is_absolute()` is false, no `ParentDir` component) and
  `a.md:stream` (NTFS ADS is lexically in-vault). The `Component::Prefix`
  match only fires for absolute prefixed paths.
- **ADVISORY** — (a) `anyhow` RUSTSEC-2026-0190 (`downcast_mut` unsoundness)
  is not in `deny.toml`'s ignore list; if `cargo deny check` gates CI it may
  fail, and if not the advisory is untriaged. (b) YAML alias-bomb rejection
  leaks `budget breached: Anchors { anchors: 1 }` (saphyr-internal); same
  family as the NEW-8 `ScalarBytes {…}` leak that
  [[iterations/iteration-219-list-splice-and-write-polish]] is already
  wrapping — coordinate so the mapping lives in one place. (c) case-probe
  files (`case_index.rs:420-437`) are created/deleted inside the vault,
  pinging file watchers and racing `git status`.

## Tasks

- [ ] M-1: catch the UTF-8 (and read) error per file in the lint loop, emit
      one diagnostic per offending file, continue; still exit non-zero at
      the end when strictness requires it. Mirror `scanner/mod.rs`'s
      skip+warn. Test: a vault with one invalid-UTF-8 file lints the rest
      and `--fix` still fixes the rest
- [ ] L-1: route `write_snapshot` through `fs_util::atomic_write`/
      `atomic_write_within` (or `resolve_write_target` + the boundary check)
      so index writes share the one symlink policy — OR amend DEC-062 to say
      index writes replace links and document why. Test: `create-index -o`
      onto a symlink follows it (or the documented decision)
- [ ] M-2: on Windows, reject any rel path with a `Prefix` component or a
      colon in the final component, in BOTH the SEC-1 snapshot check and the
      lexical `resolve_file` gate. Gate the tests behind `#[cfg(windows)]`
      (and, if feasible, a lexical unit test that runs everywhere by
      constructing the components directly)
- [ ] ADVISORY-a: triage `anyhow` RUSTSEC-2026-0190 — bump anyhow if a fixed
      version exists, else add it to `deny.toml` with a written rationale.
      Confirm whether `cargo deny check` is actually a CI gate and record it
- [ ] ADVISORY-b: map the saphyr `Anchors {…}` (and any sibling budget
      structs) to a human message; reuse the wrapper
      [[iterations/iteration-219-list-splice-and-write-polish]] adds for
      `ScalarBytes` rather than duplicating it
- [ ] ADVISORY-c: move the case-insensitivity probe to a temp dir on the
      same filesystem (or a hidden `.hyalo`-prefixed transient) so it does
      not create/delete files in the user's vault; verify no residual file
      on either branch, and that the detection result is unchanged

## Acceptance criteria

- [ ] One invalid-UTF-8 file no longer aborts `lint` or `lint --fix`; the
      rest of the vault is linted/fixed and the bad file is reported once
- [ ] `create-index -o` onto a symlink follows one consistent, tested policy
      shared with the frontmatter write path (or DEC-062 is amended)
- [ ] Windows drive-relative and ADS rel paths are rejected by both gates
      (tested under `#[cfg(windows)]`)
- [ ] No user-facing error contains a saphyr/parser-internal debug struct
- [ ] `cargo deny check` is green with a documented disposition for
      RUSTSEC-2026-0190

## Non-goals

- The H-1 config-dir escape ([[iterations/iteration-221-config-dir-boundary]])
- Concurrency/crash-recovery tests for the write path
  ([[iterations/iteration-224-test-quality-hardening]])
