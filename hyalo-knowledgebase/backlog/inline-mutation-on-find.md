---
title: "Inline mutation on find results (find-and-set)"
type: backlog
date: 2026-03-23
status: planned
priority: low
origin: dogfooding post-iter-19
tags:
  - backlog
  - cli
  - ux
---

# Inline mutation on find results (find-and-set)

## Problem

Updating a property across files that match a query requires a two-step pipeline:

```sh
hyalo find --property status=done --jq '.[].file' \
  | xargs -I{} hyalo set --property status=completed --file {}
```

This works but is verbose and requires shell plumbing. The pattern "find files matching X, then set Y on all of them" is common enough to deserve a shorthand.

## Proposal

Option A — `--set` flag on `find`:

```sh
hyalo find --property status=done --set status=completed
```

Option B — piped input on `set`:

```sh
hyalo find --property status=done --jq '.[].file' | hyalo set --property status=completed --stdin
```

Option C — just document the xargs pattern prominently in `--help` and the cookbook.

## Notes

- Option A is the most ergonomic but muddies the read-only nature of `find`
- Option B is more Unix-idiomatic but requires `--stdin` support
- Option C avoids new features but keeps the friction
- The xargs workaround is already shown in the help cookbook — this may be sufficient
