---
title: "Bulk tag rename command"
type: backlog
date: 2026-03-23
status: planned
priority: low
origin: dogfooding knowledgebase housekeeping
tags:
  - backlog
  - cli
  - tags
  - ux
---

# Bulk tag rename command

## Problem

Renaming a tag (e.g. `filtering` → `filters`) requires two commands per file:

```bash
hyalo remove --tag filtering --file <file>
hyalo set --tag filters --file <file>
```

For a tag used across many files, this is tedious and error-prone.

## Proposal

```bash
hyalo rename-tag --from filtering --to filters
```

Atomically removes the old tag and adds the new one across all files that have it. Could also be a subcommand of `set`: `hyalo set --rename-tag filtering=filters`.

## Acceptance criteria

- [ ] Single command renames a tag across all files
- [ ] Atomic: if the new tag already exists on a file, only the old one is removed
- [ ] Reports how many files were modified
- [ ] Works with `--where-property` / `--where-tag` for scoped renames

See [[rename-property-command]]