---
title: "Content search should include lines inside code blocks"
type: backlog
date: 2026-03-25
origin: dogfooding v0.3.1 against vscode-docs/docs
priority: medium
status: planned
tags: [backlog, bug, search, scanner]
---

# Content search should include lines inside code blocks

## Problem

`hyalo find "typescript"` returns 51 files while `rg -il 'typescript'` returns 61 on the same corpus. The 10 missing files have matches only inside fenced code blocks. Text/regex search silently skips all content inside fenced code blocks.

This is not a deliberate design choice — it's an unintentional side-effect of the scanner architecture. The scanner's `on_body_line()` callback was designed for link/task extraction (where skipping code blocks is correct), and `ContentSearchVisitor` inherited that behavior when it plugged into the same callback.

## Proposed fix

`ContentSearchVisitor` needs to also receive lines inside code blocks. Options:

1. Add an `on_code_block_line()` callback to `FileVisitor` and have `ContentSearchVisitor` implement it
2. Or have `ContentSearchVisitor` opt in to receiving all lines (body + code block) via a flag on the visitor

Links and tasks should continue to skip code blocks — only content search changes.

## Acceptance criteria

- [ ] `hyalo find "term"` matches lines inside fenced code blocks
- [ ] Link extraction still skips code blocks (no regression)
- [ ] Task extraction still skips code blocks (no regression)
- [ ] E2e test: search term only inside a code block is found
