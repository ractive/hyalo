---
title: "Iteration 96: Views Polish"
type: iteration
date: 2026-04-03
status: planned
branch: iter-96/views-polish
tags: [views, bug, format, dogfooding]
---

# Iteration 96: Views Polish

Fixes and enhancements from [[dogfood-results/dogfood-v080-views]] session.

## Tasks

- [ ] Fix `views list --format text` to output text instead of JSON ([[backlog/views-list-format-text]])
- [ ] Add text formatter for `views set` and `views remove` output
- [ ] Verify all `views` subcommands respect `--format` flag and `.hyalo.toml` default
- [ ] Make bare subcommands default to their list action (`views` → `views list`, `links` → `links fix --dry-run` or similar) for consistency with `tags` and `properties`
- [ ] Update help texts for all modified commands
- [ ] Add/update e2e tests for views text format output
