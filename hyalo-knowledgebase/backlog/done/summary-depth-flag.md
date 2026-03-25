---
date: 2026-03-23
origin: dogfooding docs/content vault (200+ subdirectories in summary output)
priority: low
status: completed
tags:
- backlog
- cli
- ux
- llm
title: Summary --depth flag for large vaults
type: backlog
---

# Summary --depth flag for large vaults

## Problem

`hyalo summary --format text` lists every subdirectory with file counts. On a vault with 200+ subdirectories (like GitHub docs/content), this produces 200+ lines of directory listing before getting to the useful stats (properties, tags, status, tasks).

For LLM context windows this is wasteful.

## Proposal

Add `--depth N` flag to control how deep the directory listing goes:

- `--depth 1`: only top-level directories
- `--depth 2`: two levels deep (default for large vaults?)
- `--depth 0`: no directory listing at all, just stats

## Acceptance criteria

- [ ] `--depth N` limits directory listing depth
- [ ] Stats section (properties, tags, status, tasks) is always shown regardless of depth
- [ ] Default behavior is unchanged (full depth) for backwards compatibility
