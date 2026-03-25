---
title: "Off-by-one between summary and properties summary counts"
type: backlog
date: 2026-03-25
origin: dogfooding v0.3.1 against docs/content and vscode-docs/docs
priority: low
status: planned
tags: [backlog, bug, summary, properties]
---

# Off-by-one between summary and properties summary counts

## Problem

`summary` and `properties summary` report different counts for the same data:

- docs/content: `summary` reports `title` in 3514 files, `properties summary` says 3515
- vscode-docs: `summary` reports 13 unique properties, `properties summary` lists 12

These commands should agree. Possibly a counting edge case with malformed files, case-sensitivity (`keywords` vs `Keywords`), or different code paths.

## Acceptance criteria

- [ ] `summary` and `properties summary` agree on property counts
- [ ] Root cause identified and documented in commit message
