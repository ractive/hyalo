---
title: "Rust performance audit, November 2026: repeated work before data layout"
type: research
date: 2026-11-07
tags: [research, performance, allocations, index, lint]
status: completed
related: ["[[iterations/iteration-330-remove-repeated-work]]", "[[decision-log]]", "[[iterations/iteration-335-fast-hasher]]"]
---

# Rust performance audit, November 2026

An audit of where hyalo spends time on a large vault found that most of it is the
same work done twice, not the shape of the data. Iteration 330 removes that work
without changing an answer. Every figure below says whether it was **measured**
(this iteration, on a copy of the corpus, `HYALO_INTERNAL_ALLOC_REPORT`, median
of 5) or **estimated** (the audit, or arithmetic from a measurement).

## The audit's ranked findings

| # | Finding | Audit estimate | Iteration 330 |
|---|---------|----------------|---------------|
| 1 | A named read (`backlinks`, `read`, `find --file`, `lint FILE`) under `--index` always rescans and rebuilds the graph and decodes BM25 | 6.86M allocations, 0.72 s | fixed, see below |
| 2 | `rebuild_graph` runs twice per load | about 0.9M each, about 20% of `tags --index` | fixed, graph is lazy |
| 3 | `rule.check(&doc)` re-parses comrak for 10 of 14 default rules | about 57M allocations | fixed |
| 4 | Upstream MD034 builds a `Vec<char>` per character | 256M allocations, about 10 of 19.6 s user CPU | fixed |
| 5 | `AliasVisitor` reads the whole note | about -3.4M of 7.9M allocations | partly: body reads halved, allocations not reproduced |
| 6 | `heading_slugs` rebuilt per fragment | about 1M, inferred | not reproduced, left |
| 7 | BM25 `term.to_owned()` per posting; a `Vec` per posting | 2.45M each | first fixed, second left |

## Not worth doing (the audit's list)

- Interning in `IndexEntry`, `Link` and `OutlineSection`: about 20 ms (estimated).
- The scanner and YAML parser: disk CPU is 78% kernel time, so there is little user
  code to win back (estimated from samples).
- Fuzzy scoring, `find` JSON output and `--jq`, token interning: below the noise.

## Allocator and build profile

- The allocator was 17-27% of samples across runs. A single run that suggested 42%
  was wrong and is not used.
- The release profile already has fat LTO, `codegen-units = 1` and `panic = "abort"`;
  there is no build-flag win left.
