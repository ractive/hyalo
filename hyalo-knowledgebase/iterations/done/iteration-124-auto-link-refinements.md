---
title: "Iteration 124 — Auto-link refinements: first-only & exclude-target-glob"
type: iteration
date: 2026-04-22
tags:
  - iteration
  - links
  - ux
  - feature
status: completed
branch: iter-124/auto-link-refinements
---

## Goal

Reduce false-positive noise in `hyalo links auto` with two new filtering options discovered during dogfooding on a real KB with 245+ auto-linked mentions.

## Context

Dogfooding iter-123 on a production KB surfaced two categories of noise:
- **Repetition**: a single page can get 10× `[[Lukas]]` — only the first mention per page matters
- **Noisy target pages**: entire directories (e.g. `Mail Templates/*`) produce false positives because their titles are common English words ("Start", "Offer", "Retro")

Property-based target filtering (e.g. only link to `type=person` pages) was considered but deferred — `--exclude-title` and `--exclude-target-glob` cover the practical cases. If property filtering is genuinely needed, it'll surface in future dogfooding.

## Design

### `--first-only`

- Boolean flag, default off
- When set, only emit/apply the **first** match of each target title per source file
- "First" = lowest byte offset in the source file
- Applies to both `--dry-run` (default) and `--apply` modes

### `--exclude-target-glob <pattern>`

- Repeatable flag (can pass multiple times)
- Excludes **target pages** whose relative path matches any of the given glob patterns
- Example: `--exclude-target-glob 'Mail Templates/*'` removes all pages under that directory from the title inventory
- Complements existing `--exclude-title` which filters by exact title string
- Named `--exclude-target-glob` (not `--exclude-glob`) to distinguish from `--glob` which scopes **source** files
- Glob matching uses the same engine as `--glob` (source scoping) but applied to target candidates

### Implementation approach

Both filters operate on the **title inventory** (built in `build_title_inventory`):
1. Build the full inventory as today
2. Apply `--exclude-target-glob` to remove targets by path
3. Pass the filtered inventory to the matching engine (unchanged)
4. After matching, if `--first-only`, deduplicate matches per (source_file, target_title) keeping lowest offset

This layered approach keeps the matching engine untouched — all new logic is in inventory filtering and post-match dedup.

## Tasks

- [x] Add `--first-only` boolean flag to CLI and wire to auto-link options struct
- [x] Implement first-only dedup: after collecting matches per source file, keep only the lowest-offset match per target title
- [x] Add `--exclude-target-glob` repeatable flag to CLI
- [x] Implement target inventory filtering by glob pattern in `build_title_inventory`
- [x] Add unit tests for first-only dedup logic
- [x] Add unit tests for exclude-target-glob inventory filtering
- [x] Add e2e tests covering both flags individually and in combination
- [x] Update `hyalo links auto --help` text with new options and examples
- [x] Update README.md — ensure the `hyalo links auto` section documents the new flags and includes usage examples
- [x] Update skill templates (crates/hyalo-cli/templates/rule-knowledgebase.md) — add `hyalo links auto` with key flags to the CLI reference
- [x] Update knowledgebase docs if any reference auto-link usage

## Acceptance Criteria

- [x] `--first-only` emits at most one link per target title per source file
- [x] `--exclude-target-glob 'pattern'` removes matching target pages from the inventory
- [x] Multiple `--exclude-target-glob` flags combine (union of exclusions)
- [x] Both flags work with both `--dry-run` (default) and `--apply`
- [x] Both flags compose correctly when used together
- [x] Help text, README.md, skill templates, and knowledgebase docs all document `hyalo links auto` including the new flags
- [x] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [x] `cargo test --workspace -q` passes
