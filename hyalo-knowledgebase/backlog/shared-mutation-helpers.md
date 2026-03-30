---
title: "Extract shared mutation loop for set/remove/append"
type: backlog
date: 2026-03-29
status: planned
origin: codebase review 2026-03-29
priority: low
tags:
  - refactor
  - structure
---

## Problem

`set.rs` (955), `remove.rs` (961), and `append.rs` (761) share a near-identical mutation loop pattern: collect files, iterate, parse frontmatter, apply change, write back, update snapshot index. The boilerplate is duplicated three times.

## Fix

Extract a `commands/mutation.rs` helper that owns the loop and accepts a closure or trait impl for the specific mutation logic.

## Acceptance criteria

- [ ] Shared mutation helper exists
- [ ] set/remove/append use it instead of duplicating the loop
- [ ] All existing tests pass
