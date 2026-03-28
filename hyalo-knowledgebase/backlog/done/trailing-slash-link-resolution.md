---
title: Strip trailing slash from link targets before resolution
type: backlog
date: 2026-03-26
origin: >-
  dogfooding v0.4.1 on VS Code Docs (languages/rust.md links to
  /docs/debugtest/debugging.md/)
priority: low
status: completed
---

Link `/docs/debugtest/debugging.md/` (with trailing `/`) fails to resolve even though `debugtest/debugging.md` exists. The trailing slash should be stripped before resolution.

Fix in `resolve_target` in `discovery.rs`: trim trailing `/` from the target string early in the function.
