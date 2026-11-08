---
type: iteration
title: "Iteration 335: a faster hasher for the hot hash maps (experiment, discarded)"
date: 2026-11-08
status: completed
tags:
  - iteration
  - performance
  - experiment
branch: iter-335/fast-hasher
related:
  - "[[research/rust-performance-2026-11]]"
  - "[[iterations/iteration-334-mimalloc]]"
---

# Iteration 335: a faster hasher for the hot hash maps (experiment, discarded)

Experiment 3 of the performance audit. The workspace has about 340 `HashMap`/`HashSet`
uses outside tests, all on std's SipHash-1-3 with random keys; the Rust
Performance Book says a faster hasher pays only where hashing is hot. The prototype
profiled first, then swapped only the maps the profile named, to
`foldhash::fast::RandomState` (`foldhash` 0.1.5, already in `Cargo.lock` through
`jaq-json`, licence Zlib, so `cargo deny` has nothing new to see). Verdict:
**DISCARD**. No code is merged; this note and [[research/rust-performance-2026-11]]
record the numbers.

## Criteria (fixed beforehand by the owner)

KEEP only if `backlinks --index` and `find --index` both get at least 20% faster, or
peak memory drops at least 25% on both; and `summary --index` and ranked
`find --index` are not slower; and disk commands, `create-index` and a 500-note vault
are no worse than 3%; and the snapshot is not larger; and JSON is byte-identical,
disk against `--index` and candidate against baseline, on the Obsidian Hub, GitHub
Docs and MDN. A result within noise of a threshold is NOT SHOWN, not KEEP.

## Profile: which maps are hot

`sample` (1 ms) on a release build with symbols and line tables, MDN (14,375 notes),
five runs per command, load 4 to 5. Hash frames counted are `Hasher::write` for
SipHash-1-3 and `BuildHasher::hash_one`, attributed to their caller, as samples of the
main thread (rayon workers not counted).

| command | hash samples | main-thread samples | share |
|---|---:|---:|---:|
| `backlinks --index` | 36 | 651 | 5.5% |
| `find --index` | 0 | 221 | 0% |
| `summary --index` | 159 | 1,385 | 11.5% |
| ranked `find --index` | 29 | 1,768 | 1.6% |

The hot maps, by caller:

- `CaseInsensitiveIndex::{map, stem_map, tail_map, alias_map}` (`case_index.rs`),
  probed once per link from `catalog::literal` and `discovery::probe_existence`:
  the largest share in `backlinks --index` and `summary --index`.
- `LinkGraph::index` (`insert_file_links`, `link_graph.rs`) and `lower_index`.
- `anchor::heading_slugs`' per-call `seen` map (`summary`'s broken-anchor count).
- `Bm25InvertedIndex::postings`, the term map decoded for ranked `find`. Its
  `Deserialize` is the biggest single symbol of that command (297 self samples across
  five runs), but the hash is inlined into it and cannot be separated; the 29 above
  are only the out-of-line hash frames.
- Nothing hashes in `find --index`: it has no hash-bound frame at all, so the bar
  that `find --index` gets 20% faster had no mechanism behind it from the start.

## Prototype

`pub(crate) type FastMap<K, V> = HashMap<K, V, foldhash::fast::RandomState>` and the
maps above switched to it (about 40 changed lines, one new dependency edge for
`hyalo-core`). `foldhash::fast::RandomState` keeps a per-map random seed, so
iteration order stays as unspecified as under std's `RandomState` and no new
determinism is promised.

HashDoS: yes, these maps hash attacker-influenceable text (paths, link targets and
index terms from the notes of a vault, which may be a cloned repository). The worst
case is a slowdown from crafted collisions, not memory unsafety; foldhash's per-map
seed gives less protection than SipHash, not none.

## Numbers

MDN, baseline `afe0d162` against the prototype, both release builds (fat LTO) from the
same worktree, each binary reading the snapshot it wrote, runs interleaved, medians
of 10 (two blocks of 5, same session) from `/usr/bin/time -l`, wall and `user` and
`sys` in seconds, peak RSS in MB, 1-minute load average (median) beside it.
`sys` exceeds wall because the load is multi-threaded; this machine's `sys` is the
noisiest column (1.5 to 2.0 s for the same command).

