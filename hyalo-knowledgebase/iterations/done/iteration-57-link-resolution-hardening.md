---
title: Iteration 57 — Link resolution hardening
type: iteration
date: 2026-03-28
status: completed
branch: iter-57/link-resolution
tags:
  - links
  - parser
---

# Iteration 57 — Link resolution hardening

These items all touch the link resolution pipeline and can be tested together. Fixing them improves backlink accuracy and orphan detection across real-world repos.

## Tasks

- [x] Strip query strings and fragments from link targets before resolution ([[backlog/done/query-string-link-resolution]])
  - `/docs/?dv=winzip` → `/docs/`; `file.md#section` → `file.md`
- [x] Strip trailing slash from link targets before resolution ([[backlog/done/trailing-slash-link-resolution]])
  - `/docs/debugtest/debugging.md/` → `/docs/debugtest/debugging.md`
- [x] Fix heading text inside code spans parsed as empty string ([[backlog/done/heading-code-spans-parsed-empty]])
- [x] Fix regex case sensitivity flags `(?i)`/`(?-i)` having no effect ([[backlog/done/regex-case-sensitivity-flags-inert]])
- [x] Dogfood backlinks accuracy on vscode-docs/docs after fixes
