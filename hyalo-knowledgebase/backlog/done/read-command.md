---
date: 2026-03-23
origin: dogfooding post-iter-10, dogfooding post-iter-19, iteration-13
priority: high
status: completed
tags:
- backlog
- cli
- ux
- llm
title: Read command — display file body content
type: backlog
---

# Read command — display file body content

## Problem

Hyalo can show a file's metadata (properties, tags, sections, tasks, links) but cannot display the actual body content. After finding a file via `find`, you must use an external tool (cat, Read) to see what it says. This is the #1 gap for "second brain" workflows and LLM agent use.

Identified in the post-iter-10 dogfooding report (ISSUE-5) and confirmed again during post-iter-19 cleanup.

## Proposal

```sh
# Read full file body (excluding frontmatter)
hyalo read --file research/dogfooding-post-iter-10.md

# Read a specific section
hyalo read --file research/dogfooding-post-iter-10.md --section "## Issues and Improvement Ideas"

# Read with line numbers
hyalo read --file research/dogfooding-post-iter-10.md --lines
```

## Notes

- Originally planned as [[iterations/iteration-13-read-command]] (now deferred)
- JSON output could wrap content in `{"file": "...", "content": "...", "section": "..."}`
- `--section` should match by heading text (case-insensitive substring match)
- Consider supporting `--format text` (raw content) vs `--format json` (wrapped)
