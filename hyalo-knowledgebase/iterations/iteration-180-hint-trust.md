---
title: Iteration 180 — hint trust (filter preservation, --dir, honest counters)
type: iteration
date: 2026-07-18
status: planned
branch: iter-180/hint-trust
tags:
  - iteration
  - hints
  - ux
related:
  - "[[dogfood-results/dogfood-v0180-final-pre-release]]"
  - "[[iterations/done/iteration-80-smarter-hints]]"
---

# Iteration 180 — hint trust

## Goal

Hints are copy-paste contracts — every hinted command must reproduce what
the hint describes. The dogfood found three MEDIUM violations plus stale
and misleading variants
([[dogfood-results/dogfood-v0180-final-pre-release]] BUG-7/8/9).

## Tasks

### 1. Hints carry the vault context (BUG-7, MEDIUM)

- [ ] `create-index` perf hint includes `--dir <path>` when the command
  ran with an explicit `--dir` (currently the one hint family that drops
  it — running it verbatim indexes the wrong vault)
- [ ] Fix the dangling-colon description (`... for faster queries:`) on
  both the slow-command and `summary` variants
- [ ] Audit all hint templates for other dropped global flags
  (`--index-file` and `--first-only` were the 2026-07-10 instances)

### 2. Hints preserve active filters (BUG-8, MEDIUM)

- [ ] Derived hints keep every active filter: `find --orphan` "show all"
  hint includes `--orphan`; "narrow by tag" hints compose with the
  current filter and compute counts on the filtered set (dogfood: hint
  said 79/27, commands returned 338/146)
- [ ] Generalize: hint generation takes the full active filter set as
  input rather than reconstructing a minimal command

### 3. Summary counters honest (BUG-9, MEDIUM)

- [ ] `summary`'s schema counters either apply `[lint] ignore` globs (so
  the "Lint: N errors" hint matches `hyalo lint`) or the label changes to
  say what is actually counted (e.g. "Schema (incl. lint-ignored)");
  decide and record
- [ ] Post-`lint --fix` output drops the stale pre-fix "Show all N files
  with issues" hint (recompute or suppress after apply)

### 4. Did-you-mean false positives (LOW)

- [ ] Property-value similarity suggestions skip values differing only in
  a numeric suffix (`hero-6` vs `hero-4` are distinct assets, not typos);
  reconsider whether read-only `summary` should emit them at all

### 5. Site-URL vault heuristic (enhancement, from MDN testing)

- [ ] When ~all links are unresolvable and look like absolute site URLs
  (MDN: 49,933/49,935 "broken"), emit a diagnostic hint suggesting
  `--site-prefix` instead of offering `links fix` on 50k links

### 6. Retrospective

- [ ] Update remaining planned iterations with anything learned

## Acceptance Criteria

- [ ] e2e: every emitted hint, executed verbatim in the same context,
  returns results consistent with the hint's description and counts
- [ ] `cargo fmt` / `clippy -D warnings` / `cargo test -q` clean
