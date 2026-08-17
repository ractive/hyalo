---
type: iteration
title: Iteration 197 — stopword warning heuristic for links auto candidates
date: 2026-08-17
status: planned
tags:
  - iteration
  - links
  - auto-link
branch: iter-197/auto-link-stopword-heuristic
related:
  - "[[backlog/done/auto-link-config-exclusions]]"
  - "[[iterations/iteration-195a-auto-link-config-exclusions]]"
---

# Iteration 197 — stopword warning heuristic for links auto candidates

## Goal

Carried over from [[iterations/iteration-195a-auto-link-config-exclusions]]'s
non-goals: the "optional stretch" section of
[[backlog/done/auto-link-config-exclusions]] proposed warning when a `links
auto` candidate title is a very common English word (dictionary/stopword
heuristic), since the noise iter-195a's `[links.auto]` config now suppresses
is inherent to titles like "permissions" — a vault-specific exclusion list
still requires the user to notice the noise first. This iteration explores
whether a proactive warning is worth adding.

**This is a NEW plan filed by the iter-195a review/merge sweep on
2026-08-18, not yet scoped or committed to.** Before implementing, re-assess
whether it is still wanted — the original evidence is one external-user
report from 2026-07-04, and `[links.auto]` (iter-195a) already gives that
user a durable fix. Treat this plan as a starting point for that
reassessment, not a green light to build.

## Context

- Backlog stretch text (2026-07-04):  "warning when a candidate title is a
  very common English word (dictionary/stopword heuristic) suggesting it be
  excluded — the noise source here is inherent to titles like
  'permissions', not vault-specific."
- iter-195a shipped `[links.auto] exclude_titles` / `exclude_target_globs` /
  `first_only`, which lets a user silence the noise once they've seen it,
  but does nothing for a first-time `links auto` run before the user knows
  which titles are noisy.
- No stopword/dictionary dependency exists in the workspace today
  (`cargo tree` from the vault root before deciding on an approach) —
  CLAUDE.md requires "No polyglot tooling" and "New crates ... `hyalo-<domain>`"
  but says nothing against a small embedded static word list in Rust; a
  bundled ~200-word common-English list is likely cheaper and more
  predictable than a dependency.

## Tasks

- [ ] Re-evaluate demand: has any other user hit the same noise pattern
      since iter-195a shipped `[links.auto]`? If not, consider dispositioning
      this as `wont-do` with evidence rather than implementing speculatively.
- [ ] If proceeding: design the heuristic (bundled stopword list vs.
      length/frequency heuristic vs. something else) and where in the
      `links auto` dry-run report the warning surfaces.
- [ ] Decide default-on vs. opt-in (a new `--warn-common-titles` flag or a
      `[links.auto]` key) — must not change existing dry-run/`--apply`
      output shape for vaults that don't opt in, to avoid a breaking change.
- [ ] Unit + e2e coverage.
- [ ] Docs: `links auto --help`, `docs/configuration.md` if a new config key
      is added.

## Acceptance criteria

- [ ] TBD once the re-evaluation task above concludes proceed vs. wont-do.

## Non-goals

- Changing `[links.auto]`'s existing three keys (`exclude_titles`,
  `exclude_target_globs`, `first_only`) — this is additive only.
