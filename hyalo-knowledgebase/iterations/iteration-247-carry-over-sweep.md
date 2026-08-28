---
title: "Iteration 247 — carry-over sweep from iter-246 (deep-review non-goals and dogfood notes)"
type: iteration
date: 2026-08-28
tags:
  - iteration
  - carry-over
  - review
status: planned
branch: iter-247/carry-over-sweep
related:
  - "[[iterations/iteration-246-help-coherence-review-followups]]"
  - "[[reviews/deep-review-2026-08-27]]"
  - "[[iterations/iteration-245-deferral-carryovers]]"
---

# Iteration 247 — carry-over sweep from iter-246

## Goal

Keep every item [[iterations/iteration-246-help-coherence-review-followups]]
declared a non-goal or flagged as out-of-scope from being silently
forgotten. Iteration 246 itself is fully complete (all 27 tasks/ACs ticked);
everything below was explicitly parked during it.

## Tasks

- [ ] Large-file refactor (review hotspot): `crates/hyalo-cli/src/commands/hints.rs`
      (~5,059 lines), `lint.rs` (~4,005), and the output modules are large
      single files. Split into cohesive submodules without behavior change;
      gates must stay green. Declared a non-goal of iter-246 to keep its diff
      reviewable.
- [ ] S-2 stale-index design: warn-but-serve staleness is a deliberate
      trade-off (recorded, not changed, in iter-246). The review suggests an
      opt-in `--strict-index` that falls back to disk on staleness — decide
      whether to implement it or record the decision to keep warn-but-serve
      permanently (owner call, cf. DEC-098/DEC-101 discipline).
- [ ] Vault schema drift: files under `reviews/` use `type: review`, but
      `hyalo types show review` fails and `reviews/` sits in `[lint] ignore`
      so nothing notices. Either register a `review` type in the schema or
      migrate the files to a declared type and shrink the ignore list.
- [ ] `summary` text output prints a `kb dir: hyalo-knowledgebase` banner as
      its first line — the only command doing so; mildly breaks the
      "text output is data" contract when scripting in text mode. Move it to
      stderr or behind a flag (decide with care: hints/output drift).
- [ ] Feature request (minor): `hyalo find --changed-since <ref>` as a
      friendlier built-in for the `--files-from <(git diff …)` cookbook
      pattern. Decide whether to implement or reject with rationale.

## Acceptance criteria

- [ ] Each task above is implemented, or explicitly closed with a recorded
      owner decision (decision-log entry) instead of re-deferral.
- [ ] All quality gates green; no clippy/fmt/test regressions.
