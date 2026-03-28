---
title: Iteration 43b — Dogfood Bugfixes (v0.4.0)
type: iteration
date: 2026-03-26
tags:
  - iteration
  - dogfooding
  - bug-fix
status: completed
branch: iter-43/dogfood-bugfixes
---

# Iteration 43b — Dogfood Bugfixes (v0.4.0)

## Goal

Fix 3 bugs discovered while dogfooding v0.4.0 against GitHub Docs (3520 files) and VS Code Docs (339 files).

## Tasks

- [x] Fix mv not rewriting absolute-path inbound links
- [x] Fix negation globs `!pattern` broken via Bash escaping
- [x] Fix regex filter failing silently on YAML map properties
- [x] Harden link label indexing, mv path normalization, traversal guard (review feedback)

## Acceptance Criteria

- [x] All 3 bugs fixed and verified
- [x] Review feedback addressed
- [x] All quality gates pass
