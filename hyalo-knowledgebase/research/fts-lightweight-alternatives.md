---
title: Lightweight FTS alternatives to tantivy
type: research
date: 2026-04-06
tags:
  - dependencies
  - performance
  - fts
status: completed
---

# Lightweight Full-Text Search Alternatives to Tantivy

## Context

Tantivy has 162 transitive dependencies -- too heavy for a CLI tool indexing ~1000 small markdown files. Need something much simpler.

hyalo already depends on: serde, regex, memchr, rayon, thiserror, rmp-serde (MessagePack), ahash, among others. Any crate sharing these deps adds zero marginal cost.

## Candidates Evaluated

### Tier 1: Strong Candidates

#### `bm25` (v2.3.2) -- RECOMMENDED
- **Repo**: https://github.com/Michael-JB/bm25
- **Downloads**: 1.1M+ (well-adopted)
- **Last updated**: 2025-09-07
- **What it provides**: BM25 embedder, scorer, and complete in-memory search engine. Three abstraction levels: Embedder (sparse vectors), Scorer (ranking), SearchEngine (full keyword search).
- **Dep count (no defaults)**: 3 transitive deps (fxhash + byteorder only!)
- **Dep count (default_tokenizer)**: ~25 unique crates (adds cached, rust-stemmers, stop-words, deunicode, unicode-segmentation)
- **Dep count (all defaults)**: ~35 unique crates
- **Features**: `default_tokenizer` (stemming, stop words, unicode normalization), `language_detection`, `parallelism` (rayon)
- **Key insight**: With `default-features = false`, you get the core BM25 engine with only 3 deps. You can bring your own tokenizer. With `default_tokenizer`, you get English stemming + stop words for ~25 deps.
- **API**: Builder pattern -- `SearchEngineBuilder` for the simple case, or `Embedder`+`Scorer` for custom pipelines.
- **Overlap with hyalo**: Would share rayon if `parallelism` feature enabled. fxhash is tiny (2 deps).

#### `elasticlunr-rs` (v3.0.2)
- **Repo**: https://github.com/mattico/elasticlunr-rs
- **Downloads**: 6.5M+ (very well-adopted, used by mdbook)
- **Last updated**: 2025-03-16
- **What it provides**: Port of elasticlunr.js -- tokenization, inverted index, TF-IDF scoring, multi-field search. Exports index to JSON (originally for client-side search).
- **Dep count (no defaults)**: ~15 unique crates (regex + serde + serde_json are mandatory)
- **Dep count (defaults)**: ~17 unique crates
- **Features**: Optional language support (jieba-rs for Chinese, lindera for Japanese, rust-stemmers)
- **Key insight**: serde/serde_json/regex already in hyalo, so marginal cost is essentially zero new deps. Battle-tested, mature. But: designed for JSON export (static site search), scoring is TF-IDF not BM25, API is add-doc-then-search.
- **Limitation**: No BM25. Serialization format is JSON (verbose). Designed for browser consumption.

### Tier 2: Viable but with Caveats

#### `rust-tfidf` (v1.1.1)
- **Repo**: https://github.com/ferristseng/rust-tfidf
- **Downloads**: 27K
- **Last updated**: 2021 (dormant since 2015)
- **Deps**: ZERO dependencies
- **What it provides**: Pure TF-IDF scoring via traits. No inverted index, no tokenization, no search. You pass pre-tokenized documents and get scores back.
- **Limitation**: Just a scoring function. You'd need to build the inverted index yourself. Abandoned.

#### `tfidf` (v0.3.0)
- **Repo**: https://github.com/afshinm/tf-idf
- **Downloads**: 6K
- **Last updated**: 2017 (dormant)
- **Deps**: ZERO dependencies
- **What it provides**: Similar to rust-tfidf -- bare TF-IDF computation.
- **Limitation**: Same as above. Scoring only, no index.

#### `rankfns` (v0.1.2)
- **Repo**: https://github.com/arclabs561/rankfns
- **Downloads**: 100
- **Last updated**: 2026-03-09
- **Deps**: 1 (postings)
- **What it provides**: IR ranking math kernels only -- BM25, TF-IDF, language model transforms. No indexing.
- **Limitation**: Part of an experimental arclabs561 ecosystem (postings + lexir + rankfns). Very new, very low adoption.

