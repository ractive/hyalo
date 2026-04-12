---
title: "Benchmark: Iteration 101 — BM25 Ranked Search"
type: research
date: 2026-04-10
tags:
  - performance
  - bm25
  - search
  - benchmarking
related:
  - "[[iterations/iteration-101-bm25-ranked-search]]"
---

# Benchmark: Iteration 101 — BM25 Ranked Search

Benchmarks comparing v0.10.0 (main, substring search) vs iter-101 (BM25 ranked search).

**Vault:** MDN Web Docs (`../mdn/files/en-us`) — 14,245 markdown files.
**Machine:** macOS (Apple Silicon), release build.
**Date:** 2026-04-10

## Binary Size

| Version | Size | Delta |
|---------|------|-------|
| v0.10.0 (main) | 4.71 MB (4,939,376 bytes) | — |
| iter-101 (BM25) | 5.03 MB (5,270,000 bytes) | +330 KB (+6.7%) |

New dependencies: `bm25` (2.3.2), `rust-stemmers` (1.2.0).

## Index Size and Generation Time

| Version | Index Size | Generation Time |
|---------|-----------|-----------------|
| v0.10.0 (main) | 27 MB (28,073,734 bytes) | 0.77s |
| iter-101 (BM25) | 68 MB (71,655,656 bytes) | 1.44s |
| Delta | +155% | +87% |

The increase is due to pre-tokenized BM25 data (stemmed token lists per document) stored in the MessagePack snapshot index.

## E2E Command Benchmarks (A/B, no index)

Compared using `bench-e2e.sh` with `hyperfine` (10 runs, 3 warmup).

`current` = iter-101 (BM25), `baseline` = v0.10.0 (main).

### find (no pattern, metadata only)

| Command | Mean | Relative |
|---------|------|----------|
| current | 957.6 ± 15.5 ms | **1.00** |
| baseline | 1006.7 ± 63.3 ms | 1.05 |

No regression. Slightly faster due to `bm25_tokenize: false` on live scan path.

### find-pattern (`find obsidian`)

| Command | Mean | Relative |
|---------|------|----------|
| current | 4.106 ± 0.076 s | 5.17 |
| baseline | 0.794 ± 0.028 s | **1.00** |

**Expected regression.** BM25 reads all 14K file bodies and tokenizes them (O(n) full corpus). Old substring search used fast memchr-based scanning. This is the tradeoff for relevance-ranked results. With an index, this drops to ~2.3s (see below).

### find-property (`find --property title`)

| Command | Mean | Relative |
|---------|------|----------|
| current | 966.2 ± 10.7 ms | **1.00** |
| baseline | 1000.9 ± 110.1 ms | 1.04 |

No regression.

### find-task (`find --task todo`)

| Command | Mean | Relative |
|---------|------|----------|
| current | 530.1 ± 37.4 ms | 1.03 |
| baseline | 515.1 ± 16.9 ms | **1.00** |

No regression (within noise).

### properties

| Command | Mean | Relative |
|---------|------|----------|
| current | 492.7 ± 49.6 ms | **1.00** |
| baseline | 500.0 ± 54.6 ms | 1.01 |

No regression.

### tags

| Command | Mean | Relative |
|---------|------|----------|
| current | 475.0 ± 9.7 ms | **1.00** |
| baseline | 499.5 ± 47.3 ms | 1.05 |

No regression.

### summary

| Command | Mean | Relative |
|---------|------|----------|
| current | 721.3 ± 8.8 ms | 1.00 |
| baseline | 718.4 ± 9.3 ms | **1.00** |

No regression.

### find-json (`find --format json`)

| Command | Mean | Relative |
|---------|------|----------|
| current | 1.004 ± 0.045 s | **1.00** |
| baseline | 1.025 ± 0.066 s | 1.02 |

No regression.

### find-text (`find --format text`)

| Command | Mean | Relative |
|---------|------|----------|
| current | 2.180 ± 0.063 s | 1.01 |
| baseline | 2.151 ± 0.043 s | **1.00** |

No regression.

## BM25-Specific Benchmarks

### Indexed vs non-indexed BM25 (`find 'javascript promises'`)

| Mode               | Mean            | Relative |
| ------------------ | --------------- | -------- |
| BM25 with index    | 2.288 ± 0.024 s | **1.00** |
| BM25 without index | 4.033 ± 0.121 s | 1.76     |

Index provides **1.76x speedup** by avoiding disk I/O and re-tokenization.

### BM25 indexed vs old substring (`find 'javascript'`)

| Mode | Mean | Relative |
|------|------|----------|
| Old substring (v0.10.0) | 818.8 ± 14.3 ms | **1.00** |
| BM25 with index (iter-101) | 2.264 ± 0.027 s | 2.77 |

