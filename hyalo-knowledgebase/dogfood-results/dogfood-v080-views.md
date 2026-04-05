---
title: "Dogfood v0.8.0: Views Feature"
date: 2026-04-03
origin: dogfood session 2026-04-03
tags: [dogfooding, views]
---

# Dogfood v0.8.0 — Views Feature

Session focused on evaluating the new views feature (iter-94/95) during a knowledgebase tidy pass.

## Findings

### Views work well
- `views set` / `views list` / `find --view` all function correctly
- CLI flag merging on top of views works as documented (e.g. `--view planned --limit 5`)
- Views persist correctly in `.hyalo.toml`
- The hyalo-tidy skill template already references views — good forward planning

### Bug: `views list --format text` outputs JSON
- `hyalo views list --format text` ignores the `--format text` flag and outputs JSON
- All other commands respect `--format text` — this is an inconsistency
- Low severity but confusing for agents and users expecting text output

### Observation: tidy skill creates views but they're ephemeral
- The hyalo-tidy skill creates diagnostic views in Phase 1, which is good
- However, these views accumulate across tidy runs (no cleanup step)
- Not a bug — views are cheap — but the skill could note this

## Summary

The views feature is solid for its first iteration. One minor bug (`views list` format flag) and no blockers.
