---
title: "Iteration 20: --where-property and --where-tag filters on mutation commands"
type: iteration
date: 2026-03-23
status: completed
branch: iter-20/where-filters
tags:
  - iteration
  - cli
  - ux
  - llm
---

# Iteration 20: --where-property and --where-tag filters on mutation commands

## Goal

Enable single-command bulk mutations without shell pipelines. LLMs (the primary consumers) struggle with `find | xargs set` patterns — this iteration adds inline filter flags to `set`, `remove`, and `append` so an LLM can express "find files matching X, mutate Y" in one call.

## Changes

- [x] Extract `matches_frontmatter_filters()` into `src/filter.rs` as shared function
- [x] Refactor `find` command to use the shared filter function
- [x] Add `--where-property` and `--where-tag` CLI flags to `set`, `remove`, `append`
- [x] Wire filter parsing and validation in `main.rs` dispatch
- [x] Apply where-filters in per-file loop of all three mutation commands
- [x] Update `long_about` help text for all three mutation commands
- [x] Update `after_long_help` cookbook and command reference
- [x] Update `README.md` with examples
- [x] Add unit tests for `matches_frontmatter_filters()` in `filter.rs`
- [x] Add unit tests in `set.rs`, `remove.rs`, `append.rs`
- [x] Add e2e tests covering scalar match, list-element match, nested tag match, combined AND, no-match, operators

## Design decisions

- Flag names `--where-property` / `--where-tag` mirror `find`'s `--property` / `--tag` with `where-` prefix
- AND semantics across all where-filters (same as `find`)
- Full filter operator support: `=`, `!=`, `>`, `>=`, `<`, `<=`, existence
- List-element matching: `--where-property tags=cli` matches `tags: [cli, rust]`
- Nested tag matching: `--where-tag project` matches `project/backend`
- `--file`/`--glob` still required — where-filters narrow within the file set, not replace targeting
- Filtered-out files are simply skipped (not counted in modified or skipped)

## Backlog item

Resolves [[backlog/done/inline-mutation-on-find]]
