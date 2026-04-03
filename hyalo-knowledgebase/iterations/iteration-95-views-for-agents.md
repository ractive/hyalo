---
title: "Iteration 95: Views for AI Agents"
type: iteration
date: 2026-04-03
status: in-progress
branch: iter-95/views-for-agents
tags:
  - iteration
  - views
  - llm
  - ux
---

## Goal

Make views a first-class part of the AI agent workflow. Three workstreams:
1. Hints suggest saving non-trivial find queries as views
2. Help text clearly communicates view composability
3. Skill templates teach agents to discover and create views

## Tasks

- [x] Add `view_name: Option<String>` to `HintContext` struct
- [x] Populate `view_name` in `run.rs` Find arm
- [x] Implement `suggest_save_as_view()` in `hints.rs`
- [x] Call the new hint function from `hints_for_find()`
- [x] Improve `--view` flag help text (composability)
- [x] Add VIEWS section to find command `long_about`
- [x] Improve views command and views set `long_about`
- [x] Add Views section to `skill-hyalo.md` template
- [x] Update `skill-hyalo-tidy.md` to use views in Phase 1 and Phase 3
- [x] Add/update tests for the new hint
- [x] Add e2e test for `--view` composability
- [x] Run quality gates: fmt, clippy, test
