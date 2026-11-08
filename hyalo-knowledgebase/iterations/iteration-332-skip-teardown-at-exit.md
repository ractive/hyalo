---
type: iteration
title: "Iteration 332: skip teardown at exit (experiment, discarded)"
date: 2026-11-08
status: completed
tags: [iteration, performance, experiment]
branch: iter-332/skip-teardown-at-exit
related:
  - "[[research/rust-performance-2026-11]]"
---

# Iteration 332: skip teardown at exit (experiment, discarded)

Experiment 1 of the performance audit (both the label and the audit's profile
sampling come from the iteration brief; neither is recorded in a KB document): once a command has written its output,
stop freeing the snapshot, the link graph and the entry maps one allocation at a
time and let the OS reclaim the heap. The brief put about 25% of
`backlinks --index` samples on that free side. The criteria below were fixed by the
owner before any result. Verdict: **DISCARD**. No code is merged; this note and
[[research/rust-performance-2026-11]] record the numbers.

## Criteria (fixed beforehand)

KEEP only if `backlinks --index` and `find --index` both get at least 20% faster, or
peak memory drops at least 25% on both; and `summary --index`, ranked `find --index`,
disk commands, `create-index` and a 500-note vault are not slower; and the snapshot
is not larger; and JSON is byte-identical on the Obsidian Hub, GitHub Docs and MDN.

## What was tried

- Candidate A: `std::mem::forget(snapshot_index)` in `run_inner`, after
  `OutputPipeline::finalize` has written and flushed. `SnapshotIndex` owns only
  `Vec`, `HashMap`, `String`, `OnceLock<LinkGraph>`, `AtomicU64`, a
  `Mutex<Option<Vec<u8>>>`, a `Box<Bm25InvertedIndex>` variant and plain data. The
  repository has five `Drop` impls (the `jq` `Worker`, its Windows-only `Job`, and
  `interrupt`'s `Flight`, `Tracked`, `Scratch`); none is reachable from it.
- Candidate B (upper bound, never a shippable shape): `process::exit(0)` straight
  after `finalize` with stdout and stderr flushed, so no destructor of `run_inner`
  runs at all. If B misses the bar, no variant of the idea can reach it.

## Numbers

MDN (14,375 notes), snapshot written by the baseline, `c9db6549` against the
candidate, runs interleaved, wall from `/usr/bin/time -l` (10 ms resolution, so
percentages are good to a few points), peak RSS in MB. A is the median of 11 runs,
B of 7 (5 for `summary`); each side is compared with the baseline runs interleaved
with it, so B has its own baseline. Load is the 1-minute load average read after
the A run and after the B run (one figure where only A ran); the machine was shared.

| command | A baseline | A | B baseline | B | load |
|---|---|---|---|---|---|
| `backlinks web/javascript/index.md --index` | 0.26 s, 248 MB | 0.24 s (-8%), 245 MB | 0.27 s, 248 MB | 0.24 s (-11%), 243 MB | 4.6 / 5.1 |
| `find --index --limit 0 --fields file` | 0.20 s, 247 MB | 0.18 s (-10%), 246 MB | 0.20 s, 247 MB | 0.18 s (-10%), 245 MB | 5.1 / 5.1 |
| `summary --index` | 0.42 s, 265 MB | 0.41 s (-2%), 268 MB | 0.42 s, 269 MB | 0.40 s (-5%), 266 MB | 4.9 / 4.8 |
| `find fetch api --index` (ranked) | 0.11 s, 216 MB | 0.11 s, 216 MB | not run | not run | 4.9 |
| `find --limit 0 --fields file` (disk) | 0.64 s, 61 MB | 0.64 s, 61 MB | not run | not run | 8.5 (noisy) |

Allocation counts (`HYALO_INTERNAL_ALLOC_REPORT`, `backlinks`): 2,036,065 against
2,036,067; frees are not counted, so this shows only that the report still writes.

## Verdict: DISCARD

- [x] The speed bar is missed: A gains 8% and 10%, B (the ceiling) 11% and 10%, against
      the 20% required on both. Not a close call, so the load does not decide it.
- [x] The peak-memory bar is missed, as expected: teardown happens after the peak (-1%).
- [x] The brief's "about 25% of samples on the free side" does not turn into 25% of
      wall time, and the audit profile is not comparable with these runs. It was taken
      before iteration 330, when `backlinks --index` still decoded the BM25 index.
      [[research/snapshot-load-floor-2026-09-01]] measured teardown directly: 59 ms in
      total, 43 ms of it freeing the BM25 index and 16 ms entries plus graph (DEC-264).
      With BM25 no longer decoded, about 16 ms of 0.26 s is left, roughly 6%, which is
      what the 8 to 11% here recovers (that note dates from 2026-09-01, so the 16 ms is
      an earlier measurement on the same corpus, not a re-measurement).
- Not run, because the verdict cannot change: `create-index` and 500-note timings,
  the byte comparison on all three corpora, the closed and full pipe tests,
  the mutation checks. One MDN `backlinks` run was byte-identical to baseline and the
  allocation report was written. As a negative control, cutting a `find` output
  short with `head -c` gave a different hash from the full output; no binary that
  drops output on exit was built.
- [x] Code reverted; docs only.

## What would change the verdict

A 10% gain on two commands is real but small, and `forget` adds a rule every future
owner of `SnapshotIndex` must keep (no `Drop` that does work). Revisit only if a
later iteration makes the teardown larger than the audit measured.
