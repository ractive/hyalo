---
title: "Parallel file operations with rayon"
type: backlog
date: 2026-03-21
status: ready
priority: medium
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

Implement when benchmarks on a real vault show latency above ~200ms for vault-wide commands. Not needed for small vaults (<100 files).

## References

- [[research/performance-parallelization]]: full research with benchmark methodology
