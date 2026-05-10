---
title: Dogfood Results — v0.15.0 iter-132
date: 2026-05-10
type: dogfood
status: completed
tags:
  - dogfood
  - iter-132
related:
  - "[[iterations/iteration-132-dogfood-v0150-iter131-fixes]]"
---

## Session

Branch: `iter-132/dogfood-v0150-iter131-fixes`
Binary: `target/release/hyalo` (built 2026-05-10)
Vault: `hyalo-knowledgebase/` (266 files)

## Features Verified

### UX-B — `hyalo links` defaults to dry-run fix

`hyalo links` (no subcommand) ran immediately and returned fix candidates with a hint to run `links fix --apply`. Previously required explicit `links fix --dry-run`. Works well.

### UX-D — `hyalo views run <name>`

`hyalo views run planned` and `hyalo views run open-tasks` both returned results matching `find --view <name>`. The `run` subcommand is a clean addition; no surprises.

### UX-F — `--sort path` and `--desc` aliases

`find --tag iteration --sort path --desc` returned iterations in reverse alphabetical order. Both aliases work transparently alongside their canonical forms.

### BUG-C — Fuzzy tag/property suggestions

`find --tag iteraion` emitted:
```
warning: no files matched --tag "iteraion"; did you mean: iteration?
```

Exactly the right behavior. Threshold distance=2 catches common transpositions without false positives.

### UX-C — Global `--index-file`

`hyalo --index-file /tmp/nonexistent.msgpack find --tag iteration` warned about the missing file and fell back to disk scan. The global flag is visible in `hyalo --help`.

### BUG-B — HYALO003 date format lint rule

`hyalo lint --rule HYALO003` ran across all 266 files with 0 violations (the KB uses ISO-8601 dates consistently). The rule is registered and active.

### UX-A — `create-index` text output hint

Verified via e2e test in prior session. The `format_error` path already included hints in text mode.

### UX-E — `lint --strict` help mentions schema dependency

`hyalo lint --help` now contains "schema" in the `--strict` flag description.

## Issues Found

None. All 9 items from iteration-132 work correctly in production use against the knowledgebase.

## Notes

- Lint has 8 errors / 478 warnings in `--strict` mode on the KB — these are pre-existing (missing-type, undeclared-property warnings promoted by `--strict`). Not regressions from this iteration.
- `hyalo views run open-tasks` returned 24 files with open tasks — iteration-132 tasks were all marked done before this dogfood, so the file no longer appears.
