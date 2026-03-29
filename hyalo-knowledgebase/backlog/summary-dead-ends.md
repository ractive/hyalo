---
title: Add dead-end detection to summary command
type: backlog
status: completed
date: 2026-03-29
origin: dogfood v0.6.0 tidy round
priority: medium
tags:
  - summary
  - links
  - ux
---

## Context

The `summary` command reports **orphans** (files with no inbound AND no outbound links — fully
isolated, matching Obsidian/Foam's graph-view definition). This is correct and should stay.

Missing: **dead-ends** — files that have inbound links but no outbound links. These are
navigation dead-ends: a user can arrive there but has nowhere to go next.

Wikipedia defines these as ["dead-end pages"](https://en.wikipedia.org/wiki/Wikipedia:Dead-end_pages).

## Proposal

Add a `dead_ends` section to `summary` output alongside `orphans`:

```json
{
  "orphans": { "total": 25, "files": [...] },
  "dead_ends": { "total": 42, "files": [...] }
}
```

Text format:

```
Orphans (isolated, no links in/out):  25 files
Dead-ends (no outbound links):        42 files
```

## Design consideration

Many dead-ends are not actionable — e.g. top-level files in root or well-known directories
like `/iterations/` that users navigate to by browsing, not by following links. Consider:

- Not flagging files in root or first-level directories by default
- A `--strict` flag to include all dead-ends
- Or just reporting the count and letting `find --fields links --jq ...` drill down
