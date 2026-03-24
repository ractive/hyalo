---
title: "Repeatable --file flag for targeting multiple specific files"
type: backlog
date: 2026-03-23
status: planned
priority: high
origin: dogfooding post-iter-19
tags:
  - backlog
  - cli
  - ux
---

# Repeatable --file flag for targeting multiple specific files

## Problem

`set`, `remove`, `append`, and `find` only accept a single `--file` flag. To update the same property on 3 specific files in different folders, you either need:

- A `--glob` pattern that happens to match exactly those files (often impossible)
- Three separate hyalo invocations
- A shell pipeline with `xargs`

This was hit during knowledgebase cleanup when trying to mark 3 Obsidian reference docs with `status: reference`.

## Proposal

Allow `--file` to be specified multiple times:

```sh
hyalo set --property status=reference \
  --file research/obsidian-cli-and-search.md \
  --file research/obsidian-markdown-compatibility.md \
  --file research/obsidian-properties.md
```

This should work on all commands that accept `--file`: `find`, `set`, `remove`, `append`.

## Notes

clap supports `num_args(1..)` or `action(ArgAction::Append)` for repeatable flags. The `--file` / `--glob` mutual exclusivity rule should still apply (can't mix `--file` and `--glob`), but multiple `--file` values should be allowed.

## My Comments
What commands will break when you can pass multiple --file flags?