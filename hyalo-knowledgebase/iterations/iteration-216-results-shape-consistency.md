---
title: Iteration 216 — results JSON shape consistency
type: iteration
date: 2026-08-23
status: in-progress
branch: iter-216/results-shape-consistency
tags:
  - iteration
  - ux
  - json
related:
  - "[[iterations/iteration-213-config-ux-polish]]"
  - "[[iterations/iteration-215-anchor-and-broken-links-followups]]"
  - "[[research/results-json-shape-inventory]]"
---

# Iteration 216 — results JSON shape consistency

## Goal

Survey and, where it is cheap and non-breaking, unify how commands shape
their `results` JSON envelope — field naming conventions, when a key is
omitted vs. `null` vs. `0`, and whether counts/lists are namespaced
consistently across commands that report similar things (e.g. the
`config_excluded_titles` / `config_excluded_mentions` split introduced
in iter-213, `out_of_vault`, `files_failed` vs `files_skipped`).

## Context

Carried over from [[iterations/iteration-213-config-ux-polish]]'s
Non-goals: "Unifying `.results` JSON shapes across commands — needs its
own design pass." That iteration fixed one instance of a shape problem
(`config_excluded` renamed/split) but explicitly scoped out a
vault-wide survey. This is a design/research iteration first — do not
assume every inconsistency found is worth fixing; some divergence is
justified by genuinely different semantics per command.

Also carries the other half of dogfood UX-6, left unfiled by
[[iterations/iteration-215-anchor-and-broken-links-followups]]: that
iteration fixed the "no line numbers" half of UX-6 (`LinkInfo.line`,
DEC-100) but explicitly scoped out "`.results` JSON shape varying by
command" as the larger cross-cutting concern this iteration exists to
cover. The inventory task below should treat UX-6's shape-variance
observation as one input alongside iter-213's non-goal.

## Tasks

- [x] Inventory every command's `results` envelope shape (JSON key
      names, omitted-vs-null-vs-zero conventions, count/list pairing)
      across `find`, `links auto`, `lint`, `summary`, `properties`,
      `tags`, `config`, `set`/`remove`/`append`, `mv`.
- [x] Classify each inconsistency found: genuinely-different-semantics
      (leave alone, document why) vs. accidental drift (fix).
- [x] Record findings and the fix/leave decision per item as a DEC or a
      research note under `research/`.
- [x] Implement the fixes classified as accidental drift; each is a
      breaking JSON change for scripts reading that field, so document
      it in CHANGELOG under a clear "breaking" heading.
- [x] Update `templates/rule-knowledgebase.md` / `templates/skill-hyalo.md`
      and any e2e tests whose JSON assertions the fixes touch.

## Acceptance criteria

- [x] A written inventory (DEC or research note) exists covering the
      commands listed above
- [x] Every fix applied is justified by that inventory, not ad hoc
- [x] CHANGELOG documents each shape change as breaking, with the old
      and new field names
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean

## Non-goals

- Fields already addressed in iter-213 (`config_excluded_titles` /
  `config_excluded_mentions`) — done, not re-litigated here.
- A general schema/versioning mechanism for the JSON envelope — if the
  inventory surfaces a need for one, file it as its own iteration
  rather than building it inline here.
