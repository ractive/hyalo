---
title: "Iteration 33 — Short Flag Aliases"
type: iteration
date: 2026-03-23
tags:
  - iteration
  - cli
  - ux
status: in-progress
branch: iter-33/short-flags
---

# Iteration 33 — Short Flag Aliases

## Goal

Add single-letter short aliases to frequently-used CLI flags for faster interactive use, while keeping long flags unchanged for scripts and readability.

## Design Principles

- **Consistent across commands**: `-f` always means `--file`, `-p` always means `--property`, `-t` always means `--tag`, `-g` always means `--glob`
- **No global `-f`**: reserved for `--file` in subcommands to avoid clap conflicts
- **Skip compound flags**: `--where-property` / `--where-tag` stay long-only
- **Skip infrequent flags**: `--fields`, `--sort`, `--frontmatter`, `--hints` don't need short forms

## Short Flag Reference

| Short | Long         | Available in                                      |
|-------|--------------|---------------------------------------------------|
| `-d`  | `--dir`      | all commands (global)                             |
| `-e`  | `--regexp`   | find (pre-existing)                               |
| `-p`  | `--property` | find, set, remove, append                         |
| `-t`  | `--tag`      | find, set, remove                                 |
| `-s`  | `--section`  | find, read                                        |
| `-f`  | `--file`     | find, read, set, remove, append, task *           |
| `-g`  | `--glob`     | find, set, remove, append, properties, tags, summary |
| `-n`  | `--limit`    | find                                              |
| `-n`  | `--recent`   | summary                                           |
| `-l`  | `--lines`    | read                                              |
| `-l`  | `--line`     | task read, task toggle, task set-status            |
| `-s`  | `--status`   | task set-status                                   |

## Tasks

- [x] Add short flags to all CLI arg definitions in main.rs
- [x] Update COMMAND REFERENCE in after_long_help
- [x] Update README.md with short flag table and examples
- [x] Add e2e tests for short flags (e2e_short_flags.rs)
- [x] All quality gates pass (fmt, clippy, tests)
- [x] Update knowledgebase iteration file
- [ ] Create PR

## Acceptance Criteria

- All short flags work identically to their long counterparts
- Long flags remain unchanged (backward compatible)
- `hyalo <cmd> --help` shows both short and long forms
- e2e tests cover every short flag
- README examples use short flags where natural
