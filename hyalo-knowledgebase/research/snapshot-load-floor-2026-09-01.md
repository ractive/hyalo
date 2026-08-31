---
title: "Where the snapshot-index load floor goes: 87 of 116 MiB is BM25 nobody reads"
type: research
date: 2026-09-01
status: completed
tags: [research, performance, index, bm25, messagepack]
related:
  - "[[iterations/iteration-259-index-snapshot-load-perf]]"
  - "[[iterations/iteration-256-envelope-help-forwarding-and-index-cost]]"
  - "[[iterations/iteration-260-lazy-bm25-snapshot-load]]"
  - "[[decision-log#DEC-264]]"
---

# Where the snapshot-index load floor goes

Iteration 256 recorded a ~0.37 s floor on `find` against an indexed MDN vault
and stopped there, noting only that it "dwarfs everything `find` does
afterwards". This note is the breakdown it did not have: what the floor is made
of, measured stage by stage, and which part of it is avoidable.

The short answer: it is not I/O, and it is not the entries. **76 % of the
snapshot file, and roughly 62 % of the whole command's wall time, is the BM25
inverted index — which a `find` with no text query never reads.**

## Method

- Vault: `~/devel/mdn` (`files/en-us`, 14 399 `.md` files, 14 375 indexed).
- Index: `files/en-us/.hyalo-index`, 121 614 239 bytes (116.0 MiB), rebuilt
  from current `main` with `hyalo create-index` (3.4 s).
- Binary: `cargo build --release`, hyalo 0.22.0, macOS 25.5 (Darwin), Apple
  silicon, APFS/NVMe.
- Command under test: `hyalo find --limit 1 --index --format json`, chosen
  because it does the least possible work downstream of the load, so anything
  it spends is floor.
- Instrumentation: temporary `Instant::elapsed` probes inserted directly into
  `SnapshotIndex::load` / `load_inner` in `crates/hyalo-core/src/index.rs`,
  plus throwaway `Deserialize` variants to isolate decode phases. All of it was
  reverted before this iteration's commit — none of it ships.
- `xtask bench-scale` was **not** used. It generates its own synthetic vault
  and times whole commands end to end; this question needed sub-command-level
  probes inside the load path on a *specific* real 116 MiB index, which
  bench-scale has no way to express. A one-off instrumented build was the
  faster and more trustworthy tool.
- Every figure below is the median of at least two runs that agreed to within
  a few per cent. Warm page cache unless stated otherwise.

## Baseline

| command | wall |
|---|---|
| `find --limit 1 --index` (MDN) | **0.36 s** |
| `find --limit 1` without the index (disk scan) | 0.57 s |
| `hyalo --version` | < 0.01 s |
| `find --limit 1` on an empty vault | < 0.01 s |

256's ~0.37 s reproduces exactly on current `main`. Process startup, config
resolution and arg parsing are together under 10 ms, so essentially all of the
0.36 s is the index.

## Stage breakdown

For one `find --limit 1 --index`, warm cache:

| stage | ms | share |
|---|---:|---:|
| read 116 MiB off disk (warm page cache, 6.0 GiB/s) | 19 | 5 % |
| MessagePack decode of the whole document | **240** | **67 %** |
| SEC-1 path validation (14 375 paths) | 2 | 1 % |
| SEC-3 + MED-1 BM25 validation (walks every posting) | 6–12 | 2 % |
| entries re-sort + `path_index` build | 1.3 | < 1 % |
| `graph.rebuild_lower_index` | 0.7 | < 1 % |
| filter + JSON output | small | — |
| teardown (drop at process exit) | 59 | 16 % |

`load_inner` totals 244–250 ms of that; `SnapshotIndex::load` including the
file read totals ~265 ms.

Cold cache changes little. Reading the same file with `F_NOCACHE` set gives
59–61 ms (1.9 GiB/s) against 19 ms warm — a cold run costs ~40 ms more, which
moves the total from 0.36 s to ~0.40 s. **Disk I/O is 5–17 % of this floor. It
is not the problem.**

## Inside the decode

Re-serializing each field of a decoded snapshot gives the on-disk split:

| section | size | share of file |
|---|---:|---:|
| `entries` (14 375 × `IndexEntry`) | 19 MiB | 16 % |
| `graph` (`LinkGraph`) | 8 MiB | 7 % |
| `bm25_index` (`Bm25InvertedIndex`) | **87 MiB** | **76 %** |

Note that `write_snapshot` already strips per-entry `bm25_tokens` when an
inverted index is present, so the entries above are the *slim* form. The
inverted index is large because every `Posting` carries a `positions: Vec<u32>`
of token offsets for phrase matching — millions of small vectors, each its own
MessagePack array.

Decode costs, isolated:

