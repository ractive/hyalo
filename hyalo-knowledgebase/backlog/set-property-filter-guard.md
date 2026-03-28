---
title: "Guard against filter syntax in set/remove/append --property"
type: backlog
date: 2026-03-28
tags:
  - backlog
  - ux
  - safety
priority: high
status: planned
origin: dogfooding legalize-es
---

# Guard against filter syntax in set/remove/append --property

## Problem

`hyalo set --property 'fecha_publicacion<=1900-01-01' --glob '*.md'` silently creates a literal property key `fecha_publicacion<` with value `1900-01-01` on ALL matched files. The user intended this as a filter (should be `--where-property`), but `set --property` parsed `<=` as a key-value separator.

This caused accidental writes to 8,642 files in the legalize-es dogfooding session.

## Proposal

In `set --property`, `remove --property`, and `append --property`, reject property names that contain comparison operator suffixes (`<`, `>`, `!`, `~`). Emit a clear error suggesting `--where-property` instead.

Example:

```
$ hyalo set --property 'fecha<=1900' --glob '*.md'
error: '--property' in 'set' is for mutation, not filtering.
       'fecha<=1900' looks like a filter — did you mean --where-property?
```

## Acceptance criteria

- [ ] `hyalo set --property 'key<=value'` errors with a helpful message pointing to `--where-property`
- [ ] Same for `>=`, `!=`, `~=`, `<`, `>` operators
- [ ] Bare existence check (`--property key`) and plain assignment (`--property key=value`) continue to work
- [ ] `remove` and `append` get the same guard
- [ ] Add e2e tests for rejected and accepted patterns
