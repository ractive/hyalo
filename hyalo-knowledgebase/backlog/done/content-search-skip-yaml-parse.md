---
title: ContentSearchVisitor triggers unnecessary YAML parse in index re-scan
type: backlog
date: 2026-03-29
status: completed
origin: codebase review 2026-03-29
priority: low
tags:
  - performance
  - find
---

## Problem

When `find_from_index` re-scans files for content search (`find.rs:724-749`), it uses `ContentSearchVisitor` which doesn't override `needs_frontmatter()` (defaults to `true`). The scanner parses and allocates the YAML even though the content visitor doesn't need it.

## Fix

Override `needs_frontmatter()` to return `false` in `ContentSearchVisitor` (`content_search.rs`), so the scanner skips YAML accumulation and allocation during content-only re-scans.

## Acceptance criteria

- [ ] `ContentSearchVisitor::needs_frontmatter()` returns `false`
- [ ] Content search still works correctly (all e2e tests pass)
