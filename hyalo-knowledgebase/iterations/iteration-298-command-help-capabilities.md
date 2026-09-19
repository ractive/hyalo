---
type: iteration
title: Iteration 298 — Command help capabilities
date: 2026-09-20
status: in-progress
tags:
  - iteration
  - cli
  - help
branch: iter-298/command-help-capabilities
---

# Command help capabilities

## Problem

Shared input and global arguments appeared in help for commands that reject or
ignore them. In particular, `read`, `backlinks`, and `task read` advertised
`--glob` despite requiring one target. The audit covered 53 help pages.

## Behavior

A shared presentation pass narrows command help and invocation descriptors to
applicable selectors, global options, and output formats. Short-help pointers
use the same visibility rules. Parsing retains existing diagnostics for hidden
unsupported flags. `views set --files-from` now fails before writing because
path-list input cannot be persisted with a saved view.

Explicit single-file reads retain their behavior. This iteration does not add
batch reads, content fields, summaries, or output budgets.

## Tasks

- [x] Audit command help against selection and output capabilities.
- [x] Correct unsupported flags, format choices, and stale task/view help.
- [x] Add capability-contract and unsupported-selector regressions.
- [x] Regenerate affected TypeScript documentation.
- [x] Run formatting, strict workspace Clippy, and workspace tests.
- [x] Run the supported xtask gates and release-build dogfood.
- [ ] Reconcile independent local and Copilot reviews.

## Acceptance criteria

- [x] Single-target commands do not advertise glob selection.
- [x] Short and long help agree on applicable global flags.
- [x] Advertised count and index capabilities match runtime metadata.
- [x] Unsavable path-list input is refused before changing a saved view.
- [ ] Required checks pass and every review finding has a disposition.

## Validation

Formatting, strict workspace Clippy, and all 5,207 workspace tests passed
(two doctests ignored). All 12 supported xtask checks passed: feature fanout,
help drift, command reference, bundled skills, Pi package sync, TypeScript
types, Pi runtime, Codex package, jq recipes, behavioral contracts, mutation
journal, and typed output. The release build passed; dogfood covered all 53
help pages and unsupported-selector rejection before writes.

The legacy `check-dead-primitives` and `check-todo-annotations` commands were
run and explicitly reported unsupported placeholders; they are not counted
as passing gates. Independent local and Copilot reviews remain pending.
