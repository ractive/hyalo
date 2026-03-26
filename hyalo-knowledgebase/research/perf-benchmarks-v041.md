---
date: 2026-03-26
status: completed
tags:
- dogfooding
- v0.4.1
- external-docs
title: 'Performance Benchmarks: hyalo v0.4.1'
type: research
---

# Performance Benchmarks: hyalo v0.4.1

Tested on macOS (Darwin 25.3.0, Apple Silicon). Each command run 3 times; median reported.

## GitHub Docs (3521 markdown files)

### Basic Commands

| Command | Run 1 | Run 2 | Run 3 | Median | v0.4.0 |
|---------|-------|-------|-------|--------|--------|
| `summary` | 0.213s | 0.217s | 0.233s | **0.217s** | 0.25s |
| `find` (default) | 0.367s | 0.368s | 0.372s | **0.368s** | — |
| `properties summary` | 0.155s | 0.158s | 0.181s | **0.158s** | — |
| `tags summary` | 0.152s | 0.163s | 0.181s | **0.163s** | — |

Summary is ~13% faster than v0.4.0 baseline (0.217s vs 0.25s).

### Fields Comparison (Index Overhead)

| Command | Median | Notes |
|---------|--------|-------|
| `find --fields properties` | **0.175s** | v0.4.0: 0.20s — 12% faster |
| `find --fields links` | **0.348s** | Link extraction only |
| `find --fields backlinks` | **0.296s** | Requires full index build |
| `find --fields links,backlinks` | **0.494s** | v0.4.0: 0.56s — 12% faster |
| `find --fields properties,links,backlinks` | **0.512s** | v0.4.0: 0.58s — 12% faster |

Key finding: requesting `backlinks` alone (0.296s) is faster than `links` alone (0.348s). The `links,backlinks` combo (0.494s) is close to `links` + `backlinks` individually, suggesting serial processing rather than sharing the index scan.

### Summary --depth

| Command | Median |
|---------|--------|
| `summary --depth 1` | **0.216s** |
| `summary --depth 2` | **0.213s** |
| `summary --depth 3` | **0.215s** |

No measurable cost difference across depth levels. All within noise of plain `summary` (0.217s).

### Backlinks Command

| Command | Median | Notes |
|---------|--------|-------|
| `backlinks --file graphql/reference/objects.md` (42 inbound) | **0.156s** | v0.4.0: 0.18s — 13% faster |

### Filtered Queries

| Filter | Median | Result Count |
|--------|--------|--------------|
| `find --property type` (existence) | **0.250s** | 0 |
| `find --property layout=product-landing` | **0.223s** | 3 |
| `find --glob 'graphql/**'` | **0.041s** | 27 |
| `find 'repository'` (body text) | **0.362s** | 1609 |
| `find 'authentication'` (body text) | **0.304s** | 494 |
| `find 'pull request'` (body text) | **0.312s** | 654 |
| `find -e 'deploy.*kubernetes'` (regex) | **0.240s** | 6 |
| `find -e 'GitHub Actions'` (regex) | **0.246s** | 54 |

Key finding: `--glob` filter is extremely fast (0.041s) because it prunes files before parsing. Property filters and text search both require parsing all files (~0.22-0.36s).

### --limit Performance

| Command | Median |
|---------|--------|
| `find --limit 10` | **0.366s** |
| `find --limit 100` | **0.358s** |
| `find 'repository' --limit 10` | **0.347s** |
| `find 'repository'` (no limit) | **0.365s** |

**`--limit` does NOT provide early termination.** All files are still parsed and scanned; only the output is truncated. This is a potential optimization opportunity: for `find --limit N` without sort, we could stop after N matches.

### rg Comparison (Text Search)

| Search Term | hyalo | rg | Ratio | Notes |
|-------------|-------|-----|-------|-------|
| `repository` | 0.362s | 0.050s | **7.2x** | hyalo=1609, rg=same (file match) |
| `authentication` | 0.304s | 0.047s | **6.5x** | hyalo=494, rg=same |
| `pull request` | 0.312s | 0.047s | **6.6x** | hyalo=654, rg=same |
| `deploy.*kubernetes` (regex) | 0.240s | 0.047s | **5.1x** | hyalo=6, rg=6 |
| `GitHub Actions` | 0.246s | 0.046s | **5.3x** | hyalo=54, rg=142 (rg includes frontmatter) |

hyalo is **5-7x slower** than rg for raw text search (regression from 4-5x in v0.4.0). This is expected: hyalo parses frontmatter, extracts structure, and only searches body text. The rg count difference for "GitHub Actions" (54 vs 142) confirms hyalo correctly excludes frontmatter from body search.

## VS Code Docs (339 markdown files)

### Basic Commands

| Command | Median |
|---------|--------|
| `summary` | **0.046s** |
| `find` (default) | **0.082s** |
| `properties summary` | — (not tested separately) |
| `tags summary` | — (not tested separately) |

### Fields Comparison

| Command | Median |
|---------|--------|
| `find --fields properties` | **0.036s** |
| `find --fields links` | **0.078s** |
| `find --fields links,backlinks` | **0.109s** |

Index overhead for backlinks: +0.031s (40% increase over links-only).

### Backlinks

| Command | Median |
|---------|--------|
| `backlinks --file configure/settings.md` (116 inbound) | **0.041s** |
| `backlinks --file debugtest/debugging.md` (70 inbound) | **0.041s** |

### Filtered Queries

| Filter | Median |
|--------|--------|
| `find --property Order` | **0.050s** |
| `find --glob 'copilot/**'` | **0.035s** |
| `find 'extension'` | **0.084s** |
| `find 'extension' --limit 10` | **0.079s** |

### Summary --depth

| Command | Median |
|---------|--------|
| `summary --depth 1` | **0.044s** |
| `summary --depth 2` | **0.044s** |
| `summary --depth 3` | **0.044s** |

No cost difference across depth levels.

## Summary of Findings

### Improvements vs v0.4.0
- **~12-13% faster** across all comparable benchmarks (summary, find, backlinks)
- Rayon parallelization from iter-44 is delivering consistent gains

### Issues Found
1. **`--limit` does not enable early termination**: `find --limit 10` takes the same time as a full scan. For large repos, this is a missed optimization — we could short-circuit after N matches when no sort is applied.
2. **rg gap widened slightly**: 5-7x slower (was 4-5x). Likely within measurement noise, but worth monitoring.
3. **`--content` flag does not exist**: The positional PATTERN argument is the correct way to do body text search. Documentation could be clearer about this.
4. **`properties` and `tags` now require subcommands**: Changed from v0.4.0 — need `properties summary` and `tags summary` instead of bare `properties`/`tags`.

### Scaling Observations
- GitHub Docs (3521 files): ~0.2-0.5s for most operations
- VS Code Docs (339 files): ~0.04-0.11s for most operations
- Roughly linear scaling: 10x files = ~4-5x time (sublinear due to parallelism)
- `--glob` filter is the fastest path — prunes before parse (0.041s for 27/3521 files)

### Optimization Opportunities
1. **Early termination for `--limit`** without sort — stop scanning after N matches
2. **Incremental index** — cache link index across invocations for backlinks
3. **Parallel text search** — the rg gap suggests the text matching loop could benefit from further parallelization or memory-mapped I/O
