---
title: "Full-Text Search & Vector Embeddings Feasibility"
type: research
date: 2026-04-06
tags: [research, search, fts, embeddings, vector, performance]
status: completed
---

# Full-Text Search & Vector Embeddings Feasibility

Research into two approaches for enhancing hyalo's search capabilities beyond the current substring/regex matching.

## Approach 1: Full-Text Search Index (Tantivy)

### Library: tantivy (v0.22)

The de facto Rust FTS library. Lucene-inspired, actively maintained by Quickwit.

**Pros:**
- ~2x faster than Lucene in query latency benchmarks
- Supports RamDirectory (in-memory index, no disk persistence needed)
- BM25 ranking, phrase queries, fuzzy matching, boolean queries out of the box
- MmapDirectory for persistent indexes with very low resident memory
- Pure Rust, cross-platform (macOS/Linux/Windows)
- ~162 unique transitive dependencies (manageable)
- Well-maintained: used by Quickwit, Meilisearch, ParadeDB

**Cons:**
- A single growing segment can use ~15 MB RAM even for few docs (fixed overhead)
- Adds meaningful compile time and binary size
- Heavyweight for a ~1000-file knowledge base (designed for millions of docs)

**Performance estimates for ~1000 markdown files:**
- Indexing: At 45k docs/sec throughput, 1000 files would index in ~22ms (excluding I/O)
- Realistic with file I/O: likely 50-200ms total for a cold `create-index` command
- Query latency: sub-millisecond for a corpus this small
- Index size on disk: a few hundred KB to low single-digit MB for 1000 short docs

**Architecture options:**
1. **On-the-fly (RamDirectory)**: Build index in memory at each invocation. ~100-200ms startup cost. No stale index issues. Simple. Best fit for hyalo's current approach.
2. **Persistent (MmapDirectory)**: Build once, query many times. Near-instant queries. Requires index invalidation logic (watch mtimes, rebuild on change). More complexity.
3. **Hybrid**: Extend current MessagePack snapshot index to include FTS data. Rebuild when snapshot is stale.

**Verdict:** Highly feasible. On-the-fly indexing of 1000 files is fast enough for a CLI tool. The RamDirectory approach keeps it simple. Persistent index adds complexity but gives instant queries.

### Lightweight Alternatives

- **Sonic**: Minimal memory footprint, but it's a server (not embeddable as a library)
- **Custom BM25**: Could implement simple TF-IDF/BM25 scoring over the existing in-memory document set without tantivy. Much less code, fewer deps, but no fuzzy/phrase queries.
- **strsim + regex**: Already have substring/regex. Adding a simple relevance scorer on top might be "good enough" for small corpora.

## Approach 2: Vector Store with Embeddings

### Embedding Generation

**Best option: fastembed-rs (v4.9)**
- Wraps ONNX Runtime via `ort` crate
- Downloads models from HuggingFace on first use (cached locally)
- Default model: all-MiniLM-L6-v2 (22M params, 384 dimensions)
- ONNX model file: ~90 MB download, cached in ~/.cache
- ~508 unique transitive dependencies (very heavy)
- Cross-platform (macOS/Linux/Windows)

**Alternative: ort directly**
- Lower-level, fewer deps than fastembed
- Need to handle tokenization yourself (add `tokenizers` crate)
- More control, but more code

**Alternative: candle (HuggingFace)**
- Pure Rust ML framework
- No ONNX dependency, but models need conversion
- Less mature for production embedding inference

**Performance estimates for ~1000 markdown files (CPU, all-MiniLM-L6-v2):**
- Model load (cold start): ~500ms-2s (ONNX session init + model load from disk cache)
- Embedding generation: ~50-100 sentences/sec single-threaded on CPU
- For 1000 short markdown files (~1-2 paragraphs each): ~10-20 seconds
- With batching (batch_size=256): possibly 3-8 seconds total
- First-ever run: add ~30s-2min for model download (~90 MB)

### Vector Similarity Search

