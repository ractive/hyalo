---
title: "Parallel file operations with rayon"
type: backlog
date: 2026-03-21
status: shelved
priority: low
origin: research/performance-parallelization.md
tags:
  - backlog
  - performance
  - rayon
---

# Parallel file operations with rayon

## Problem

All multi-file operations (properties summary, tags summary, outline vault-wide) iterate sequentially. On large vaults (1000+ files), this becomes a bottleneck — each file requires a filesystem read and YAML parse.

## Proposal

Use `rayon` for parallel reads on multi-file paths. Research in [[research/performance-parallelization]] suggests 4-7x speedup on multi-core machines.

Scope:
- Parallelize `collect_files()` consumers: properties summary/list, tags summary/list, outline vault-wide
- Single-file commands don't benefit
- Mutation commands (property set, tag add) should remain sequential for safety

## Trigger

Revisit when users report latency on vaults >20,000 files, or when per-file processing becomes heavier (indexing, link graph). For current vault sizes, sequential processing is fast enough.

## Iteration 18 outcome

Implemented and benchmarked — only 1-2x speedup on 6,540 files (I/O-dominated workload on SSD). Shelved due to poor complexity-to-gain ratio. Branch `iter-18/parallel-processing` has the full working implementation. See [[iterations/done/iteration-18-parallel-processing]] for details.

## References

- [[research/performance-parallelization]]: full research with benchmark results
- [[iterations/done/iteration-18-parallel-processing]]: iteration details and decision rationale
