---
title: "views list --format text outputs JSON instead of text"
type: backlog
date: 2026-04-03
origin: dogfood v0.8.0 session 2026-04-03
priority: low
status: planned
tags: [views, bug, format]
---

## Problem

`hyalo views list --format text` ignores the `--format text` flag and outputs JSON. All other commands (`find`, `tags summary`, `properties summary`, etc.) respect the text format flag.

## Expected behavior

Should output a human-readable text table or list, consistent with other commands' text format.

## Acceptance criteria

- [ ] `views list --format text` outputs a text representation (not JSON)
- [ ] `views list --format json` continues to work as before
- [ ] Default format respects `.hyalo.toml` setting
