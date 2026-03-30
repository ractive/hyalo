---
title: "Code structure: split large files for AI-friendliness"
type: iteration
date: 2026-03-29
tags:
  - refactor
  - structure
  - ai-friendliness
status: completed
branch: iter-70/code-structure
---

## Goal

Split the largest files in the codebase into smaller, well-scoped modules so that AI agents (and humans) can find, understand, and modify code without reading thousands of lines. No behavioral changes — pure structural refactor.

## Tasks

### Split main.rs (1906 lines)

- [x] Extract CLI structs (`Cli`, `Commands`, sub-enums) into `cli/args.rs` — see [[backlog/split-main-rs]]
- [x] Extract help text constants and filtering into `cli/help.rs`
- [x] Move dispatch + config-merge logic into `run.rs` as `run()` entry point, re-exported from `lib.rs`
- [x] Shrink `main.rs` to 3 lines

### Split find.rs (2830 lines)

- [x] Extract `extract_title`, `TitleMatcher`, `matches_task_filter` into `commands/find/build.rs` — see [[backlog/done/split-find-rs]]
- [x] Extract sort/limit logic into `commands/find/sort.rs`
- [x] Extract `filter_index_entries()` and `needs_body()` into `commands/find/filter_index.rs`
- [x] Keep entry point in `commands/find/mod.rs`

### Extract shared mutation helpers

- [x] Extract shared index-update and save helpers from set/remove/append into `commands/mutation.rs` — see [[backlog/shared-mutation-helpers]]

### Misc structure

- [x] Move embedded templates in `init.rs` to `include_str!()` files — already done (all templates use `include_str!`)

### Quality gate

- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace` (498 tests pass)
