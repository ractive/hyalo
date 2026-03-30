---
title: Suggest creating an index for large vaults
type: backlog
date: 2026-03-30
origin: dogfood MDN runs 2026-03-30
priority: medium
status: planned
tags:
  - ux
  - performance
  - index
---

## Problem

For large vaults (>500 files), property-only queries are 10-15x slower without an index
(~1.5s vs ~80ms on 14K files). Hyalo doesn't suggest creating one — users must discover
this themselves.

## Proposal

Two complementary approaches:

### 1. Hint after slow queries
When a non-indexed query takes >500ms, emit a hint:
`"This query took 1.4s. Create an index for faster queries: hyalo create-index"`

### 2. Warning in summary for large vaults
When `hyalo summary` reports >500 files and no index is active, include a hint suggesting
`hyalo create-index`.

### 3. Auto-index config in .hyalo.toml (future)
Add `auto_index = true` so vaults can opt in to automatic index creation.

## Acceptance criteria

- [ ] Slow query hint emitted when elapsed >500ms and no `--index`
- [ ] `summary` hints include index suggestion for vaults >500 files
- [ ] Hint is suppressed when `--index` is already in use
- [ ] `--quiet` suppresses the slow-query hint
