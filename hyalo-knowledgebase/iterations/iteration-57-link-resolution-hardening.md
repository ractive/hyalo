---
title: "Iteration 57 — Link resolution hardening"
type: iteration
date: 2026-03-28
status: planned
branch: iter-57/link-resolution
tags:
  - links
  - parsing
---

# Iteration 57 — Link resolution hardening

These items all touch the link resolution pipeline and can be tested together. Fixing them improves backlink accuracy and orphan detection across real-world repos.

## Tasks

- [ ] Strip query strings and fragments from link targets before resolution ([[backlog/query-string-link-resolution.md]])
  - `/docs/?dv=winzip` → `/docs/`; `file.md#section` → `file.md`
- [ ] Strip trailing slash from link targets before resolution ([[backlog/trailing-slash-link-resolution.md]])
  - `/docs/debugtest/debugging.md/` → `/docs/debugtest/debugging.md`
- [ ] Fix heading text inside code spans parsed as empty string ([[backlog/heading-code-spans-parsed-empty.md]])
- [ ] Fix regex case sensitivity flags `(?i)`/`(?-i)` having no effect ([[backlog/regex-case-sensitivity-flags-inert.md]])
- [ ] Dogfood backlinks accuracy on vscode-docs/docs after fixes