BM25 is ~2.8x slower than old substring search. This is the cost of relevance ranking — the BM25 library must score all 14K documents against the query embedding. The old substring search only needed to find the first match per file.

### Regex search (unchanged path)

| Mode | Mean | Relative |
|------|------|----------|
| regex (iter-101) | 1.365 ± 0.030 s | 1.36 |
| regex (v0.10.0) | 1.002 ± 0.011 s | **1.00** |

Small regression on regex path — investigated and found to be caused by initial `bm25_tokenize` overhead during live scan. Fixed by adding `bm25_tokenize: false` flag (see "Perf fix" below).

## Perf Fix: `bm25_tokenize` Flag

Initial implementation had a **1.7x regression on all scan_body commands** because `scan_one_file` re-read every file for BM25 tokenization even during live queries. Fixed by adding `bm25_tokenize: bool` to `ScanOptions`:

- `bm25_tokenize: true` only during `create-index`
- `bm25_tokenize: false` for all live scan paths

After fix: all non-BM25 commands at parity with baseline.

## Iteration 101b — Serializable BM25 Inverted Index

**Date:** 2026-04-12

Replaced the external `bm25` crate with a custom serializable `Bm25InvertedIndex`. The inverted index is persisted in the snapshot, eliminating corpus rebuild at query time.

### Binary Size

| Version | Size | Delta vs main |
|---------|------|---------------|
| v0.10.0 (main) | 4.71 MB | — |
| iter-101a (bm25 crate) | 5.03 MB | +6.7% |
| iter-101b (inverted idx) | 5.05 MB | +7.2% |

Minimal binary size change (+33 KB over 101a). The `bm25` crate was removed; `rust-stemmers` remains.

### Index Size and Generation Time

| Version | Index Size | Generation Time |
|---------|-----------|-----------------|
| iter-101a (bm25 crate) | 68 MB | 1.42s |
| iter-101b (inverted idx) | 86 MB | 2.41s |
| Delta | +26% | +70% |

Index grew by 18 MB due to the persisted inverted index (postings, doc_lengths, doc_token_sets, avgdl). Per-entry `bm25_tokens` are stripped when the inverted index is present to avoid duplication. Without stripping, the index was 134 MB.

Generation time increased because building the inverted index requires an extra pass over all tokenized documents.

### BM25 Indexed Query Performance (the main win)

`find 'javascript promises' --index ... --limit 10` on MDN (14,245 docs):

| Version | Mean | Speedup vs 101a |
|---------|------|-----------------|
| iter-101a (bm25 crate) | 2,423 ms | 1.00x |
| **iter-101b (inverted idx)** | **371 ms** | **6.5x** |

The persisted inverted index eliminates all corpus rebuild overhead. Query is now just: deserialize index, hash-lookup postings, compute BM25 scores, sort.

### BM25 Indexed vs Main Substring

`find 'javascript' --limit 10` on MDN:

| Version | Mean | Relative |
|---------|------|----------|
| main (substring, no index) | 854 ms | 1.87x |
| **101b (BM25, indexed)** | **458 ms** | **1.00x** |

BM25 indexed search is now **faster than main's substring search** despite doing relevance ranking, stemming, and scoring. The index avoids all disk I/O while substring search must scan 14K files from disk.

### BM25 Non-Indexed (live scan fallback)

`find 'javascript' --dir ... --limit 10` (no `--index`):

| Version | Mean |
|---------|------|
| iter-101b | 3,271 ms |

The non-indexed path builds the inverted index in memory from disk reads. Slower than indexed but still functional as a fallback.

### Non-Search Regression Check

`find --property title --limit 10` (metadata only, no BM25):

| Version | Mean | Relative |
|---------|------|----------|
| main (v0.10.0) | 496 ms | **1.00** |
| iter-101b | 503 ms | 1.01 |

No regression on non-search commands.

## Key Takeaways

1. **No regression on non-search commands** — metadata queries, properties, tags, summary all unchanged.
2. **BM25 body search is slower than substring** but provides relevance-ranked results with stemming.
3. **Index provides 1.76x speedup** for BM25 queries by caching pre-tokenized data.
4. **Index is 2.5x larger** due to stored BM25 tokens — acceptable tradeoff for query-time performance.
5. **Binary grew 6.7%** from `bm25` and `rust-stemmers` crates.
6. **101b: Serializable inverted index achieves 6.5x speedup** over 101a for indexed BM25 queries (2.4s → 371ms).
7. **101b: BM25 indexed is now faster than main's substring search** (371ms vs 854ms).
8. **101b: Index grew 26%** (68MB → 86MB) due to persisted inverted index, but per-entry tokens are stripped to limit bloat.
9. **101b: `bm25` crate dependency removed** — replaced with ~200 lines of custom BM25 scoring code.
