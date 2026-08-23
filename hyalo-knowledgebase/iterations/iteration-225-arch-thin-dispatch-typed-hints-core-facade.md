---
title: "Iteration 225 — architecture: thin dispatch, typed hints, hyalo-core façade"
type: iteration
date: 2026-08-23
status: planned
branch: iter-225/arch-thin-dispatch-typed-hints
tags: [iteration, architecture, refactor, tech-debt]
related:
  - "[[reviews/deep-analysis-2-2026-08-23]]"
---

# Iteration 225 — architecture: thin dispatch, typed hints, core façade

## Goal

Address the architecture findings from
[[reviews/deep-analysis-2-2026-08-23]] that share one theme — a typed
surface replacing hand-maintained parallel copies: ARCH-1 (god dispatcher),
ARCH-4 (string-built hints that drift), ARCH-5 (hyalo-core exposes
everything). ARCH-6 (presentation-layer mass) is largely subsumed and used
as the success measure.

**Not release-gating.** This is maintainability debt, larger and riskier
than the correctness iterations. Sequence it AFTER the security/correctness
iterations (221–224) and after the in-flight 219/220 land, to avoid enormous
merge conflicts in `dispatch.rs`/`hints.rs`. Consider splitting each ARCH
item into its own PR under this branch's plan if the diff gets unreviewable.

## Context

- **ARCH-1** — `crates/hyalo-cli/src/dispatch.rs:494` is one 2,100-line
  `match` with 26 `Commands::` arms carrying real business logic (the `Find`
  arm ~240 lines, `Lint` arm ~390): filter merging, warning policy, view
  adaptation (`adapt_view_result_to_ext:256`), snapshot patching
  (`patch_index_for_modified_files:427`). Command behavior can't be
  unit-tested without the whole dispatch path — hence process-spawning e2e.
- **ARCH-4** — `crates/hyalo-cli/src/hints.rs` (4,522 lines, 168 fns, 29
  hand-assembled `"hyalo ..."` strings) is a parallel copy of the CLI
  surface that must mirror clap by hand. `tests/e2e/hint_execution.rs`
  exists precisely because the design guarantees drift (the `tags --limit 0`
  hint that satisfied a substring test while failing to run).
- **ARCH-5** — `crates/hyalo-core/src/lib.rs:1-26` marks all 26 modules
  `pub` including plumbing (`fs_util`, `util`, `warn`) and internals; every
  internal refactor is semver-breaking and invariants are doc-comment-only.
  (The `math.rs` example the review cited was uncommitted test scaffolding,
  since removed — the façade point stands on its own.)

## Tasks

- [ ] ARCH-1: extract one handler per command
      (`commands::<cmd>::run(<Args>) -> CommandOutcome`), moving the inline
      filter-merge / warning-policy / view-adaptation / snapshot-patch logic
      out of the dispatch arms. `dispatch` shrinks to parsing `Commands` into
      args structs and calling handlers. The extracted warnings become
      in-process unit-testable. Do this incrementally, one command per
      commit, starting with the largest arms (Find, Lint)
- [ ] ARCH-4: stop building hints as hand-written strings. Introduce a
      `HintBuilder::cmd(argv)` API that serializes through the existing
      `shell_quote`, and require all NEW/edited hints to go through it;
      ideally back it with a typed command registry shared with
      `cli/args.rs` so the hinted command cannot reference a flag the command
      doesn't accept. Migrate the existing 29 hand-assembled strings
      opportunistically, prioritizing the ones `hint_execution.rs` can't
      reach
- [ ] ARCH-5: curate a root `pub use` façade in `hyalo-core/lib.rs`
      (parse / find / mutate / link-graph / index types), demote internal
      modules (`fs_util`, `util`, `warn`, `case_index`, `common_words`, …)
      to `pub(crate)`, and mark the supported surface. Where a module must
      stay `pub` for the CLI, re-export the specific items instead of the
      whole module. This is semver-cheap NOW, expensive after external
      consumers appear
- [ ] Prove the ARCH-1 win: at least one command's warning/policy behavior
      gains an in-process unit test that previously required an e2e spawn
- [ ] Docs: decision-log entries for the handler pattern, the hint API, and
      the core façade boundary; update any contributor notes on "where does a
      new command go"

## Acceptance criteria

- [ ] `dispatch.rs` arms no longer contain command business logic — each arm
      parses args and calls a `commands::<cmd>::run`; the god-function line
      count drops materially (record before/after)
- [ ] New hints cannot be written as raw strings without going through the
      argv-based `HintBuilder`; the migrated hints still pass
      `hint_execution.rs`
- [ ] `hyalo-core`'s public surface is a curated façade; plumbing modules are
      `pub(crate)`; the workspace still builds and all tests pass
- [ ] At least one previously-e2e-only behavior is now covered by an
      in-process unit test

## Non-goals

- Moving the lint subsystem into hyalo-mdlint (ARCH-2) and unifying index
  maintenance (ARCH-3) — [[iterations/iteration-226-arch-lint-crate-index-journal]]
- Any behavior change — this is a pure structural refactor; output and exit
  codes stay byte-identical (the e2e suite is the guard)
