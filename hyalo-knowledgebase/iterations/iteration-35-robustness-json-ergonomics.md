---
title: Iteration 35 — Robustness & JSON Ergonomics
type: iteration
date: 2026-03-23
tags:
  - iteration
  - robustness
  - json
  - ux
  - llm
status: planned
branch: iter-35/robustness-json-ergonomics
---

# Iteration 35 — Robustness & JSON Ergonomics

## Goal

Make hyalo reliable on real-world vaults (including broken files) and fix the JSON output ergonomics that make every `--jq` query unnecessarily verbose. This is the highest-impact iteration for Claude Code usage.

## Backlog items

- [[backlog/graceful-parse-error-recovery]] (critical)
- [[backlog/properties-as-map-in-json]] (medium)
- [[backlog/summary-depth-flag]] (low)
- iter-12 cleanup items (from code review)

## Tasks

### Graceful parse error recovery
- [ ] Unclosed frontmatter emits warning to stderr, skips file, continues scan
- [ ] Frontmatter exceeding budget emits warning to stderr, skips file, continues scan
- [ ] All warning messages include the file path
- [ ] `hyalo summary` completes successfully even with broken files
- [ ] Exit code is 0 when broken files are skipped (not 2)
- [ ] E2e tests cover broken-file scenarios

### Properties as map in JSON output
- [ ] Properties output as `{"key": value}` map instead of `[{name, value, type}]` array
- [ ] `jq` queries like `.properties.status` work directly
- [ ] Type information accessible via `--fields properties-typed` or similar escape hatch
- [ ] Existing `--jq` queries in skill/docs updated to new format
- [ ] E2e tests updated for new JSON shape
See
### Summary --depth flag
- [ ] `--depth N` limits directory listing depth in summary output
- [ ] Stats section (properties, tags, status, tasks) always shown regardless of depth
- [ ] Default behavior unchanged (full depth) for backwards compatibility
- [ ] E2e tests cover depth flag

### iter-12 cleanup
- [ ] Rename `src/commands/outline.rs` → `src/commands/section_scanner.rs`
- [ ] Add missing `--format text` e2e tests for `remove` and `append`
- [ ] Add combined 4-filter e2e test for `find` (property + tag + task + pattern)
- [ ] Add `--jq` and `--hints` e2e tests for mutation commands

## Acceptance Criteria

- [ ] `hyalo summary` on a vault with broken files completes without error
- [ ] `.properties.status` works in `--jq` queries (no more `select(.name==...)`)
- [ ] Large vault summary output is manageable with `--depth`
- [ ] All quality gates pass (fmt, clippy, tests)
