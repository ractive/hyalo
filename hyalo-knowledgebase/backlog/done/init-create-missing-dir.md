---
title: "init: create vault directory if it doesn't exist"
type: backlog
date: 2026-04-01
tags:
  - init
  - ux
status: completed
priority: low
---

## Problem

`hyalo init --dir foo` writes `dir = "foo"` to `.hyalo.toml` but does **not** create the `foo/` directory if it doesn't exist. Subsequent commands then fail because the configured directory is missing.

## Expected behaviour

If the user passes `--dir foo` and `foo/` doesn't exist, `hyalo init` should create it (similar to how it already creates `.claude/skills/` and `.claude/rules/` directories).

## Implementation notes

- Add `fs::create_dir_all(&dir_path)` after resolving the directory value in `run_init_in()`
- Only create when `--dir` is explicitly provided (auto-detected dirs already exist by definition)
- Print `created  foo/` in the summary output
