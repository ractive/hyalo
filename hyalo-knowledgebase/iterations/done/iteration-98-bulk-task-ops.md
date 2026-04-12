---
title: "Iteration 98: Bulk Task Operations"
type: iteration
date: 2026-04-06
tags:
  - iteration
  - cli
  - ux
  - tasks
status: completed
branch: iter-98/bulk-task-ops
---

Make `hyalo task toggle/set-status/read` ergonomic for bulk operations. Currently each
invocation targets a single `--line`, forcing agents into ugly shell loops. Add three
mutually exclusive task selectors: repeatable `--line`, `--section`, and `--all`.

## Motivation

LLM agents repeatedly struggle with the single-line-per-call constraint. They resort to
`for line in 23 24 25 ...; do hyalo task toggle --file f --line $line; done` which is
token-wasteful, error-prone, and unergonomic. The `--toggle` vs `toggle` subcommand
confusion (already mitigated by suggest.rs) compounds the problem.

## Design

Three mutually exclusive selectors (exactly one required):

| Selector | Example | Behavior |
|---|---|---|
| `--line` (repeatable) | `--line 23 --line 24 --line 25` | Target specific lines |
| `--section` | `--section "Acceptance criteria"` | All tasks under that heading |
| `--all` | `--all` | Every task in the file |

All three work with `read`, `toggle`, and `set-status`. `--file` is always required.

Core does a single read-modify-write pass for multi-line mutations (not N separate file rewrites).

## Tasks

- [x] Make `--line` repeatable (`Vec<usize>`) in CLI args for all three task subcommands
- [x] Add `--section` selector using SectionScanner logic to find tasks under a heading
- [x] Add `--all` selector to target every task in the file
- [x] Make selectors mutually exclusive with clear error on conflict
- [x] Update core `toggle_task`/`set_task_status` to accept multiple lines in single pass
- [x] Update `task_read` to return multiple tasks
- [x] Update help text for task subcommands to document new selectors
- [x] Update suggest.rs if needed for new flag patterns
- [x] Add e2e tests for repeatable `--line`
- [x] Add e2e tests for `--section`
- [x] Add e2e tests for `--all`
- [x] Run fmt, clippy, test quality gates
