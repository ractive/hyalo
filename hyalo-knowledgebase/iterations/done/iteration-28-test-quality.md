---
branch: iter-28/test-quality
date: 2026-03-23
status: completed
tags:
- iteration
- testing
- quality
title: 'Iteration 28: Test Quality & Coverage Gaps'
type: iteration
---

# Iteration 28: Test Quality & Coverage Gaps

## Goal

Fix the systematic test diagnostics issue (silent JSON parse failures) and add missing edge-case tests identified in the code review. When a test fails, the error message should immediately point to the problem.

## Tasks

### Fix JSON-parsing helpers (systematic)
- [x] Update `find_json` helper in test common/helpers to use `unwrap_or_else(|e| panic!("invalid JSON: {e}\nstdout: {stdout}\nstderr: {stderr}"))` — follow the `e2e_config.rs` pattern
- [x] Apply same fix to all `*_json` helpers across all e2e test files (`e2e_set.rs`, `e2e_remove.rs`, `e2e_append.rs`, `e2e_task.rs`, `e2e_tags.rs`, `e2e_properties.rs`, `e2e_summary.rs`, etc.)
- [x] Replace bare `.as_str().unwrap()` / `.as_array().unwrap()` / `.as_u64().unwrap()` with `.expect("field 'X' should be a Y")` in the most critical test files (`e2e_find.rs`, `e2e_task.rs`)

### Missing edge-case tests
- [x] Add e2e test for `task read --line 0` (out-of-range boundary case)
- [x] Add e2e test for `read` on a file with no body (frontmatter-only)
- [x] Add e2e tests for `--type` forcing via `set --property key=value --type text|number|checkbox`
- [x] Add e2e test for `--jq` on mutation commands (`set`, `remove`, `append`)
- [x] Add e2e test for `append` on a file with no frontmatter

### CLI validation improvement
- [x] Add Clap `value_parser` for `--format` to reject invalid values at parse time instead of runtime

### Quality gates
- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`

## Acceptance Criteria

- [x] All JSON-parsing test helpers include stdout/stderr in panic messages
- [x] No bare `.unwrap()` on JSON field access in critical test files
- [x] All 5 missing edge-case tests pass
- [x] `--format foo` produces a Clap error, not a runtime error
- [x] All quality gates pass
