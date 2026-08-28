---
title: "Iteration 248 — remove --strict-index"
type: iteration
date: 2026-08-28
tags:
  - iteration
  - cli-surface
status: completed
branch: iter-248/remove-strict-index
related:
  - "[[iterations/iteration-247-carry-over-sweep]]"
  - "[[decision-log]]"
---

# Iteration 248 — remove --strict-index

## Goal

Remove the global `--strict-index` CLI flag added in
[[iterations/iteration-247-carry-over-sweep|iter-247]] (PR #285). Owner
decision (2026-08-28): the flag is redundant — a caller who wants a
guaranteed disk scan already gets one by omitting `--index`/`--index-file` —
the name is a misnomer ("strict" suggests the run can fail; it only ever
degrades to a slower correct path), and it grew the global CLI surface
without a reported need. Warn-but-serve remains the only behaviour on a
stale snapshot, unchanged from DEC-241/DEC-245.

## Tasks

- [x] Remove the `strict_index` clap field and its doc comment from
      `crates/hyalo-cli/src/cli/args.rs`, plus the `--strict-index` mention in
      the `create-index` long-form help paragraph.
- [x] Remove the flag's plumbing in `crates/hyalo-cli/src/run.rs` — the
      `stale && strict_index` branch collapses to the unconditional
      warn-but-serve path, and the warning text drops the
      "(or pass --strict-index to rescan disk instead)" suffix.
- [x] Remove the `--strict-index` line from the GLOBAL FLAGS block in
      `crates/hyalo-cli/src/cli/help.rs`.
- [x] Remove `"--strict-index"` from `GLOBAL_FLAGS` in
      `crates/xtask/src/command_reference.rs`.
- [x] Delete the four e2e tests that existed solely to exercise the flag
      (`strict_index_falls_back_to_disk_when_index_is_stale`,
      `stale_index_without_strict_still_serves_the_snapshot`,
      `strict_index_is_a_noop_on_a_fresh_index`,
      `strict_index_without_an_index_is_inert`) from
      `crates/hyalo-cli/tests/e2e/index.rs`; the remaining stale-index warning
      test (`stale_index_warns_when_vault_is_newer`) already asserts only the
      substrings that survive the wording change, so it needed no edit.
- [x] Remove the `--strict-index` "Added" entry from CHANGELOG.md's
      `[Unreleased]` section (the flag was never released, so this is a
      removal from the pending changelog, not a "Removed" entry).
- [x] Record the decision in the knowledgebase: DEC-249, noting it supersedes
      the implementation half of DEC-245 (the warn-but-serve default DEC-245
      recorded stays current and unaffected).
- [x] Note in iteration-247's S-2 outcome bullet that the flag was superseded
      by this iteration, without un-ticking its original task.
- [x] Run `target/release/hyalo lint --strict` on the vault and fix anything
      introduced by this iteration's own edits.

## Acceptance criteria

- [x] `hyalo --help` no longer lists `--strict-index` anywhere (global flags
      block or `create-index` long help).
- [x] `grep -rn 'strict.index|strict_index' --exclude-dir=target .` matches
      only historical knowledgebase entries (DEC-245, DEC-249, iter-247,
      the deep-review note) — no live code, help text, or test.
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      and `cargo test --workspace -q` are all green.
- [x] `cargo run -p xtask -- --help` gates (command-reference / flag-accuracy
      checks) pass.