**Best options:**
- **usearch**: Single-file HNSW implementation, C++ core with Rust bindings. Fast, tiny footprint.
- **hnswlib-rs**: Pure Rust HNSW. Multithreaded insert/search.
- **hora**: Pure Rust, multiple index types (HNSW, SSG, PQIVF). SIMD-accelerated.
- **Simple brute force**: For 1000 vectors of 384 dims, brute-force cosine similarity takes <1ms. No index needed.

**Storage:** 1000 vectors x 384 dims x 4 bytes = ~1.5 MB. Trivially fits in the MessagePack index.

### Architecture for hyalo

1. `hyalo embed` or `hyalo index --embeddings`: generate embeddings for all files, store in snapshot index
2. `hyalo find --semantic "concept I'm looking for"`: cosine similarity search against stored embeddings
3. Model cached in `~/.cache/fastembed` (or similar), downloaded on first use

**Verdict:** Technically feasible but problematic for a fast CLI tool:
- **Cold start of 0.5-2s** just to load the model kills the "instant" CLI feel
- **10-20s** to embed 1000 files is too slow for on-the-fly; requires persistent index
- **90 MB model download** is a steep first-run cost for a lightweight tool
- **508 dependencies** nearly quadruples the dep count
- **Binary size** would grow significantly (ONNX runtime is large)
- Need to handle the "model not downloaded yet" UX gracefully

## Comparison Summary

| Criterion | FTS (tantivy) | Vector Embeddings |
|---|---|---|
| On-the-fly feasibility | Yes (~100-200ms) | No (10-20s embed time) |
| Persistent index feasibility | Yes | Yes (with pre-computed embeddings) |
| Query quality | Exact + fuzzy text matching | Semantic/conceptual similarity |
| Cold start overhead | ~0ms (in-memory) | ~0.5-2s (model load) |
| New dependencies | ~162 crates | ~508 crates |
| External downloads | None | ~90 MB model file |
| Binary size impact | Moderate (+2-4 MB est.) | Large (+10-20 MB est. with ONNX) |
| Cross-platform | Native Rust | Needs ONNX runtime per platform |
| Complexity | Low-medium | High |

## Conclusion (2026-04-06)

**Verdict: Parked for now, but multi-term search has real value.**

### The actual gap in current `find`
The current grep-style search handles single substrings and regex well, but falls short for:
- **Boolean queries**: `rust OR golang` — currently requires manual regex `rust|golang`
- **Multi-term AND across non-adjacent text**: `rust async error` — finds files containing all three terms anywhere, not as a literal substring. Currently impossible without multiple passes or ugly lookahead regex.
- **Phrase matching**: `"error handling"` as an exact phrase

This is a real workflow pain point, especially for LLM agents that frequently construct `find -e "term1|term2|term3"` patterns as a workaround.

### Why not rush to implement
For a personal KB of hundreds of files, ranked results and noise filtering matter less — users typically know what they're looking for, and grep + property/tag filtering is predictable and precise. FTS shines more at scale (thousands of docs, many authors).

### Implementation options when we're ready

**Option A: `bm25` crate** (recommended) — See [[research/fts-lightweight-alternatives]]
- `default-features = false`: **3 new deps** (fxhash + byteorder). BM25 scoring + search engine, bring your own tokenizer.
- `default_tokenizer` feature: **~10 new deps**. Adds stemming, stop words, unicode normalization.
- Provides ranked results, multi-term AND/OR, and noise filtering out of the box.

**Option B: `elasticlunr-rs`** — See [[research/fts-lightweight-alternatives]]
- **~0 new deps** (hyalo already has serde, regex, memchr). Used by mdbook, 6.5M downloads.
- TF-IDF scoring (not BM25), tokenization pipeline, multi-field search, boolean queries.
- Designed for JSON export (static site search) but works fine in-memory.

**Option C: DIY** — Zero new deps
- ~100-200 lines on top of existing `ContentSearchVisitor`.
- Split query into terms, check each file for presence, AND/OR semantics.
- No ranking or stemming, but solves the core boolean/multi-term gap.

**Not recommended:**
- **tantivy**: 162 deps, overkill for this scale.
- **Vector embeddings**: 508 deps, 90 MB model download, 10-20s embedding time. Only revisit if KB grows to a scale where grep results become unmanageable.
