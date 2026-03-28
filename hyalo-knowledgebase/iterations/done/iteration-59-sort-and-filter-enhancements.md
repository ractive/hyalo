---
title: Iteration 59 — Sort & filter enhancements
type: iteration
date: 2026-03-28
status: completed
branch: iter-59/sort-filter
tags:
  - find
  - sorting
  - filtering
---

# Iteration 59 — Sort & filter enhancements

Extend the `find` command's sorting and filtering capabilities. These touch overlapping code paths (the sort/filter pipeline) and should be designed together.

## Tasks

- [x] Support `--sort` by frontmatter property value ([[backlog/done/sort-by-property-value]])
  - `--sort title` sorts by resolved title (frontmatter title → first H1)
  - `--sort date` as alias for `property:date`
  - `--sort property:KEY` for arbitrary property sorting
  - Files missing the sort property sort last; file-path tie-breaker
- [x] Add `backlinks_count` and `links_count` as sort fields ([[backlog/done/sort-by-backlinks-count]])
- [x] Support repeatable `--glob` flag for combining include/exclude patterns ([[backlog/done/repeatable-glob-flag]])
- [x] Content search should include lines inside code blocks ([[backlog/done/text-search-skips-code-blocks-undocumented]])
