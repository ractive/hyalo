---
date: 2026-03-23
origin: dogfooding vscode-docs vault
priority: low
status: wont-do
tags:
- backlog
- cli
- filtering
title: Date-aware property comparison
type: backlog
---

# Date-aware property comparison

## Problem

Property comparisons (`>`, `<`, `>=`, `<=`) are lexicographic (string-based). When used on date-like text values (e.g. `DateApproved: 8/4/2023`), the results are wrong because `"8" > "1"` lexicographically, so `8/4/2023 > 12/01/2025` evaluates to true.

Hyalo already has a `date` type in its type inference (for ISO 8601 dates like `2026-03-23`). But non-ISO date strings like `MM/DD/YYYY` are inferred as `text` and compared as strings.

## Proposal

Two options:
1. **Best-effort date parsing**: if both sides of a comparison look like dates (in common formats), parse them as dates before comparing
2. **Explicit type hint**: `--property 'DateApproved>2025-01-01:date'` to force date comparison

## Note

This is low priority since properly formatted vaults use ISO 8601 dates where lexicographic and chronological order align. The VS Code docs vault uses `MM/DD/YYYY` format which is the problematic case.

## Acceptance criteria

- [x] Date comparisons produce chronologically correct results
- [x] Works with common date formats (ISO 8601, MM/DD/YYYY, YYYY-MM-DD)
- [x] Non-date values fall back to string comparison
