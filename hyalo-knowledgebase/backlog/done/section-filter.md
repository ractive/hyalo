---
date: 2026-03-23
origin: dogfooding post-iter-19
priority: medium
status: completed
tags:
  - backlog
  - search
  - cli
title: Section-scoped filter for find command
type: backlog
---

# Section-scoped filter for find command

## Problem

Currently `hyalo find` searches across the entire body of each file. There's no way to scope content search, task filtering, or matches output to a specific document section. For example, if you want to find open tasks only under `## Tasks` headings, you have to search all tasks and manually filter the results by section.

## Proposal

Add `--section HEADING` flag to `hyalo find` that restricts body-level operations (content search, task filtering, matches output) to lines under the specified heading.

```sh
# Only match tasks under "## Tasks" sections
hyalo find --section "## Tasks" --task todo

# Content search scoped to a specific section
hyalo find --section "## Notes" "retry"

# Regex search within a section
hyalo find --section "## Design" -e "TODO|FIXME"
```

## Notes

- Should support partial matching (e.g. `--section Tasks` matches `## Tasks`) or require exact heading format — needs a design decision.
- Section boundaries: from the heading until the next heading of equal or higher level, matching the existing `SectionScanner` behaviour.
- Multiple `--section` flags could use OR semantics (match any of the listed sections).