| what was decoded | ms |
|---|---:|
| whole document, fully materialized (production path) | **240** |
| whole document with everything as `IgnoredAny` (walk, materialize nothing) | **179** |
| whole document, `bm25_index` as `IgnoredAny` | 205 |
| whole document, `graph` **and** `bm25_index` as `IgnoredAny` | 197 |
| `entries` alone, from its own 19 MiB buffer — materialized | 41 |
| `entries` alone, from its own 19 MiB buffer — walk only | 20 |

Two things fall out of that table.

**First: three quarters of the decode is byte traversal, not construction.**
Walking the document while building nothing still costs 179 ms of the 240 ms.
Materializing every structure adds only ~60 ms on top. So the usual
suspects — allocation shape, intermediate buffers, `String` copies — are the
minority cost. `entries` in particular is cheap: 41 ms to materialize all
14 375 of them.

**Second: `IgnoredAny` is not a fix.** Marking `bm25_index` as ignored saves
only 35 ms (240 → 205), because serde still has to token-walk all 87 MiB of
postings to find the end of the value. Skipping the *materialization* is nearly
worthless while the *traversal* remains.

## Teardown is real, and it is also BM25

Dropping the decoded snapshot costs 59 ms, split 16 ms for entries + graph and
**43 ms for the BM25 index** — freeing several million individual `Vec<u32>`
allocations. This is paid at process exit on every single indexed command,
and no profiler that stops at "the command finished" attributes it.

## The avoidable part

Adding up what a text-query-free `find` spends on a BM25 index it never
touches: ~180 ms of decode traversal, ~6–12 ms of SEC-3/MED-1 validation, and
~43 ms of teardown — **~230 ms of a ~360 ms command.**

`rmp_serde::to_vec_named` writes the snapshot as a MessagePack **map with
string keys**, and `bm25_index` is the last key. So the section can be skipped
by simply *stopping the parse* when that key is reached, without reading its
value and without changing a single byte on disk. Measured, with a hand-written
`Deserialize` impl that visits the top-level map and `break`s on `"bm25_index"`:

| decode | ms |
|---|---:|
| production, full document | 240 |
| early-stop at `bm25_index` | **61.5** |

`rmp_serde::from_slice` accepts the unconsumed tail without error. Together
with the skipped validation and teardown this projects `find --limit 1 --index`
from ~360 ms to **~150 ms**, a ~2.4× improvement on the whole command, with the
snapshot format byte-identical and every existing index still readable.

## Why this is not a small patch

The measurement is done; the fix is not a one-liner, for four reasons — each
of which is a design decision, not a coding chore:

1. **`save_to` would destroy search data.** `write_snapshot` re-serializes
   `self.bm25_index`. Every mutating command that patches a loaded snapshot —
   `set`, `remove`, `append`, `task toggle`, `mv`, `lint --fix --index` —
   would then write a snapshot with the BM25 section silently gone. Any lazy
   design must either load eagerly whenever a save is possible, or carry the
   raw BM25 bytes through a save unchanged.
2. **The security checks move.** SEC-3 (`total_postings`) and MED-1
   (`validate_doc_ids`) run today *before* the index is exposed, and
   `load_inner`'s contract on failure is "reject the whole snapshot, fall back
   to a disk scan". A lazily-decoded section fails at first *use*, mid-query,
   where that fallback no longer composes. The guarantee has to be preserved
   under a different control flow.
3. **Someone has to decide when BM25 is needed.** `find <text>` needs it;
   `find --property` does not; `links`, `lint`, `summary`, `properties`, `tags`
   do not. That is either a parameter threaded through the load call sites or a
   lazy re-read keyed off the stored index path — with a measurable cost either
   way for the queries that *do* search text.
4. **Early-stop depends on field order.** It works only because `bm25_index`
   is derive-emitted last. That is an implicit invariant of a `#[derive]` field
   list, which a future edit could silently break with no test failing. It
   needs to be pinned by a test, or replaced by an offset-based skip that does
   not care.

Filed as [[iterations/iteration-260-lazy-bm25-snapshot-load]]. The decision to
pursue it rather than record the floor as inherent is [[decision-log#DEC-264]].

## What was ruled out

- **I/O.** 5 % warm, 17 % cold. Streaming the read, `mmap` (already rejected on
  macOS — see [[research/performance-parallelization]]), or a faster disk
  changes nothing material.
- **Allocation shape in `entries`.** 41 ms to materialize 14 375 entries. There
  is no win hiding there.
- **`IgnoredAny`-based lazy fields.** 35 ms of 240 ms. Traversal, not
  materialization, is the cost.
- **The post-decode reconstruction the plan suspected** — re-sort,
  `path_index`, `rebuild_lower_index` — is 2 ms combined. 256's DEC-259 already
  removed the one genuinely quadratic piece (`CaseInsensitiveIndex::insert`).
