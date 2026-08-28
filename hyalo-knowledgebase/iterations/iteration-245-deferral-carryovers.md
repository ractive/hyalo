---
title: "Iteration 245 — deferral carry-overs from iter-244 review"
type: iteration
date: 2026-09-14
tags:
  - iteration
  - carry-over
status: completed
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

- [x] UX-3 follow-up — extend dot-path property-filter traversal to nested
      arrays of maps (e.g. `contacts` as a list of `{name, email}` maps):
      either auto-descent ("any element matches") or an indexed segment
      form; decide, implement, and add unit + e2e tests alongside the
      existing object traversal in `resolve_prop`
- [x] DEC-101 / release — cut v0.21.0 (BUG-4 parity + UX-3/UX-6 warrant a
      minor bump) or record the owner's explicit decision to stay on
      0.20.x; owner call, do not tag without it

## Acceptance criteria

- [x] `find --property '<path>.<key>=v'` handles frontmatter where the
      intermediate segment is an array of maps (documented behaviour +
      tests), or the limitation is documented as a known constraint with a
      workaround hint
- [x] The release question is either executed or explicitly closed with an
      owner-recorded decision

## Non-goals

- Concurrency guarantees — permanent non-goal per owner verdict
  2026-08-27 (single-writer atomicity only); listed here so it is visible,
  not to be worked on

## Outcome

- **UX-3 follow-up — implemented, not documented away.** `resolve_prop` in
  `crates/hyalo-core/src/filter/match_props.rs` now walks sequences as well
  as mappings: a numeric segment indexes one element
  (`contacts.0.email`), any other segment auto-descends into every element
  and collects the hits into a sequence (`contacts.email` matches when any
  contact carries the value). Both forms the plan offered are supported and
  compose; see [[decision-log#DEC-243: dot-path property filters descend sequences by auto-descent, with numeric segments as an index (2026-08-28)]] for the precedence rule and why the
  hits are collected as a list. `resolve_prop` returns `Cow<'_, Value>` so
  only the sequence path allocates. `PropertyFilter::matches` now delegates
  its comparison to `matches_value`, removing the duplicated operator match.
- **Tests.** 12 new unit tests in `crates/hyalo-core/src/filter/mod.rs`
  (auto-descent, index segments, out-of-range index, scalar sequences,
  `!=`/regex/exists/absent, single-hit ordering ops, nested-sequence
  flattening, map-then-sequence composition, literal-dotted-key precedence)
  and 4 new e2e tests in
  `crates/hyalo-cli/tests/e2e/iteration245_followups.rs` covering the disk
  scan, the persisted index, and `set --where-property`.
- **Docs.** `find --help` FILTERS block, the `--property` short help, an
  example line, `README.md`, `CHANGELOG.md`, and both bundled skill
  documents (`crates/hyalo-cli/templates/skill-hyalo.md`,
  `pi-package/skills/hyalo/SKILL.md`) now describe dot-path traversal
  through sequences.
- **Release — recorded, not executed.** No tag was cut: releases are on
  hold by standing owner decision and the implementing loop was barred from
  tagging. The question is closed as a recorded decision,
  [[decision-log#DEC-244: v0.21.0 is deferred; the release stays parked pending an explicit owner decision (2026-08-28)]], which also lists what the `Unreleased` section
  has queued for whenever the release is unparked.

## Links

- [[iterations/iteration-244-index-remaining-deferrals]]
- [[dogfood-results/dogfood-v0200-arch-refactors-and-agent-cli-followups]]
