---
date: 2026-03-23
origin: dogfooding vscode-docs vault
priority: medium
status: completed
tags:
- backlog
- cli
- frontmatter
- ux
title: 'set command: support creating list-type properties'
type: backlog
---

# set command: support creating list-type properties

## Problem

`hyalo set --property 'Keywords=[copilot,ai]'` creates a text property with the literal string `"[copilot,ai]"`, not a YAML list. There is no syntax to create a list via `set`.

The workaround is to use `append` repeatedly (one call per item), which works but is verbose for initializing a list from scratch.

## Proposal

Support a list literal syntax in `set`, e.g.:
- `--property 'Keywords=[copilot, ai, agents]'` — creates a YAML list
- Or auto-detect: if the value looks like `[a, b, c]`, parse as list

## Acceptance criteria

- [ ] `set --property 'K=[a, b, c]'` creates a YAML list property
- [ ] Existing text values with brackets are not accidentally converted
- [ ] Help text documents the list syntax
