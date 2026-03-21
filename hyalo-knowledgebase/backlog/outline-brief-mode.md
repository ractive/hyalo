---
title: "Outline --brief flag to omit property values"
type: backlog
date: 2026-03-21
status: idea
priority: low
origin: dogfooding iteration-06
tags:
  - backlog
  - outline
  - cli
---

# Outline --brief flag to omit property values

## Problem

The `outline` command includes full property values (entire tag lists, long title strings, date values). For LLM navigation — deciding *where* to look — knowing that a file has a `title` (text) and `tags` (list) property is enough. The actual values add noise when scanning many files.

## Proposal

Add `--brief` flag to `outline` that:
- Shows property names and types only (no values)
- Omits empty sections (no links, no tasks, no code blocks)
- Omits empty `code_blocks` and `links` arrays

```json
{
  "file": "iterations/iteration-06-outline.md",
  "properties": [
    { "name": "title", "type": "text" },
    { "name": "status", "type": "text" },
    { "name": "tags", "type": "list" }
  ],
  "tags": ["iteration", "outline"],
  "sections": [
    { "level": 2, "heading": "Tasks", "line": 104 },
    { "level": 3, "heading": "Core implementation", "line": 106, "tasks": { "total": 5, "done": 5 } }
  ]
}
```

## Notes

Low priority — the current full output works fine and isn't excessively large for typical vaults. Revisit if vault size grows or token budgets become a concern.
