---
date: 2026-03-23
origin: dogfooding docs/content vault jq queries
priority: medium
status: completed
tags:
- backlog
- cli
- output
- llm
- ux
title: Properties as map (not array) in JSON output
type: backlog
---

# Properties as map (not array) in JSON output

## Problem (resolved)

Properties were emitted as an array of `{name, type, value}` objects, making `--jq` queries verbose (`select(.name == "status") | .value`).

## Solution (iter-35)

Properties in `find` output are now a `{"key": value}` map:

```json
"properties": {"status": "completed", "title": "My Title"}
```

Direct access: `.properties.status` — dramatically simpler jq one-liners.

Type information is available via `--fields properties-typed`, which returns the old `[{name, type, value}]` array format.

## Acceptance criteria

- [x] Properties output as a map by default
- [x] jq queries like `.properties.status` work
- [x] Type information is still accessible via `--fields properties-typed`
- [x] Existing scripts/queries get a migration path