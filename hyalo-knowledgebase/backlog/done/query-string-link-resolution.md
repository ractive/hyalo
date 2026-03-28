---
title: Strip query strings and fragments from link targets before resolution
type: backlog
date: 2026-03-26
origin: dogfooding v0.4.1 on VS Code Docs (setup/windows.md links to /docs/?dv=winzip)
priority: low
status: completed
---

Link `/docs/?dv=winzip` is not resolved because the `?dv=winzip` query string is included in the path lookup. Similarly, `#fragment` anchors should be stripped.

Fix in `resolve_target` in `discovery.rs`: strip `?...` and `#...` suffixes from the target string before attempting file lookup.
