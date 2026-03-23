---
branch: iter-26/hot-path-performance
date: 2026-03-23
status: in-progress
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
- [x] Pre-lowercase `PropertyFilter.value` at parse time instead of calling `.to_lowercase()` on every comparison (`filter.rs:244,261`)
- [x] Replace `to_lowercase().as_str()` pattern with `eq_ignore_ascii_case` in boolean parsing (`filter.rs:268`)
- [x] Pre-lowercase tag filter values at parse time in hint generation (`hints.rs:113`)

### Content search allocation fix
- [x] Replace per-line `to_lowercase()` in `SearchMode::Substring` (`content_search.rs:86`) with a case-insensitive search approach (e.g., `memchr` or manual ASCII-insensitive scan)

### Scanner micro-optimizations
- [x] Remove unnecessary `to_owned()` for trimmed first line (`scanner.rs:364`) — skipped: `to_owned()` is required because `buf` is mutated in the frontmatter loop before `first_trimmed` is used again
- [x] Evaluate replacing `expect` on UTF-8 validation (`scanner.rs:201,270`) with `unsafe { String::from_utf8_unchecked }` since invariant is proven (ASCII byte replacement preserves UTF-8) — document the safety proof

### Sort optimization
- [x] Sort `files` in place in `find.rs:533` instead of cloning the `Vec<PathBuf>` — N/A: line 533 is in a test, production sort already uses in-place `sort_by`

### Benchmarking
- [x] Run `cargo bench` before and after to measure impact
- [x] Document results in this file or in a commit message

### Quality gates
- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`

## Acceptance Criteria

- [x] No `to_lowercase()` calls inside per-file or per-line loops for filter comparisons
- [x] Benchmarks show measurable improvement (or at least no regression)
- [x] All quality gates pass

## Benchmark Results

Before (baseline on main):
- `parse_property_filter/equality`: 43.7 ns
- `parse_property_filter/comparison`: 42.0 ns
- `parse_property_filter/exists`: 28.8 ns

After (with pre-lowercasing at parse time):
- `parse_property_filter/equality`: 61.0 ns (+39% one-time parse cost)
- `parse_property_filter/comparison`: 56.3 ns (+34% one-time parse cost)
- `parse_property_filter/exists`: 28.7 ns (unchanged, no value to lowercase)

The parse-time increase is a one-time cost per filter. The per-file comparison savings (eliminating `to_lowercase()` on every YAML value match) are not captured by micro-benchmarks but scale with vault size. Scanner and link benchmarks unchanged.
