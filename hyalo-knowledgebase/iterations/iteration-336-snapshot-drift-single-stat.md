---
type: iteration
title: "Iteration 336: one stat per file in the snapshot drift check"
date: 2026-11-08
status: in-progress
tags:
  - iteration
  - performance
branch: iter-336/snapshot-drift-single-stat
related:
  - "[[research/rust-performance-2026-11]]"
  - "[[iterations/iteration-335-fast-hasher]]"
---

# Iteration 336: one stat per file in the snapshot drift check

An audit found that the per-file drift check (DEC-302) called `stat` twice on each
indexed file. `snapshot_drift` in `crates/hyalo-core/src/index.rs` called
`std::fs::metadata` to compare the size, then `entry_is_stale_on_disk` called it again
to compare the mtime. On MDN that is 14,375 extra `stat` calls on every `--index` read
that repairs drift. The staleness rules (DEC-302, DEC-339, DEC-368, DEC-389) are
unchanged: the mtime comparison moved into `entry_is_stale_given`, which takes the
`Metadata` the caller already holds. `files_modified_since_snapshot` and
`first_file_modified_since_snapshot` stat once per entry already and keep
`entry_is_stale_on_disk`.

The iteration also records the end of the performance-experiment series; see
[[research/rust-performance-2026-11]], "The end of the experiment series".

## Tasks

- [x] Remove the second `stat` in `snapshot_drift`
- [x] Show that a test fails when the mtime comparison is broken, and add one if none does
- [x] Measure `backlinks --index` and `find --index` on MDN against `224f2dd9`, interleaved, median of 5
- [x] Record the end of the experiment series (PGO and interning skipped on a measured bound)
- [x] fmt, clippy, tests, the xtask gates except check-help-drift, npm gate, deny and fuzz check pass
- [ ] check-help-drift passes (not run to completion locally: it did not finish in 540 s; no help text changed, so CI is the check)
- [ ] Open the PR

## Results

- Before: no existing test failed with the mtime comparison removed from
  `snapshot_drift` (the 97 `index::` tests passed), because the one drift test changes
  the file size. Added `snapshot_drift_sees_a_same_size_overwrite_by_mtime_alone`.
  With the comparison removed it fails; restored, it passes.
- MDN, snapshot outside the vault, interleaved, median of 5, load 5.8 to 6.0, output
  byte-identical: `backlinks web/javascript/index.md` wall 0.25 s to 0.23 s (-8%),
  user 0.15 s to 0.14 s, sys 0.10 s to 0.08 s; `find --count` wall 0.21 s to 0.18 s
  (-14%), user 0.10 s level, sys 0.10 s to 0.08 s. Short of the 20% bar; the saving is
  system time, about 0.02 s.
