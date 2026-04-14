---
title: Support --fields title as shorthand for the document title
type: backlog
date: 2026-03-28
status: completed
priority: low
origin: dogfooding v0.4.2 on docs/content
tags:
  - cli,find,ux
---

`title` is the most commonly wanted field when browsing files, but `--fields title` is not supported. Users must use `--fields properties` which returns the entire property bag.

**Proposed behavior:** `--fields title` adds a top-level `"title"` field to each result entry, sourced from: (1) the `title` frontmatter property, or (2) the first H1 heading in the document body, or (3) null.

This would be a convenience shorthand, not a new field type — it just extracts a single commonly-needed value.
