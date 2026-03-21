---
date: 2026-03-21
origin: dogfooding iteration-06
priority: medium
status: completed
tags:
- backlog
- cli
- llm
title: Vault dashboard — single-call project overview
type: backlog
---

# Vault dashboard — single-call project overview

## Problem

Starting a new session with a knowledgebase requires 3-4 separate commands to understand the current state: `properties summary`, `tags summary`, `outline` on key files, checking which iterations are in progress. An LLM agent's first action is always "orient myself" — this should be one call.

## Proposal

A `dashboard` or `status` command that returns a structured project overview in a single call:

```json
{
  "files": { "total": 15, "by_type": { "iteration": 7, "research": 4, "decisions": 1, ... } },
  "properties": { "unique": 6, "most_common": ["tags", "title", "date"] },
  "tags": { "unique": 24, "most_common": ["iteration", "properties", "cli"] },
  "status_overview": {
    "in-progress": ["iterations/iteration-06-outline.md"],
    "completed": ["iterations/iteration-01-frontmatter-properties.md", ...],
    "planned": ["iterations/iteration-07-search.md"]
  },
  "tasks": { "total": 87, "done": 72, "open": 15 },
  "recent_files": ["iterations/iteration-06-outline.md", "decision-log.md"]
}
```

## Design considerations

- Uses `status` property if present for the status overview (configurable property name?)
- `recent_files` based on git mtime or filesystem mtime
- Task counts aggregated vault-wide (requires full scan — may want to limit to files with `status: in-progress`)
- Text format: human-readable summary paragraph
- Could be built on top of existing commands internally (outline + properties + tags)

## Notes

This would be the "entry point" command for LLM agents. Currently the closest equivalent is `outline` on the whole vault, but that's too detailed — it gives document structure, not project state.

## My Comments
- Should hyalo propose "standard" properties and tags - define a convention that's used, when the user does not propose something else?
- All available types in types.rs (and everywhere else) that are used for other --format json output should be used inside the "summary"
- I don't think "dashboard" fits well here. What about "summary"?
  `hyalo --dir ./knowledgebase summary`? Other ideas?
- Thinking even further: Should the hyalo help propose conventions also how to structure the knowledgebase with directories like "iterations" etc.? Or is this too intrusive?