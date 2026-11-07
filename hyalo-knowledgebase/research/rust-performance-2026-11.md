---
title: "Rust performance audit, November 2026: repeated work before data layout"
type: research
date: 2026-11-07
tags: [research, performance, allocations, index, lint]
status: completed
related: ["[[iterations/iteration-330-remove-repeated-work]]", "[[decision-log]]"]
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
  public types, string interning, a different allocator, skipping teardown at exit.

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
- **`mimalloc` or `tikv-jemallocator`.** The allocator was 17-27% of samples, so
  there is room, but the book calls the gain workload-dependent; it also needs a
  check on all three platforms and on peak memory (`create-index` peaks at 850 MiB).
  Listed as a separate experiment above.
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
| `find --fields links --index` | 5.84M | 4.76M | 420 MiB | 410 MiB |
| ranked `find --index` | 5.81M | 3.66M | 374 MiB | 345 MiB |
| `summary` (disk) | 7.40M | 7.41M | 110 MiB | 110 MiB |
| `lint` | 331.89M | 34.40M | 42 MiB | 41 MiB |
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
- **Item 3 and 4**: lint allocations fall by 297M (-90%). The audit estimated 57M
  plus 256M; the measurement is 313M less 16M that remain in other rules.
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

## Method note

Differential testing against a baseline binary caught nothing this time, and the
table-driven tests carry the forms the corpora lack. See
[[iterations/iteration-330-remove-repeated-work]] for the proof.
