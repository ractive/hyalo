---
type: iteration
title: Iteration 234 — remove dead LintOutput / lint_files_with_options
date: 2026-08-25
status: planned
tags:
  - iteration
  - cleanup
  - tech-debt
branch: iter-234/lint-dead-output-cleanup
related:
  - "[[iterations/iteration-216-results-shape-consistency]]"
  - "[[research/results-json-shape-inventory]]"
---

# Iteration 234 — remove dead LintOutput / lint_files_with_options

## Goal

Delete the unreachable `LintOutput` struct and `lint_files_with_options`
function in `crates/hyalo-cli/src/commands/lint.rs`, and everything that
exists only to serve them (the plain `FileLintResult` shape, if it has no
other caller).

## Context

Carried over from [[iterations/iteration-216-results-shape-consistency]]'s
`results-json-shape-inventory.md`, finding **J-9**: `lint_files_with_options`
/ `LintOutput` is not reachable from any production code path — grep
confirms its only callers are two unit tests in `lint.rs`
(`lint_files_with_options` at the two call sites, not `dispatch.rs` or
`run.rs`). The live command path uses `lint_files_extended` /
`ExtLintOutput` exclusively.

`LintOutput` still carries the pre-iter-216 `files_with_issues` field name
(not renamed to `files_with_violations` — see J-9) precisely because
renaming a dead shape was judged not worth the churn during iter-216. That
reasoning holds only as long as the shape stays dead and undocumented as a
public output format. Since nothing produces this JSON at runtime, the
cleanest fix is deletion, not a rename that then needs re-justifying next
time someone surveys `results` shapes.

## Tasks

- [ ] Confirm (re-grep, since code moves) that `lint_files_with_options`
      and `LintOutput` have no caller outside `#[cfg(test)]` code.
- [ ] Delete `LintOutput`, `lint_files_with_options`, and their two unit
      tests (`lint_json_counters_describe_whole_run_on_large_clean_vault`-
      style tests already cover the equivalent behavior through
      `lint_files_extended` — verify before deleting, don't just assume).
- [ ] Delete `FileLintResult` and any other type that exists solely to
      support the deleted function, after confirming no other caller.
- [ ] Re-run the full test suite and confirm no coverage gap opened up —
      if `lint_files_with_options` tested something `lint_files_extended`
      doesn't, port that assertion first.

## Acceptance criteria

- [ ] `grep -rn 'lint_files_with_options\|struct LintOutput'` across
      `crates/` returns nothing (including in tests)
- [ ] No test coverage lost — assertions that only existed against the
      deleted path are ported to the `ExtLintOutput` / `lint_files_extended`
      path first
- [ ] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean

## Non-goals

- Any further `results` shape renames — iter-216 already surveyed those;
  this iteration is pure dead-code removal.
