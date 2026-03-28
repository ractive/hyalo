---
title: "Iteration 59 — Sort & filter enhancements"
type: iteration
date: 2026-03-28
status: planned
branch: iter-59/sort-filter
tags:
  - find
  - sorting
  - filtering
---

# Iteration 59 — Sort & filter enhancements

Extend the `find` command's sorting and filtering capabilities. These touch overlapping code paths (the sort/filter pipeline) and should be designed together.

## Tasks

- [ ] Support `--sort` by frontmatter property value ([[backlog/sort-by-property-value.md]])
  - `--sort title`, `--sort date` as built-in aliases
  - `--sort property:KEY` for arbitrary property sorting
  - Files missing the sort property sort last
- [ ] Add `backlinks_count` and `links_count` as sort fields ([[backlog/sort-by-backlinks-count.md]])
- [ ] Support repeatable `--glob` flag for combining include/exclude patterns ([[backlog/repeatable-glob-flag.md]])
- [ ] Content search should include lines inside code blocks ([[backlog/text-search-skips-code-blocks-undocumented.md]])
