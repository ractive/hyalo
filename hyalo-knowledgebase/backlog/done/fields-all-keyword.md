---
title: Support --fields all as a keyword
type: backlog
date: 2026-03-28
status: completed
priority: low
origin: dogfooding v0.4.2 on docs/content
---

`hyalo find --fields all` returns `unknown field "all"`. The help text says "default: all except properties-typed and backlinks", implying `all` should be a valid keyword.

**Proposed behavior:** `--fields all` includes every field including `properties-typed` and `backlinks`. This is the explicit way to get everything.

**Implementation:** Add `"all"` to the field parser, expanding it to the full field set.
