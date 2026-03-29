---
title: "Reduce memory usage when find --limit + --sort file are combined"
type: backlog
status: planned
date: 2026-03-29
origin: PR #78 review — Copilot flagged memory cost of accumulating all results
priority: low
tags:
  - find
  - performance
---

## Problem

When `--limit N` is used, `find` accumulates all matching `FileObject`s in memory before
truncating to N. This is needed to compute the accurate `total` in the results envelope.
On large vaults (10k+ files), this causes an unnecessary memory spike when the caller only
wants a few results.

## Proposal

When `--sort file` is **explicitly** specified and `--limit` is active (and no `--reverse`),
stop pushing results after N matches but continue scanning to count the total:

```rust
total_matching += 1;
if results.len() < limit {
    results.push(obj);
}
// else: obj dropped immediately, only counter advances
```

This reduces memory from O(all matches) to O(limit) while still reporting an accurate total.

### Conditions for the optimization

- `--limit` is present
- `--sort file` is explicitly passed (not the implicit default — without explicit sort,
  filesystem traversal order is non-deterministic)
- `!reverse`
- `!fields.backlinks` (backlink sort needs all results)

### Why not the default sort?

Without `--sort`, file order depends on `readdir` which varies across OS/filesystem. The
first N files encountered would be arbitrary, giving different results each run.

## Scope

- Both `find()` and `find_from_index()` paths
- Add e2e test confirming deterministic results with `--sort file --limit N`
