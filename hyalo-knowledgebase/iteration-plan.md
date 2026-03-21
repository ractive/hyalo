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

Parse `[[wikilinks]]`, `![[embeds]]`, and `[markdown](links)`. Custom streaming scanner for line-by-line processing. Simple direct link resolution via filesystem probes.

**Commands:** `links` (with `--resolved`/`--unresolved` filter flags)

**Deferred to Indexing:** `backlinks`, `orphans`, `deadends` (require full vault scan per call)

## Iteration 3 — Tags & Tasks

Parse inline `#tags` (including nested) and task checkboxes with any status character.

**Commands:** `tags`, `tag`, `tasks`, `task` (with toggle/status)

## Iteration 4 — Property Find & List Operations

`property find` for searching files by frontmatter values. Generic list-property mutations (`add-to-list`, `remove-from-list`). Refactored `tag add/remove` to delegate to generic list ops.

**Commands:** `property find`, `property add-to-list`, `property remove-from-list`

## Iteration 5 — Summary + List Subcommand Refactor

Split `properties` and `tags` into `summary` (aggregate, default) and `list` (per-file detail) subcommands. Extracted shared helpers, fixed clippy pedantic warnings.

**Commands:** `properties summary|list`, `tags summary|list`

## Iteration 6 — Search

Property query syntax, boolean logic, operators. Ties iterations 1–5 together into one query interface.

**Commands:** `search` with `[prop:value]`, `path:`, `file:`, `tag:`, `content:`, `task-todo:`, `task-done:`

## Iteration 7 — Move/Rename with Link Updates

Move or rename a file and update all wikilinks across the knowledge base.

**Commands:** `move`, `rename`

## Iteration 8 — Outline & Polish

Heading extraction, output formats (JSON, text, tree), CLI ergonomics.

**Commands:** `outline`

## Later — Indexing

SQLite or similar index for properties, tags, and links. Incremental updates based on file mtime. Triggered when file scanning becomes a bottleneck on large vaults.

**Deferred commands from iteration 2:** `backlinks`, `orphans`, `deadends` — these require full vault scans and benefit most from indexing.

**Deferred from iteration 2:** Obsidian shortest-path resolution (`[[foo]]` matching `sub/foo.md`). Currently link resolution requires explicit paths; shortest-path lookup can be added once an index exists.

## Dependencies

```
Iteration 1 (frontmatter) ──→ Iteration 4 (find needs parser)
Iteration 2 (link graph)  ──→ Iteration 7 (rename needs links)
Iterations 1–5             ──→ Iteration 6 (search unifies all)
```

## Dogfooding

Starting after iteration 1, hyalo manages its own `hyalo-knowledgebase/`.
