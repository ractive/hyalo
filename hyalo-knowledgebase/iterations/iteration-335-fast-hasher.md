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

Experiment 3 of the performance audit. The workspace's hash maps (the audit counted 332;
a grep for `HashMap|HashSet` in `crates/**/*.rs`, `tests/` directories excluded, returns
353 lines on this commit, `use` lines and in-source unit tests included, so the figure
counts mentions, not types, and does not matter to the verdict) all sit on std's
SipHash-1-3 with random keys; the Rust
Performance Book says a faster hasher pays only where hashing is hot. The prototype
profiled first, then swapped only the maps the profile named, to
`foldhash::fast::RandomState` (`foldhash` 0.1.5, already in `Cargo.lock` through
`jaq-json`, licence Zlib, which `deny.toml` allows; `cargo deny check` was not run). Verdict:
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
main thread (rayon workers not counted). Only hash code that was not inlined shows up as
a frame: with fat LTO the rest is inside its callers, and rayon workers were excluded.
So each share below is a **lower bound** on the cost of hashing, not an upper bound on
what a faster hasher could save; the conclusion rests on the measured `user` time
further down, not on these shares. (The vault held one extra snapshot file, outside
the notes, while profiling; the main-thread time of `backlinks --index`, about 130 ms a
run, matches the clean-vault `user` time below.)

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
- `find --index` shows no out-of-line hash frame on the main thread, so the bar that it
  gets 20% faster had no visible mechanism behind it from the start (inlined or
  worker-side hashing is not excluded by this).

## Prototype

`pub(crate) type FastMap<K, V> = HashMap<K, V, foldhash::fast::RandomState>` and the
maps above switched to it (32 lines added and 23 removed in seven files, plus a
10-line module, one new dependency edge for `hyalo-core`). `foldhash::fast::RandomState` keeps a per-map random seed, so
iteration order stays as unspecified as under std's `RandomState` and no new
determinism is promised.

HashDoS: yes, these maps hash attacker-influenceable text (paths, link targets and
index terms from the notes of a vault, which may be a cloned repository). The worst
case is a slowdown from crafted collisions, not memory unsafety; foldhash's per-map
seed gives less protection than SipHash, not none.

## Numbers

MDN (14,375 notes), baseline `afe0d162` against the prototype, both release builds (fat
LTO) of the same sources apart from the prototype, each binary reading the snapshot it
wrote (stored outside the vault, see below), runs interleaved, medians of 10 (two blocks
of 5, same session) from `/usr/bin/time -l`, which reports wall, `user` and `sys` to
10 ms. Peak RSS in MB, 1-minute load average (median) beside it.

| command | wall base / cand | user base / cand | sys base / cand | RSS base / cand | load |
|---|---|---|---|---|---:|
| `backlinks web/css/index.md --index-file <snapshot>` | 0.280 / 0.270 s (-3.6%) | 0.160 / 0.150 s (-6.2%) | 0.110 / 0.110 s | 258.3 / 259.7 (+0.5%) | 7.4 |
| `find --index-file <snapshot>` (no pattern, default fields and limit) | 0.190 / 0.190 s (0%) | 0.080 / 0.085 s (+6.2%, one 10 ms step) | 0.100 / 0.100 s | 229.1 / 230.1 (+0.4%) | 7.4 |
| `summary --index-file <snapshot>` | 0.460 / 0.440 s (-4.3%) | 0.340 / 0.315 s (-7.4%) | 0.110 / 0.110 s | 272.2 / 273.4 (+0.4%) | 8.9 |
| `find "flexbox grid layout" --index-file <snapshot>` (ranked) | 0.575 / 0.570 s (-0.9%) | 0.460 / 0.460 s (0%) | 0.140 / 0.145 s | 418.1 / 419.6 (+0.4%) | 9.3 / 10.0 |

All four with `--dir mdn --site-prefix en-US/docs`. The `backlinks` target and the
`find` form differ from [[iterations/iteration-334-mimalloc]] (which used
`web/javascript/reference/global_objects/array/index.md` and `find --index --limit 0
--fields file`), so only the ratios inside each experiment compare, not the absolute
times. Noise on this run: the baseline `backlinks` wall ranged 0.26 to 0.52 s over 10
runs with one slow outlier, the candidate 0.25 to 0.32 s; the other rows stayed within
0.19 to 0.21, 0.44 to 0.50 and 0.55 to 0.66 s.

