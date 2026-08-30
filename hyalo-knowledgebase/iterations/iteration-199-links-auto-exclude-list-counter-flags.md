---
type: iteration
title: Iteration 199 — counter-flags for links auto exclude-list config keys
date: 2026-08-18
status: superseded
tags:
  - iteration
  - links
  - auto-link
branch: iter-199/links-auto-exclude-list-counter-flags
related:
  - "[[iterations/iteration-198-links-auto-no-first-only-flag]]"
---

# Iteration 199 — counter-flags for links auto exclude-list config keys

## Goal

Carried over from [[decision-log#DEC-068]] (2026-08-18, filed by the iter-198
review/merge sweep). DEC-068 gave `links auto` a `--no-first-only`
counter-flag for the one boolean `[links.auto]` key, and closed with an
explicit "Not done":

> counter-flags for `exclude_titles` / `exclude_target_globs`. They are
> unioned lists, so "ignore the config's list for this run" is a different
> and larger question (partial vs. total override) with no demand behind it.

This plan exists so that "no demand behind it" is a tracked, revisitable
position rather than a sentence that gets lost in a decision log. It is filed
`status: deferred`, matching DEC-067/DEC-068's own precedent: `first_only`
sat deferred for two iterations before internal evidence (the
`warn_common_titles` asymmetry) tipped it to `proceed` with near-zero
implementation cost. The same could happen here, or the "partial vs. total
override" question below could turn out to have no clean answer at all —
either outcome is a legitimate re-evaluation result, not a foregone `proceed`.

## Context

- `exclude_titles` and `exclude_target_globs` are **unioned**:
  `AutoFilters::union_exclude_titles` / `union_exclude_target_globs` append
  the CLI-flag values to the config list. A run can only ever *add*
  exclusions relative to the config, never remove them.
- Unlike `first_only` (a bool with one bit to flip), a counter-flag here has
  to answer a harder design question before any code gets written: does
  "ignore the config's list for this run" mean
  - **(a) total override** — CLI values fully replace the config list for
    this run (simple, but a scripted `--exclude-target-glob` invocation that
    forgets this now silently drops the vault's shared exclusions instead of
    extending them — the opposite failure mode from `first_only`, where the
    surprise is a stricter run, not a looser one), or
  - **(b) partial override** — some way to un-exclude a *specific* config
    entry for one run (e.g. `--include-title T` / `--include-target-glob G`
    as a subtraction pass), which is a second flag surface per list, not one?
  DEC-068 did not pick between these; that choice is this plan's first task,
  not an implementation detail to improvise later.
- Both list keys already have a today-workaround the boolean key did not:
  `--file`/`--glob` scoping narrows *what is scanned*, which composes fine
  with "I don't want this file's candidates excluded" in a way it never did
  for `first_only` (scoping doesn't change *how* a scanned candidate gets
  linked).

## Tasks

- [ ] Re-evaluate demand: has any user hit "I need the config's
      `exclude_titles`/`exclude_target_globs` off for one run" since
      iter-195a shipped the config keys? If not, and the design question
      below still has no clean answer, close this `wont-do` with evidence
      rather than force a decision.
- [ ] If proceeding: decide (a) total override vs. (b) partial override
      per list, and whether the two lists must pick the same answer or can
      differ.
- [ ] If proceeding: unit tests for the chosen semantics in
      `AutoFilters` (mirroring the `effective_first_only` coverage style from
      iter-198).
- [ ] If proceeding: e2e coverage.
- [ ] If proceeding: docs — `links auto --help`, `docs/configuration.md`,
      `CHANGELOG.md`, `rule-knowledgebase.md`.

## Acceptance criteria

- [ ] TBD once the re-evaluation task above concludes proceed vs. wont-do,
      and — if proceeding — once the total-vs-partial override design
      question is settled.

## Non-goals

- Revisiting `first_only`'s already-shipped `--no-first-only` semantics
  (iter-198, DEC-068).
