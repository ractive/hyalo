---
type: iteration
title: >-
  Iteration 256 — envelope unification, help forwarding, and --fields all index
  cost
date: 2026-08-30
status: planned
tags:
  - iteration
  - dogfood-fixes
  - consistency
branch: iter-256/envelope-help-forwarding-index-cost
depends-on: "[[iterations/iteration-254-dogfood-v0220-help-and-shape-fixes]]"
---

# Iteration 256 — envelope unification, help forwarding, and `--fields all` index cost

## Goal

Carry-over sweep from [[iterations/iteration-254-dogfood-v0220-help-and-shape-fixes]]
(PR #296): three findings 254 explicitly named as Non-goals ("file a DEC or
its own iteration" / "decide separately" / "revisit after the next dogfood
round") plus one cosmetic reshuffle 254 deferred the same way. None of the
four touch result shape or short-help wording, so none belonged in 254's
scope; grouped here because each is a standalone decide-then-implement
task, not because they are related to each other.

## Tasks

### COH-9: "every mutating command reports dry_run and skipped_count" is false [0/2]

- [ ] Read `dogfood-v0220-help-efficiency-and-find-shape.md`'s COH-9 section
      in full (`hyalo read dogfood-results/dogfood-v0220-help-efficiency-and-find-shape.md --section COH-9`)
      for the exact commands whose envelope omits `dry_run`/`skipped_count`
      despite `rule-knowledgebase.md` claiming it is universal. Enumerate
      every mutating command's actual envelope shape.
- [ ] Decide: either make the envelope actually uniform (add the missing
      keys to the outliers) or soften `rule-knowledgebase.md`'s claim to
      name the exceptions. Record as a DEC either way — this is a
      documented-contract question, not a one-line fix. Update the bundled
      `SKILL.md`s and `CLAUDE.md` if the contract text changes.

### HELP-5: `hyalo help <cmd>` renders the full `--help`, not `-h` [0/1]

- [ ] Decide whether `hyalo help <cmd>` should forward to the short `-h`
      page instead (agents default to `help <cmd>` out of habit and get the
      26 KB long form when they likely wanted the 2-3 KB short one). This is
      a clap `Subcommand::Help` behaviour change — confirm clap-derive
      supports intercepting it before committing to the approach; if not,
      document the `hyalo <cmd> -h` alternative more prominently instead
      (root `-h`'s "Everything else" line already added `hyalo help <cmd> =
      --help` in iter-251/254 — verify it's still accurate) and close this
      as won't-fix with a DEC explaining the clap constraint.

### FIND-8: `--fields all` costs ~20% wall time on an indexed vault [0/1]

- [ ] Profile `find --fields all --index --limit 1` vs `find --index --limit 1`
      on a large indexed vault (reuse the `bench_scale` xtask harness —
      `cargo run -p xtask -- bench-scale` — or the MDN/GitHub-Docs vaults
      from the `dogfood` skill) to confirm the ~20% figure and find where
      the cost actually is (materialising `sections`/`links`/`tasks`/
      `backlinks`/`properties-typed` from the snapshot index vs. computing
      them fresh). Decide whether it's worth lazy-computing only the
      fields actually requested when reading from an index (the DEC-254
      exact-projection machinery this iteration depends on may make that
      easier now than when FIND-8 was first filed) or whether the cost is
      inherent and just needs documenting on `--fields all`'s help text.

### Root `-h` example set and command-group reshuffle (LOW) [0/1]

- [ ] Revisit the top-level `-h` COMMANDS grouping and the five EXAMPLES
      lines chosen in iter-251/254 against a fresh dogfood pass — 254's
      Non-goals flagged this as "revisit after the next dogfood round",
      which this iteration's own carry-over sweep now is. If nothing reads
      wrong on a fresh read, close as no-op with a one-line note; don't
      force a change for its own sake.

## Acceptance criteria

- [ ] COH-9 has a recorded DEC (either envelope made uniform, with tests, or
      the claim corrected in docs) and `rule-knowledgebase.md` matches
      reality either way.
- [ ] HELP-5 has a recorded decision (forwarded, or won't-fix with the clap
      constraint documented) — not left ambiguous.
- [ ] FIND-8's cost is measured on this iteration's code (not assumed from
      254's dogfood run) and either fixed or documented as inherent, with
      the number recorded.
- [ ] The root `-h` reshuffle item is explicitly closed (changed or
      no-op'd), not silently dropped.
- [ ] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, all `xtask check-*`, `hyalo lint --strict`.

## Non-goals

- None of these four items are load-bearing for a release; if time is
  short, COH-9 and HELP-5 (both decision debt) take priority over FIND-8
  (a measured-but-tolerable perf cost) and the cosmetic reshuffle.

## Links

- [[dogfood-results/dogfood-v0220-help-efficiency-and-find-shape]]
- [[iterations/iteration-254-dogfood-v0220-help-and-shape-fixes]]
