---
title: Regex case sensitivity flags (?i)/(?-i) have no effect
type: backlog
date: 2026-03-25
origin: dogfooding v0.3.1 against vscode-docs/docs
priority: low
status: completed
tags:
  - backlog
  - bug
  - search
  - regex
---

# Regex case sensitivity flags (?i)/(?-i) have no effect

## Problem

The help text says search is "case-insensitive by default; use `(?-i)` to override", but the flags appear inert. `find -e 'typescript'`, `find -e 'TypeScript'`, and `find -e '(?i)typescript'` all return the same results. There's no way to toggle case sensitivity.

## Repro

```bash
hyalo find --dir /path/to/vscode-docs/docs -e 'typescript'      # 51 files
hyalo find --dir /path/to/vscode-docs/docs -e 'TypeScript'      # 51 files
hyalo find --dir /path/to/vscode-docs/docs -e '(?-i)typescript' # 51 files (should differ)
```

## Proposed fix

Check whether the regex is compiled with case-insensitive mode and whether `(?-i)` inline flags are passed through to the regex engine.

## Acceptance criteria

- [x] `(?-i)` makes the search case-sensitive
- [x] Default behavior (case-insensitive) is unchanged
- [x] E2e test: case-sensitive search returns fewer results than case-insensitive
