---
title: Iteration 101 — BM25 Ranked Search
type: iteration
date: 2026-04-09
status: completed
branch: iter-101/bm25-ranked-search
tags:
  - iteration
  - search
  - fts
  - bm25
  - performance
---

# Iteration 101 — BM25 Ranked Search

## Goal

Add relevance-ranked full-text search to `hyalo find` using BM25 scoring with multi-language stemming. Currently content search (`hyalo find "query"`) does substring/regex matching and returns results in file-modification order. For large vaults (hundreds+ of pages), relevance ranking is essential — the most relevant results should come first.

## Background

Research completed in [[research/fts-and-vector-search]] and [[research/fts-lightweight-alternatives]].

**Why BM25 over substring:** BM25 considers term frequency, inverse document frequency, and document length normalization. A document mentioning "search" 5 times in 200 words ranks higher than one mentioning it once in 5000 words. Substring matching can't distinguish relevance.

**Motivation:** Karpathy's LLM Wiki pattern ([[research/karpathy-llm-wiki]]) identifies ranked search as the key scaling bottleneck. Multiple independent implementations converged on BM25 as the right first step before vector embeddings.

## Search Modes in `hyalo find`

| Syntax | Mode | Description |
|---|---|---|
| `hyalo find "query"` | **BM25 ranked** | Stemmed, relevance-ranked full-text search |
| `hyalo find -e 'pattern'` | **Regex** | Pattern matching via regex crate, unranked |

The old memchr/memmem substring scanner is removed entirely. Plain text queries use BM25 with stemming. For literal string matching, use `-e "literal"` (the regex engine internally optimizes literals via aho-corasick).

**Breaking change:** `hyalo find "query"` currently does case-insensitive substring matching. After this iteration it does BM25 ranked search with stemming. Results will differ (ranked by relevance, stemmed matching). Help texts must explain the two modes clearly.

## Language Support

### Precedence (most specific wins)

1. **Frontmatter** `language: fr` on the file
2. **CLI flag** `--language french`
3. **Config** `[search] language = "french"` in .hyalo.toml
4. **Default:** English

### Implementation

Use `rust-stemmers` directly (Snowball stemmers, ~20 languages) with a custom `bm25::Tokenizer` impl. This avoids the heavy `default_tokenizer` feature of the bm25 crate.

Tokenizer pipeline per document:
1. Lowercase (Unicode-aware)
2. Split on non-alphanumeric characters
3. Stem using Snowball stemmer for the document's language
4. Feed tokens to BM25

Mixed-language vaults work correctly — each document is tokenized with its own language's stemmer. The BM25 index handles heterogeneous token sets.

### Dependencies

| Crate | New deps | What it provides |
|---|---|---|
| `bm25` (default-features = false) | ~3 (fxhash, byteorder) | BM25 embedder, scorer, search engine |
| `rust-stemmers` | ~0 (serde already in tree) | Snowball stemmers for ~20 languages |
| **Total** | **~3 new transitive deps** | |

The heavy deps (~50) from bm25's `default_tokenizer` are avoided entirely by implementing our own tokenizer with `rust-stemmers`.

## Index Integration

`create-index` builds BM25 data, `drop-index` cleans it up. Without an index, BM25 is built on-the-fly during the vault scan.

**Storage strategy (to decide during implementation):**

- **Option A: Extend `.hyalo-index`** — add an optional `bm25_index` field to `SnapshotData`. Works if BM25 data serializes cleanly with rmp_serde (inverted index as `HashMap<String, Vec<(doc_id, freq)>>` + doc lengths). Old indexes without BM25 deserialize with `None` (backwards compatible).
- **Option B: Separate file** (e.g. `.hyalo-fts`) — if the bm25 crate has opaque internal structures that don't serialize well, keep the two indexes independent. `create-index` builds both, `drop-index` removes both.

Prefer option A if feasible, fall back to B.

## Design Decisions

- [ ] Search modes: `find "query"` = BM25, `find -e 'pattern'` = regex. Old substring scanner removed.
- [ ] BM25 indexes body + title property (title is the most relevant text for ranking).
- [x] Result format: add a `score` field to the find output envelope?
- [x] Interaction with existing filters: filter first (--property, --tag, --glob), then rank? Or rank all, then filter?
- [x] Index storage: extend `.hyalo-index` (option A) or separate `.hyalo-fts` file (option B)?
- [x] How to handle the breaking change: warn on first use? Migration guide?

## Tasks

### Core Implementation
- [x] Add `bm25` crate dependency (`default-features = false`)
- [x] Add `rust-stemmers` crate dependency
- [x] Implement custom `bm25::Tokenizer`: lowercase, split, stem per language
- [x] Parse `[search]` section from .hyalo.toml (`language` setting)
- [x] Add `--language` flag to `find` command
- [x] Read `language` property from frontmatter per file
- [x] Implement language precedence: frontmatter > flag > config > english
- [x] Build BM25 index from scanned documents during vault walk
- [x] Remove old memchr/memmem substring body scanner (`ContentSearchVisitor` Substring mode)
- [x] Replace with BM25 for plain text queries in `find`
- [x] Index body content + title property in BM25
- [x] Keep regex search (`-e`) unchanged
- [x] Add `score` field to output (JSON and text format)
- [x] Sort results by BM25 score descending
- [x] Ensure combination with property/tag/glob filters works
- [x] Update help texts (short and long) explaining the two search modes

### Snapshot Index Integration
- [x] Extend `.hyalo-index` to store BM25 tokenized data
- [x] Rebuild BM25 index from snapshot when `--index` is used
- [x] Decide storage strategy (option A vs B) based on bm25 crate internals
- [x] `create-index` builds BM25 data (in .hyalo-index or .hyalo-fts)
- [x] `drop-index` cleans up all index files
- [x] Benchmark: indexed BM25 vs on-the-fly BM25

### Tests
- [x] Unit tests for tokenizer (English stemming, French stemming, mixed)
- [x] Unit tests for language precedence resolution
- [x] Unit tests for BM25 index building and scoring
- [x] E2E tests: ranked results appear in score order
- [x] E2E tests: BM25 + property filter combination
- [x] E2E tests: empty query, no matches, single match
- [x] E2E tests: `--language` flag overrides config
- [x] E2E tests: frontmatter `language` property overrides flag
- [x] E2E tests: regex search (`-e`) still works unchanged
- [x] E2E tests: BM25 via `--index` matches on-the-fly results
- [x] Benchmark: BM25 search on obsidian-hub (~6.5k files) vs current substring search

### Quality Gates
- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`
- [x] Dogfood on hyalo-knowledgebase

## Acceptance Criteria

- [x] `hyalo find "query"` returns results ranked by BM25 relevance score
- [x] Stemming works: `hyalo find "running"` matches documents containing "run"
- [x] `--language` flag and frontmatter `language` property select the stemmer
- [x] Default language is English, configurable via .hyalo.toml
- [x] Regex search (`hyalo find -e 'pattern'`) unchanged
- [x] Results include a `score` field in JSON output
- [x] Combining BM25 search with `--property`/`--tag`/`--glob` filters works
- [x] BM25 data persisted via `create-index`, cleaned by `drop-index`
- [x] Performance: BM25 search on 6.5k files completes in <2s without index
- [x] No more than 5 new transitive dependencies

## Dependencies

- `bm25` v2.3.2 (`default-features = false`) — BM25 engine (~3 new deps)
- `rust-stemmers` — Snowball stemmers for ~20 languages (~0 new deps, serde already in tree)
