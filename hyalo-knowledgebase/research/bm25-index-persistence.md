---
title: BM25 Index Persistence — Crate Survey & DIY Analysis
type: research
date: 2026-04-10
tags:
  - full-text-search
  - bm25
  - performance
  - serialization
  - index
status: active
supersedes: "[[research/fts-lightweight-alternatives]]"
---

# BM25 Index Persistence — Crate Survey & DIY Analysis

## Problem

hyalo uses the `bm25` crate (v2.3.2) for ranked full-text search. The snapshot index already stores pre-tokenized BM25 tokens per document (`bm25_tokens` in `IndexEntry`), but the in-memory `Scorer` and `Embedder` structs from the `bm25` crate **cannot be serialized** — they don't derive `Serialize`/`Deserialize`, and their internal `HashMap<K, Embedding<D>>` + `HashMap<D, HashSet<K>>` are private.

This means every `hyalo find "query"` must:
1. Load the snapshot index (fast, ~50ms for 14K docs)
2. **Rebuild the entire BM25 Scorer** from tokens — O(N) over all documents (~2s for 14K docs)
3. Execute the query against the Scorer (~1ms)

Step 2 dominates. We need query times in the low hundreds of milliseconds, not 2 seconds.

## Options Evaluated

### Option A: Tantivy

- **Crate**: `tantivy` v0.26.0
- **BM25**: Yes, default scoring algorithm (same as Lucene/Elasticsearch)
- **Index persistence**: Yes — writes segment files to a directory on disk. Designed for persistent indexes. Opening an existing index is O(1) (just reads metadata + mmap segments).
- **Custom tokenizer**: Yes — pluggable `Tokenizer` trait, `TextAnalyzer::builder()`. The `tantivy-stemmers` crate provides Snowball stemming for all languages we support.
- **Query performance**: Excellent — inverted index with skip lists, O(query_terms * matching_docs). Sub-millisecond for typical queries on 100K docs.
- **Dependency weight**: **162 transitive dependencies**. Adds lz4_flex, zstd, memmap2, fs4, uuid, serde_json, and many more. This is the heaviest option by far.
- **Binary size impact**: Estimated +2-4 MB to release binary.
- **Deal-breakers**:
  - Massive dependency footprint — 120+ new deps for hyalo
  - Overkill for 10K-100K small markdown files
  - Brings its own file format, segment merging, commit protocol — complexity we don't need
  - Would replace our MessagePack snapshot index with a separate tantivy index directory

**Verdict: Too heavy. Correct solution for a search service, wrong solution for a CLI tool.**

### Option B: milli (Meilisearch core)

- **Crate**: `milli-core` on crates.io (fork of milli for embedding)
- **BM25**: No — Meilisearch uses "bucket sort" ranking (typos, proximity, exact match), not BM25/TF-IDF
- **Index persistence**: Yes (LMDB-backed)
- **Custom tokenizer**: Limited — uses its own `charabia` tokenizer
- **Deal-breakers**:
  - No BM25 ranking
  - LMDB dependency (C library, cross-compilation issues)
  - Even heavier than tantivy
  - Designed for typo-tolerant instant search, not relevance-ranked FTS

**Verdict: Wrong tool. Not BM25, too heavy, C dependency.**

### Option C: probly-search

- **Crate**: `probly-search` v2.0.1
- **BM25**: Yes, plus zero-to-one scoring and custom `ScoreCalculator` trait
- **Index persistence**: **No** — no serde support on the `Index` struct. Entirely in-memory.
- **Custom tokenizer**: Yes — pluggable tokenizer function
- **Dependency weight**: Light (~5 deps)
- **Deal-breaker**: Same problem as current `bm25` crate — no serialization. Would still need to rebuild the index on every query.

**Verdict: Same serialization gap as our current crate. No improvement.**

### Option D: elasticlunr-rs

- **Crate**: `elasticlunr-rs` v3.0.2
- **BM25**: No — TF-IDF only
- **Index persistence**: Yes — serializes to JSON (designed for static site search)
- **Custom tokenizer**: Yes
- **Dependency weight**: ~0 new deps (serde + regex already in hyalo)
- **Drawbacks**: TF-IDF not BM25, JSON serialization is verbose, designed for browser consumption

**Verdict: Could work but downgrades from BM25 to TF-IDF. JSON index would be large for 14K docs.**

### Option E: Stork / tinysearch / nucleo

