---
type: iteration
title: "Iteration 334: mimalloc as the global allocator (experiment, discarded)"
date: 2026-11-08
status: completed
tags: [iteration, performance, experiment]
branch: iter-334/mimalloc
related:
  - "[[research/rust-performance-2026-11]]"
  - "[[iterations/iteration-332-skip-teardown-at-exit]]"
---

# Iteration 334: mimalloc as the global allocator (experiment, discarded)

Experiment 2 of the performance audit. The audit put the allocator at 17 to 27% of
samples across runs (the figure as recorded in [[research/rust-performance-2026-11]],
from the iteration brief); the idea is to swap the system allocator for `mimalloc`.
The prototype changed `alloc_metrics.rs` so that its counting `#[global_allocator]`
wrapper delegates to `mimalloc::MiMalloc` instead of `System` (so
`HYALO_INTERNAL_ALLOC_REPORT` kept working) and added `mimalloc = "0.1"` (resolved
to mimalloc 0.1.52, libmimalloc-sys 0.1.49, default features, i.e. mimalloc v2) to
`hyalo-cli`. Verdict: **DISCARD**. No code is merged; this note and
[[research/rust-performance-2026-11]] record the numbers.

## Criteria (fixed beforehand by the owner)

KEEP only if `backlinks --index` and `find --index` both get at least 20% faster, or
peak memory drops at least 25% on both; and `summary --index` and ranked
`find --index` are not slower; and disk commands, `create-index` and a 500-note vault
are no worse than 3%; and the snapshot is not larger; and JSON is byte-identical,
disk against `--index` and candidate against baseline, on the Obsidian Hub, GitHub
Docs and MDN. A result within noise of a threshold is NOT SHOWN, not KEEP.

## Numbers

MDN (14,375 notes), baseline `a4f8daff` against the prototype, both release builds
(fat LTO) made in the same worktree, snapshot written by each binary, runs
interleaved, wall and user time from `/usr/bin/time -l` (10 ms resolution), peak RSS
in MB. The machine was shared and busy the whole time: the 1-minute load average
read between 10 and 14 for the figures below (iteration 332 ran at 4 to 5). Against
its idle baselines the two matching commands ran 2.35x (`find --index`, 0.20 s to
0.47 s) and 1.64x (`summary --index`, 0.42 s to 0.69 s) as long; the `backlinks`
target and the ranked query differ from iteration 332's, so they do not compare. Only
the interleaved ratios mean anything.

| command | baseline wall | candidate wall | user time | peak RSS | load |
|---|---|---|---|---|---|
| `backlinks web/javascript/reference/global_objects/array/index.md --index` (15 runs) | 0.54 s | 0.49 s (-9%) | -20% | 249 to 236 MB (-5%) | 12.2 |
| `find --index --limit 0 --fields file` (15 runs) | 0.47 s | 0.44 s (-6%) | -21% | 252 to 237 MB (-6%) | 10.5 |
| `summary --index` (5 runs) | 0.69 s | 0.62 s (-10%) | -20% | 269 to 258 MB (-4%) | 13.9 |
| `find "array buffer" --index` (ranked, 5 runs) | 0.84 s | 0.76 s (-10%) | -17% | 402 to 383 MB (-5%) | 14.1 |

A first, five-run pass at load 15 to 18 showed -57% and -60% on the two headline
commands. A repeat with `user` and `sys` split showed the baseline's `sys` time
swinging between 1.5 s and 3.1 s from run to run for the same command, so that pass
measured contention on the shared machine, not the allocator. It is not used. The
stable signal is the `user` column.

Supporting measurements, five runs each, load 13 to 28 (too noisy for anything finer
than a direction): disk `find` -3%, disk `summary` -1%, disk `backlinks` -7%, disk
ranked `find` -10%, `create-index --force` -13% wall, with peak RSS 1214 to 1144 MB (-6%; that is
`/usr/bin/time -l` maximum resident set, not the research note's peak-live MiB). On a
500-note subset of GitHub Docs the wall times were level within the 10 ms resolution,
but peak RSS of the disk commands rose from 14 to 19 MB (+36%) and from 19 to 26 MB
(+37%). **The 500-note vault therefore fails the "no worse than 3%" criterion on peak
memory**, a second reason for the verdict; every other disk wall time was faster or
level.

Snapshot size: byte-identical for both binaries on MDN (141,719,621), the Obsidian
Hub (50,903,231), GitHub Docs (42,261,269) and the 500-note subset (4,938,130).
Allocation counts (`HYALO_INTERNAL_ALLOC_REPORT`, `backlinks`): 2,321,426 against
2,321,448 allocations, 360.1 MB against 360.3 MB requested, so the report keeps
working and the program asks for the same memory; only who serves it changes.

## Verdict: DISCARD

- [x] The speed bar is missed: -9% and -6% wall on `backlinks --index` and
      `find --index`, against the 20% required on both (11 and 14 points short). Not a
      close call.
- [x] The memory bar is missed: -5% and -6%, against 25% (about 19 to 20 points short).
- [x] The 500-note vault's peak RSS rose 36 to 37%, against the 3% allowed.
- Not checked, because the first criterion already decided it: JSON byte identity
      on the three corpora, the Windows, musl and aarch64 build checks, and
      `cargo deny check`.
- [x] Code reverted; only this note and the research note are committed.

Why the sample share did not become wall time: mimalloc removes about a fifth of the
user CPU time (17 to 21% in every `--index` row), which fits the audit's 17 to 27% of
samples, but wall time also holds kernel time (reading the snapshot, page faults;
`sys` exceeded `user` in the runs I looked at, though the load inflates it), which
mimalloc barely moves: it requests the same bytes and reads the same snapshot. The same lesson as in
[[iterations/iteration-332-skip-teardown-at-exit]]: a sampled share is not a wall-time
saving.

Left open: a retest at idle load could move the wall figure by a few points, not the
11 to 14 points of speed or 19 to 20 points of memory it lacks; nobody should re-run this without a change in what the commands
allocate.
