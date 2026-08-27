---
title: "Iteration 243 — index/disk parity bugfix wave: BUG-1 upsert, BUG-2 heal, BUG-5 sort, BUG-4 (timeboxed)"
type: iteration
date: 2026-08-27
tags:
  - iteration
  - bug-fix
  - index
  - mutation-journal
  - testing
status: planned
branch: iter-243/index-parity-bugfixes
---

# Iteration 243 — index/disk parity bugfix wave

## Goal

Close the four remaining open BUGs from the v0.20.0 dogfood report
([[dogfood-results/dogfood-v0200-arch-refactors-and-agent-cli-followups]]),
unified by one theme: **`--index` output must be indistinguishable from a
disk scan**. This is the bugfix-wave precedent of iter-240, applied to the
snapshot-index/MutationJournal stack from iters 226/241.

## Context

- BUG-1 is the anchor and the only MEDIUM-HIGH: every mutating command
  except `new` refreshes only entries the index already knows, so files
  created outside hyalo (editor/Obsidian) and then mutated through hyalo
  stay invisible to all indexed reads and never contribute links to the
  graph. The journal knows the rel_path and that the write succeeded — an
  upsert-on-miss closes it (iter-226 ARCH-3 territory).
- BUG-2's *detection* half landed in iter-241 (DEC-241: mtime-check +
  warning). The heal half (rescan drift before the `links fix` discovery
  pass) and the misleading `applied: true` cosmetic remain.
- BUG-3 (`--iteration abc` error message) is **closed by removal** —
  `--iteration` was deleted in iter-242 (DEC-242). Record-only, no code.
- BUG-4 (BM25 corpus-statistic drift) is LOW and harmless, but makes
  indexed-vs-disk output non-diffable — same theme, included with a
  timebox.

## Tasks

- [ ] BUG-1 — MutationJournal refresh upserts on miss: after a successful
      write, a file absent from the index gets a full entry (frontmatter,
      tasks, links) inserted, not just existing entries refreshed
- [ ] BUG-1 — e2e tests per mutating command (`set`, `set --tag`,
      `task toggle`, `append`, `remove`, `lint --fix`, `links fix --apply`,
      `mv`, `tags rename`): mutate an index-unknown file with `--index`,
      then `find --file` / `backlinks` via index match the disk scan
      (reuse the dogfood diff-harness method: indexed vs disk JSON for
      find/backlinks/summary queries)
- [ ] BUG-2 — heal half: when the pre-discovery mtime check (iter-241)
      detects drift, rescan the drifted entries so `links fix --apply
      --apply-fuzzy --index` actually finds and fixes editor-introduced
      broken links (dogfood BUG-2 repro must pass end-to-end)
- [ ] BUG-2 — `applied` in the `links fix` output means "something was
      applied", not "apply mode"; `fixes: 0` must report `applied: false`
      in both text and JSON
- [ ] BUG-5 — `backlinks` sorted by `(source, line)` on both the index and
      disk paths so outputs are diffable and stable across refreshes
- [ ] BUG-4 (TIMEBOXED to half a day) — find and fix the divergent BM25
      corpus statistic (avg doc length / token count) between index and
      disk paths; if root cause isn't found in the box, close the task as
      "investigated, not fixed" and leave the dogfood note updated — do
      not let it stall the iteration
- [ ] Record BUG-3 as closed-by-removal in the dogfood note
- [ ] E2e tests for every changed command surface; existing tests stay
      green
- [ ] Keep help text, COMMAND REFERENCE, CHANGELOG [Unreleased] in sync
      with any output changes (`applied` semantics, backlinks order)
- [ ] `cargo fmt` / `cargo clippy --workspace --all-targets -- -D warnings`
      / `cargo test --workspace -q` clean; xtask gates green
      (`check-help-drift`, `check-command-reference`,
      `check-mutation-journal`)
- [ ] Dogfood the release build against this vault and the scratch-copy
      diff harness before merge; update the dogfood note's BUG list

## Acceptance criteria

- [ ] The dogfood BUG-1 repro passes: `set` (and every other mutating
      command) on an index-unknown file makes that file findable via
      `--index` (`total: 1`, not 0) and its outgoing links appear in
      `backlinks` via `--index` — matching disk scan exactly
- [ ] The dogfood BUG-2 repro passes: editor-appended broken `[[…]]` is
      discovered and fixed by `links fix --apply --apply-fuzzy --index`,
      no silent trust, `applied` reflects actual fixes
- [ ] `backlinks <target> --index` output is byte-identical to the disk
      scan after a mutation wave (same entries, same order)
- [ ] BUG-4 either fixed (identical BM25 scores, both paths, pre- and
      post-mutation) or explicitly timeboxed-out with an updated note
- [ ] All quality gates green; PR merged through the standard flow

## Non-goals

- UX-3 (nested YAML dot-path property filters), UX-5 (`read --iteration`
  body-less files — flag removed anyway), UX-6 (MDN case-insensitive
  resolve) — remain parked
- No release cut — v0.21.0 stays ON HOLD per iter-241
- No schema/API changes beyond the `applied` field semantics

## Links

- [[dogfood-results/dogfood-v0200-arch-refactors-and-agent-cli-followups]]
- [[iterations/iteration-226-arch-lint-crate-index-journal]]
- [[iterations/iteration-240-review-followups-bugfixes]]
- [[iterations/iteration-241-stale-index-detection-and-ux-fixes]]
- [[iterations/iteration-242-remove-iteration-flag]]