**A measurement error, corrected.** The first version of this note reported `sys`
times of 1.5 to 1.9 s against walls of 0.45 to 0.85 s, and walls and `user` about
twice these. Cause, reproduced afterwards: my harness wrote the three snapshots
(141 MB each) *into the vault directory*. With two extra 141 MB files copied in,
`backlinks` went from 0.26 s real, 0.15 s `user`, 0.10 s `sys` to 0.53 to 0.70 s real,
about 0.25 s `user` and 1.8 to 3.0 s `sys` on the same binary. Why the extra files cost
that much `sys` time was not investigated. Those figures are withdrawn and the table above is a re-run with
the snapshots outside the vault (`create-index -o <dir> --allow-outside-vault`), where
`sys` is 0.10 to 0.14 s and cannot exceed wall. The first run's conclusion did not
change (candidate level or within a few percent), but its ratios were taken on a
vault where a constant, unrelated cost diluted any saving, so they are not used.

Not measured, because the first criterion had already decided it: `tags --index`
(only in the withdrawn run), disk commands, `create-index` and the 500-note vault
against the 3% bar, the cross-target builds and `cargo deny check`.

## Iteration order and the snapshot

Byte comparison. Commands: `find --fields all --limit 0`, `summary`, `tags`,
`properties`, `backlinks` (up to three notes per corpus), five ranked queries (AND,
`OR`, a `~3` phrase, a prefix with a negation under `--granularity section`, one with
two `--facet`s), `lint --limit 0 --max-per-rule 0` and `find --broken-links --limit 0`:
11 commands plus the backlinks targets. The unit is one *comparison* = one command on
one corpus in one mode (disk or `--index`), with stdout and exit code both compared.
MDN had 14 commands (three backlinks targets); the Obsidian Hub and GitHub Docs
together supplied 3 backlinks targets, not 6 (the pick was the first three notes
with no space in their path among the top 12 by backlinks; I did not recount which
corpus fell short, and the output files are deleted). That gives 39 command-and-corpus
cases in two modes: **78 of 78 baseline-against-candidate comparisons identical**
(stdout and exit code). The one normalisation is the hint text `Command took N ms`,
which differs in any two runs of the same binary; with it unmasked 15 disk-mode
comparisons differ and every difference is that number. Disk against `--index` was
equal within each binary on **39 of 39 pairs** (hints and the
`source`/`index_format_version` fields stripped). The negative control
(two different queries, and a one-byte mutation of a good output) is rejected by the
same comparison. Stderr is identical in 76 of the 78 comparisons; the two that differ are
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

- [x] The speed bar is missed: `backlinks --index` -3.6% wall (-6.2% `user`) and
      `find --index` 0% wall (+6.2% `user`, one 10 ms step), against 20% required on
      both.
- [x] The memory bar is missed: peak RSS +0.5% and +0.4%, against -25%.
- [x] `summary --index` (-4.3% wall) and ranked `find --index` (-0.9%) are not slower,
      so that criterion holds; it does not rescue the first.
- [x] Byte identity holds (78 of 78 comparisons, 39 of 39 disk-against-index pairs), so
      correctness was not the reason.
- [x] Code reverted; only this note and the research note are committed.

Why this is DISCARD and not NOT SHOWN: the rule applies to a result within noise of a
threshold. Here the best wall figure is -3.6% against a 20% bar, 16 points short, and
the clean-vault noise is a few percent (one 10 ms step on 190 to 280 ms), so the
effect and the noise are both far below the bar. The first run's 20% wall noise came
from the vault pollution described above, not from the machine, and is not the
argument. `user` time, the least contended column, carries the evidence: -6% on
`backlinks --index`, level on `find --index` and ranked `find`, -7% on `summary
--index`; no run comes near 20%.

What the profile adds is a floor, not a ceiling: out-of-line SipHash frames are 5.5%
of the main thread in `backlinks --index` and 11.5% in `summary --index`, a lower
bound on the hashing cost because inlined hash code and worker threads are not
counted, and the measured `user` savings (6% and 7%) are of that size. Whatever the
hidden share is, the whole command changed by single-digit percent when the hashers
of every map the profile named were replaced. The rest of the work is reading and
decoding the snapshot, `stat` calls (the stale-index check) and freeing. The same
lesson as in [[iterations/iteration-332-skip-teardown-at-exit]] and
[[iterations/iteration-334-mimalloc]]: a sampled share does not turn into a wall-time
saving of the same size.

Left open: replacing the HashMap with a different structure where the map is
decoded (the 297 self samples of the postings map's `Deserialize` in ranked `find`)
would be a layout change, not a hasher change, and is outside this experiment.
