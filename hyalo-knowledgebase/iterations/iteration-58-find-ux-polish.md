---
title: Iteration 58 — Find command UX polish
type: iteration
date: 2026-03-28
status: completed
branch: iter-58/find-ux-polish
tags:
  - ux
  - find
  - cli
---

# Iteration 58 — Find command UX polish

Small, independent improvements to the `find` command that improve discoverability and ergonomics. All are additive (no breaking changes) and can be done in parallel.

## Tasks

- [x] Support `--fields all` keyword ([[backlog/fields-all-keyword.md]])
- [x] Support `--fields title` shorthand ([[backlog/fields-title-shorthand.md]])
- [x] Improve clap suggestion when user types `--filter` to suggest `--property` instead of `--file` ([[backlog/filter-typo-suggestion.md]])
- [x] Add `--quiet` / `-q` flag to suppress warnings ([[backlog/quiet-flag-warning-dedup.md]])
- [x] Deduplicate identical warnings within a single invocation (show summary count)
- [x] Fix `--hints` flag silently accepted but no-op for find/properties/tags ([[backlog/hints-flag-no-op-for-most-commands.md]])
