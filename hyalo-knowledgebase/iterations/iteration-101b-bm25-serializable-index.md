---
title: Iteration 101b — Serializable BM25 Inverted Index
type: iteration
date: 2026-04-12
tags:
  - iteration
  - search
  - bm25
  - performance
  - serialization
status: completed
branch: iter-101/bm25-index-integration
related:
  - "[[iterations/iteration-101-bm25-ranked-search]]"
  - "[[research/bm25-index-persistence]]"
---

# Iteration 101b — Serializable BM25 Inverted Index

## Goal

Replace the external `bm25` crate with a custom serializable BM25 inverted index. Query time on 14K docs should drop from ~2s to <100ms by persisting the scoring structures in the snapshot index instead of rebuilding them on every query.

## Problem

The `bm25` crate's `Scorer` and `Embedder` are in-memory-only — private fields, no serde support, generic type parameters that prevent serialization. Even with pre-tokenized data cached in the index, every query rebuilds the full BM25 inverted index from scratch: joining tokens back into strings, re-embedding 14K documents, building hash maps. This takes ~2s on MDN (14K docs), making the index nearly useless for search performance.

## Design

### Data Structure

```rust
#[derive(Serialize, Deserialize)]
struct Posting {
    doc_id: u32,
    term_freq: u32,
}

#[derive(Serialize, Deserialize)]
pub struct Bm25InvertedIndex {
    /// Term → list of (doc_id, term_frequency) postings, sorted by doc_id.
    postings: HashMap<String, Vec<Posting>>,
    /// Number of tokens in each document (indexed by doc_id).
    doc_lengths: Vec<u32>,
    /// doc_id → relative path mapping.
    doc_paths: Vec<String>,
    /// Per-document token sets for negation query support.
    doc_token_sets: Vec<HashSet<String>>,
    /// Average document length (pre-computed).
    avgdl: f32,
}
```

### BM25 Scoring (at query time)

```
score(q, d) = Σ IDF(t) × (tf(t,d) × (k1 + 1)) / (tf(t,d) + k1 × (1 - b + b × |d|/avgdl))

where:
  IDF(t) = ln(1 + (N - n(t) + 0.5) / (n(t) + 0.5))
  k1 = 1.2, b = 0.75 (standard Okapi BM25 parameters)
```

### Query Flow (indexed path)

1. Deserialize `Bm25InvertedIndex` from snapshot (stored alongside existing index data)
2. Tokenize + stem query terms (~microseconds)
3. For each query term, look up postings list → O(query_terms)
4. Compute BM25 score per candidate doc → O(matching_docs)
5. Sort by score, apply limit

No corpus rebuild. No re-embedding. Just hash lookups + arithmetic.

### Build Flow (create-index)

1. Tokenize each document (same as now — Snowball stemming per language)
2. Build postings map: for each token in each doc, append (doc_id, tf)
3. Compute doc_lengths and avgdl
4. Serialize `Bm25InvertedIndex` into the snapshot alongside existing `IndexEntry` data

### What Changes

- Remove `bm25` crate dependency (and transitive `fxhash`, `byteorder`)
- Replace `Bm25Corpus` (wraps bm25 crate) with `Bm25InvertedIndex` (self-contained)
- `SnapshotIndex` gains an optional `bm25_index: Option<Bm25InvertedIndex>` field
- `IndexEntry.bm25_tokens` stays for the non-indexed live scan path (fallback)
- `find` indexed path: deserialize → query → score (no corpus rebuild)
- `find` non-indexed path: tokenize docs → build `Bm25InvertedIndex` in memory → query

### What Stays the Same

- All tokenization code (`tokenize()`, `StemLanguage`, `resolve_language`)
- Language precedence logic
- Negation query syntax (`-term`)
- `--language` flag and `[search].language` config
- All CLI args, output format, `score` field
- Regex search path (unchanged)

## Tasks

- [x] Design `Bm25InvertedIndex` struct with serde derives
- [x] Implement `Bm25InvertedIndex::build(docs)` from tokenized documents
- [x] Implement `Bm25InvertedIndex::search(query, stemmer, limit)` with BM25 scoring
- [x] Implement negation support using `doc_token_sets`
- [x] Store `Bm25InvertedIndex` in `SnapshotIndex` (new optional field)
- [x] Update `create-index` to build and persist the inverted index
- [x] Update `find` indexed path to use persisted index (no corpus rebuild)
- [x] Update `find` non-indexed path to build index in memory then query
- [x] Remove `bm25` crate dependency from Cargo.toml
- [x] Remove `Bm25Corpus`, `WhitespaceTokenizer`, `Embedder`/`Scorer` wrappers
- [x] Update unit tests for new `Bm25InvertedIndex` API
- [x] Update e2e tests (behavior should be identical)
- [x] Benchmark: query time with index on MDN (target: <100ms)
- [x] Benchmark: index size change (should shrink — no raw token lists needed if inverted index is stored)
- [x] Run quality gates (fmt, clippy, test)

## Acceptance Criteria

- [x] `hyalo find 'javascript' --index ...` on MDN completes in <200ms
- [x] All existing BM25 e2e tests pass without modification (same output)
- [x] `bm25` crate removed from dependency tree
- [x] Index size does not grow significantly (ideally shrinks)
- [x] No regression on non-search commands
