---
type: iteration
title: Iteration 209 — configurable thresholds for the links auto noisy-title note
date: 2026-08-22
status: superseded
tags:
  - iteration
  - links
  - auto-link
branch: iter-209/links-auto-configurable-noise-thresholds
related:
  - "[[iterations/iteration-205-common-title-frequency-trigger]]"
  - "[[iterations/iteration-199-links-auto-exclude-list-counter-flags]]"
---

# Iteration 209 — configurable thresholds for the links auto noisy-title note

## Goal

Carried over from iteration 205's Non-goals (DEC-205, 2026-08-22): "Per-vault
configurable thresholds (wait for demand — DEC it)." Iteration 205 hardcoded
the frequency trigger's constants — `FREQUENT_TITLE_MIN_MATCHES = 25` and
`FREQUENT_TITLE_SHARE_DIVISOR = 40` (2.5%) — tuned against three measured
corpora (own KB, a GitHub Docs slice, `vscode-docs`). No corpus in that
measurement sat on a knife edge, so there was no evidence a fixed constant
would misfire elsewhere. This plan exists so "wait for demand" is a tracked,
revisitable position rather than a sentence buried in a completed iteration's
Non-goals section — matching the precedent [[iterations/iteration-199-links-auto-exclude-list-counter-flags]]
set for exactly this situation.

## Context

- The two constants live in `crates/hyalo-cli/src/commands/links.rs`
  (`FREQUENT_TITLE_MIN_MATCHES`, `FREQUENT_TITLE_SHARE_DIVISOR`), feeding
  `frequent_title_threshold(total) = max(25, ceil(total / 40))`.
- `[links.auto] warn_common_titles` already exists as a single on/off switch
  governing both the wordlist and frequency triggers. A configurable
  threshold would need new key(s) — e.g. `frequent_title_min_matches` /
  `frequent_title_share` — under the same `[links.auto]` table, plus CLI
  flag equivalents if per-run overrides are wanted (mirroring the
  `exclude_titles` config-vs-flag precedent from iter-195a).
- DEC-205's measurement is the evidence base to re-check before touching
  this: own KB (195 links, threshold 25, flags `backlinks` at 67%), GitHub
  Docs slice (1,179 links, threshold 30), `vscode-docs` (33,859 links,
  threshold 847). If a future corpus needs a threshold outside what those
  three could justify, that is the demand signal this plan is waiting for.

## Tasks

- [ ] Re-evaluate demand: has any user or dogfood session hit a corpus where
      the fixed 25-match / 2.5%-share constants produce a wrong call (a true
      offender under the floor, or a false positive the share shouldn't have
      caught) since iter-205 shipped? If not, close this `wont-do` with the
      evidence rather than force a decision.
- [ ] If proceeding: decide the config surface — new `[links.auto]` keys
      only, or also CLI flags for a single-run override (precedent:
      `--min-length` is CLI-only with no config key, by iter-205's own
      design note; `exclude_titles` is both).
- [ ] If proceeding: decide whether the floor and the share get independent
      overrides or must move together.
- [ ] If proceeding: unit tests for the chosen semantics (mirroring the
      `frequent_title_threshold` coverage style from iter-205).
- [ ] If proceeding: e2e coverage plus docs — `links auto --help`,
      `docs/configuration.md`, `CHANGELOG.md`, `rule-knowledgebase.md`.

## Acceptance criteria

- [ ] TBD once the re-evaluation task above concludes proceed vs. wont-do,
      and — if proceeding — once the config-surface question is settled.

## Non-goals

- Revisiting the frequency trigger's already-shipped default values
  (iter-205, DEC-205) absent a concrete counter-example.
- Revisiting the wordlist trigger (`is_common_word`) or its own thresholds
  (title-length floor, plural stemming) — out of scope for this plan.
