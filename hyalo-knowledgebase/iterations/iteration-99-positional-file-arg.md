---
title: "Positional file arguments for all commands"
type: iteration
date: 2026-04-07
tags:
  - iteration
  - cli
  - ergonomics
status: in-progress
branch: iter-99/positional-file-arg
---

# Positional File Arguments

Allow passing target files as positional arguments instead of requiring `--file` across all commands.

## Motivation

LLM agents (and humans) frequently pass the file as a bare positional arg (`hyalo read note.md`) instead of using the flag form (`hyalo read --file note.md`). This causes confusing errors. Supporting both forms makes the CLI more forgiving and ergonomic.

## Tasks

- [x] Add positional file support to single-file commands (read, backlinks, mv)
- [x] Add positional file support to task subcommands (read, toggle, set-status)
- [x] Add positional file support to multi-file commands (set, remove, append)
- [x] Add positional file support to find (after PATTERN)
- [x] Update hints to emit positional form
- [x] Update help text and long_about descriptions
- [x] Update skill templates and CLAUDE.md
- [x] Add e2e tests for positional form (15 tests)
- [x] Fix review issues (conflicts_with, error handling, hint context)
- [ ] Merge PR #110
