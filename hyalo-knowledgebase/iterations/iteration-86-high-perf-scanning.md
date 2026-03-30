---
title: "Iteration 86: High-performance file scanning (memchr + rayon + parallel walk)"
type: iteration
date: 2026-03-30
status: in-progress
branch: iter-86/high-perf-scanning
tags:
  - performance
  - parallelization
  - rayon
  - memchr
---

# Iteration 86: High-Performance File Scanning

## Goal

Close the ~4x performance gap between hyalo `find` body search and ripgrep by combining:
1. SIMD-accelerated substring search via `memchr::memmem::Finder`
2. Parallel file processing via `rayon`
3. Parallel directory walking via `ignore::WalkBuilder::build_parallel()`
4. Fast-reject: skip files that cannot match before running the full scanner

## Context

- Iter-18 showed rayon alone gave only 1.05x on `find` (I/O-dominated)
- ripgrep disables mmap on macOS — we use `std::fs::read` + memchr instead
- ripgrep's speed on macOS comes from parallelism + memchr line splitting + fast reject

## Research

- [[research/performance-parallelization]] — iter-18 rayon experiment results

## Tasks

- [x] Add memchr and rayon dependencies
- [x] Replace naive substring search with memchr::memmem::Finder
- [x] Add scan_slice_multi (memchr line splitting on &[u8])
- [x] Add fast-reject for content search (skip non-matching files)
- [x] Parallel discover_files with WalkBuilder::build_parallel()
- [x] Parallel ScannedIndex::build with rayon
- [x] Parallel content search in find command
- [x] Add A/B benchmark tests (criterion groups)
- [x] Run quality gates (fmt, clippy, test)

## Results

On configured vault (~6.5k files, Apple Silicon):
- **Before**: 2.23s (86% CPU, single-threaded)
- **After**: 0.86s (409% CPU, multi-threaded)
- **Speedup**: 2.6x

On MDN en-us vault (14,245 files, 162MB):
- Criterion A/B: parallel vs sequential nearly identical (~2.37s) — I/O-dominated on SSD
- Fast-reject shows no improvement for "XMLHttpRequest" (too many files match)
- Confirms iter-18 finding: rayon helps most when per-file CPU work is significant

Gap vs ripgrep narrowed from 4.5x to 1.7x. Remaining gap is expected: hyalo parses YAML frontmatter + sections + tasks + links on every file while rg only searches text.
