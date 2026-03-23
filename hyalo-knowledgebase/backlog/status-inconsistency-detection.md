---
title: "Status value inconsistency detection"
type: backlog
date: 2026-03-23
status: planned
priority: low
origin: dogfooding post-iter-19
tags:
  - backlog
  - cli
  - ux
  - data-quality
---

# Status value inconsistency detection

## Problem

During knowledgebase cleanup, one file had `status: done` while all others used `status: completed`. This inconsistency was only discovered by manually inspecting the `summary` output and noticing the oddball. Hyalo could detect and warn about likely typos or inconsistencies in property values.

## Proposal

A lint or validation mode that flags property values appearing only once (or very rarely) when most files use a different value for the same property:

```sh
hyalo lint
# Warning: property "status" value "done" appears in 1 file — did you mean "completed" (26 files)?
```

Alternatively, this could be a flag on `summary` or `properties`:

```sh
hyalo properties --warn-rare
```

## Notes

- Useful for any controlled-vocabulary property (status, type, priority)
- Could use a threshold: warn if a value appears in <2% of files for that property
- Could also detect near-duplicates via string similarity (e.g., "planed" vs "planned")
- Low priority — the current workflow of `find --property status=X` per status value works, just requires manual attention
