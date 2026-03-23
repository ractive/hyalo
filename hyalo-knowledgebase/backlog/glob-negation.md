---
title: "Glob negation / exclude pattern support"
type: backlog
date: 2026-03-23
status: planned
priority: medium
origin: dogfooding docs/content vault
tags:
  - backlog
  - cli
  - filtering
  - ux
---

# Glob negation / exclude pattern support

## Problem

There is no way to exclude files by glob pattern. When a broken file blocks a scan, you cannot work around it with `--glob "!**/index.md"` or `--exclude "path/to/broken.md"`.

This is a standard feature in tools like rg (`--glob '!pattern'`) and gitignore syntax.

## Proposal

Support negation globs prefixed with `!`:
- `--glob '!**/index.md'` — exclude all index.md files
- `--glob '!code-security/concepts/index.md'` — exclude one specific file

Alternatively, add a separate `--exclude` flag.

## Acceptance criteria

- [ ] Can exclude files matching a glob pattern
- [ ] Works in combination with positive globs (include + exclude)
- [ ] Help text documents the syntax
- [ ] E2e tests cover negation globs
