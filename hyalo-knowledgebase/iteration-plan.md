---
title: "Hyalo — High-Level Iteration Plan"
type: plan
date: 2026-03-20
status: active
tags:
  - plan
  - iterations
---

# High-Level Iteration Plan

## Iteration 1 — Frontmatter Parser & Property Commands

The foundation. Parse YAML frontmatter, infer types, implement property commands. After this iteration, hyalo will be used to manage its own knowledgebase (dogfooding).

**Commands:** `properties`, `property:read`, `property:set`, `property:remove`

## Iteration 2 — Wikilink Parser & Link Commands

Parse `[[wikilinks]]`, `![[embeds]]`, and `[markdown](links)`. Custom streaming scanner for line-by-line processing. File index with Obsidian shortest-path resolution.

**Commands:** `links`, `unresolved`

**Deferred to Indexing:** `backlinks`, `orphans`, `deadends` (require full vault scan per call)

## Iteration 3 — Tags & Tasks

Parse inline `#tags` (including nested) and task checkboxes with any status character.

**Commands:** `tags`, `tag`, `tasks`, `task` (with toggle/status)

## Iteration 4 — Search

Property query syntax, boolean logic, operators. Ties iterations 1–3 together into one query interface.

**Commands:** `search` with `[prop:value]`, `path:`, `file:`, `tag:`, `content:`, `task-todo:`, `task-done:`

## Iteration 5 — Move/Rename with Link Updates

Move or rename a file and update all wikilinks across the knowledge base.

**Commands:** `move`, `rename`

## Iteration 6 — Outline & Polish

Heading extraction, output formats (JSON, text, tree), CLI ergonomics.

**Commands:** `outline`

## Later — Indexing

SQLite or similar index for properties, tags, and links. Incremental updates based on file mtime. Triggered when file scanning becomes a bottleneck on large vaults.

**Deferred commands from iteration 2:** `backlinks`, `orphans`, `deadends` — these require full vault scans and benefit most from indexing.

**Deferred from iteration 2:** Obsidian shortest-path resolution (`[[foo]]` matching `sub/foo.md`). Currently link resolution requires explicit paths; shortest-path lookup can be added once an index exists.

## Dependencies

```
Iteration 1 (frontmatter) ──→ Iteration 4 (search needs parser)
Iteration 2 (link graph)  ──→ Iteration 5 (rename needs links)
Iterations 1–3             ──→ Iteration 4 (search unifies all)
```

## Dogfooding

Starting after iteration 1, hyalo manages its own `hyalo-knowledgebase/`.
