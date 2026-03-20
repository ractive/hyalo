---
title: "Performance: Parallel File Operations"
type: research
date: 2026-03-20
tags:
  - performance
  - parallelization
  - rayon
---

# Performance: Parallel File Operations

## Current State (Iteration 1)

All file operations are **sequential**. Commands like `properties` (no `--path`) and glob-matched `properties` iterate over every `.md` file one at a time:

```
discover_files(dir)  →  Vec<PathBuf>  (sequential walk)
    ↓
for file in files {
    read_frontmatter(file)  →  sequential I/O per file
}
```

For a typical Obsidian vault with 100–500 files, this is fast enough (<100ms). For large vaults (5,000+ files), this becomes a bottleneck — each file requires an `open()` + `read()` syscall, and the CPU sits idle waiting for I/O between files.

## Opportunity: rayon for Parallel File Reads

The `properties_all` aggregation and `properties_glob` paths are embarrassingly parallel — each file read is independent, and the aggregation step is a simple merge.

### Approach

```rust
use rayon::prelude::*;

// Replace sequential loop:
let results: Vec<_> = files
    .par_iter()
    .map(|file| frontmatter::read_frontmatter(file))
    .collect::<Result<Vec<_>>>()?;
```

### Expected Impact

| Vault size | Sequential (est.) | Parallel (est., 8 cores) | Speedup |
|---|---|---|---|
| 100 files | ~20ms | ~5ms | ~4x |
| 1,000 files | ~200ms | ~30ms | ~6x |
| 10,000 files | ~2s | ~300ms | ~6-7x |

The speedup is sub-linear because:
- I/O bandwidth is shared (SSD throughput is the ceiling)
- Thread pool overhead for small workloads
- The aggregation step is sequential

### Implementation Notes

- **Dependency:** `rayon = "1"` (~30s additional compile time, widely used, no transitive bloat)
- **Error handling:** `par_iter().map().collect::<Result<Vec<_>>>()` short-circuits on first error, same as sequential
- **Thread safety:** `read_frontmatter` is already `Send + Sync` — no shared mutable state
- **Aggregation:** The `BTreeMap` merge after parallel reads stays sequential but is CPU-bound and fast
- **Scope:** Only parallelize multi-file read paths (`properties_all`, `properties_glob`). Single-file commands (`property read/set/remove`) don't benefit.

### When to Implement

- Not needed for iteration 1 (correctness and API surface are the priority)
- Consider for iteration 4 (search) where query performance across all files matters
- Definitely needed if/when indexing (iteration plan: "Later") is deferred and full-scan remains the primary access pattern

### Alternative: Indexing

For truly large vaults (50,000+ files), parallelization alone won't suffice. A persistent index (SQLite, see [[iteration-plan]]) that maps properties/tags/links with incremental mtime-based updates would eliminate the need to scan files at all. Parallelization and indexing are complementary — parallel reads for cold cache, index for warm.

## Other Performance Notes

- **Streaming reader is already optimal for single-file reads.** `read_frontmatter` stops at the closing `---` and never reads the body. No further optimization needed there.
- **`discover_files` uses the `ignore` crate** which is already fast (same engine as ripgrep). Parallelizing the walk itself is possible (`WalkBuilder::build_parallel()`) but unlikely to matter until vault sizes exceed 50,000 files.
- **Avoid `String` allocations in hot loops.** The `properties_all` aggregation clones property keys for the `BTreeMap` entry API. With rayon, consider pre-sizing or using a `DashMap` for concurrent insertion, though the sequential merge may be simpler and fast enough.
