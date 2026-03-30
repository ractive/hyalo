---
title: "Iteration 81: Internal quality cleanup"
type: iteration
date: 2026-03-30
tags:
  - iteration
  - refactoring
  - code-quality
status: in-progress
branch: iter-81/internal-quality-cleanup
---

# Iteration 81 — Internal Quality Cleanup

Tackle four backlog items that improve internal code quality without changing external behavior.

## Tasks

- [x] Tighten pub-crate visibility — narrowed 7 items to pub(crate); 9 of the original 16 are actually used outside the crate
- [x] Split find/mod.rs — extracted 1,469 lines of tests to find/tests.rs (mod.rs now 491 lines)
- [x] Change TaskInfo.status from String to char — updated types, call sites, and comparisons
- [x] Clippy pedantic cleanup — no action needed; all 76 warnings are intentionally allowed in workspace config
