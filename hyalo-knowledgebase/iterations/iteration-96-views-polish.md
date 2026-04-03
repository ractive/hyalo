---
title: "Iteration 96: Views Polish"
type: iteration
date: 2026-04-03
status: in-progress
branch: iter-96/views-polish
tags:
  - views
  - bug
  - format
  - dogfooding
---

# Iteration 96: Views Polish

Fixes and enhancements from [[dogfood-results/dogfood-v080-views]] session.

## Tasks

- [x] Fix `views list --format text` to output text instead of JSON ([[backlog/views-list-format-text]])
- [x] Add text formatter for `views set` and `views remove` output
- [x] Verify all `views` subcommands respect `--format` flag and `.hyalo.toml` default
- [x] Make `hyalo views` (no subcommand) default to `views list` for consistency with `tags` and `properties`
- [x] Update help texts for all modified commands
- [x] Add/update e2e tests for views text format output
