---
title: Show total match count when --limit truncates results
type: backlog
status: completed
date: 2026-03-29
origin: dogfood v0.6.0 tidy round
priority: medium
tags:
  - find
  - ux
---

## Problem

When using `--limit N`, the output is silently truncated. There's no way to know whether
there were N+1 or N+10,000 matches without running a second query with `--jq 'length'`.

This came up repeatedly during dogfooding: agents ran the same query twice — once with
`--limit 5` for examples, once with `--jq 'length'` for the count.

## Proposal

When `--limit` is active, always report the total number of matches:

**Text mode:** append a line like `showing 5 of 342 matches`

**JSON mode:** wrap output in an envelope:
```json
{"total": 342, "results": [...5 items...]}
```

When `--limit` is not used, output stays unchanged (flat array / plain list).