- Out of scope here, and needing this iteration as their baseline: `LinkGraph`'s
  public types, string interning, a different allocator. (Skipping teardown at exit
  was tried afterwards and discarded: see "Skipping teardown at exit" below. A
  different allocator was tried in iteration 334 and discarded: see "mimalloc as the
  global allocator" below.)

## External reading

- Goose benchmark summary (<https://github.com/aardappel/goose/blob/master/bench/summary.md>,
  recorded from the iteration brief, not re-read): `bintrees` takes 206/204 ms
  against Rust's arena at 235 ms, per-node rows take over 3 s, `records` is
  2.02-2.26x faster on 3.98x less memory, flat numeric rows are nearly level, and
  the headline figure combines different implementations. Read: layout matters for
  pointer-heavy trees and row-per-node data; hyalo's hot data is neither.
- The Rust Performance Book on heap allocations
  (<https://nnethercote.github.io/perf-book/heap-allocations.html>): reduce
  allocations first. That is what this iteration does.

## The Rust Performance Book against hyalo

Source: <https://nnethercote.github.io/perf-book/>, read in full on 2026-11-07. The
book's own rule is that the biggest wins usually come from algorithms and data
structures, not from tuning; that matches the audit, where the wins were repeated work.
Everything under "candidates" is **unmeasured**: each needs a measurement before any
change, and none is part of iteration 330.

**Already done**
- `codegen-units = 1`, fat LTO and `panic = "abort"` in the release profile.
- rayon for the per-note scans.
- `BufReader` and raw-byte line splitting in the scanner.

**Candidates (measure first)**
- **Fast hasher** (`rustc-hash` or `ahash`). hyalo has 332 std `HashMap`/`HashSet`
  uses and none sets a hasher. The book's caveat: only worth it if hashing is hot,
  and only where HashDoS is not a concern. Notes are local files, but a hasher swap
  also changes iteration order, so any output that leaks map order would change.
  Tried in iteration 335 and discarded (see "A faster hasher for the hot maps" below).
- **`mimalloc` or `tikv-jemallocator`.** The allocator was 17-27% of samples, so
  there is room, but the book calls the gain workload-dependent; it also needs a
  check on all three platforms and on peak memory (`create-index` peaks at 850 MiB).
  Listed as a separate experiment above; `mimalloc` was tried in iteration 334 and
  discarded (see "mimalloc as the global allocator" below), `tikv-jemallocator` was
  not tried.
- **PGO via `cargo-pgo`.** The book quotes "10% or more". It does not reach users who
  `cargo install`, so it would only help the prebuilt release binaries, and it adds a
  training-run step to the release pipeline.
- **Hot types over 128 bytes.** The book notes they are `memcpy`d; check
  `IndexEntry`, `Link` and `OutlineSection` with `-Zprint-type-sizes` or
  `top-type-sizes`. The audit already put interning those types at about 20 ms.
- **`dhat-rs`-style heap tests** as a regression guard for allocation counts. The
  `HYALO_INTERNAL_ALLOC_REPORT` counter already gives the numbers; a test that
  asserts a ceiling on a fixed fixture would stop a repeat of the work removed here.

**Not applicable**
- `target-cpu=native`: the binaries are distributed, so it would be wrong on other CPUs.
- `parking_lot`: locks are not hot here.
- `get_unchecked`: the book's last resort, and nothing here justifies unsafe.

## Measured results

MDN (14,375 notes), baseline `5e2512ed` against iteration 330. Allocations are
the median of 5 runs and are deterministic to within 0.1%. Wall time is the median
of 5 on a shared machine with a load average between 6 and 167, so it is
noise-bound and only the allocation counts carry the claim.

| Command | Allocations before | after | Peak live before | after |
|---------|-------------------:|------:|-----------------:|------:|
| `tags --index` | 3.11M | 0.95M | 247 MiB | 206 MiB |
| `summary --index` | 5.31M | 4.24M | 247 MiB | 234 MiB |
| `backlinks --index` (one note) | 6.81M | 2.02M | 373 MiB | 217 MiB |
| `find --fields links --index` | 5.84M | 3.67M | 420 MiB | 391 MiB |
| `find --file --index` (one note) | 6.74M | 0.86M | not recorded | not recorded |
| ranked `find --index` | 5.81M | 3.66M | 374 MiB | 345 MiB |
| `summary` (disk) | 7.40M | 7.41M | 110 MiB | 110 MiB |
| `lint` (default rules) | 331.89M | 34.40M | 42 MiB | 41 MiB |
| `lint` (every stock rule on) | 483.47M | 84.35M | not recorded | not recorded |
| `create-index` (cold) | 49.26M | 46.93M | 854 MiB | 847 MiB |
| ranked `find` (disk) | 30.28M | 27.95M | 429 MiB | 429 MiB |

Wall time, same runs, indicative only: `tags --index` 0.35 s to 0.20 s,
`backlinks --index` 0.78 s to 0.28 s, `lint` 162 s (load 167) to 8.2 s (load 31),
`create-index` 12.3 s to 10.1 s, disk `summary` 2.83 s to 2.71 s.

### What each result says

- **Item 1 and 2** are one saving for `--index`: the graph is resolved on first read
  instead of at load, at `configure` and after every refresh. `tags --index` never
  reads it, so it never pays (-69% allocations, measured). `backlinks --index`
  needs it once and no longer decodes BM25 (-70%, measured). The audit's 0.9M per
  rebuild matches: 3.11M - 0.95M = 2.16M saved by removing two rebuilds and the
  decode that followed.
- **Item 3 and 4**: lint allocations fall by 297M (-90%) with the default rules and
  by 399M (-83%) with every stock rule on. Sharing the parse is limited to the 26
  `AstRule`s: MD051, MD056 and MD059 answer differently for `Some` and `None`, so a
  first draft that shared with every rule changed `lint` output (caught in review).
  Uncapped (`--limit 0 --max-per-rule 0`) the first draft read MD051 0 to 286 and MD056 422 to 0 on GitHub Docs, and MD051 5 to 2377 and MD056 15 to 0 on MDN. The default rules do not include those three, which is why the default-rule
  number did not move when the allow-list came in.
- **Item 5**: the one-line `needs_body` fix halves logical body reads on a disk
  scan (28,750 to 14,375, measured), but disk `summary` allocations did not move
  (7.40M to 7.41M). The audit's -3.4M assumed the alias pass is replaced by the scan's
  own entries; that was not built. The saving is I/O, not allocation.
- **Item 6** did not reproduce. `find --broken-links` costs 0.14M allocations more
  than `find --fields links` on GitHub Docs (2.58M against 2.44M) and 0.53M more on
  MDN, which bounds everything the anchor check does, `heading_slugs` included,
  below the audit's 1M. On the Obsidian Hub it costs less than `--fields links`.
- **Item 7**: `create-index` -2.33M and disk ranked `find` -2.33M, matching the
  audit's 2.45M. The `Vec<u32>` per posting (2.45M per text query) was left: `Posting`
  is serialized and read in a dozen places, and an arena that keeps the wire format
  is not contained.

### Report-only

- **Double `stat` per note** (`snapshot_drift`, `index.rs`): the filter stats a
  note and, when the size matches (the usual case), `entry_is_stale_on_disk` stats
  it again. The second stat guards nothing: its answer only decides whether the
  note is re-scanned, and the re-scan reads the file anyway. Removable by passing
  the metadata already taken. About one extra `stat` per note, 14,375 on MDN; the
  time was not measured.
- **`rooted.rs` temp-file copy**: does not happen for read-only `lint`. A plain
  `lint` and `lint --fix --dry-run` both succeed with `TMPDIR` pointing at a
  directory that does not exist, so no temp file is created; only
  `FixMode::Apply` calls `capture_fix_input`. For `lint --fix` the copy is the
  exact-bytes snapshot the later `verify()` compares against (a lost-update
  guard). It could hold the bytes in memory instead of a temp file and keep the
  guarantee; it could not be dropped without losing it. The audit's "80% of
  lint-floor samples" therefore belongs to `--fix`, if to anything.

## Skipping teardown at exit

Experiment 1, run on 2026-11-08 against criteria fixed beforehand; the full table is
in [[iterations/iteration-332-skip-teardown-at-exit]]. Verdict: **discarded**.

- `mem::forget` of the snapshot after the output is flushed: `backlinks --index` on
  MDN 0.26 s to 0.24 s (-7.7%), `find --index` 0.20 s to 0.18 s (-10%), `summary
  --index` -2%, ranked `find --index` and the disk scan unchanged, peak RSS -1%.
- The ceiling, `process::exit(0)` right after the flush so that no destructor runs:
  -11% and -10%. No version of the idea reaches the 20% bar, and peak memory cannot
  move because teardown follows the peak.
- The brief's 25% of samples on the free side was profiled before iteration 330, when
  `backlinks --index` still decoded BM25. [[research/snapshot-load-floor-2026-09-01]]
  measured teardown directly at 59 ms: 43 ms freeing the BM25 index, 16 ms entries
  plus graph. Without the BM25 decode about 16 ms of 0.26 s (about 6%) is left, in line
  with the 8 to 11% measured here. A profile and a wall time from different code
  states do not compare.

## mimalloc as the global allocator

Experiment 2, run on 2026-11-08 against criteria fixed beforehand; the full table is
in [[iterations/iteration-334-mimalloc]]. Verdict: **discarded**.

- `mimalloc` 0.1.52 (v2) under the counting allocator wrapper, MDN, interleaved
  against the baseline, load 10 to 14: `backlinks --index` 0.54 s to 0.49 s (-9%),
  `find --index` 0.47 s to 0.44 s (-6%), `summary --index` -10%, ranked `find --index`
  -10%, peak RSS -4 to -6%. The bars were 20% faster or 25% less memory on both.
- User CPU time did drop 17 to 21% in every row, matching the audit's allocator share
  of samples. Wall time did not follow because wall time also holds kernel time
  (snapshot read, page faults) that the allocator barely moves; `sys` exceeded
  `user` in the runs I looked at, though load inflates it.
- A five-run pass at load 15 to 18 reported -57% and -60%; the baseline's `sys` time
  varied 2x between identical runs, so it measured machine contention. Compare ratios
  from interleaved runs, and look at `user` when the machine is not idle.
- A 500-note subset of GitHub Docs: wall time level, but peak RSS of the disk commands
  rose 36 to 37% (14 to 19 MB, 19 to 26 MB), which fails the 3% criterion.
- Snapshot sizes identical, allocation counts identical to within 22 of 2.3 million.
  Cross-target builds and `cargo deny` were not checked: the first bar had failed.

## A faster hasher for the hot maps

Experiment 3, run on 2026-11-08 against criteria fixed beforehand; the full tables
are in [[iterations/iteration-335-fast-hasher]]. Verdict: **discarded**.

- Profile first (`sample`, MDN, main-thread samples): SipHash `write`/`hash_one` is
  5.5% of `backlinks --index`, 0% of `find --index`, 11.5% of `summary --index` and
  1.6% of ranked `find --index`. The hot maps are `CaseInsensitiveIndex`'s four
  (probed per link), `LinkGraph::index`/`lower_index`, `heading_slugs`' `seen` and the
  BM25 `postings` map (whose hash is inlined into its decoder and cannot be split out).
- Prototype: those maps on `foldhash::fast::RandomState` (0.1.5, already in the lock
  through `jaq-json`). Medians of 10, interleaved, load 7 to 10: `backlinks --index`
  0.545 s to 0.540 s wall (-0.9%), `user` 0.250 s to 0.240 s (-4.0%); `find --index`
  0.465 s to 0.450 s (-3.2%, `user` unchanged, and its code path hashes nothing);
  `summary --index` `user` -7.1%, wall level; ranked `find --index` level; peak RSS
  +0.1% to +0.5%. The bars were 20% faster or 25% less memory on both.
- Wall time moved by up to 20% between two blocks of five on the same command, so only
  `user` is readable, and it matches the profile share (a real hasher recovers part
  of 5.5%, not 20%).
- Correctness was not the problem: 156 of 156 JSON outputs and exit codes identical
  between baseline and candidate on MDN, the Obsidian Hub and GitHub Docs, disk and
  `--index`, once the hint text `Command took N ms` is masked. Iteration order does
  not reach any output, which is expected because std's `RandomState` already orders
  maps differently on every run. Snapshot size 141,795,837 bytes against 141,795,826
  and 141,795,846 from two baseline builds: the baseline's own bytes vary by 20, and
  each binary loaded the other's snapshot.
- Not run, because the first bar had failed: disk commands, `create-index` and the
  500-note vault against 3%, cross-target builds, `cargo deny check`.

## Method note

Differential testing against a baseline binary caught nothing this time, and the
table-driven tests carry the forms the corpora lack. See
[[iterations/iteration-330-remove-repeated-work]] for the proof.