#### `lexir` (v0.1.2)
- **Repo**: https://github.com/arclabs561/lexir
- **Downloads**: 41
- **Last updated**: 2026-03-09
- **What it provides**: BM25/TF-IDF scoring on top of postings lists. Has InvertedIndex, add_document(), retrieve().
- **Limitation**: Experimental, 41 downloads, depends on postings+rankfns from same author. Self-described as "reference implementation."

#### `bm25x` (v0.3.1)
- **Repo**: https://github.com/lightonai/bm25x
- **Downloads**: 84
- **Last updated**: 2026-03-19
- **Deps**: 8 non-optional (bincode, memmap2, rayon, rust-stemmers, rustc-hash, serde, tempfile, unicode-normalization)
- **What it provides**: Streaming-friendly BM25 with mmap support. Designed for large-scale retrieval.
- **Limitation**: Overkill for 1000 files. Very new, low adoption.

### Tier 3: Not Suitable

#### `nanofts` (v0.7.0)
- 31 non-optional deps including mimalloc, roaring bitmaps, zstd, crossbeam, dashmap
- LSM-tree architecture "scalable to billions of documents"
- Way overkill. More deps than tantivy would save.

#### `stork-lib` (v2.0.0-beta.2)
- 26 deps, still in beta, includes wasm-bindgen
- Designed for static sites, not CLI tools

#### `terrain` (v0.1.0)
- 22 downloads, depends on traverze (which depends on tantivy!)
- Defeats the purpose entirely

#### `bm25_turbo` (v0.2.0)
- 34 deps including tokio, tonic, axum, tower
- HTTP server / gRPC-oriented. Way too heavy.

#### `tinysearch` (v0.10.0)
- WASM-oriented for static sites, uses cuckoo filters
- Not a general-purpose search library

### Tier 4: DIY Option

#### Roll your own with existing deps
- hyalo already has: memchr, regex, rayon, serde, ahash
- A simple inverted index with BM25 scoring is ~200-300 lines of Rust
- HashMap<String, Vec<(DocId, f32)>> for the index
- Standard BM25 formula: score = IDF * (tf * (k1+1)) / (tf + k1 * (1 - b + b * dl/avgdl))
- Tokenization: split on whitespace + punctuation, lowercase, optional stemming
- For 1000 docs, in-memory is trivial
- **Downside**: No stemming without rust-stemmers (~2 deps), no stop words without a list

## Recommendation

**Primary: `bm25` with `default-features = false`** (3 deps)
- If you need stemming/stop-words: enable `default_tokenizer` feature (~25 deps, but rust-stemmers + stop-words are worth it for search quality)
- Provides a complete, tested BM25 search engine
- Well-maintained (1M+ downloads), good API design
- Can bring your own tokenizer for full control

**Secondary: `elasticlunr-rs`** (~0 new deps for hyalo since serde+regex already present)
- If TF-IDF is acceptable over BM25
- Battle-tested (6.5M downloads, used by mdbook)
- Near-zero marginal dep cost since hyalo already has serde+regex
- But: JSON-oriented serialization, no MessagePack option

**Fallback: DIY inverted index + `bm25` no-defaults for scoring only**
- Use `bm25`'s Embedder/Scorer without the SearchEngine
- Build your own inverted index with HashMap
- Use your own tokenizer (regex-based split)
- Total new deps: 3 (fxhash + byteorder)

## Dep Cost Summary

| Crate | Config | Total transitive deps | New deps for hyalo |
|-------|--------|----------------------|-------------------|
| bm25 | no defaults | 3 | ~2 (fxhash, byteorder) |
| bm25 | default_tokenizer | ~25 | ~8-10 |
| bm25 | all features | ~35 | ~15 |
| elasticlunr-rs | defaults | ~17 | ~0 (all overlap) |
| tantivy | defaults | 162 | ~120+ |
| DIY | n/a | 0 | 0 |
