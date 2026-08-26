---
title: "Iteration 239 — pi install/update verification, DEC-101 tag strategy, conditional tooling"
type: iteration
date: 2026-08-26
status: planned
branch: iter-239/pi-install-verification
tags:
  - iteration
  - pi-package
  - verification
---

# Iteration 239 — pi install/update verification & conditional follow-ups

## Goal

Home the carry-over items triaged-and-deferred out of
[[iterations/iteration-238-agent-cli-followups]] so they are not silently
forgotten. Most require **attended** access to the owner's live global pi
installation (`~/.pi`) and, for the update-cycle items, at least one real
`pi update` cycle — so this plan cannot run autonomously end-to-end; treat
it as the attended-session checklist.

## Context

Iter-238 folded in the Rust-side carry-overs from iter-235 and closed the
triage-skipped tasks with recorded evidence. What remained were the
post-merge verifications from [[iterations/iteration-237-pi-package-distribution]]
and two conditionals whose triggers have not fired yet.

## Tasks

- [ ] Verify the git-source install end-to-end: `pi install git:github.com/ractive/hyalo`
      from a scratch checkout, confirm tool + skills register, then tick AC-1 of
      [[iterations/iteration-237-pi-package-distribution]]
- [ ] Verify `pi update --extensions` delivers a pushed change to that install
      (trivial marker change, e.g. package.json version bump), then tick AC-2 of
      [[iterations/iteration-237-pi-package-distribution]]
- [ ] Decide the tag-per-release pinning strategy after the first real update
      cycle (DEC-101 carry-over): tag naming, whether README recommends a tag
      ref over main HEAD

## Acceptance criteria

- [ ] Both post-merge verifications above are done against the real global pi
      installation and the corresponding iter-237 ACs are ticked
- [ ] DEC-101 has a recorded decision in [[decision-log]]

## Non-goals

- Any change to the Rust crates
- npm registry publishing of pi-package (git source sufficient until asked)
- Conditional tooling below stays unimplemented unless its trigger fires

## Conditionals (implement only if their trigger fires)

- [ ] (Conditional) `hyalo doctor`-style check reporting extension/hyalo
      version compatibility drift — only if dogfooding shows drift confusion
- [ ] (Conditional, from [[iterations/iteration-236-typed-pi-tools]] via 237)
      `--jq` passthrough on `hyalo_find` and a `hyalo_lint` typed tool — only
      if observed model friction justifies

## Out of scope / carry-over candidates

- `--property 'title~='` normalization for non-iteration types — revisit only
  if a concrete friction case appears on a non-`iteration` type
- `--iteration` addressing on `links` — no consumer friction observed yet
