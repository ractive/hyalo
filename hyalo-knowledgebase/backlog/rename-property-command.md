---
title: "Rename property command (bulk property key rename)"
type: backlog
date: 2026-03-23
status: planned
priority: low
origin: dogfooding vscode-docs vault (keywords vs Keywords casing)
tags:
  - backlog
  - cli
  - frontmatter
  - ux
---

# Rename property command (bulk property key rename)

## Problem

Renaming a property key (e.g. fixing `keywords` → `Keywords` casing) requires three steps per file:
1. Read the current value
2. Remove the old property
3. Set the new property with the old value

For list properties, `set` can't even recreate the value correctly (see set-list-property backlog item), requiring `append` calls per item instead.

## Proposal

```bash
hyalo rename-property --from keywords --to Keywords
```

Atomically renames the property key across all matching files, preserving the value and type.

## Acceptance criteria

- [ ] Single command renames a property key across files
- [ ] Preserves value and type (text, list, number, etc.)
- [ ] Works with `--where-property` / `--where-tag` for scoped renames
- [ ] Reports how many files were modified

## My Comments
What about
`hyalo properties rename--from foo --to bar`

The same for tags
`hyalo tags rename--from foo --to bar`
[[bulk-tag-rename]]