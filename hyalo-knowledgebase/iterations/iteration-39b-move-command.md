---
title: "Iteration 39b — Move Command"
type: iteration
date: 2026-03-25
tags: [iteration, links, wikilinks, cli]
status: planned
branch: iter-39b/move-command
---

# Iteration 39b — Move Command

## Goal

Add a `hyalo mv` command that renames/moves a markdown file and updates all inbound wikilinks across the vault, using the in-memory link graph built in [[iterations/iteration-39a-link-graph]].

## Tasks

### Move/rename command
- [ ] `hyalo mv --file <old> --to <new>` moves file and updates all inbound wikilinks
- [ ] Uses in-memory graph to find inbound links, rewrites them in-place
- [ ] Handles both `[[path]]` and `[[path|alias]]` forms
- [ ] Dry-run mode (`--dry-run`) shows what would change without writing
- [ ] E2e tests cover move with link updates

## Acceptance Criteria

- [ ] Move command correctly updates all inbound links
- [ ] Dry-run produces accurate preview without side effects
- [ ] All quality gates pass (fmt, clippy, tests)
