---
title: Add --quiet flag and deduplicate repeated warnings
type: backlog
date: 2026-03-28
status: completed
priority: low
origin: dogfooding v0.4.2 on docs/content
tags:
  - cli,ux
---

The warning `skipping code-security/concepts/index.md: unclosed frontmatter` appears on every full-scan command. When running multiple commands in sequence (common in batch/scripting), this adds noise.

**Proposed changes:**
1. `--quiet` / `-q` flag to suppress all warnings on stderr
2. Deduplicate identical warnings within a single command invocation (show "N files skipped" summary instead of per-file warnings)
