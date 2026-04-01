---
title: Consistent JSON envelope for all commands
type: backlog
date: 2026-03-30
origin: dogfood MDN runs 2026-03-30
priority: high
status: completed
tags:
  - ux
  - json
  - breaking-change
---

## Problem

Different commands return different JSON shapes, and `--hints` vs `--no-hints` changes the
top-level structure — breaking `--jq` expressions when switching between modes.

Current shapes:

| Command | With hints | Without hints |
|---------|-----------|---------------|
| find | `{data: {results, total}, hints}` | `{results, total}` |
| summary | `{data: {9 keys}, hints}` | `{9 keys}` |
| tags | `{data: {tags, total}, hints}` | `{tags, total}` |
| read | `{data: {content, file}, hints}` | `{content, file}` |
| properties | `[{count, name, type}]` (bare array, never wraps!) | identical |
| mutations | varies | varies |

Three issues:
1. `properties` never wraps — outlier
2. Hints changes the jq path (`.data.results[]` vs `.results[]`)
3. No consistent inner shape across commands

## Proposal

Use a common envelope for **all** JSON responses:

```json
{
  "results": <command-specific payload>,
  "total": <count, where applicable>,
  "hints": []
}
```

Key design decisions:
- `--jq` always operates on `.results` (not the full envelope)
- `hints` is always present (empty array when `--no-hints`) — shape never changes
- `--no-hints` suppresses hint **generation**, not the field itself
- `total` is present for list commands (find, tags, properties), omitted for others

## Acceptance criteria

- [x] All commands use the same envelope shape
- [x] `--jq` operates on the full envelope (e.g. `.results[].file`, `.total`)
- [x] `properties` command wraps in envelope (breaking change)
- [x] Shape is identical with and without `--hints`
- [x] `--help` documents the JSON envelope structure
- [x] `-h` short help mentions the envelope
- [x] Skill file updated with envelope documentation
