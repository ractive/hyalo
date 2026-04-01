---
title: "Performance quick wins: clones, allocations, panics"
type: iteration
date: 2026-03-29
tags:
  - performance
  - bug
status: completed
branch: iter-69/perf-quick-wins
---

## Goal

Address the low-hanging performance issues and the one bug found in the codebase review. These are all small, independent fixes that don't require architectural changes.

## Tasks

### Bug fix

- [x] Fix `--status ""` panic — validate non-empty before `.chars().next().unwrap()` in `main.rs:1659`; add e2e test — see [[backlog/done/empty-status-panic]]

### Avoidable clones

- [x] Reorder `props.clone()` → move in 5 mutation commands (set, append, remove, properties rename, tags rename) — see [[backlog/done/avoidable-clones-in-mutations]]
- [x] `ContentSearchVisitor`: override `needs_frontmatter()` → `false` to skip YAML parse during content-only re-scan — see [[backlog/done/content-search-skip-yaml-parse]]

### Allocation reduction

- [x] `filter_index_entries`: convert `files_arg` to `HashSet<&str>` for O(1) lookup — see [[backlog/done/filter-index-entries-hashset]]
- [x] `write_snapshot`: have `SnapshotData` borrow `&[IndexEntry]` instead of cloning — see [[backlog/done/write-snapshot-clone]]
- [x] `tasks.rs:473,513`: rewrite task mutation to avoid N string allocations — see [[backlog/tasks-vec-string-allocation]]

### Quality gate

- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`
