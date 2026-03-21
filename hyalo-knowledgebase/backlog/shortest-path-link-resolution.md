---
title: "Obsidian-style shortest-path link resolution"
type: backlog
date: 2026-03-21
status: deferred
priority: medium
origin: DEC-014, iteration-plan
blocked-by: indexing
tags:
  - backlog
  - links
  - indexing
---

# Obsidian-style shortest-path link resolution

## Problem

Currently `[[foo]]` resolves only via direct filesystem probes: check `foo` then `foo.md` relative to vault root. Obsidian uses shortest-path resolution — `[[foo]]` can match `sub/deep/foo.md` if it's the only `foo.md` in the vault. This means hyalo marks links as unresolved that Obsidian considers valid.

## Proposal

Implement Obsidian's resolution algorithm:
1. Exact match at vault root
2. Shortest unique path match across all vault files
3. Case-insensitive matching

## Constraints

Requires knowing all files in the vault to find the shortest match — essentially an index. Without it, every link resolution would need a full directory walk. Deferred to the indexing iteration.

## References

- [[decision-log#DEC-014]]: current simple resolution, explicitly defers shortest-path
- [[iteration-plan#Later — Indexing]]: mentions this as a deferred feature
