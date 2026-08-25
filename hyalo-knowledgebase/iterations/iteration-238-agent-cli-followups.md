---
title: "Iteration 238 — agent-CLI ergonomics follow-ups: --filenames0, --iteration on read/task, properties envelope, title~= normalization"
type: iteration
date: 2026-08-25
status: planned
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

- [ ] `find --filenames0` — NUL-delimited sibling of `--filenames-only` for
      `xargs -0` / newline-in-filename safety. Shares the
      `project_filenames_only` projection; clap-conflicts with `--filenames-only`
- [ ] `--iteration <ID>` on `read` and `task` subcommands — reuse
      `commands::iteration::resolve_iteration_globs`; same exactly-one-match
      error as `set --iteration` (read/task are single-file)
- [ ] `properties` output envelope unification with the find result shape
      (finding #4 — see [[research/results-json-shape-inventory]] and
      [[iterations/iteration-216-results-shape-consistency]])
- [ ] `--property 'title~='` normalization for non-iteration types (finding #2
      remainder — obsoleted for iterations by `--iteration`; revisit only if
      it bites on other types)

## Acceptance criteria

- [ ] For every task above that ships: e2e tests mirroring the
      `iteration_ergonomics.rs` patterns, help/command-reference updates
      (`check-help-drift`, `check-command-reference` green), CHANGELOG entry
- [ ] Tasks intentionally skipped after dogfood triage are moved to
      Out of scope with a note naming the triage evidence

## Non-goals

- Touching the pi extension or templates (236/237 territory)
- Any change to `--format text`'s human layout

## Carry-over from [[iterations/iteration-237-pi-package-distribution]]

Post-merge verifications and deferred decisions from iter-237 (folded here so they are not silently forgotten):

- [ ] Verify the git-source install end-to-end once iter-237 is merged: `pi install git:github.com/ractive/hyalo` from a scratch checkout, confirm tool + skills register, then tick AC-1 of iter-237
- [ ] Verify `pi update --extensions` delivers a pushed change to that install (trivial marker change, e.g. package.json version bump), then tick AC-2 of iter-237
- [ ] Decide the tag-per-release pinning strategy after the first real update cycle (DEC-101 carry-over): tag naming, whether README recommends a tag ref over main HEAD
- [ ] (Conditional) `hyalo doctor`-style check reporting extension/hyalo version compatibility drift — only if dogfooding shows drift confusion
- [ ] (Conditional, from iter-236 via 237) `--jq` passthrough on `hyalo_find` and a `hyalo_lint` typed tool — only if observed model friction justifies

## Out of scope / carry-over candidates

- `--iteration` on `links` (no consumer friction observed yet)
- npm registry publishing of pi-package (git source sufficient until asked)
