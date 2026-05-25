---
title: Iteration 144 — Index-suggestion hint (slow query + large vault)
type: iteration
date: 2026-05-24
status: planned
branch: iter-144/index-suggestion-hint
tags:
  - iteration
  - hints
  - performance
  - index
related:
  - "[[backlog/index-suggestion-hint]]"
  - "[[iterations/iteration-143-hint-and-files-from-polish]]"
---

## Goal

Close `backlog/index-suggestion-hint.md` — when a user runs an
expensive query against a large vault without `--index`, hyalo should
suggest creating one. Two surfaces:

1. **Slow-query hint**: any query whose wall-clock exceeds a threshold
   (default 500 ms) and ran without `--index` surfaces a hint to run
   `hyalo create-index`.
2. **Summary hint for large vaults**: `hyalo summary` against a vault
   of >500 files without an active snapshot surfaces the same
   suggestion.

Both suppressed when `--index` / `--index-file` is already in use.
Slow-query hint additionally respects `--quiet`.

Pulled from the MDN dogfooding ticket where property-only queries on a
14K-file vault take ~1.5 s without an index vs ~80 ms with one.

## Steps

### Elapsed-time measurement

- [ ] Capture command elapsed wall-clock in the dispatch layer (start
      before command body, stop after the JSON envelope is built).
- [ ] Thread the elapsed `Duration` into the hint context so
      `hints_for_*` can read it.
- [ ] Constant `SLOW_QUERY_THRESHOLD_MS: u64 = 500`. Document the
      number in code with a one-line rationale.

### Slow-query hint

- [ ] Eligible commands: every command that benefits from `--index`
      (per the iter-139 / earlier list — `find`, `lint`, `backlinks`,
      `properties summary`, `tags summary`, `summary`, `read`).
- [ ] Fire when: elapsed > threshold AND `--index` is NOT active AND
      `--quiet` is NOT set.
- [ ] Hint text: `Query took <N> ms. Create an index for faster
      queries:` → `hyalo create-index`.
- [ ] Counts toward `MAX_HINTS` like any other hint.

### Large-vault summary hint

- [ ] In `hints_for_summary`: when `files_total > 500` AND `--index`
      is NOT active, surface `Vault has <N> files — create an index
      for faster queries:` → `hyalo create-index`.
- [ ] Threshold constant `LARGE_VAULT_FILE_COUNT: u64 = 500`.

### Suppression

- [ ] Verify the existing `--quiet` flag actually suppresses hints
      end-to-end (the slow-query hint is the test case).
- [ ] Per-command `--no-hints` already suppresses everything; nothing
      new there.

### Docs + tests

- [ ] CHANGELOG `Unreleased` entry under Added.
- [ ] README: short note in the perf section about how the hint
      surfaces the index recommendation.
- [ ] Decision-log: brief DEC-045 (or extension of the existing UX
      decisions) capturing thresholds + the choice of wall-clock vs
      file-count signals.
- [ ] Unit tests for both hint paths.
- [ ] E2E: `hyalo find --property X=Y --no-hints` does NOT emit the
      hint even when slow; with hints on it does (using a deliberately
      slow synthetic delay or by adjusting the threshold for the test).
- [ ] Move `backlog/index-suggestion-hint.md` → `backlog/done/` and
      set `status=completed`.

## Tasks

- [x] Wire elapsed-time capture in dispatch
- [x] Plumb elapsed into `HintContext`
- [x] Implement slow-query hint generator
- [x] Implement large-vault summary hint
- [x] Verify `--quiet` suppression path
- [x] Tests (unit + e2e)
- [x] CHANGELOG + decision-log + README
- [x] Close backlog item
- [x] All three CI platforms green

## Acceptance criteria (mirrors the backlog item)

- [x] Slow-query hint emitted when elapsed > 500 ms and no `--index`
- [x] `summary` hints include index suggestion for vaults > 500 files
- [x] Hint is suppressed when `--index` is already in use
- [x] `--quiet` suppresses the slow-query hint

## Design notes

- **Wall-clock, not CPU time.** I/O is the dominant cost for hyalo
  queries; wall-clock matches what the user perceives as "slow".
- **500 ms is a calibrated guess, not a benchmark.** Tunable later.
  Reasoning: shorter than human "wait, this is slow" threshold (~1 s)
  with margin; longer than typical scans on small vaults (~100 ms).
- **Per-command vs global.** A global "always emit when elapsed >
  threshold" is simpler than per-command thresholds. Start global.
- **Don't measure the hint-rendering step itself.** Capture elapsed
  before the hint generator runs; rendering hints should be cheap and
  shouldn't trip its own threshold.

## Out of scope

- Auto-index config (`auto_index = true` in `.hyalo.toml`). The
  original backlog item listed this as a future direction; hyalo
  doesn't want to manage index lifecycle silently. Lint and hints
  surface the suggestion; the user runs `create-index`.
- Tracking per-command performance baselines or detailed profiling
  output. Single elapsed number is enough for the hint decision.

## References

- [[index-suggestion-hint]] — the source ticket (origin: MDN
  dogfood, 2026-03-30)
- [[iteration-143-hint-and-files-from-polish]] — predecessor
  hint-polish iteration
