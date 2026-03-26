---
title: "Empty body pattern matches all files — should error or warn"
type: backlog
date: 2026-03-26
origin: dogfooding v0.4.1 on GitHub Docs
priority: low
status: planned
---

`hyalo find ""` returns all 3520 files because empty string is a substring of everything. This is technically correct but surprising — no warning is emitted.

Options:
1. Error: "body pattern must not be empty" (strictest, may break scripts)
2. Warn on stderr: "empty body pattern matches all files" (gentle)
3. Document the behavior and leave as-is

Option 2 (warn) seems best — it's informative without breaking anything.
