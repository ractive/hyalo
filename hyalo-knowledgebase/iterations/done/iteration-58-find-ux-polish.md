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

- [x] Support `--fields all` keyword ([[backlog/done/fields-all-keyword]])
- [x] Support `--fields title` shorthand ([[backlog/done/fields-title-shorthand]])
- [x] Improve clap suggestion when user types `--filter` to suggest `--property` instead of `--file` ([[backlog/done/filter-typo-suggestion]])
- [x] Add `--quiet` / `-q` flag to suppress warnings ([[backlog/done/quiet-flag-warning-dedup]])
- [x] Deduplicate identical warnings within a single invocation (show summary count)
- [x] Fix `--hints` flag silently accepted but no-op for find/properties/tags ([[backlog/done/hints-flag-no-op-for-most-commands]])
