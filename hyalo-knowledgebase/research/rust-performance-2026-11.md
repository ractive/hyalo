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
  Skipped, see "The end of the experiment series" below.
- The scanner and YAML parser: disk CPU is 78% kernel time, so there is little user
  code to win back (estimated from samples).
- Fuzzy scoring, `find` JSON output and `--jq`, token interning: below the noise.

## Allocator and build profile

- The allocator was 17-27% of samples across runs. A single run that suggested 42%
  was wrong and is not used.
- The release profile already has fat LTO, `codegen-units = 1` and `panic = "abort"`;
  there is no build-flag win left.
- Out of scope here, and needing this iteration as their baseline: `LinkGraph`'s
  public types, string interning (skipped, see "The end of the experiment series"
  below), a different allocator. (Skipping teardown at exit
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
  training-run step to the release pipeline. Skipped, see "The end of the experiment
  series" below.
- **Hot types over 128 bytes.** The book notes they are `memcpy`d; check
  `IndexEntry`, `Link` and `OutlineSection` with `-Zprint-type-sizes` or
  `top-type-sizes`. The audit already put interning those types at about 20 ms.
  Interning is skipped, see "The end of the experiment series" below.
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
  `user` in the runs I looked at, though load inflates it (the `sys` claim is unconfirmed: see "A faster hasher for the hot maps" below.)
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

- Profile first (`sample`, MDN, main-thread samples): out-of-line SipHash
  `write`/`hash_one` frames are 5.5% of `backlinks --index`, 11.5% of `summary
  --index`, 1.6% of ranked `find --index`, and `find --index` shows no such frame on the
  main thread. These are lower bounds on the cost of hashing (fat LTO inlines the
  rest, rayon workers were not counted), not ceilings on a saving. The hot maps are
  `CaseInsensitiveIndex`'s four (probed per link), `LinkGraph::index`/`lower_index`,
  `heading_slugs`' `seen` and the BM25 `postings` map (whose hash is inlined into its
  decoder and cannot be split out).
- Prototype: those maps on `foldhash::fast::RandomState` (0.1.5, already in the lock
  through `jaq-json`). Medians of 10, interleaved, clean vault, load 7 to 10:
  `backlinks --index` 0.280 s to 0.270 s wall (-3.6%), `user` 0.160 s to 0.150 s
  (-6.2%); `find --index` 0.190 s level (`user` +6.2%, one 10 ms step); `summary
  --index` 0.460 s to 0.440 s (-4.3%), `user` -7.4%; ranked `find --index` level
  (-0.9%); peak RSS +0.4% to +0.5%. The bars were 20% faster or 25% less memory on both.
  Best wall figure 16 points short of the bar, noise a few percent: not a NOT SHOWN case.
- Measurement error found in review: the first run wrote its three snapshots into the
  vault, and with extra 141 MB files there `backlinks` read 1.8 to 3.0 s `sys` against
  0.53 to 0.70 s real (0.10 s `sys`, 0.26 s real without them). Those numbers, and the
  20% wall noise they implied, are withdrawn; the table above is the re-run with the
  snapshots outside the vault. The cause of the extra `sys` time was not investigated.
  Iteration 334's remark that `sys` exceeded `user` was measured with the same tool
  and is **unconfirmed**: it may have the same cause (snapshots inside the vault), and
  I did not re-run that experiment to find out.
- Correctness was not the problem: 78 of 78 baseline-against-candidate comparisons
  (one command on one corpus in one mode: stdout and exit code) identical on MDN, the
  Obsidian Hub and GitHub Docs, disk and `--index`, once the hint text `Command took N
  ms` is masked; 39 of 39 disk-against-`--index` pairs equal within each binary. Iteration
  order does not reach any output, which is expected because std's `RandomState`
  already orders maps differently on every run. Snapshot size 141,795,837 bytes against
  141,795,826 and 141,795,846 from two baseline builds: the baseline's own bytes vary by
  20, and each binary loaded the other's snapshot.
- Not run, because the first bar had failed: disk commands, `create-index` and the
  500-note vault against 3%, cross-target builds, `cargo deny check`.

## The end of the experiment series

The owner's bar was 20% faster wall time on both `backlinks --index` and `find --index`.
Iterations 332, 334 and 335 tested it and were discarded. PGO and interning, the two
remaining candidates, are **skipped on a measured bound, not run**:

- On MDN, `backlinks web/javascript/index.md --index` measured real 0.25 s, user 0.15 s
  and sys 0.10 s with default threads and with `RAYON_NUM_THREADS=1`, 3 runs each, load
  about 7 on 2026-11-08 (rechecked on `224f2dd9` at load 5.6 to 6.0: real 0.25 to
  0.27 s, user 0.14 to 0.15 s, sys 0.09 to 0.11 s, both thread settings). The read is
  effectively single-threaded, wall is about user plus sys, and a wall saving cannot
  exceed the user saving.
- 20% of wall needs user time to fall about 33% on `backlinks --index` (0.05 s of
  0.15 s). On `find --index` it needs about 47%, from iteration 335's clean run: 0.19 s
  wall, 0.08 s user (0.038 s of 0.08 s).
- PGO reduces user-space work only. The Rust Performance Book quotes "10% or more", and
  a 33 to 47% cut in user time would be far beyond published PGO results for comparable
  workloads. Interning attacks the allocation cost that `mimalloc` already attacked: it
  saved about 20% of user time and still missed, and the audit estimated interning at
  about 20 ms. Interning would also shrink the heap, and so part of the page reclaims
  behind the sys share, but the sys-side effect is expected to be small: `mimalloc`
  lowered peak RSS by only 4 to 6%, and the 141 MB snapshot dominates what is loaded.
  Neither candidate is expected to reach the bar. This is a bound from those
  measurements, not a measurement of PGO or interning.
- About 40% of the read is sys time (0.10 s of 0.25 s, about 15,800 page reclaims,
  255 MB maximum RSS in the recheck), consistent with loading the 141 MB snapshot. Any
  further gain has to come mainly from reading less of the snapshot, not from faster
  user code.

### The one saving that was left: a second `stat` per file

Not an experiment against the bar, a plain fix ([[iterations/iteration-336-snapshot-drift-single-stat]]).
`snapshot_drift`, the per-file drift check (DEC-302, DEC-339), run by unscoped or graph-reading `--index`
reads (`find`, `backlinks`, `summary`) whether or not anything drifted,
called `std::fs::metadata` on each indexed file to compare the size and then again,
inside `entry_is_stale_on_disk`, to compare the mtime: 14,375 extra `stat` calls on MDN.
It now reuses the first `Metadata`. Interleaved, median of 5, snapshot outside the vault,
load about 5.8 to 6.0:

| Command | Wall before | after | User before | after | Sys before | after |
|---------|------------:|------:|------------:|------:|-----------:|------:|
| `backlinks web/javascript/index.md --index-file` | 0.25 s | 0.23 s | 0.15 s | 0.14 s | 0.10 s | 0.08 s |
| `find --count --index-file` | 0.21 s | 0.18 s | 0.10 s | 0.10 s | 0.10 s | 0.08 s |

The saving is sys time, about 0.02 s, which matches the order of magnitude of 14,375
`stat` calls. Wall falls about 8% on `backlinks` and about 14% on `find`, short of the
bar, and the `find` command measured here (`--count`) is not iteration 335's. Output was
byte-identical on both commands.

## Method note

Differential testing against a baseline binary caught nothing this time, and the
table-driven tests carry the forms the corpora lack. See
[[iterations/iteration-330-remove-repeated-work]] for the proof.
