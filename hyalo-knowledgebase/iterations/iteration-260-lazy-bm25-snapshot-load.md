---
type: iteration
title: Iteration 260 — lazy BM25 section in snapshot load
date: 2026-09-01
status: completed
tags: [iteration, performance, index, bm25]
depends-on: "[[iterations/iteration-259-index-snapshot-load-perf]]"
branch: iter-260/lazy-bm25-snapshot-load
---

# Iteration 260 — lazy BM25 section in snapshot load

## Goal

Land the fix [[iterations/iteration-259-index-snapshot-load-perf]] measured and
[[decision-log#DEC-264]] approved: stop decoding the BM25 inverted index when
loading a `.hyalo-index` snapshot, and decode it only when a command actually
searches text.

Measured ceiling on the MDN vault (14 375 entries, 116 MiB index, release
build): whole-document decode 240 ms → early-stop decode 61.5 ms, plus ~6–12 ms
of skipped BM25 validation and ~43 ms of skipped teardown. Projected
`find --limit 1 --index`: ~360 ms → ~150 ms. The snapshot format does not
change — see [[research/snapshot-load-floor-2026-09-01]] for the full
breakdown and for why every other candidate (I/O, allocation shape,
`IgnoredAny` lazy fields, post-decode reconstruction) was ruled out.

## Tasks

### LAZY-1: early-stop deserialization [1/1]

- [x] Replace the derived `Deserialize` on the snapshot envelope with a
      hand-written impl that visits the top-level MessagePack map and stops
      when it reaches the `bm25_index` key, without reading its value.
      `rmp_serde::from_slice` accepts the unconsumed tail — verified in 259.
- [x] Add a test pinning the invariant this depends on: `bm25_index` must be
      the **last** key `rmp_serde::to_vec_named` emits for the snapshot
      envelope. A future field reorder must fail loudly, not silently cost
      180 ms again.

### LAZY-2: on-demand BM25 access without losing it on save [1/1]

- [x] Decide and implement how `SnapshotIndex::bm25_index()` gets its data
      after an early-stopped load. Two candidates, both viable: (a) a
      `load_with(bm25: bool)` decision made at the call site, or (b) a lazy
      re-read keyed off the stored index path plus a `OnceCell`. Whichever
      wins, record the reasoning — the queries that *do* search text must not
      regress by more than the cost of one extra read.
- [x] Close the save hazard. `write_snapshot` re-serializes
      `self.bm25_index`; after a lazy load that field is absent, so `set`,
      `remove`, `append`, `task toggle`, `mv` and `lint --fix --index` would
      write a snapshot with the BM25 section silently deleted. Either load
      eagerly whenever a save is possible, or carry the raw BM25 bytes through
      a save unchanged.
- [x] Regression test the hazard directly: build an index with BM25, run a
      mutating command against it, then assert a text `find --index` still
      returns BM25-ranked results from the rewritten snapshot.

### LAZY-3: preserve the security contract [1/1]

- [x] SEC-3 (`Bm25InvertedIndex::total_postings` posting cap) and MED-1
      (`validate_doc_ids` bounds check) run today *before* the index is
      exposed, and `load_inner` rejects the entire snapshot on failure,
      falling back to a disk scan. Under lazy decoding they fire at first use,
      mid-query, where that fallback no longer composes. Decide the new
      contract and implement it so a crafted snapshot is still refused.
- [x] Keep the existing `load_inner_rejects_bm25_*` tests passing, adapting
      them to the new control flow rather than weakening them.

### LAZY-4: measure and record [1/1]

- [x] Re-measure `find --limit 1 --index` and a text `find <query> --index` on
      the MDN vault, before and after, release build, median of ≥ 5 runs.
      Confirm the projected ~2.4× on the non-text path and quantify whatever
      the text path now pays.
- [x] Record the result in [[research/snapshot-load-floor-2026-09-01]] and in
      the changelog.

## Acceptance criteria

- [x] `.hyalo-index` files written before this iteration load unchanged, and
      files written after it are readable by the previous release — the wire
      format is byte-identical.
- [x] No mutating command can drop the BM25 section from a snapshot it
      rewrites, proven by a test.
- [x] The SEC-3 / MED-1 rejections still refuse a crafted snapshot.
- [x] `find --limit 1 --index` on a MDN-scale vault is measurably faster, with
      numbers in the outcome; a text `find --index` is not materially slower.
- [x] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo test --workspace -q`, all `xtask check-*`,
      `hyalo lint --strict`.

## Outcome

Landed. MDN vault (14 375 entries, 121 614 239-byte index), release build,
median of 7 runs, warm cache; `before` = `origin/main` at the 259 merge:

| command | before | after | |
|---|---:|---:|---|
| `find --limit 1 --index` (no text query) | 396.2 ms | **151.3 ms** | **2.6x** |
| `find promise --limit 10 --index` (BM25 path) | 398.8 ms | 402.1 ms | +0.8 %, noise |

The after-number landed within 1 ms of the ~150 ms projection. The text path —
the open question, since a deferred-then-decoded section does strictly more
work than an inline one — pays nothing measurable; mutating commands take the
same forced-decode path via `save_to` and are covered by that number.

Cross-version compatibility verified in both directions: an index written by
`origin/main` reads on the branch and vice versa, with identical BM25 scores.
The two files differ byte-for-byte, but so do two indexes built back to back by
the *same* binary (`created_at`/`pid` in the header, `HashMap` iteration order
in the graph) — pre-existing nondeterminism, unrelated to this change; the
schema and key order are what stayed identical.

Design decisions recorded as [[decision-log#DEC-265]]: keep the snapshot buffer
rather than re-read the file, force the decode on save rather than splice raw
bytes, and demote SEC-3/MED-1 from "reject the snapshot" to "refuse the
section". Full numbers in [[research/snapshot-load-floor-2026-09-01]].

## Non-goals

- Changing the on-disk snapshot format. DEC-264 explicitly parked the
  "opaque length-prefixed BM25 blob" design: it is the more robust shape but
  breaks every index in the field to buy the same 180 ms.
- Shrinking the BM25 index itself (e.g. dropping or delta-encoding
  `Posting::positions`). That is a separate size/latency trade with its own
  correctness surface — phrase matching depends on those offsets.
- Re-opening mmap. Already rejected on macOS in
  [[research/performance-parallelization]], and I/O is 5–17 % of this floor
  anyway.

## Links

- [[iterations/iteration-259-index-snapshot-load-perf]]
- [[research/snapshot-load-floor-2026-09-01]]
- [[decision-log#DEC-264]]
- [[decision-log#DEC-265]]
