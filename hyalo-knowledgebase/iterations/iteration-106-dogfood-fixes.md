---
title: "Iteration 106 — Dogfood Fixes: Status Sort, Views Pattern, Orphan Warning"
date: 2026-04-12
type: iteration
status: in-progress
branch: iter-106/dogfood-fixes
tags:
  - iteration
  - ux
  - dogfooding
  - views
  - summary
  - bm25
---

# Iteration 106 — Dogfood Fixes

## Goal

Fix 4 UX issues found during the v0.10.0 dogfood round.

## Changes

### 1. Summary status sorted by count (not alphabetical)

**File:** `crates/hyalo-cli/src/commands/summary.rs` lines 254-258

The status `BTreeMap` preserves alphabetical order. Sort the resulting `Vec<StatusGroup>` by count descending (ties broken alphabetically) before returning. The text formatter already does `sort_by(-.count)` via jaq — this change makes JSON output match.

### 2. `--orphan --dead-end` warning

**File:** `crates/hyalo-cli/src/dispatch.rs`

Orphans (no links in or out) and dead-ends (inbound but no outbound) are disjoint sets by definition. When both flags are true, emit a stderr warning:

```
warning: --orphan and --dead-end are mutually exclusive (no file can be both); results will always be empty
```

### 3. Views store BM25 patterns

**Files:** `crates/hyalo-cli/src/cli/args.rs`, `crates/hyalo-cli/src/commands/views.rs`, `crates/hyalo-cli/src/dispatch.rs`, `crates/hyalo-cli/src/run.rs`

Add `pattern: Option<String>` to `FindFilters`. This lets `views set` save a BM25 pattern and `find --view` recall it. The CLI positional `PATTERN` on `find` overrides the view's pattern (same as other scalar fields). On `views set`, add a positional `PATTERN` arg before `NAME` would conflict with clap, so add it as a second positional after NAME.

### 4. Stopword-only BM25 warning

**File:** `crates/hyalo-core/src/bm25.rs`

After parsing the boolean query, check if all positive clauses have zero tokens (all stripped as noise). If so, emit a warning to stderr:

```
warning: all search terms are common words with very low discriminative power; results may not be meaningful
```

This is better than silently returning near-zero-score results for every file.

## Tasks

- [x] Sort status by count descending in summary
- [x] Add warning for --orphan + --dead-end
- [x] Add `pattern` field to `FindFilters`
- [x] Update `views set` to accept pattern
- [x] Update view merge to handle pattern
- [x] Update `find --view` dispatch to use view pattern
- [x] Update views list text output to show pattern
- [x] Add BM25 low-discriminative-power warning
- [x] Update e2e tests
- [x] cargo fmt
- [x] cargo clippy --workspace --all-targets -- -D warnings
- [x] cargo test --workspace

## Acceptance Criteria

- [x] `hyalo summary --format json` returns status sorted by count descending
- [x] `hyalo find --orphan --dead-end` prints a warning to stderr
- [x] `hyalo views set my-search "performance" --tag iteration` saves the pattern
- [x] `hyalo find --view my-search` uses the saved BM25 pattern
- [x] `hyalo find --view my-search "override"` overrides the view's pattern
- [x] `hyalo find "the and or"` prints a low-discriminative-power warning
