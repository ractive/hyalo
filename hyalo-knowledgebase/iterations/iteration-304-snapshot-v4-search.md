---
title: "Iteration 304: snapshot v4 search upgrade"
type: iteration
date: 2026-10-03
tags: [iteration, search, bm25, index]
status: completed
branch: iter-304/snapshot-v4-search
---

# Snapshot v4 search upgrade

## Problem

Ranked search after [[iterations/iteration-302-search-query-language]] and
[[iterations/iteration-303-section-hits-and-facets]] still tokenizes like
iteration 243: `résumé` and `resume` are different words, `getUserName` is one
opaque token, code samples weigh as much as prose, a title hit scores like a
body hit, a phrase must be exact, and adjacent query terms rank no higher than
terms a page apart. Every one of those fixes changes the token stream or the
postings, so they ship together behind one snapshot format bump. The same
bump makes the index incremental: `create-index` keeps unchanged files, and
`find --index` repairs stale entries in memory instead of serving stale
answers.

## Tasks

- [x] Tokenizer v4: NFKD diacritic folding before lowercasing and stemming, CJK bigrams kept (DEC-336)
- [x] Tokenizer v4: identifier splitting emits the whole identifier and its camelCase / snake_case / kebab-case parts (DEC-336)
- [x] `[search] code_blocks = "index" | "skip"`, reported by `hyalo config` (DEC-336)
- [x] BM25F field weighting over title, headings, tags (+ aliases) and body; `[search.weights]` (DEC-337)
- [x] Section mode scores the body field plus the section heading in the headings field
- [x] Phrase slop `"a b"~N` in ranked queries (DEC-338)
- [x] Proximity bonus over the top 200 candidates; `[search] proximity_bonus` (DEC-338)
- [x] Snippets prefer the line with the smallest window over the required terms
- [x] Incremental BM25 postings update (remove + add) shared by mutations and `create-index` (DEC-339)
- [x] `create-index` reuses unchanged entries; `--force`; `results.{reused, refreshed, removed, rebuilt}` (DEC-339)
- [x] `--index` reads repair stale entries in memory with a `-q`-proof note; no read command writes the snapshot (DEC-339)
- [x] `SNAPSHOT_FORMAT_VERSION` 4 and `TOKENIZER_VERSION` 4; `validate_bm25` covers per-field term frequencies
- [x] Fuzz seed regenerated; fuzz crate still compiles
- [x] Docs sync: `find --help`, `create-index --help`, `hyalo config`, docs/configuration.md, skills, `.claude/CLAUDE.md`, CHANGELOG
- [x] Unit tests and e2e tests `search_tokenizer_v4.rs`, `search_field_weights.rs`, `search_proximity.rs`, `index_incremental.rs`
- [x] Timings recorded below

## Acceptance criteria

- [x] `résumé` finds `resume`, `Häuser` finds `hauser`, `user` finds `getUserName`, `getUserName` still finds itself
- [x] `code_blocks = "skip"` drops fenced text from the corpus; inline code stays
- [x] A title hit outranks the same term in the body; weights from `.hyalo.toml` change the order
- [x] `"a b"~3` matches with up to three extra positions, in order only
- [x] Adjacent required terms outrank the same terms far apart
- [x] Incremental postings give scores identical to a full rebuild
- [x] A v3 snapshot is refused and the run falls back to disk
- [x] `--index` and disk agree on eight queries, including section mode and facets
- [x] fmt, clippy, tests, xtask quality gates and `hyalo lint --strict` pass

## Validation

Gates on the branch (2026-10-03): `cargo fmt`, `cargo clippy --workspace
--all-targets -- -D warnings` clean, `cargo test --workspace -q` with 0
failures (32 new e2e tests in `search_tokenizer_v4.rs`, `search_field_weights.rs`,
`search_proximity.rs` and `index_incremental.rs`; 17 new core unit tests for
folding, identifiers, code blocks, BM25F, slop, proximity, incremental postings
and in-memory repair). Every xtask gate of the CI quality-gates job exits 0.
`cargo test -p hyalo-cli --test e2e agent_discoverability` passes; `find -h`
stays 2949 bytes and `create-index -h` is 525. The fuzz crate compiles with
`cargo +nightly check` and the snapshot seed was regenerated.

Parity: seven ranked queries (`snapshot index`, `getUserName`, `résumé`,
`"error handling"~3`, `bm25 -tantivy`, `title:iteration stemming`, `conf*`)
and section mode with `--facet tags` produce identical JSON with and without
`--index` on this knowledgebase; `grid layout` is identical on MDN after an
in-memory repair.

Incremental postings are bit-identical to a rebuild
(`incremental_updates_score_exactly_like_a_rebuild`, ten query shapes).

Two fixes the measurements forced:

- Replacing a snapshot larger than 100 MiB failed (`file too large`), because
  publication captured the old file under the note size cap. The capture now
  uses the 512 MiB index cap; MDN's snapshot is 141 MB.
- `validate_before_changes` refused any refresh of a snapshot whose full token
  reconstruction exceeds 256 MiB (MDN does), which blocked both the in-memory
  repair and mutations under `--index`. The incremental update never
  reconstructs untouched documents, so the check is gone.

Timings (Apple M-series, release build, best of three):

| Corpus | Full `create-index` | Incremental, one file changed | `find --index`, one stale file | `find --index`, clean | `find` from disk |
|---|---|---|---|---|---|
| This knowledgebase (509 files) | 0.36 s | 0.14 s | 0.06 s | 0.06 s | 0.31 s |
| MDN `files/en-us` (14,375 files) | 3.42 s | 1.75 s | 0.85 s | 0.71 s | 4.89 s |

On MDN the incremental build is bounded by decoding and re-serialising the
141 MB snapshot, not by scanning: it reused 14,374 entries and re-scanned one.
