---
title: Skip full-body index build for content-only search
type: backlog
date: 2026-03-30
status: planned
priority: high
origin: iterations/iteration-86-high-perf-scanning.md
tags:
  - backlog
  - performance
---

# Skip full-body index build for content-only search

## Problem

`hyalo find "pattern"` currently sets `scan_body: true` because `has_content_search` is true in `needs_body()`. This triggers the full 4-visitor scan (frontmatter, sections, tasks, links) on every file during index build — then content search re-reads each matching file again.

For a simple body search without section/task/link filters or output fields, the section/task/link visitors are wasted work.

## Proposal

Decouple `scan_body` from `has_content_search` in `dispatch.rs`. When only content search is requested (no section/task/link filters or fields), build the index with `scan_body: false` (frontmatter only), then run content search directly on files.

### Key files
- `crates/hyalo-cli/src/dispatch.rs:132-144` — where `needs_body` is computed
- `crates/hyalo-cli/src/commands/find/filter_index.rs:129-142` — `needs_body()` function
- `crates/hyalo-cli/src/commands/find/mod.rs` — content search loop

### Expected impact
Should roughly halve the time for `hyalo find "pattern"` by eliminating the redundant full-body index scan (~50% of current wall time is index build with 4 visitors).

## References
- [[iterations/iteration-86-high-perf-scanning]] — discovered during perf iteration
- [[research/performance-parallelization]] — I/O dominance confirmed
