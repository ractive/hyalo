---
title: "Iteration 238 — agent-CLI ergonomics follow-ups: --filenames0, --iteration on read/task, properties envelope, title~= normalization"
type: iteration
date: 2026-08-25
status: completed
branch: iter-238/agent-cli-followups
tags:
  - iteration
  - cli
  - ergonomics
---

# Iteration 238 — agent-CLI ergonomics follow-ups

## Goal

Fold in the carry-over candidates deliberately deferred out of
[[iterations/iteration-235-agent-cli-ergonomics]] so they are not silently
forgotten. Each is small and independently shippable; land only the ones a
dogfood session still shows friction for — treat this plan as a queue, not a
must-ship-all list.

## Context

Iteration 235 shipped `find --filenames-only`, `find`/`set --iteration <ID>`,
and the self-healing vault-boundary error. Its non-goals (all deliberate)
became this plan. None of them fit iterations 236 (typed pi tools, template
only) or 237 (pi package distribution, template only), which are both
"no change to the Rust crate" iterations.

## Tasks

- [x] `find --filenames0` — NUL-delimited sibling of `--filenames-only` for
      `xargs -0` / newline-in-filename safety. Shares the
      `project_filenames_only` projection; clap-conflicts with `--filenames-only`
- [x] `--iteration <ID>` on `read` and `task` subcommands — reuse
      `commands::iteration::resolve_iteration_globs`; same exactly-one-match
      error as `set --iteration` (read/task are single-file)
- [x] ~~`properties` output envelope unification with the find result shape~~
      — **skipped after dogfood triage** (moved to Out of scope, see below)
- [x] ~~`--property 'title~='` normalization for non-iteration types~~
      (finding #2 remainder) — **skipped after dogfood triage** (moved to
      Out of scope, see below)

## Acceptance criteria

- [x] For every task above that ships: e2e tests mirroring the
      `iteration_ergonomics.rs` patterns, help/command-reference updates
      (`check-help-drift`, `check-command-reference` green), CHANGELOG entry
- [x] Tasks intentionally skipped after dogfood triage are moved to
      Out of scope with a note naming the triage evidence

## Non-goals

- Touching the pi extension or templates (236/237 territory)
- Any change to `--format text`'s human layout

## Carry-over from [[iterations/iteration-237-pi-package-distribution]]

Post-merge verifications and deferred decisions from iter-237 (folded here so they are not silently forgotten). All five were **triaged and deferred** in iter-238: each requires mutating the owner's live global pi installation or a real follow-up update cycle, which an autonomous iteration run cannot safely do; see Out of scope for the notes.

- [ ] Verify the git-source install end-to-end once iter-237 is merged: `pi install git:github.com/ractive/hyalo` from a scratch checkout, confirm tool + skills register, then tick AC-1 of iter-237
- [ ] Verify `pi update --extensions` delivers a pushed change to that install (trivial marker change, e.g. package.json version bump), then tick AC-2 of iter-237
- [ ] Decide the tag-per-release pinning strategy after the first real update cycle (DEC-101 carry-over): tag naming, whether README recommends a tag ref over main HEAD
- [ ] (Conditional) `hyalo doctor`-style check reporting extension/hyalo version compatibility drift — only if dogfooding shows drift confusion
- [ ] (Conditional, from iter-236 via 237) `--jq` passthrough on `hyalo_find` and a `hyalo_lint` typed tool — only if observed model friction justifies

## Carry-over from [[iterations/iteration-234-lint-dead-output-cleanup]]

Deferred by that iteration's non-goals ("any further `results` shape renames"):

- [x] Inventory finding **D-5**: rename `summary`'s `schema.files_with_issues`
      to `files_with_violations` so it matches `lint`'s field name for the
      same quantity (`output.rs` already carries a compatibility shim reading
      both). Remediation R4, flagged "yes" in
      [[research/results-json-shape-inventory]]. Iteration 234 deleted the
      dead `LintOutput` half of J-9; this is the remaining *live* drift.

## Out of scope / carry-over candidates

- `properties` envelope unification with the find result shape (research
  finding #4) — skipped after dogfood triage 2026-08-25: running the built
  binary against this vault shows `hyalo properties --no-hints` already emits
  the exact find envelope `{hints, results, total}` (the envelope unification
  itself landed with iter-216's results-shape work), and the remaining
  per-item shape difference is recorded as justified divergence J-8 in
  [[research/results-json-shape-inventory]]. No single-file status-read
  friction was observed this session (`find --glob X --jq '.results[0].properties.status'`
  suffices).
- `--property 'title~='` normalization for non-iteration types (finding #2
  remainder) — skipped after dogfood triage: obsoleted for iterations by
  `--iteration`, and no friction on other types was observed during this
  session. Revisit only if a concrete case appears.
- Iter-237 carry-overs (git-source install verification, `pi update`
  delivery check, DEC-101 tag-per-release decision, conditional `hyalo
  doctor` drift check, conditional `hyalo_find --jq` / `hyalo_lint` typed
  tools) — deferred: they need attended access to the owner's live `~/.
p i` installation and, for AC-2/DEC-101, at least one real update cycle,
  neither of which exists yet. The conditional items remain not-triggered
  (no drift confusion or model friction observed while dogfooding 238).
- `--iteration` on `links` (no consumer friction observed yet)
- npm registry publishing of pi-package (git source sufficient until asked)
