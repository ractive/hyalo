---
title: "Improve help/cookbook discoverability: --regexp, --jq '.total', case-insensitive regex"
type: iteration
date: 2026-03-30
tags:
  - ux
  - docs
  - dogfood
status: completed
priority: 3
branch: iter-77/regexp-help-visibility
---

## Goal

Add cookbook/help examples for underappreciated features: `--regexp`/`-e`, `--jq '.total'` for counting, and `(?i)` for case-insensitive property regex.

## Context

Found during v0.6.0 dogfooding (iteration 74). Several powerful features are hard to discover because they lack examples in the short help (`-h`), cookbook (`--help`), or `CLAUDE.md`:
- **`--regexp`/`-e`**: zero standalone examples anywhere. A dogfood agent didn't know it existed.
- **`--jq '.total'`**: the `total` field in find output is useful for counting matches but never shown.
- **`(?i)` in `--property 'title~=...'`**: case-insensitive regex is supported but users assume it doesn't work since `--title` is case-insensitive by default.

## Tasks

- [x] Add a regex body search example to the short help in `HELP_EXAMPLES` (e.g. `Regex body search: hyalo find -e 'perf(ormance)?'`)
- [x] Add 1–2 `--regexp` examples to the cookbook in `HELP_LONG` (standalone + combined with other filters)
- [x] Add a `--jq '.total'` counting example to the cookbook (e.g. `# Count matching files` / `hyalo find --property status=draft --jq '.total'`)
- [x] Add a `(?i)` case-insensitive property regex example to the cookbook — already present as `/pattern/i` syntax at line 113-114
- [x] Mention `-e`/`--regexp` in `CLAUDE.md` body search bullet
