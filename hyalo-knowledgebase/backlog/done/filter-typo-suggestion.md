---
title: Improve --filter typo suggestion to include --property
type: backlog
date: 2026-03-28
status: completed
priority: low
origin: dogfooding v0.4.2 on docs/content
tags:
  - cli,ux,error-handling
---

When a user types `--filter` (which doesn't exist), clap suggests `--file` as the closest match. Since the user almost certainly meant `--property`, the error message is misleading.

**Fix:** Customize clap's error handling so that unknown flags like `--filter` suggest `--property` instead of the misleading `--file` suggestion based on string similarity alone.
