---
title: "Iteration 15: Performance Benchmark Suite"
type: iteration
date: 2026-03-22
tags:
  - iteration
  - performance
  - benchmarking
  - criterion
  - hyperfine
status: completed
branch: iter-15/performance-benchmarks
---

# Iteration 15: Performance Benchmark Suite

Establish a benchmark infrastructure for hyalo using the obsidian-hub vault (6,540 files) as the real-world test bed. This enables data-driven optimization decisions for future iterations (parallelization, indexing, caching).

See [[performance-benchmarking]] for full research and rationale.

## Goals

- Criterion micro-benchmarks for hot-path functions
- Hyperfine end-to-end CLI benchmarks against obsidian-hub
- A/B comparison workflow for measuring optimizations
- Baseline results documented for future comparison

## Tasks

### Setup

- [x] Add `criterion` as a dev-dependency with `html_reports` feature
- [x] Add bench profile to `Cargo.toml` (`codegen-units = 1`, `lto = true`)
- [x] Create `benches/` directory structure
- [x] Make `extract_links_from_text` public for direct benchmarking

### Micro-benchmarks (Criterion) — `benches/micro.rs`

- [x] Benchmark `strip_inline_code()` — zero-copy vs allocation path
- [x] Benchmark `strip_inline_comments()` — zero-copy vs allocation path
- [x] Benchmark `detect_task_checkbox()` — task line, non-task, indented
- [x] Benchmark `extract_links_from_text()` — sparse vs dense links
- [x] Benchmark `parse_property_filter()` — equality, comparison, exists

### Vault benchmarks (Criterion) — `benches/vault.rs`

- [x] Benchmark `discover_files()` on obsidian-hub
- [x] Benchmark `read_frontmatter()` on sample files
- [x] Benchmark `read_all_frontmatter()` on full vault
- [x] Benchmark `scan_file_multi()` with TaskCounter + ContentSearchVisitor

### End-to-end benchmarks (Hyperfine) — `bench-e2e.sh`

- [x] Write `bench-e2e.sh` with A/B comparison support
- [x] Benchmark: `find` (full vault scan)
- [x] Benchmark: `find <pattern>` (content search)
- [x] Benchmark: `find --property`, `find --task` (filtered)
- [x] Benchmark: `properties`, `tags`, `summary` (aggregation)
- [x] Benchmark: `--format json` vs `--format text` (output overhead)
- [x] Export results as markdown to `bench-results/`

### Documentation — `benches/README.md`

- [x] Document prerequisites (critcmp, hyperfine)
- [x] Document micro and vault benchmark usage
- [x] Document A/B comparison workflow (criterion baselines, hyperfine side-by-side, git worktrees)
- [x] Document memory measurement (macOS `time -l`, Linux `/usr/bin/time -v`)

### jaq filter caching — A/B test case

- [x] Save baseline binary before optimization
- [x] Implement `JaqFilterCache` in `output.rs` — `HashMap<String, Filter<Native<Val>>>`
- [x] Thread cache through `format_value_as_text` → `apply_jq_filter`
- [x] Measure improvement: `find --format text` 2.82x faster (2.91s → 1.03s on 6540 files)
- [x] Document results in knowledgebase

## Acceptance criteria

- [x] `cargo bench --bench micro` runs and generates HTML reports
- [x] `cargo bench --bench vault` runs vault benchmarks (skips gracefully if vault absent)
- [x] `./bench-e2e.sh` runs hyperfine benchmarks and exports markdown
- [x] A/B comparison demonstrated with jaq caching optimization
- [x] Results documented in knowledgebase

## Deliberately excluded

- **dhat / memory profiling** — `time -l` documented in README instead
- **Scaling groups** — `discover_files()` always walks full vault; slicing adds complexity without value
- **`write_frontmatter` benchmarks** — writes are rare; would need tempdir setup
- **CI integration** — on-demand only for now

## Results: Initial Baseline (2026-03-22)

### E2E (obsidian-hub, 6540 files, with jaq cache)

| Command | Mean | σ |
|---------|------|---|
| `find` | 454ms | ±5ms |
| `find obsidian` (pattern) | 512ms | ±14ms |
| `find --property title` | 284ms | ±2ms |
| `find --task todo` | 295ms | ±7ms |
| `properties` | 55ms | ±9ms |
| `tags` | 51ms | ±2ms |
| `summary` | 227ms | ±3ms |
| `find --format json` | 452ms | ±4ms |
| `find --format text` | 1.043s | ±17ms |

### jaq cache A/B comparison

| Metric | Before | After | Speedup |
|--------|--------|-------|---------|
| `find --format text` | 2.91s | 1.03s | **2.82x** |
| `find --format json` | 451ms | 452ms | 1.00x (no change) |

The text/json gap narrowed from 6.4x to 2.3x. The remaining ~580ms delta is jaq filter *execution* overhead (running the compiled filter per value), not compilation.
