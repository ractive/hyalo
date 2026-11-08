---
type: iteration
title: "Iteration 339: decide a no-op create-index before the expensive work"
date: 2026-11-08
status: completed
tags:
  - iteration
  - performance
  - experiment
branch: iter-339/create-index-early-noop
related:
  - "[[research/rust-performance-2026-11]]"
  - "[[iterations/iteration-336-snapshot-drift-single-stat]]"
  - "[[backlog/regression-dogfood-2026-10-05-leftovers]]"
---

# Iteration 339: decide a no-op create-index before the expensive work

Experiment 4 of the performance audit, and the first one aimed at a write command. A
no-op `create-index` on MDN (14,375 notes, 141.8 MB snapshot) measured 0.78 s user,
2.5 s system and 550 MB resident on 2026-11-05 ([[backlog/regression-dogfood-2026-10-05-leftovers]]),
against 0.39 s user for `summary --index`. On main `55f72949` it took the postings out
of the previous snapshot (a full decode and `validate_bm25`), cloned every reusable
entry twice, rebuilt the whole link graph, and only then decided that nothing changed
and wrote nothing. The change decides first: after the walk and a `stat` of every note
it compares the previous snapshot against what it found, and on a match returns the
same `written: false` result without decoding, cloning or resolving anything. Anything
else takes the old path. Decision: [[decision-log]] DEC-414.

The brief put a correctness question first: the old no-op validated the postings, so an
early exit leaves a damaged section in place. The answer, with the cost of the check
that was kept and the evidence, is in DEC-414 and summarised under "Results".

## Criteria (fixed beforehand by the owner)

KEEP only if the median wall time of the no-op rerun on MDN falls by at least 20% and
the output is identical (`create-index --format json` byte for byte, and the snapshot
file untouched: same mtime and sha256 before and after a no-op run, on both binaries).
Otherwise DISCARD. The changed-file run must not get slower (within 5%).

## Tasks

- [x] Branch from `origin/main` `55f72949`; keep the pristine release binary as the baseline
- [x] Settle what a reader does with a damaged postings section and what it tells the user
- [x] Early no-op in `create_index.rs`: same conditions as the late check, no clone, no graph
- [x] Keep or replace the integrity check the decode gave (cost measured: DEC-414)
- [x] Name the cure in the three reader warnings (`create-index --force`)
- [x] e2e test per no-op-breaking condition, plus the truncated snapshot; a core unit test for the framing check
- [x] Mutation-check `early_no_op` (nine mutations, eight caught, the ninth overlaps another check)
- [x] Measure the no-op rerun, interleaved, base against new, two sets
- [x] Measure the changed-file run, interleaved
- [x] Identity: results JSON and stderr byte-identical, snapshot sha256 and mtime unchanged
- [x] fmt, clippy, tests, `hyalo lint` on the knowledgebase pass
- [x] CHANGELOG entry, research note section, DEC-414
- [x] Open the PR

## Results

**Verdict: KEEP.** MDN `files/en-us` (14,375 notes), an APFS clone per binary with its own
default `.hyalo-index` (141.8 MB), baseline `55f72949` against this change, both release
builds from the same worktree, runs interleaved with the order alternating each round,
`/usr/bin/time -l`. The machine was shared and busy (1-minute load 8.5 to 12.1; the
`sys` column swings from 3.3 s to 8.5 s for the same binary), so the wall figures are
noise-bound and the stable columns are `user` and the resident set. Median [min-max].

| Run | Binary | wall (s) | user (s) | sys (s) | max RSS (MB) |
|---|---|---|---|---|---|
| no-op, set 1 (5 runs, load 8.5-9.1) | base | 1.37 [1.10-1.74] | 0.73 [0.72-0.73] | 5.32 [3.53-8.51] | 526 [522-528] |
| | new | 1.06 [0.78-1.14] | 0.43 [0.43-0.44] | 5.22 [3.26-6.23] | 226 [225-228] |
| | change | **-22.6%** | -41.1% | | -57% |
| no-op, set 2 (7 runs, load 9.2-12.1) | base | 1.62 [1.39-1.86] | 0.76 [0.72-0.80] | 4.96 [3.55-5.89] | 523 [522-526] |
| | new | 1.17 [0.97-1.33] | 0.45 [0.44-0.47] | 3.91 [3.48-6.24] | 227 [226-228] |
| | change | **-27.8%** | -40.8% | | -57% |
| one note touched (5 runs, load 11.1-11.6) | base | 2.51 [2.39-2.57] | 1.02 [1.00-1.05] | 5.87 [4.77-8.34] | 699 [698-699] |
| | new | 2.52 [1.99-3.14] | 1.04 [1.03-1.09] | 5.36 [3.49-6.10] | 697 [685-699] |
| | change | +0.4% | +2.0% | | -0.3% |

Both no-op sets clear the 20% bar, the second by more; in set 1 the ranges overlap by
0.04 s (base's fastest run 1.10 s, new's slowest 1.14 s), which is the noise the load
adds, not a result in doubt: every one of the 12 base runs is slower than the new
median of its own set. The touched-note run rewrites the snapshot on both binaries
(`reused` 14,374, `refreshed` 1, `written: true`) and is within the 5% allowance.

**Identity.** `create-index --format json` of a no-op rerun, base binary and new binary
on the same vault, on both vault copies: stdout byte-identical (709 and 704 bytes, hints
included), stderr empty and identical. The snapshot's sha256, size and mtime are
unchanged by a no-op run of either binary, on both copies.

**Where the time goes now** (temporary phase timing, removed; two runs, cumulative ms):
walk 261 to 565, attachment walk +200, `load_for_reuse` +90, per-note `stat` +14, header
and `tree_moved` +17, total 580 to 890. The two directory walks are about 80% of what is
left.

**The integrity check.** Three early-path variants against base on the same vault, wall
median of 5 under load 13 to 22: no check 0.84 s (-44% against base 1.51 s), a MessagePack
framing walk of the undecoded postings 1.06 s (-30%), the full decode and validate base
does about 1.6 to 1.9 s (two runs, user +0.35 s and 190 MB over no check, no better than
base). The framing walk is the check that was kept: it catches every truncation and every
multi-byte damage tried on MDN's snapshot, and misses a single flipped byte that leaves
the framing valid (DEC-414 has the table). The three reader warnings now end with
"`hyalo create-index --force` rebuilds it".

**Tests.** 16 new e2e tests and 1 core unit test; the workspace run is 1,145 + 2,730 +
1,818 + 276 + 85 + 3 tests, all passing. Mutation check: nine mutations of `early_no_op`,
eight make a test fail (the count shortcut: `a_removed_note_is_dropped`; the per-note
check: seven tests; the framing walk: `a_truncated_postings_section_is_rebuilt_by_a_plain_rerun`;
`[scan] exclude`, `tree_moved`, attachments, the gitignore count and the invalid-UTF-8
count each by one test), the ninth (dropping the re-scan of previously unparsable notes)
survives because `header_still_describes` compares the skip list too and catches the same
case. Left as it is: both checks stay.

## Findings

- `index_incremental::racily_clean_entry_is_rescanned_not_reused` is flaky under load,
  independent of this change: on `55f72949` it failed 2 of 5 runs on this machine (it
  needs the first build to finish within the same second as the file's write). Not
  touched here.
- The old late no-op check is removed, not kept: after the early check it could not fire.
- Next saving, not tried: one walk for notes and attachments, or loading the snapshot on
  a second thread during the walks.
