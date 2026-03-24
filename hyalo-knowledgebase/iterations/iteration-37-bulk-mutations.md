---
title: "Iteration 37 — Bulk Mutations & Multi-file Targeting"
type: iteration
date: 2026-03-23
tags: [iteration, mutations, cli, ux]
status: planned
branch: iter-37/bulk-mutations
---

# Iteration 37 — Bulk Mutations & Multi-file Targeting

## Goal

Reduce the number of CLI calls needed for multi-file mutations. Currently Claude Code must call `hyalo set` once per file — this iteration enables batch operations.

## Backlog items

- [[backlog/repeatable-file-flag]] (high)
- [[backlog/set-list-property]] (medium)
- [[backlog/bulk-tag-rename]] (low)
- [[backlog/rename-property-command]] (low)
- [[backlog/frontmatter-reformatting]] (low)

## Tasks

### Repeatable --file flag
- [ ] `--file` accepts multiple values (`--file a.md --file b.md`)
- [ ] Works on `set`, `remove`, `append` commands
- [ ] Reports per-file results
- [ ] E2e tests cover multi-file targeting

### Set list-type properties
- [ ] `hyalo set --property 'K=[a, b, c]'` creates a YAML list property
- [ ] Existing text values with brackets are not accidentally converted
- [ ] Help text documents the list syntax
- [ ] E2e tests cover list property creation

### Bulk tag rename
- [ ] `hyalo tag rename old-tag new-tag` renames across all matching files
- [ ] Atomic: if new tag already exists on a file, only old one is removed
- [ ] Reports how many files were modified
- [ ] Works with `--where-property` / `--where-tag` for scoped renames
- [ ] E2e tests cover tag rename

### Rename property command
- [ ] `hyalo property rename old-key new-key` renames across files
- [ ] Preserves value and type
- [ ] Works with `--where-property` / `--where-tag` for scoped renames
- [ ] Reports how many files were modified
- [ ] E2e tests cover property rename

### Frontmatter reformatting
- [ ] Investigate preserving key order and list indentation on write
- [ ] If feasible, implement consistent formatting (or document limitation)

## Acceptance Criteria

- [ ] Can target multiple files in a single `set` / `remove` / `append` call
- [ ] Can create list-type properties via CLI
- [ ] Bulk rename works for both tags and properties
- [ ] All quality gates pass (fmt, clippy, tests)
