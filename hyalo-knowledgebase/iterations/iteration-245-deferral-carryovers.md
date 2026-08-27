---
title: "Iteration 245 — deferral carry-overs from iter-244 review"
type: iteration
date: 2026-09-14
tags:
  - iteration
  - carry-over
status: planned
branch: iter-245/deferral-carryovers
---

# Iteration 245 — deferral carry-overs from iter-244 review

## Goal

Keep the small items carried out of [[iterations/iteration-244-index-remaining-deferrals]]
(iteration itself completed; items below were flagged during its review or
left as explicit non-goal owner decisions) from being silently forgotten.

## Context

- iter-244 closed the last dogfood findings (BUG-1..5, UX-1..6 all fixed,
  closed, or moot). Its review found one parity gap in the new UX-6 flag
  (fixed in the same PR, commit f80bfa1) and surfaced one UX-3 limitation
  that was deliberately left out of scope.
- The v0.21.0 release decision is still parked with the owner (DEC-101).

## Tasks

- [ ] UX-3 follow-up — extend dot-path property-filter traversal to nested
      arrays of maps (e.g. `contacts` as a list of `{name, email}` maps):
      either auto-descent ("any element matches") or an indexed segment
      form; decide, implement, and add unit + e2e tests alongside the
      existing object traversal in `resolve_prop`
- [ ] DEC-101 / release — cut v0.21.0 (BUG-4 parity + UX-3/UX-6 warrant a
      minor bump) or record the owner's explicit decision to stay on
      0.20.x; owner call, do not tag without it

## Acceptance criteria

- [ ] `find --property '<path>.<key>=v'` handles frontmatter where the
      intermediate segment is an array of maps (documented behaviour +
      tests), or the limitation is documented as a known constraint with a
      workaround hint
- [ ] The release question is either executed or explicitly closed with an
      owner-recorded decision

## Non-goals

- Concurrency guarantees — permanent non-goal per owner verdict
  2026-08-27 (single-writer atomicity only); listed here so it is visible,
  not to be worked on

## Links

- [[iterations/iteration-244-index-remaining-deferrals]]
- [[dogfood-results/dogfood-v0200-arch-refactors-and-agent-cli-followups]]
