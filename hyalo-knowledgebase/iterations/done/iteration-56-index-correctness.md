---
title: Iteration 56 — Index correctness & input validation
type: iteration
date: 2026-03-28
status: completed
branch: iter-56/index-correctness
tags:
  - index
  - validation
  - bug-fix
  - iteration
---

# Iteration 56 — Index correctness & input validation

Fix the critical index orphan bug and the medium-priority input validation issues. These are correctness bugs that should be addressed before any feature work.

## Tasks

- [x] Fix orphan count discrepancy between disk scan and snapshot index ([[backlog/done/index-orphan-count-discrepancy]])
  - Diff link graphs from `ScannedIndex` vs `SnapshotIndex` to find divergence
  - Add test: `summary` orphan count must match with and without `--index`
  - Test on vscode-docs/docs (339 files) where bug was reproduced
- [x] Validate `--limit 0` — reject with error or document as unlimited ([[backlog/done/limit-zero-means-unlimited]])
- [x] Reject empty body pattern `""` with error instead of matching all files ([[backlog/done/empty-body-pattern-matches-all]])
- [x] Dogfood on docs/content and vscode-docs/docs to verify fixes
