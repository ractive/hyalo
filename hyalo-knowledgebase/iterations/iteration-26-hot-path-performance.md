---
branch: iter-26/hot-path-performance
date: 2026-03-23
status: planned
tags:
- iteration
- performance
- filters
title: 'Iteration 26: Hot-Path Performance Fixes'
type: iteration
---

# Iteration 26: Hot-Path Performance Fixes

## Goal

Eliminate unnecessary allocations on hot paths in filtering and content search. These are the performance issues identified in the code review that affect per-file and per-line processing.

## Tasks

### Filter allocation fixes
- [ ] Pre-lowercase `PropertyFilter.value` at parse time instead of calling `.to_lowercase()` on every comparison (`filter.rs:244,261`)
- [ ] Replace `to_lowercase().as_str()` pattern with `eq_ignore_ascii_case` in boolean parsing (`filter.rs:268`)
- [ ] Pre-lowercase tag filter values at parse time in hint generation (`hints.rs:113`)

### Content search allocation fix
- [ ] Replace per-line `to_lowercase()` in `SearchMode::Substring` (`content_search.rs:86`) with a case-insensitive search approach (e.g., `memchr` or manual ASCII-insensitive scan)

### Scanner micro-optimizations
- [ ] Remove unnecessary `to_owned()` for trimmed first line (`scanner.rs:364`) — restructure scope to keep as `&str`
- [ ] Evaluate replacing `expect` on UTF-8 validation (`scanner.rs:201,270`) with `unsafe { String::from_utf8_unchecked }` since invariant is proven (ASCII byte replacement preserves UTF-8) — document the safety proof

### Sort optimization
- [ ] Sort `files` in place in `find.rs:533` instead of cloning the `Vec<PathBuf>`

### Benchmarking
- [ ] Run `cargo bench` before and after to measure impact
- [ ] Document results in this file or in a commit message

### Quality gates
- [ ] `cargo fmt`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`

## Acceptance Criteria

- [ ] No `to_lowercase()` calls inside per-file or per-line loops for filter comparisons
- [ ] Benchmarks show measurable improvement (or at least no regression)
- [ ] All quality gates pass
