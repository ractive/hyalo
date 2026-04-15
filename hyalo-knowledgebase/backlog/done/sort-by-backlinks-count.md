---
title: Add backlinks_count and links_count as sort fields
type: backlog
date: 2026-03-26
origin: dogfooding v0.4.1 on GitHub Docs and VS Code Docs
priority: medium
status: completed
tags:
  - find
  - links
  - sorting
---

`find --sort backlinks_count` errors with "unknown sort field". Only `file` and `modified` are valid. This makes it impossible to natively find the most-linked-to files — users must pipe through jq.

Add two new `SortField` variants:
- `backlinks_count` — sort by number of inbound links (requires backlinks computation)
- `links_count` — sort by number of outbound links

Naming convention: snake_case to match existing JSON field names (`total_links_updated`, `dry_run`).

When `--sort backlinks_count` is used, backlinks must be computed even if `--fields` doesn't include `backlinks`. This disables `--limit` short-circuit (same as when backlinks are explicitly requested).
