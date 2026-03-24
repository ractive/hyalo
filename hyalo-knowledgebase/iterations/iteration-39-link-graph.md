---
title: "Iteration 39 — Link Graph & SQLite Index"
type: iteration
date: 2026-03-23
tags: [iteration, indexing, links, performance]
status: planned
branch: iter-39/link-graph
---

# Iteration 39 — Link Graph & SQLite Index

## Goal

Build a SQLite-backed index to enable vault-wide link operations (backlinks, move/rename with wikilink updates, shortest-path resolution). This is a larger effort that unblocks several deferred backlog items.

## Backlog items

- [[backlog/sqlite-indexing]] (high)
- [[backlog/backlinks]] (medium, blocked by indexing)
- [[backlog/move-rename-command]] (medium)
- [[backlog/shortest-path-link-resolution]] (medium, blocked by indexing)

## Tasks

### SQLite index
- [ ] Design index schema (files, properties, tags, links, sections)
- [ ] `hyalo index` command builds/rebuilds index from vault
- [ ] Incremental updates based on file mtime
- [ ] Index stored in `.hyalo/index.db` (gitignored)
- [ ] Fallback to full scan when index is stale or missing
- [ ] E2e tests cover index build and incremental update

### Backlinks
- [ ] `hyalo backlinks <file>` lists files that link to the given file
- [ ] Works with both `[[wikilink]]` and relative path links
- [ ] Uses index for performance
- [ ] E2e tests cover backlinks

### Move/rename command
- [ ] `hyalo move <old> <new>` renames file and updates all inbound wikilinks
- [ ] Updates links across the entire vault using index
- [ ] Dry-run mode (`--dry-run`) shows what would change
- [ ] E2e tests cover move with link updates

### Shortest-path link resolution
- [ ] Obsidian-style `[[filename]]` resolves to shortest unambiguous path
- [ ] Ambiguous links reported as warnings
- [ ] E2e tests cover resolution

## Acceptance Criteria

- [ ] Index builds in under 1s for 1000-file vaults
- [ ] Backlinks query returns results in under 100ms with index
- [ ] Move command correctly updates all inbound links
- [ ] All quality gates pass (fmt, clippy, tests)

## Notes

This is a larger iteration — consider splitting into 39a (index + backlinks) and 39b (move/rename + shortest-path) if scope is too large.