- **Stork**: Abandoned by maintainer. WASM-oriented for static sites.
- **tinysearch**: WASM-only, uses cuckoo filters, not a general search library.
- **nucleo**: Fuzzy string matcher (like fzf), not a full-text search engine. No BM25/TF-IDF, no inverted index, no document corpus concept.
- **indicium**: Simple in-memory autocompletion search, no BM25, no serialization.

**Verdict: None are suitable. Wrong problem domain.**

### Option F: bm25-rs (logan-markewich)

- **Crate**: Not yet published to crates.io (GitHub only)
- **BM25**: Yes, with inverted index
- **Index persistence**: Unknown, likely no
- **Status**: Incomplete TODOs (no CI, not published)

**Verdict: Too immature.**

### Option G: DIY Serializable BM25 Inverted Index (RECOMMENDED)

Build our own BM25 scorer with a serializable inverted index. The algorithm is simple and well-understood.

**What we need (data structures):**

```rust
#[derive(Serialize, Deserialize)]
struct Bm25Index {
    /// term -> list of (doc_id, term_frequency)
    postings: HashMap<String, Vec<(u32, u32)>>,
    /// doc_id -> document length (in tokens)
    doc_lengths: Vec<u32>,
    /// Total number of documents
    num_docs: u32,
    /// Average document length
    avgdl: f32,
}
```

**BM25 scoring (the core formula):**

```
score(q, d) = sum over terms t in q:
    IDF(t) * (tf(t,d) * (k1 + 1)) / (tf(t,d) + k1 * (1 - b + b * dl/avgdl))

where:
    IDF(t) = ln((N - df(t) + 0.5) / (df(t) + 0.5) + 1)
    tf(t,d) = term frequency of t in document d
    df(t) = number of documents containing term t
    N = total number of documents
    dl = length of document d (in tokens)
    avgdl = average document length
    k1 = 1.2 (tuning parameter)
    b = 0.75 (tuning parameter)
```

**Implementation estimate**: ~150-200 lines of Rust for the index + scoring. We already have:
- Tokenization with Snowball stemming (18 languages) in `bm25.rs`
- Pre-tokenized data in the snapshot index (`bm25_tokens`)
- serde + rmp-serde for MessagePack serialization
- The `Bm25Corpus` wrapper that manages doc IDs and negation filtering

**Query performance**: O(query_terms * avg_postings_length). For a 2-term query on 14K docs where each term appears in ~500 docs, that's ~1000 lookups + scores. Sub-millisecond.

**Build performance**: O(total_tokens) to build from pre-tokenized data. Same as now, but we only do it once and persist.

**Serialization**: The entire index is trivially serializable with serde. MessagePack encoding of the inverted index for 14K docs with ~5M total tokens would be ~10-20 MB on disk. With the snapshot index already storing tokens, we could store just the postings + corpus stats separately.

**New dependencies**: Zero. We already have everything we need (serde, rmp-serde, HashMap).

**What we drop**: The `bm25` crate (saves fxhash + byteorder, 2 deps).

## Architecture Sketch for Option G

```
create-index (or implicit rebuild):
  1. Scan all docs, tokenize (already done)
  2. Build inverted index from tokens
  3. Serialize Bm25Index to MessagePack alongside snapshot index

find "query" --ranked:
  1. Load snapshot index (50ms)
  2. Deserialize Bm25Index from MessagePack (~20ms for 14K docs)
  3. Tokenize + stem query terms (~0.1ms)
  4. Look up each query term in postings, compute BM25 scores (~1ms)
  5. Sort results by score, return top N
  Total: ~70ms (down from ~2050ms)
```

## Recommendation

**Go with Option G: DIY serializable BM25 inverted index.**

Rationale:
1. Zero new dependencies (actually removes 2)
2. Trivially serializable with serde — the whole point
3. ~150-200 lines of straightforward, well-tested code
4. Query time drops from ~2s to ~70ms (28x faster)
5. We already have all the tokenization infrastructure
6. Full control over the index format, versioning, and evolution
7. The BM25 algorithm is a simple, well-documented formula — no rocket science

The `bm25` crate was the right choice when we didn't need persistence. Now that persistence is the bottleneck, and the crate's internals aren't serializable, building our own is the pragmatic path.

## References

- [[research/fts-lightweight-alternatives]] — prior research that led to adopting `bm25` crate
- [[research/fts-and-vector-search]] — broader FTS/vector feasibility study
- BM25 formula: Robertson & Zaragoza, "The Probabilistic Relevance Framework: BM25 and Beyond" (2009)
- Current implementation: `crates/hyalo-core/src/bm25.rs`
- Snapshot index: `crates/hyalo-core/src/index.rs` (`IndexEntry.bm25_tokens`)
