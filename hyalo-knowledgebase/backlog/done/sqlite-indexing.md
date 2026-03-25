---
date: 2026-03-21
origin: iteration-plan, DEC-013
priority: high
status: wont-do
tags:
- backlog
- indexing
- performance
title: SQLite-backed index for vault-wide operations
type: backlog
---

# SQLite-backed index for vault-wide operations

## Problem

Several commands are deferred because they require scanning all vault files on every call — O(n) full-file reads per invocation. Without an index, these operations are too slow for large vaults.

## Proposal

SQLite or similar index for properties, tags, links, and file metadata. Incremental updates based on file mtime. Enables:

- `backlinks` — which files link to a given file
- `orphans` — files with no incoming links
- `deadends` — files with no outgoing links
- Vault-wide task search
- Shortest-path link resolution (see [[backlog/shortest-path-link-resolution]])

## Trigger

Implement when file scanning becomes a bottleneck on real vaults, or when backlinks/orphans become the next priority feature.

## References

- [[iteration-plan#Later — Indexing]]: scoped in the plan
- [[decision-log#DEC-013]]: defers backlinks/orphans/deadends to indexing
- [[decision-log#DEC-021]]: defers vault-wide task search to indexing
- [[decision-log#DEC-014]]: defers shortest-path resolution to indexing
