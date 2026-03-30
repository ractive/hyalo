---
title: Use include_str!() for embedded templates in init.rs
type: backlog
date: 2026-03-29
status: completed
origin: codebase review 2026-03-29
priority: low
tags:
  - refactor
  - structure
---

## Problem

`init.rs` (1010 lines) contains large embedded static strings (Claude skill TOML template, CLAUDE.md content) as inline string literals. This makes the code hard to read and the templates hard to update.

## Fix

Move templates to files (e.g. `templates/claude-skill.toml`, `templates/CLAUDE.md`) and use `include_str!()` to embed them at compile time.

## Acceptance criteria

- [ ] Template content lives in separate files
- [ ] `include_str!()` used for compile-time embedding
- [ ] `init` command output is unchanged
- [ ] All existing tests pass
