---
title: Iteration 103 — Boolean Search Operators & Implicit AND
type: iteration
date: 2026-04-12
tags:
  - iteration
  - search
  - bm25
  - ux
status: completed
branch: iter-103/boolean-search
related:
  - "[[iterations/iteration-101-bm25-ranked-search]]"
  - "[[iterations/iteration-101b-bm25-serializable-index]]"
  - "[[research/benchmark-iter101-bm25]]"
---

# Iteration 103 — Boolean Search Operators & Implicit AND

## Goal

Make BM25 search behave like every major search engine: space means AND, `OR` keyword for disjunction, `-` prefix for negation (already done), and `"quoted phrases"` for exact matching. Also remove the unnecessary `doc_token_sets` field from the inverted index, which was only needed for negation but can be replaced by postings lookups.

## Problem

### 1. Implicit OR is surprising

Currently `hyalo find "rust golang"` returns any document mentioning *either* word, ranked by combined BM25 score. Every major search system (Google, GitHub, Gmail, Lucene, Outlook) treats space as implicit AND — users expect both words to be required. This is the single biggest UX gap in the current search.

### 2. No way to express OR

Users cannot search for "documents about rust or golang". There is no OR operator.

### 3. No phrase search

`hyalo find "javascript promises"` stems and splits the words independently. There is no way to search for the literal phrase "javascript promises" appearing consecutively.

### 4. `doc_token_sets` bloats the index

The `Vec<HashSet<String>>` field stores every document's full deduplicated token vocabulary solely for negation. On MDN (14K docs) this is the dominant contributor to the 86 MB index. The postings map already contains this information — look up the negated term in `postings`, collect doc_ids to exclude. Removing `doc_token_sets` shrinks the index significantly and eliminates millions of String deserializations on load.

## Design

### Query Syntax (convention-aligned)

| Syntax | Semantics | Example |
|---|---|---|
| `foo bar` | **AND** — both required | `rust golang` → must contain both |
| `foo OR bar` | **OR** — either term, ranked by combined score | `rust OR golang` |
| `-foo` | **NOT** — exclude documents containing term | `rust -java` |
| `"foo bar"` | **Phrase** — exact consecutive match | `"javascript promises"` |
| `foo OR bar -baz` | Mixed — either foo or bar, not baz | |

`OR` is case-insensitive as a keyword (also accepts `or`). `AND` keyword accepted but optional (implicit).

### Precedence rules

- `-` prefix binds tightest (always negation)
- `"quotes"` bind next (phrase grouping)
- `OR` separates alternatives
- Everything else is AND (implicit)
- No parentheses grouping in v1 — keep it simple

### Query AST

Replace `ParsedQuery { positive: Vec<String>, negative: Vec<String> }` with:

```rust
enum Clause {
    /// Document must contain this term (implicit AND)
    Must(Vec<String>),       // single stemmed token, or phrase tokens
    /// Document may contain this term (OR group member, contributes score)
    Should(Vec<String>),
    /// Document must not contain this term
    MustNot(Vec<String>),
}

struct BooleanQuery {
    clauses: Vec<Clause>,
}
```

Each clause holds a `Vec<String>` — single-element for a word, multi-element for a phrase.

### Scoring semantics

| Query type | Candidate selection | Score |
|---|---|---|
| All AND (default) | Intersection of posting lists | Sum of BM25 term weights |
| Mixed AND + OR | Must-terms intersected, then OR-terms add score | Sum of all matching term weights |
| All OR | Union of posting lists | Sum of matching term weights |
| NOT | Excluded from candidates via postings lookup | — |
| Phrase | AND of all phrase terms, then positional check | Sum of phrase term weights |

### Phrase matching

For the indexed path: require all phrase terms present (via postings intersection), then verify consecutive positions. This requires storing **term positions** in the postings list — extend `Posting` with a positions field:

```rust
struct Posting {
    doc_id: u32,
    term_freq: u32,
    positions: Vec<u32>,  // NEW: token offsets within document
}
```

For the non-indexed (live scan) path: after BM25 candidate selection, verify the phrase appears as a substring in the raw document text (case-insensitive, unstemmed). This is simpler and correct since we already read the file.

### `doc_token_sets` removal

Replace the negation filter:

```rust
// Before (doc_token_sets):
if parsed.negative.iter().any(|neg| self.doc_token_sets[doc_id].contains(neg)) { skip }

// After (postings lookup):
let excluded: HashSet<u32> = query.must_not_terms().iter()
    .filter_map(|t| self.postings.get(t))
    .flat_map(|posts| posts.iter().map(|p| p.doc_id))
    .collect();
// Then: if excluded.contains(&doc_id) { skip }
```

This removes the `doc_token_sets` field entirely from `Bm25InvertedIndex` serialization.

### Breaking changes

- `hyalo find "foo bar"` changes from OR to AND semantics. Documents must contain *all* terms.
- Users who relied on implicit OR must now use `foo OR bar` explicitly.
- Index format changes (positions added, doc_token_sets removed) — old indexes must be rebuilt with `create-index`.

## Tasks

### Boolean Search Operators

