---
branch: iter-35/robustness-json-ergonomics
date: 2026-03-23
status: completed
tags:
- iteration
- robustness
- json
- ux
- llm
title: Iteration 35 — Robustness & JSON Ergonomics
type: iteration
---

# Iteration 35 — Robustness & JSON Ergonomics

## Goal

Make hyalo reliable on real-world vaults (including broken files) and fix the JSON output ergonomics that make every `--jq` query unnecessarily verbose. This is the highest-impact iteration for Claude Code usage.

## Backlog items

- [[backlog/done/graceful-parse-error-recovery]] (critical)
- [[backlog/done/properties-as-map-in-json]] (medium)
- [[backlog/done/summary-depth-flag]] (low)
- iter-12 cleanup items (from code review)

## Tasks

### Graceful parse error recovery
- [x] Unclosed frontmatter emits warning to stderr, skips file, continues scan
- [x] Frontmatter exceeding budget emits warning to stderr, skips file, continues scan
- [x] All warning messages include the file path
- [x] `hyalo summary` completes successfully even with broken files
- [x] Exit code is 0 when broken files are skipped (not 2)
- [x] E2e tests cover broken-file scenarios

### Properties as map in JSON output
- [x] Properties output as `{"key": value}` map instead of `[{name, value, type}]` array
- [x] `jq` queries like `.properties.status` work directly
- [x] Type information accessible via `--fields properties-typed` or similar escape hatch
- [x] Existing `--jq` queries in skill/docs updated to new format
- [x] E2e tests updated for new JSON shape
See
### Summary --depth flag
- [x] `--depth N` limits directory listing depth in summary output
- [x] Stats section (properties, tags, status, tasks) always shown regardless of depth
- [x] Default behavior unchanged (full depth) for backwards compatibility
- [x] E2e tests cover depth flag

### iter-12 cleanup
- [x] Rename `src/commands/outline.rs` → `src/commands/section_scanner.rs`
- [x] Add missing `--format text` e2e tests for `remove` and `append`
- [x] Add combined 4-filter e2e test for `find` (property + tag + task + pattern)
- [x] Add `--jq` and `--hints` e2e tests for mutation commands

## Acceptance Criteria

- [x] `hyalo summary` on a vault with broken files completes without error
- [x] `.properties.status` works in `--jq` queries (no more `select(.name==...)`)
- [x] Large vault summary output is manageable with `--depth`
- [x] All quality gates pass (fmt, clippy, tests)
