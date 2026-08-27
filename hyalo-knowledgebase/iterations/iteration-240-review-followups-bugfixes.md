---
title: "Iteration 240 — review follow-ups for iter-225/226: index upsert, CI journal gate, iteration-ID errors"
type: iteration
date: 2026-08-27
status: completed
branch: iter-240/review-followups-docs
tags:
  - iteration
  - bugfix
  - index
  - review
related:
  - "[[dogfood-results/dogfood-v0200-arch-refactors-and-agent-cli-followups]]"
  - "[[iterations/iteration-225-arch-thin-dispatch-typed-hints-core-facade]]"
  - "[[iterations/iteration-226-arch-lint-crate-index-journal]]"
  - "[[iterations/iteration-238-agent-cli-followups]]"
---

# Iteration 240 — review follow-ups for iter-225/226

## Goal

Close the gaps found by an independent code review of PRs #270–#274 and the
[[dogfood-results/dogfood-v0200-arch-refactors-and-agent-cli-followups]]
session that followed: one missing CI gate, one medium-high index bug the
`MutationJournal` refactor left open, two smaller correctness/UX bugs, one
test-coverage gap and three nits.

## Context

The code landed in **PR #275** on branch `fix/review-followups-iter-225-226`
(merged 2026-08-27, commit `1e2fe2e`) *before* this iteration file existed —
the fixes were small enough to ship straight from the review session. This
iteration is the retroactive record; its own branch (`iter-240/…`, required
by the iteration schema's `branch` pattern) carries only this documentation
and the decision-log entry DEC-240 in [[decision-log]].

Review findings that motivated it (per PR):

- **#274 / iter-226** (B+): `xtask check-mutation-journal` was described in
  the PR body, CHANGELOG and DEC-226 as a CI gate but was never added to
  `quality-gates.yml`. Architecture (lint crate boundary, single journal)
  verified as genuinely achieved.
- **#273 / iter-225** (A-): dispatch and façade goals achieved; "typed hints"
  oversold — `Hint.cmd` is still a flattened string. Dead façade re-export
  `MIN_COMMON_WORD_LEN`.
- **#271 / iter-238** (B+): `task set --iteration` claimed and wired but
  untested; needless `sel.clone()`.
- **#272 / iter-206** (A-), **#270 / iter-234** (A): no functional findings;
  one stale `LintOutput` doc comment.

## Tasks

- [x] Wire `cargo run -p xtask -- check-mutation-journal` into
      `.github/workflows/quality-gates.yml` (same pattern as the other four
      xtask gates)
- [x] BUG-1 — `MutationJournal::{update_entry, update_task, rescan_modified}`
      upsert a file the snapshot index has never seen instead of no-op'ing;
      new `SnapshotIndex::insert_or_replace_entry_with_links` inserts the
      entry and registers outbound edges in one scan
- [x] BUG-2 (text half) — `links fix --apply` text summary reads
      `Applied: yes (N fixes)`; JSON `applied` keeps its D-4 "apply mode was
      used" meaning
- [x] BUG-3 — `--iteration abc` reports `iteration ID 'abc' is not numeric`
      via new `IterationIdParseError::NotNumeric` instead of "is empty"
- [x] Add e2e test `task_set_iteration_resolves_and_sets`
- [x] Nits: remove `MIN_COMMON_WORD_LEN` re-export from `hyalo-core/lib.rs`;
      fix `LintOutput` → `ExtLintOutput` doc comment in `output.rs`;
      `selection_with_iteration_resolved` takes `InputSelection` by value
- [x] CHANGELOG `[Unreleased] > Fixed` entries for BUG-1/2/3
- [x] Commit the dogfood v0.20.0 report
- [x] Decision-log entry DEC-240 (journal upsert semantics; `applied` kept)

## Acceptance criteria

- [x] `find --file ext.md --index` returns the file after
      `set ext.md … --index` on a file created after `create-index`
      (e2e: `set_upserts_unindexed_file_into_persisted_index`,
      `task_toggle_upserts_unindexed_file_into_persisted_index`)
- [x] `check-mutation-journal` runs and passes on PR CI
- [x] `cargo fmt` / `clippy -D warnings` / `cargo test --workspace` clean;
      4,121 tests pass on Linux, macOS and Windows
- [x] `check-help-drift` and `check-command-reference` green

## Non-goals

- Stale-index *detection* for `--index` mutations (BUG-2's other half) —
  needs a design decision (mtime check vs. warning vs. refuse), see below
- Changing the JSON `applied` semantics

## Out of scope / carry-over candidates

Open findings from the dogfood report, not addressed here:

- **BUG-2 (detection)**: `links fix --apply --index` trusts a stale index
  silently — externally added broken links yield `broken: 0`, `applied:
  true`, file untouched, no warning
- **BUG-4/5 (LOW)**: BM25 scores (1.33928 vs 1.33902) and backlink order
  differ marginally indexed vs disk; ranking/counts equal
- **UX-1**: error hints say `hyalo find --file <glob>` but `--file` does not
  glob (`--glob` does)
- **UX-2**: `--iteration 2` cannot reach `iterations/done/iteration-02-*.md`
  (zero-padding + subdirectory)
- **UX-3**: nested-YAML dot-path filters (`versions.fpt=*`) return no results
  silently; `versions~=fpt` works but is undiscoverable
- **UX-4**: lint text summary counts errors that the truncated 50-file
  listing hides; only visible with `--limit 0`. The 4 GH-Docs errors are
  MD011 false positives on regex text like `(3rd|[Tt]hird)[-_]`
- **UX-5**: `read --iteration` on a body-less file prints nothing, exit 0
- **UX-6**: MDN with the correct site prefix reports 49,262 "case
  mismatches" (lowercase dirs); a case-insensitive resolve option is missing
- No way to lint a file under `[lint] ignore` on demand
- Typed hint data model (`Hint.cmd` as argv end-to-end) — the ARCH-4 stretch
  goal iter-225 explicitly deferred
- Batch `mv` calls `journal.rename_entry` with the whole batch's
  `rewritten_paths` per pair (N×M rescans; pre-existing, idempotent)