- [x] Replace `ParsedQuery` with `BooleanQuery` AST in `bm25.rs`
- [x] Implement `parse_boolean_query` — handle implicit AND, `OR` keyword, `-` prefix, `"quoted phrases"`
- [x] Change `ranked_matches` to use intersection (AND) for Must clauses, union for Should clauses
- [x] Implement negation via postings lookup instead of `doc_token_sets`
- [x] Remove `doc_token_sets` field from `Bm25InvertedIndex`
- [x] Add `positions: Vec<u32>` to `Posting` struct
- [x] Store token positions during index build (`build` and `build_from_entries`)
- [x] Implement phrase matching for indexed path (consecutive position check)
- [x] Implement phrase matching for non-indexed path (substring verification)
- [x] Update non-indexed `build()` path to handle new query types
- [x] Update `score()` and `ranked_matches()` return types if needed
- [x] Update CLI help text in `args.rs` — document query syntax with examples
- [x] Add unit tests for `parse_boolean_query` (AND, OR, NOT, phrase, mixed, edge cases)
- [x] Add unit tests for AND scoring (intersection behavior)
- [x] Add unit tests for OR scoring (union behavior, explicit OR keyword)
- [x] Add unit tests for phrase matching (consecutive positions)
- [x] Add unit tests for negation via postings (no `doc_token_sets`)
- [x] Add e2e tests for implicit AND (`find "foo bar"` requires both)
- [x] Add e2e tests for explicit OR (`find "foo OR bar"`)
- [x] Add e2e tests for phrase search (`find '"exact phrase"'`)
- [x] Add e2e tests for mixed queries (`find "foo OR bar -baz"`)
- [x] Add e2e test for index rebuild (old index without positions triggers rebuild hint)
- [ ] Verify index size reduction after `doc_token_sets` removal (benchmark on MDN if available)
- [x] Run full quality gates: fmt, clippy, test

### BM25 Code Quality (review findings)

- [x] Deduplicate `build()` — it duplicates the `tokenize_document` body (`bm25.rs:311-322`), extract shared helper
- [x] Eliminate double file read in `scan_one_file` (`index.rs:686-762`) — accumulate body lines during scan when `bm25_tokenize = true` instead of re-reading with `read_to_string`
- [ ] Avoid cloning token lists in `build_from_entries` (`bm25.rs:384-399`) — consume entries or build inverted index during initial scan pass
- [x] Replace scores `Vec<f64>` with `HashMap<u32, f64>` in `ranked_matches` (`bm25.rs:432`) — avoids allocating N slots when only a few docs match
- [x] Change `avgdl` from `f32` to `f64` (`bm25.rs:297`) — avoid gratuitous precision loss
- [x] Replace `eprintln!` with `crate::warn::warn` in `scan_one_file` (`index.rs:754`)
- [x] Fix comment typo `/ Reading the file` → `// Reading the file` (`index.rs:720`)
- [ ] Extract `ScanOptions.default_language` into an `Option<Bm25Options>` sub-struct to make the `bm25_tokenize` coupling explicit
- [x] Consolidate `StemLanguage` match arms — replace three 18-arm match blocks (`parse_language`, `canonical_name`, `to_algorithm`) with a const table or macro
- [x] Remove unused `search()` method or clarify `score()` vs `search()` naming (`bm25.rs:414`)
- [x] Consider per-token `to_lowercase()` instead of full-text `to_lowercase()` in `tokenize()` (`bm25.rs:158`) — avoids allocating a full copy of input

### Test Gaps (review findings)

- [x] Add e2e test for section-scoped BM25 search (`find "query" --section "Heading"`)
- [x] Add unit test for `tokenize_document()` function
- [ ] Add e2e test for config `[search] language` — three-tier precedence (frontmatter > CLI > config)
- [ ] Add e2e test for stale index fallback (index without BM25 data → graceful live scan)
- [x] Add e2e test for `--reverse` with BM25 score ordering
- [x] Add test for empty corpus (0 documents) in `build_from_tokens` / `build`
- [x] Add test for whitespace-only query (`"   "`)

## Acceptance Criteria

- [x] `hyalo find "rust golang"` returns only documents containing both "rust" and "golang"
- [x] `hyalo find "rust OR golang"` returns documents containing either, ranked by combined score
- [x] `hyalo find "rust -java"` returns docs with "rust" but without "java"
- [x] `hyalo find '"javascript promises"'` matches only documents with the exact phrase
- [x] `doc_token_sets` removed from `Bm25InvertedIndex` — negation uses postings only
- [x] Index format includes term positions; old indexes gracefully handled (hint to rebuild)
- [x] All existing BM25 tests still pass (updated for AND semantics)
- [x] No regression on non-search commands (metadata, properties, tags)
- [x] Help text documents the query syntax with examples
- [x] No duplicate logic between `build()` and `tokenize_document`
- [x] No double file reads during `create-index`
- [x] `avgdl` stored as `f64`
- [x] `StemLanguage` match arms consolidated (no three separate 18-arm blocks)
- [x] Section-scoped BM25 search covered by e2e test

## Dependencies

No new crate dependencies. All changes are within `bm25.rs`, `find/mod.rs`, `args.rs`, and test files.
