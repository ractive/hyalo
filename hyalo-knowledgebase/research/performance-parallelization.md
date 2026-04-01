---
date: 2026-03-22
status: completed
tags:
- performance
- parallelization
- rayon
- research
title: 'Performance: Parallel File Operations'
type: research
---

# Performance: Parallel File Operations

## Current State

All file operations are **sequential**. Commands like `properties`, `tags`, `find`, and `summary` iterate over every `.md` file one at a time:

```
discover_files(dir)  →  Vec<PathBuf>  (sequential walk)
    ↓
for file in files {
    read_frontmatter(file)  or  scan_file_multi(file)  →  sequential I/O per file
}
```

For a typical Obsidian vault with 100-500 files, this is fast enough (<50ms). For large vaults (5,000+ files), latency reaches 100-200ms.

## Experiment: rayon `par_iter()` (Iteration 18)

Full implementation on branch `iter-18/parallel-processing`. Replaced sequential for-loops with `par_iter().map().collect()` in all four read-only multi-file commands.

### Approach

Created a shared utility `par_process_files<T, F>`:

```rust
pub enum FileResult<T> { Ok(T), Skipped }

pub fn par_process_files<T, F>(files: &[(PathBuf, String)], f: F) -> Result<Vec<T>>
where
    T: Send,
    F: Fn(&Path, &str) -> Result<FileResult<T>> + Sync,
```

Each command provides a closure for per-file work. Errors propagated after parallel phase; skipped files (malformed YAML) emit warnings via `eprintln!`.

### Actual Benchmark Results (obsidian-hub, 6,540 files, Apple Silicon SSD)

| Command | Sequential | Parallel (rayon) | Speedup |
|---------|-----------|-----------------|---------|
| `find` (all) | ~195ms | 185ms | **1.05x** |
| `find` (content) | ~215ms | 207ms | **1.04x** |
| `properties` | ~135ms | 95ms | **1.4x** |
| `tags` | ~135ms | 98ms | **1.4x** |
| `summary` | ~195ms | 98ms | **2.0x** |

### Why the Estimates Were Wrong

Original estimates predicted 4-7x speedup. The actual speedup was 1-2x because:

1. **I/O-dominated, not CPU-dominated**: frontmatter parsing (`serde_yaml_ng`) is fast (~15µs per file). The bottleneck is `open()` + `read()` syscalls, which the OS already pipelines via readahead on SSDs
2. **Warm page cache**: on repeated runs (and during normal use), files are already in the kernel page cache. The kernel serializes physical I/O regardless of thread count
3. **Thread pool overhead**: rayon's work-stealing pool has non-trivial per-task overhead that offsets gains on cheap tasks
4. **Small per-file work**: each file takes ~20-30µs to process. At this granularity, scheduling overhead is a significant fraction of useful work

### Conclusion

**Not worth the complexity for current vault sizes.** The added dependency (rayon + 4 transitive crates), new abstraction, and closure-based control flow don't justify 1-2x speedup when commands already complete in <200ms.

### When to Revisit

- If users report latency on vaults >20,000 files
- If per-file processing becomes heavier (e.g., full-text indexing, link graph construction)
- If targeting cold cache scenarios (first run after reboot) where I/O parallelism helps more

### Better Alternative: Indexing

For truly large vaults, a persistent index (SQLite with mtime-based invalidation) would eliminate scanning entirely. This is fundamentally better than parallelizing the scan — O(1) lookups vs O(n) parallel reads. Parallelization and indexing are complementary: parallel reads for cold index rebuilds, index for warm queries.

## Iteration 86: memchr + rayon + parallel walk (2026-03-30)

Revisited parallelism with a broader approach combining memchr, rayon, and parallel walk. See [[iterations/done/iteration-86-high-perf-scanning]].

### Changes
- `memchr::memmem::Finder` for SIMD-accelerated substring search (replaced naive sliding window)
- `scan_slice_multi`: zero-copy line splitting via `memchr::memchr_iter` on `&[u8]` buffer
- `WalkBuilder::build_parallel()` for parallel directory traversal
- `rayon::par_iter` in `ScannedIndex::build` for parallel file scanning
- Fast-reject: skip files that cannot match before full scanner parse

### Results (configured vault, ~6.5k files, Apple Silicon)
| Metric | Before | After |
|--------|--------|-------|
| Wall time | 2.23s | 0.86s |
| CPU usage | 86% | 409% |
| Speedup | — | **2.6x** |

### Key insight
The combination works where rayon alone didn't (iter-18 gave 1.05x) because:
1. `scan_slice_multi` replaced `BufRead::read_line` with zero-copy memchr splitting — less per-file overhead makes parallelism profitable
2. The vault is large enough (~6.5k files) that rayon's scheduling overhead is amortized
3. Parallel walk + parallel scan compound

### Remaining bottleneck
`hyalo find "pattern"` still builds a full 4-visitor index (`scan_body: true`) even though content search only needs frontmatter + body text. This redundant work is ~50% of wall time. Tracked in [[backlog/done/skip-body-index-for-content-search]].

## Other Performance Notes

- **Streaming reader is already optimal for single-file reads.** `read_frontmatter` stops at the closing `---` and never reads the body
- **`discover_files` uses the `ignore` crate** (same engine as ripgrep). Parallelizing the walk (`WalkBuilder::build_parallel()`) is unlikely to matter for <50,000 files
- **Sequential baselines are already fast**: `properties`/`tags` complete in ~55ms on 6,540 files, `find`/`summary` in ~200ms
