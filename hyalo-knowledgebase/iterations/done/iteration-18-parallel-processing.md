---
title: "Iteration 18: Parallel Processing (shelved)"
type: iteration
date: 2026-03-22
tags:
  - iteration
  - performance
  - parallelization
  - rayon
status: shelved
branch: iter-18/parallel-processing
---

# Iteration 18: Parallel Processing (shelved)

Attempted to add parallel file processing to all read-only multi-file commands using rayon `par_iter()`. Implementation was completed and all tests passed, but benchmarks showed the complexity-to-gain ratio was not worth it. Branch left open for future reference.

## What was built

- `rayon = "1"` dependency
- Shared `par_process_files<T, F>` utility in `src/parallel.rs` with `FileResult::Ok(T)` / `FileResult::Skipped` enum
- Parallelized `find`, `properties`, `tags`, `summary` commands using the shared utility
- All 593 tests passing, clippy clean

## Benchmark results (obsidian-hub, 6,540 files, Apple Silicon, SSD)

| Command | Sequential | Parallel | Speedup |
|---------|-----------|----------|---------|
| `find` (all) | ~195ms | 185ms | 1.05x |
| `find` (content search) | ~215ms | 207ms | 1.04x |
| `properties` | ~135ms | 95ms | 1.4x |
| `tags` | ~135ms | 98ms | 1.4x |
| `summary` | ~195ms | 98ms | 2.0x |

## Why shelved

- **Modest speedup**: 1-2x on a 6,540-file vault, not the 4-6x estimated from napkin math
- **I/O-dominated workload**: on SSD with warm page cache, the OS readahead already pipelines I/O; rayon adds thread pool overhead that eats into gains
- **Added complexity**: new dependency (rayon + 4 transitive crates), new module, new abstraction (`FileResult` enum), closures replacing straightforward for-loops in every command
- **Small vaults unaffected**: for typical vaults (<1,000 files), commands already complete in <50ms — parallelism adds overhead for no perceptible benefit

## Learnings

- Embarrassingly parallel ≠ automatically fast. When the bottleneck is filesystem I/O on a fast SSD, thread-level parallelism has diminishing returns because the OS already does readahead and the kernel serializes I/O anyway
- The estimated speedups from [[research/performance-parallelization]] were overly optimistic — they assumed CPU-bound work, but frontmatter parsing is cheap relative to `open()` + `read()` syscalls
- For real gains on large vaults, an **index** (SQLite with mtime-based invalidation) would eliminate scanning entirely, which is a fundamentally different approach than parallelizing the scan
- If revisiting: only parallelize `find` and `summary` (the two commands with meaningful gains), skip `properties`/`tags` (already fast enough)

## Tasks

- [x] Add rayon dependency to Cargo.toml
- [x] Create shared `par_process_files` utility in `src/parallel.rs`
- [x] Parallelize `find`, `properties`, `tags`, `summary` commands
- [x] All quality gates pass (fmt, clippy, tests)
- [x] Dogfood with hyalo-knowledgebase
- [x] Run benchmarks against obsidian-hub — results underwhelming
- [x] Decision: shelve, keep branch for reference
