---
title: --limit 0 silently returns all files — should validate
type: backlog
date: 2026-03-28
origin: dogfooding v0.4.1 + v0.4.2
priority: medium
status: completed
tags:
  - cli,validation
---

`hyalo find --limit 0` returns ALL files (330,981 lines of JSON on docs/content). It behaves as if `--limit` were omitted entirely — `0` is treated as "no limit".

This violates the principle of least surprise. A user passing `--limit 0` likely expects either zero results or an error, not a full dump.

**Fix:** Either reject `--limit 0` with an error ("limit must be >= 1") or explicitly document that 0 means unlimited. The former is safer for scripts that compute the limit value dynamically.
