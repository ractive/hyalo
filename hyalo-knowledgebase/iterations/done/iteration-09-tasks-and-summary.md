---
branch: iter-9/tasks-and-summary
date: 2026-03-21
status: completed
tags:
- iteration
- tasks
- summary
- scanner
- performance
title: Iteration 9 — Task Commands + Summary + Unified Scanner
type: iteration
---

# Iteration 9 — Task Commands + Summary + Unified Scanner

## Goal

Three features plus an architectural improvement:

1. **Task commands** — `tasks` (list) and `task` (read/toggle/set-status) with `--file`, `--glob`, and vault-wide support
2. **Summary command** — single-call vault overview aggregating files, properties, tags, status groups, tasks, recent files
3. **Glob on bare `properties`/`tags`** — UX fix so `hyalo properties --glob` works
4. **Unified file scanner** — single-pass architecture so summary (and future commands) don't re-read files multiple times

## Completed

- [x] Task infrastructure (`src/tasks.rs`) — detect, extract, count, read, toggle, set-status
- [x] Task types (`TaskInfo`, `FileTasks`, `TaskReadResult`) in `src/types.rs`
- [x] Refactor `outline.rs` to use shared `tasks::detect_task_checkbox`
- [x] Task command handlers (`src/commands/tasks.rs`) — `tasks_list`, `task_read`, `task_toggle`, `task_set_status`
- [x] Wire `tasks`/`task` in `main.rs` with clap + dispatch
- [x] Text format filters for task types in `output.rs`
- [x] 36 e2e tests for task commands (happy + unhappy paths)
- [x] Quality gates pass: fmt, clippy, test

## Scanner & Summary

- [x] Design unified file scanner (single-pass multi-objective parsing)
- [x] Summary command (`src/commands/summary.rs`)
- [x] Summary e2e tests
- [x] Glob UX fix on bare `properties`/`tags`
- [x] Help text updates (COMMAND REFERENCE, COOKBOOK, OUTPUT SHAPES)
- [x] Update `tests/e2e_help.rs`
- [x] Update decision log (DEC-028, DEC-029, DEC-030)
- [x] Dogfooding

## Architecture: Multi-Visitor Scanner (resolved)

Chose **option 1: multi-visitor scanner** — see [[decision-log#DEC-028]].

`FileVisitor` trait with `on_frontmatter`, `on_body_line`, `on_code_fence_open`, `on_code_fence_close`. `scan_file_multi` drives multiple visitors in a single pass. All commands now open each file exactly once:

| Command | File opens per file |
|---------|:---:|
| properties summary/list | **1** |
| outline | **1** (was 2) |
| summary | **1** |
| tasks list | **1** |
