---
branch: iter-37/bulk-mutations
date: 2026-03-25
status: completed
tags:
- iteration
- mutations
- cli
- ux
title: Iteration 37 — Bulk Mutations & Multi-file Targeting
type: iteration
---

# Iteration 37 — Bulk Mutations & Multi-file Targeting

## Goal

Reduce the number of CLI calls needed for multi-file mutations. Currently Claude Code must call `hyalo set` once per file — this iteration enables batch operations and adds rename subcommands.

## Backlog items

- [[backlog/done/repeatable-file-flag]] (high)
- [[backlog/done/set-list-property]] (medium)
- [[backlog/done/bulk-tag-rename]] (low)
- [[backlog/done/rename-property-command]] (low)

## Design decisions

### Repeatable `--file` output contract

Currently `--file` returns a single object; `--glob` returns an array. With multiple `--file` values, the result is an array — consistent with `--glob`. Best-effort partial failure: process all files, report failures at the end rather than aborting on first error.

### `properties` and `tags` become subcommand groups

`hyalo properties` and `hyalo tags` gain explicit subcommands:

```
hyalo properties summary   # was: hyalo properties
hyalo properties rename --from old-key --to new-key

hyalo tags summary          # was: hyalo tags
hyalo tags rename --from old-tag --to new-tag
```

Bare `hyalo properties` / `hyalo tags` (no subcommand) shows help. No backward-compatibility shim needed.

### List property syntax

`hyalo set --property 'K=[a, b, c]'` — if the value starts with `[` and ends with `]`, it is parsed as a YAML sequence. This matches YAML inline sequence syntax and Obsidian's behavior.

## Tasks

### Repeatable --file flag
- [x] Change `--file` from `Option<String>` to `Vec<String>` with `action(ArgAction::Append)`
- [x] Update `collect_files()` to accept `&[String]` and resolve each file individually
- [x] Works on `find`, `set`, `remove`, `append` commands
- [x] Multi-file returns array result (consistent with `--glob`)
- [x] Best-effort: collect per-file errors, report failures at end
- [x] `--file` / `--glob` remain mutually exclusive
- [x] E2e tests cover multi-file targeting (success + partial failure)

### Set list-type properties
- [x] Detect `[...]` syntax in `infer_value()` and parse as YAML sequence
- [x] Items are trimmed but kept as strings (no type inference on list items)
- [x] Empty brackets `[]` creates an empty list
- [x] Values that don't match `^\[.*\]$` exactly remain strings
- [x] Help text documents the list syntax
- [x] E2e tests cover list property creation

### Subcommand structure for `tags` and `properties`
- [x] Convert `tags` to subcommand group with `summary` and `rename` subcommands
- [x] Convert `properties` to subcommand group with `summary` and `rename` subcommands
- [x] Bare `hyalo tags` / `hyalo properties` shows help
- [x] `--glob` moves to the subcommand level

### Bulk tag rename (`hyalo tags rename`)
- [x] `hyalo tags rename --from old-tag --to new-tag` renames across all files
- [x] Scoped by `--glob` if provided
- [x] Atomic per-file: if new tag already exists, only old one is removed
- [x] Reports modified count and lists affected files
- [x] E2e tests cover tag rename (basic, already-exists, scoped with --glob)

### Bulk property rename (`hyalo properties rename`)
- [x] `hyalo properties rename --from old-key --to new-key` renames across files
- [x] Preserves value and type (moves `Value` in the map)
- [x] Skips files where target key already exists (warn, report as conflict)
- [x] Reports modified count and lists affected files
- [x] Scoped by `--glob` if provided
- [x] E2e tests cover property rename (basic, conflict, scoped with --glob)

### Documentation & help text
- [x] Update `find`, `set`, `remove`, `append` help text for repeatable `--file`
- [x] Update `properties`, `tags` help text for new subcommands
- [x] Update README.md with new CLI examples
- [x] Update SKILL.md with new commands
- [x] Mark backlog items as completed
- [x] Add decision log entry for subcommand restructuring

## Acceptance Criteria

- [x] Can target multiple files in a single `set` / `remove` / `append` call
- [x] Can create list-type properties via CLI
- [x] `hyalo properties summary` and `hyalo tags summary` work as explicit subcommands
- [x] Bulk rename works for both tags and properties
- [x] All quality gates pass (fmt, clippy, tests)

## Deferred

- **Frontmatter reformatting (IndexMap swap)** — Replace `BTreeMap` with `IndexMap` to preserve key insertion order on write. See [[backlog/done/frontmatter-reformatting]] for full analysis. Deferred because it's a serialization internals change unrelated to the mutation features in this iteration.
