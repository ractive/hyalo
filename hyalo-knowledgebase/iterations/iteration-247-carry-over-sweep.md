---
title: "Iteration 247 — carry-over sweep from iter-246 (deep-review non-goals and dogfood notes)"
type: iteration
date: 2026-08-28
tags:
  - iteration
  - carry-over
  - review
status: completed
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

- [x] Large-file refactor (review hotspot): `crates/hyalo-cli/src/commands/hints.rs`
      (~5,059 lines), `lint.rs` (~4,005), and the output modules are large
      single files. Split into cohesive submodules without behavior change;
      gates must stay green. Declared a non-goal of iter-246 to keep its diff
      reviewable.
- [x] S-2 stale-index design: warn-but-serve staleness is a deliberate
      trade-off (recorded, not changed, in iter-246). The review suggests an
      opt-in `--strict-index` that falls back to disk on staleness — decide
      whether to implement it or record the decision to keep warn-but-serve
      permanently (owner call, cf. DEC-098/DEC-101 discipline).
- [x] Vault schema drift: files under `reviews/` use `type: review`, but
      `hyalo types show review` fails and `reviews/` sits in `[lint] ignore`
      so nothing notices. Either register a `review` type in the schema or
      migrate the files to a declared type and shrink the ignore list.
- [x] `summary` text output prints a `kb dir: hyalo-knowledgebase` banner as
      its first line — the only command doing so; mildly breaks the
      "text output is data" contract when scripting in text mode. Move it to
      stderr or behind a flag (decide with care: hints/output drift).
- [x] Feature request (minor): `hyalo find --changed-since <ref>` as a
      friendlier built-in for the `--files-from <(git diff …)` cookbook
      pattern. Decide whether to implement or reject with rationale.

## Acceptance criteria

- [x] Each task above is implemented, or explicitly closed with a recorded
      owner decision (decision-log entry) instead of re-deferral.
- [x] All quality gates green; no clippy/fmt/test regressions.

## Outcome

All five carry-overs are closed — three implemented, two settled by recorded
decision as the plan's acceptance criteria allow.

- **Large-file refactor.** `crates/hyalo-cli/src/hints.rs` (5,059 lines),
  `crates/hyalo-cli/src/commands/lint.rs` (4,005) and
  `crates/hyalo-cli/src/output.rs` (3,744) are now module directories. Largest
  remaining non-test file among them: `commands/lint/file.rs` at 974 lines,
  which is one function (`lint_one_file_extended`) and cannot shrink without a
  behaviour-changing refactor. Pure file moves: every item keeps the visibility
  it had inside the single module, imports are explicit (no `use super::*`
  outside test modules), and the `no_raw_hyalo_command_literals` guard test
  learned to skip whole-file test modules now that one exists.
- **S-2 stale index.** `--strict-index` implemented as a global opt-in;
  warn-but-serve kept as the default and recorded as deliberate — see
  [[decision-log]] DEC-245.
- **Vault schema drift.** `review` is a declared type, `reviews/**` is out of
  `[lint] ignore`, two mislabelled `type: research` files are migrated, and
  `hyalo lint --strict` is clean vault-wide — DEC-248.
- **`summary` banner.** Moved to stderr as `note: kb dir: …` — DEC-247.
- **`find --changed-since`.** Rejected with rationale (hyalo spawns no
  subprocess and takes no VCS dependency); the `--files-from -` recipe is now
  documented on the flag itself — DEC-246.
