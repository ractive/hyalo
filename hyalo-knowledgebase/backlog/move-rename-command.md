---
title: "Move/rename command with wikilink updates"
type: backlog
date: 2026-03-23
status: planned
priority: medium
origin: dogfooding post-iter-19, iteration-plan
tags:
  - backlog
  - cli
  - links
---

# Move/rename command with wikilink updates

## Problem

Moving a file (e.g., from `backlog/` to `backlog/done/`) requires a shell `mv` command. Any `[[wikilinks]]` pointing to the old path break silently. Hyalo has no way to move a file and update all inbound links.

This was hit during knowledgebase cleanup when moving `combined-queries.md` to `backlog/done/`.

## Proposal

```sh
hyalo mv --file backlog/combined-queries.md --to backlog/done/combined-queries.md
```

Behaviour:
1. Move the file on disk
2. Scan all `.md` files in `--dir` for wikilinks pointing to the old path
3. Rewrite those links to point to the new path
4. Report which files were updated

## Constraints

- Requires scanning all files (O(n) on vault size) — acceptable for small/medium vaults
- For large vaults, this would benefit from the SQLite index ([[backlog/sqlite-indexing]])
- Must handle both `[[path]]` and `[[path|alias]]` forms
- Should work with the current direct-path resolution model

## References

Originally planned as "Iteration 9 — Move/Rename with Link Updates" in [[iteration-plan]] but was never implemented. Now tracked as a backlog item.
