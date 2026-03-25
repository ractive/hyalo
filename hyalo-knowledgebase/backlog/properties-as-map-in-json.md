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

## Problem

Properties are emitted as an array of `{name, type, value}` objects:

```json
"properties": [
  {"name": "status", "type": "text", "value": "completed"},
  {"name": "title", "type": "text", "value": "My Title"}
]
```

This makes `--jq` queries verbose:
```
.properties[] | select(.name == "status") | .value
```

A map shape would be much simpler:
```json
"properties": {"status": "completed", "title": "My Title"}
```

Enabling: `.properties.status` — dramatically simpler jq one-liners.

## Trade-off

The array format preserves type information. A map loses it. Possible compromise:
- Default to map for `--jq` ergonomics
- Add `--properties-format array` flag for the rare case where type info is needed
- Or nest type info: `{"status": {"type": "text", "value": "completed"}}`

## Acceptance criteria

- [ ] Properties output as a map by default (or via flag)
- [ ] jq queries like `.properties.status` work
- [ ] Type information is still accessible when needed
- [ ] Existing scripts/queries get a migration path

## My Comments
What about all the other commands? Isn't it way harder to use a map there?