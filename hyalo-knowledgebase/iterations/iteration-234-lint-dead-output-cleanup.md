---
type: iteration
title: Iteration 234 — remove dead LintOutput / lint_files_with_options
date: 2026-08-25
status: completed
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

- [x] Confirm (re-grep, since code moves) that `lint_files_with_options`
      and `LintOutput` have no caller outside `#[cfg(test)]` code.
- [x] Delete `LintOutput`, `lint_files_with_options`, and their two unit
      tests (`lint_json_counters_describe_whole_run_on_large_clean_vault`-
      style tests already cover the equivalent behavior through
      `lint_files_extended` — verify before deleting, don't just assume).
- [x] Delete `FileLintResult` and any other type that exists solely to
      support the deleted function, after confirming no other caller.
- [x] Re-run the full test suite and confirm no coverage gap opened up —
      if `lint_files_with_options` tested something `lint_files_extended`
      doesn't, port that assertion first.

## Acceptance criteria

- [x] `grep -rn 'lint_files_with_options\|struct LintOutput'` across
      `crates/` returns nothing (including in tests)
- [x] No test coverage lost — assertions that only existed against the
      deleted path are ported to the `ExtLintOutput` / `lint_files_extended`
      path first
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean

## Non-goals

- Any further `results` shape renames — iter-216 already surveyed those;
  this iteration is pure dead-code removal.

## Outcome

- Deleted `LintOutput` and `lint_files_with_options`; their only callers
  were two unit tests in `lint.rs`.
- `FileLintResult` was **kept**: it has live callers outside the deleted
  function (`dispatch.rs` config-lint results, `lint_counts_only`,
  `lint_file_with_fix`, `validate_views`, `validate_schema_config`), so it
  does not exist solely to serve the deleted path. `FixMode`, `FileFixResult`
  and `FixAction` are likewise still used by the extended path.
- Both deleted tests were ported to the `lint_files_extended` path:
  `lint_no_schema_no_violations` now runs through `lint_extended_strict`, and
  `lint_fix_splits_comma_joined_tags` drives a real `ExtLintOptions` with
  `FixMode::Apply`, asserting the on-disk split-tag result.
- Stale comments referencing `LintOutput` updated (`output.rs` shape-detection,
  e2e `common/mod.rs`). J-9 in `research/results-json-shape-inventory.md`
  marked resolved.
