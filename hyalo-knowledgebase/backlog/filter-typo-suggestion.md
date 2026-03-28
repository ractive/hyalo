---
title: "Improve --filter typo suggestion to include --property"
type: backlog
date: 2026-03-28
status: planned
priority: low
origin: dogfooding v0.4.2 on docs/content
---

When a user types `--filter` (which doesn't exist), clap suggests `--file` as the closest match. Since the user almost certainly meant `--property`, the error message is misleading.

**Fix options:**
1. Add `--filter` as a hidden alias for `--property` (most user-friendly)
2. Customize the clap error to suggest `--property` explicitly
3. Add `visible_alias("filter")` to the property arg in clap
