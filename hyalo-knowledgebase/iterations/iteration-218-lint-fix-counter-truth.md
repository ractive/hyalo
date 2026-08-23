---
title: "Iteration 218 — lint --fix counter truth and column-semantics stragglers"
type: iteration
date: 2026-08-23
status: in-progress
branch: iter-218/lint-fix-counter-truth
tags: [iteration, lint, cli, output-truth]
related:
  - "[[dogfood-results/dogfood-v0210-pre3-fix-waves-207-214]]"
  - "[[iterations/iteration-210-output-truth]]"
---

# Iteration 218 — lint --fix counter truth and column-semantics stragglers

## Goal

Finish what [[iterations/iteration-210-output-truth]] started: the `--fix`
path still computes its totals over the display-truncated file list
(NEW-6), MD010 still reports byte columns (NEW-11), and one `[writes]`
hint promises fixes it will not make (NEW-14).

## Context

From [[dogfood-results/dogfood-v0210-pre3-fix-waves-207-214]], found
independently by two agents:

- **NEW-6 (MEDIUM)**: on GH Docs at the default limit, `lint --fix` prints
  `3710 files checked: fixed 646 · remaining 939 · conflicts 0` while
  actually modifying 671 files (~1,604 fixes); `--limit 100000` shows the
  truth: `fixed 1618 · remaining 6109 · conflicts 12`. A user who dry-runs
  at the default limit sees `conflicts 0` and silently hits 12 on apply.
  Writes are complete (byte-identical trees with/without `--limit 0`) —
  only the report lies. Both text footer (`commands/lint.rs` /
  `output.rs`) and JSON (`total_fixed`, `total_remaining`,
  `total_conflicts`) are affected. Plain `lint` counters are correct and
  limit-invariant; reuse that accumulation for the fix path.
- **NEW-6b**: `errors`/`warnings` silently change meaning between plain
  `lint` (whole-run severity counts) and `lint --fix` (remaining-only)
  under the same JSON key names. Make the keys mean one thing, or rename.
- **NEW-11 (LOW-MEDIUM)**: MD010 columns are byte-indexed; DEC-073 says
  1-based Unicode scalars. `àéî\tTAB` → reported col 7, expected 4; emoji
  line → 5, expected 2. MD009/MD011/MD034 are scalar-correct. Audit all
  rules that emit columns, not just MD010.
- **NEW-14 (LOW)**: when every fuzzy candidate is below the confidence
  floor, `hyalo links` still emits `=> hyalo links fix --apply
  --apply-fuzzy … # Review then apply 3253 lower-confidence fuzzy fixes
  [writes]`; running it verbatim prints `Applied: yes` and changes 0
  files. Count post-floor candidates in the hint, or point at
  `--min-confidence` when the applicable count is 0.

## Tasks

- [x] Accumulate `lint --fix` totals (fixed / remaining / conflicts, per
      rule and whole-run) over the full run before display truncation, in
      both text footer and JSON; `--limit` affects listing only
- [x] Unify or rename `errors`/`warnings` so the same key never switches
      between whole-run and remaining-only semantics across modes
- [x] MD010 (and any other straggler found by an audit of column-emitting
      rules) reports 1-based Unicode scalar columns per DEC-073
- [x] The links `[writes]` fuzzy hint counts only candidates at/above the
      effective floor; when 0, the hint suggests `--min-confidence`
      review instead of promising an apply
- [x] e2e tests: fix totals invariant across `--limit 1/50/100000` on a
      fixture with >50 violating files including conflicts; MD010 column
      on multibyte line; hint text at 0-applicable-fuzzy
- [x] Docs sync in same PR: `lint --help` counter wording if it changes,
      CHANGELOG

## Acceptance criteria

- [x] `lint --fix --dry-run` totals identical at any `--limit`, equal to
      `--limit 0`, and equal to what a real `--fix` run then writes
- [x] `conflicts` is never understated by truncation
- [x] MD010 on `àéî\tTAB` reports column 4; emoji line reports column 2
- [x] The below-floor hint either names an accurate count or does not
      claim it will apply anything

## Non-goals

- `.results` JSON shape unification across commands
  ([[iterations/iteration-216-results-shape-consistency]])
- New lint rules
