---
date: 2026-03-21
origin: dogfooding iteration-06
priority: high
status: completed
tags:
- backlog
- search
- cli
title: Combined queries across properties, tags, and content
type: backlog
---

# Combined queries across properties, tags, and content

## Problem

Finding files that match multiple criteria requires separate CLI calls and manual intersection. Example: "files tagged `iteration` with `status: in-progress`" needs `tag find` + `property find` + scripted intersection. An LLM agent shouldn't need to orchestrate this.

## Proposal

This is what [[iterations/done/iteration-02-links#Iteration 7 — Search]] addresses. The `search` command should support:

```sh
hyalo search 'tag:iteration [status:in-progress]'
hyalo search '[type:iteration] task-todo:>0'
hyalo search 'path:research/* content:parallelization'
```

Boolean AND is implicit (space-separated). Operators on properties (`=`, `!=`, `>`, `<`, `contains`).

## Notes

Already scoped in the iteration plan. This backlog item exists to capture the specific friction point and use case from dogfooding, not to redefine the feature.
