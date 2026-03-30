---
title: write_snapshot clones entire entries Vec for serialization
type: backlog
date: 2026-03-29
status: completed
origin: codebase review 2026-03-29
priority: low
tags:
  - performance
  - index
---

## Problem

`index.rs:375` — `entries: index.entries().to_vec()` does a full deep clone of all `IndexEntry` structs (including all `IndexMap<String, Value>` property trees) just to serialize them via `SnapshotData`.

## Fix

Have `SnapshotData` hold `&[IndexEntry]` with a lifetime instead of `Vec<IndexEntry>`, so serialization borrows rather than clones.

## Acceptance criteria

- [ ] `SnapshotData` borrows entries instead of owning a cloned Vec
- [ ] `create-index` and snapshot write still work correctly
- [ ] All existing tests pass
