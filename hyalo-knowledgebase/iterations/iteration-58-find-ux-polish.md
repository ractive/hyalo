---
title: "Iteration 58 — Find command UX polish"
type: iteration
date: 2026-03-28
status: planned
branch: iter-58/find-ux-polish
tags:
  - ux
  - find
  - cli
---

# Iteration 58 — Find command UX polish

Small, independent improvements to the `find` command that improve discoverability and ergonomics. All are additive (no breaking changes) and can be done in parallel.

## Tasks

- [ ] Support `--fields all` keyword ([[backlog/fields-all-keyword.md]])
- [ ] Support `--fields title` shorthand ([[backlog/fields-title-shorthand.md]])
- [ ] Add `--filter` as hidden alias for `--property` ([[backlog/filter-typo-suggestion.md]])
- [ ] Add `--quiet` / `-q` flag to suppress warnings ([[backlog/quiet-flag-warning-dedup.md]])
- [ ] Deduplicate identical warnings within a single invocation (show summary count)
- [ ] Fix `--hints` flag silently accepted but no-op for find/properties/tags ([[backlog/hints-flag-no-op-for-most-commands.md]])
