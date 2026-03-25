---
branch: iter-39b/move-command
date: 2026-03-25
status: completed
tags:
- iteration
- links
- wikilinks
- cli
title: Iteration 39b — Move Command
type: iteration
---

# Iteration 39b — Move Command

## Goal

Add a `hyalo mv` command that renames/moves a markdown file and updates all inbound wikilinks across the vault, using the in-memory link graph built in [[iterations/done/iteration-39a-link-graph]].

## Tasks

### Move/rename command
- [x] `hyalo mv --file <old> --to <new>` moves file and updates all inbound wikilinks
- [x] Uses in-memory graph to find inbound links, rewrites them in-place
- [x] Handles both `[[path]]` and `[[path|alias]]` forms
- [x] Dry-run mode (`--dry-run`) shows what would change without writing
- [x] E2e tests cover move with link updates

## Acceptance Criteria

- [x] Move command correctly updates all inbound links
- [x] Dry-run produces accurate preview without side effects
- [x] All quality gates pass (fmt, clippy, tests)
