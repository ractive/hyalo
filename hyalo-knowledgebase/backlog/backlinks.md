---
title: "Backlinks — which files link to this one?"
type: backlog
date: 2026-03-21
status: deferred
priority: medium
origin: dogfooding iteration-06
blocked-by: indexing
tags:
  - backlog
  - links
  - indexing
---

# Backlinks — which files link to this one?

## Problem

When navigating the knowledgebase, knowing *what links to this file* is as important as knowing *what this file links to*. During iteration 6 dogfooding, we looked at `decision-log.md` and wanted to know which iteration files reference specific decisions. Had to fall back to grep.

## Proposal

```sh
hyalo backlinks --file decision-log.md
```

Returns a list of files that contain `[[decision-log]]` or `[[decision-log#DEC-024]]` wikilinks targeting the given file.

## Constraints

Requires scanning all files in the vault for every call — O(n) full-file reads. Already deferred to the indexing iteration (see [[decision-log#DEC-013]]). Without an index, this is too slow for large vaults.

## Workaround

For now, an LLM agent can use external `grep` or the future `search` command with `content:[[decision-log]]`.
