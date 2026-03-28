---
title: Iteration 59 — Sort & filter enhancements
type: iteration
date: 2026-03-28
status: in-progress
branch: iter-59/sort-filter
tags:
  - find
  - sorting
  - filtering
---

# Iteration 59 — Sort & filter enhancements

Extend the `find` command's sorting and filtering capabilities. These touch overlapping code paths (the sort/filter pipeline) and should be designed together.

## Tasks

- [x] Support `--sort` by frontmatter property value ([[backlog/sort-by-property-value.md]])
  - `--sort title` sorts by resolved title (frontmatter title → first H1)
  - `--sort date` as alias for `property:date`
  - `--sort property:KEY` for arbitrary property sorting
  - Files missing the sort property sort last; file-path tie-breaker
- [x] Add `backlinks_count` and `links_count` as sort fields ([[backlog/sort-by-backlinks-count.md]])
- [x] Support repeatable `--glob` flag for combining include/exclude patterns ([[backlog/repeatable-glob-flag.md]])
- [x] Content search should include lines inside code blocks ([[backlog/text-search-skips-code-blocks-undocumented.md]])
