---
title: filter_index_entries O(n×m) for files_arg — use HashSet
type: backlog
date: 2026-03-29
status: completed
origin: codebase review 2026-03-29
priority: low
tags:
  - performance
  - find
---

## Problem

`find.rs:461-466`:
```rust
entries.iter().filter(|e| files_arg.iter().any(|f| f == &e.rel_path))
```

This is O(n × m) where n = entries, m = files_arg length. For typical usage m=1, but `--file a.md --file b.md ... --file z.md` with a large vault degrades.

## Fix

Convert `files_arg` to a `HashSet<&str>` before the loop for O(n).

## Acceptance criteria

- [ ] `files_arg` converted to `HashSet` before filtering
- [ ] All existing tests pass
