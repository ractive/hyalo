---
date: 2026-03-22
status: completed
tags:
- performance
- benchmarking
- testing
- criterion
- hyperfine
title: 'Performance Benchmarking: Tools, Strategy & Test Plan'
type: research
---

# Performance Benchmarking: Tools, Strategy & Test Plan

Research into how to measure, compare, and track hyalo's performance using a real-world Obsidian vault (obsidian-hub, 6,540 files) as the test bed.

## Test Bed: obsidian-hub

Cloned locally. Key characteristics:

| Metric | Value |
|--------|-------|
| Markdown files | 6,540 |
| Total lines | 85,433 |
| Total size | 87 MB |
| Avg lines/file | ~51 |
| Largest file | 2,923 lines |
| YAML frontmatter | 83% of files |
| Wikilinks | 98% of files |
| Tags | 56% of files |
| Directory depth | up to 4 levels |

This vault is large enough to stress-test file discovery, scanning, and aggregation, while being realistic (community-maintained, diverse content).

## Two-Layer Benchmarking Approach

### Layer 1: Criterion.rs — Micro-benchmarks

[Criterion.rs](https://github.com/bheisler/criterion.rs) is the de facto Rust benchmarking framework. It provides:

- **Statistical analysis** — bootstrapped confidence intervals, t-tests against previous runs
- **HTML reports** — auto-generated under `target/criterion/report/index.html` with PDF plots, regression charts, and comparison violin plots
- **Baseline management** — `--save-baseline main` / `--baseline main` for cross-branch comparison
- **Benchmark groups** — compare multiple implementations on the same inputs
- **Scaling analysis** — `BenchmarkGroup::bench_with_input()` for varying input sizes

**Companion tool: [critcmp](https://github.com/BurntSushi/critcmp)** — side-by-side terminal tables comparing two saved baselines with % change.

**Comparison workflow:**
```sh
# Save baseline on main
git checkout main && cargo bench -- --save-baseline main

# Bench on feature branch
git checkout iter-N/feature && cargo bench -- --save-baseline feature

# Compare
critcmp main feature
```

### Layer 2: Hyperfine — End-to-End CLI Benchmarks

[Hyperfine](https://github.com/sharkdp/hyperfine) benchmarks full CLI invocations:

- Auto-determines run count (min 10 runs, min 3 seconds)
- Warmup runs (`-w 3`) to fill filesystem caches
- Exports to **markdown** (`--export-markdown`), JSON, CSV
- Parameterized benchmarks for scaling analysis
- Can compare two binaries side by side

**Example:**
```sh
hyperfine \
  'target/release/hyalo --dir ../obsidian-hub find' \
  'target/release/hyalo --dir ../obsidian-hub find --content obsidian' \
  -w 3 --export-markdown bench-e2e.md
```

### Memory Measurement

- **dhat-rs** — tracks every heap allocation, writes JSON viewable in DHAT viewer. Supports heap-usage assertions in tests.
- **stats_alloc** — lightweight allocator wrapper with `Region` snapshots (allocations, bytes allocated/deallocated)
- **Peak RSS** — `getrusage(RUSAGE_SELF)` on macOS gives `ru_maxrss`

### Alternatives Considered

| Tool | Verdict |
|------|---------|
| **Divan** | Simpler API, but no HTML reports and no baseline comparison — deal-breakers for our comparison use case |
| **cargo bench** | Nightly-only, minimal stats — not suitable |
| **Iai/Cachegrind** | Instruction-count based (deterministic), but Linux-only — doesn't work on macOS |
| **Bencher.dev** | SaaS tracking — overkill for now, revisit if we want CI regression tracking |

## What to Benchmark

### Micro-benchmarks (Criterion)

| Benchmark | Function | Why |
|-----------|----------|-----|
| Frontmatter parse | `read_frontmatter()` | Core hot path — every command reads this |
| Frontmatter write | `write_frontmatter()` round-trip | Write commands mutate many files |
| Wikilink extraction | Link parser on lines with varying link density | 98% of vault uses wikilinks |
| Task detection | `detect_task_checkbox()` | Task commands depend on this |
| Content search | Case-insensitive substring match | `find --content` scans every body line |
| File discovery | `discover_files()` on vault | First step of every command |
| Multi-visitor scan | `scan_file_multi()` with all visitors | The "real" per-file cost |
| Property filtering | Filter evaluation on varying frontmatter | Complex filter stacks |
| YAML serialization | `BTreeMap` → YAML string | Bottleneck for bulk writes |
| Inline stripping | `strip_inline_code()` + `strip_inline_comments()` | Zero-copy vs allocation path |

**Scaling dimensions:**
- File count: 100, 500, 1000, 3000, 6540 files (subsets of obsidian-hub)
- File size: small (10 lines), medium (100 lines), large (1000+ lines)
- Frontmatter complexity: 2 props vs 10 props vs 20 props

### End-to-end benchmarks (Hyperfine)

| Benchmark | Command | What it reveals |
|-----------|---------|-----------------|
| Full vault scan | `find` | Baseline wall-clock for 6,540 files |
| Filtered find | `find --tag X --property K=V` | Filter evaluation cost at scale |
| Content search | `find --content "obsidian"` | Full-text search performance |
| Properties agg | `properties` | Aggregation over entire vault |
| Tags agg | `tags` | Similar aggregation path |
| Summary | `summary` | Combined aggregation |
| JSON vs text | `--format json` vs `--format text` | Output formatting overhead |
| jq filtering | `find --jq '.[] \| .title'` | jq evaluation overhead |

### Memory benchmarks

- Peak heap during full vault `find` (dhat-rs)
- Allocations-per-file during scan (stats_alloc regions)
- Peak RSS via `getrusage` wrapper

## Best Practices

- **Always `black_box()`** inputs and outputs to prevent dead code elimination
- **`codegen-units = 1` and `lto = true`** in bench profile for reproducible codegen
- **Warm filesystem caches** — use `-w 3` in hyperfine; criterion has built-in warmup
- **Fixed input data** — use obsidian-hub at a pinned commit, or generate deterministic fixtures
- **obsidian-hub must be read-only** for benchmarks — use a copy or git-reset for write tests
- **Separate bench profile** in Cargo.toml for consistent optimization levels

## Report Strategy

1. **Criterion** auto-generates HTML under `target/criterion/report/` — commit-free, inspect locally
2. **Hyperfine** exports markdown tables — save to `hyalo-knowledgebase/research/` for tracking
3. **Custom bench runner** (shell script) runs both layers and collects into a single markdown report
4. **Before/after comparisons** via `critcmp` and hyperfine side-by-side for each optimization iteration

## Related

- [[performance-parallelization]] — rayon opportunities identified earlier
- [[iteration-plan]] — indexing as the long-term alternative to full scans
