---
title: "Split find.rs (2830 lines) into sub-modules"
type: backlog
date: 2026-03-29
status: planned
origin: codebase review 2026-03-29
priority: medium
tags:
  - refactor
  - structure
  - ai-friendliness
---

## Problem

`find.rs` combines filtering, sorting, field projection, index dispatch, section scoping, and `build_file_object` construction in one file. Any behavioral change to find must navigate 2830 lines.

## Proposed decomposition

- `commands/find/mod.rs` — public entry point `find()` (after iter-68 unification, only one variant)
- `commands/find/build.rs` — `build_file_object()` and projection logic
- `commands/find/sort.rs` — sort/limit logic
- `commands/find/filter_index.rs` — `filter_index_entries()` (shared by other commands)

## Acceptance criteria

- [ ] No single file in `commands/find/` exceeds 800 lines
- [ ] `filter_index_entries` is importable from a clean path
- [ ] All existing tests pass