| command | wall base / cand | user base / cand | sys base / cand | RSS base / cand | load |
|---|---|---|---|---|---:|
| `backlinks web/css/index.md --index` | 0.545 / 0.540 (-0.9%) | 0.250 / 0.240 (-4.0%) | 1.77 / 1.86 | 264.1 / 264.4 (+0.1%) | 7.6 |
| `find --index` | 0.465 / 0.450 (-3.2%) | 0.180 / 0.180 | 1.87 / 1.68 | 235.9 / 236.7 (+0.3%) | 8.8 |
| `summary --index` | 0.705 / 0.705 (0%) | 0.425 / 0.395 (-7.1%) | 1.80 / 1.72 | 275.0 / 276.3 (+0.5%) | 8.7 |
| ranked `find "flexbox grid layout" --index` | 0.830 / 0.830 (0%) | 0.555 / 0.555 | 1.62 / 1.63 | 421.1 / 422.2 (+0.3%) | 8.9 |
| `tags --index` | 0.455 / 0.450 (-1.1%) | 0.180 / 0.180 | 1.55 / 1.49 | 235.9 / 236.4 (+0.2%) | 10.0 |

The two blocks of 5 disagree with each other by more than the effect: `backlinks`
read +5.6% wall in the first (load 5.7) and -5.5% in the second (load 9.5);
`find --index`, whose code path hashes nothing, read -20.0% in the first (0.600 s to
0.480 s) and 0% in the second. That spread is the noise floor of this machine, about
20% on wall, so only `user` is readable, and `user` agrees with the profile: -4% where
the profile said 5.5% of samples, -7% where it said 11.5%.

Not measured, because the first criterion had already decided it: disk commands,
`create-index` and the 500-note vault against the 3% bar, the cross-target builds and
`cargo deny check`.

## Iteration order and the snapshot

Byte comparison, `find --fields all --limit 0`, `summary`, `tags`, `properties`,
`backlinks` (three notes per corpus), five ranked queries (AND, `OR`, a `~3` phrase,
a prefix with a negation under `--granularity section`, one with two `--facet`s),
`lint --limit 0 --max-per-rule 0` and `find --broken-links --limit 0`, each on disk
and under `--index`, on MDN, the Obsidian Hub and GitHub Docs, baseline against
candidate: **156 of 156 stdout files and exit codes identical**. The one
normalisation is the hint text `Command took N ms`, which differs in any two runs of
the same binary; with it unmasked 15 disk-mode files differ and every difference is
that number. Disk against `--index` was equal within each binary on 39 of 39 pairs
(hints and the `source`/`index_format_version` fields stripped). The negative control
(two different queries, and a one-byte mutation of a good output) is rejected by the
same comparison. Stderr is identical on 76 of 78 files; the two that differ are
`lint` warnings on GitHub Docs that arrive in a different order, and a baseline run
twice against itself differs in that order too (350 lines each, equal after sorting).

So no output depends on the iteration order of a changed map. That is expected:
std's `RandomState` is already seeded per map and per process, so an output that
depended on map order would already be unstable between two runs of the baseline.

Snapshot: 141,795,837 bytes from the candidate against 141,795,826 and 141,795,846
from two builds of the baseline on the same input. The baseline varies by 20 bytes
between two identical builds, so snapshot bytes are not reproducible today; the
candidate is inside that spread, not larger in any way that means something (the
cause of the baseline's variation was not investigated). Each binary loaded the
other's snapshot and printed the same `backlinks` and `summary` output.

## Verdict: DISCARD

- [x] The speed bar is missed: `backlinks --index` -0.9% wall (-4.0% `user`) and
      `find --index` -3.2% wall (0% `user`), against 20% required on both.
- [x] The memory bar is missed: peak RSS +0.1% and +0.3%, against -25%.
- [x] `summary --index` and ranked `find --index` are not slower (0% wall on both,
      `user` -7.1% and 0%), so that criterion holds; it does not rescue the first.
- [x] Byte identity holds (156 of 156), so correctness was not the reason.
- [x] Code reverted; only this note and the research note are committed.

Why it cannot work here: even a hasher that costs nothing could remove only the
hash share of the profile, 5.5% of `backlinks --index`, 0% of `find --index`, against
a 20% bar; a real hasher removes part of that, and the measured `user` change (-4%)
is that part. The rest of the command is reading and decoding the snapshot, `stat`
calls (the stale-index check) and freeing. The only command with a double-digit hash
share is `summary --index` (11.5%), and it got 7% less `user` time and no wall-time
change. The same lesson as in [[iterations/iteration-332-skip-teardown-at-exit]] and
[[iterations/iteration-334-mimalloc]]: a sampled share is an upper bound on a saving,
and here the share was small to begin with.

Left open: replacing the HashMap with a different structure where the map is
decoded (the 297 self samples of the postings map's `Deserialize` in ranked `find`)
would be a layout change, not a hasher change, and is outside this experiment.
