---
type: backlog
title: create-index still clones every entry on a changed rerun and walks the vault twice
date: 2026-11-09
status: planned
priority: low
origin: PR 423 review, leftover of iteration 339 (DEC-414)
---

## Problem

[[iterations/iteration-339-create-index-early-noop]] decides a no-op before any expensive work.
Two costs remain, both measured on MDN (14 375 notes, 141.8 MB snapshot, release build,
2026-11-08, see DEC-414):

- **A rerun that changed something** (one note touched): 2.51 s wall, 1.02 s user, 699 MB RSS,
  the same as before iteration 339. It still decodes the postings, clones every reused
  `IndexEntry` (create_index.rs `reusable_entry`) and clones its links and self anchors again
  for the graph (index.rs `ScannedIndex::build_reusing`), then rebuilds the whole graph.
- **The no-op itself** is 0.6 to 0.9 s, and about 80% of it is the two directory walks:
  the note walk took 261 to 565 ms and the attachment walk about 200 ms more, in temporary
  phase timing that has since been removed.

## Candidate changes

Untried, from iteration 339's report and the read-only code audit of 2026-11-08:

- One walk for notes and attachments instead of two.
- Load the previous snapshot on a second thread while the walk runs.
- Move entries out of the previous snapshot instead of cloning them. This is structural: the
  reuse closure borrows `&SnapshotIndex` under `par_iter`, `SnapshotIndex.entries` is private
  to hyalo-core, so it needs a `take_entries`-style API plus either one lock per slot or a
  two-phase reuse (a parallel stat that picks indices, then a serial move). It removes one of
  the two clones; the graph's clone stays unless `FileLinks` borrows.

## Acceptance criteria

- [ ] Measured like iterations 332 to 339: interleaved runs on an APFS clone of MDN, with
      medians and ranges, and the 20% wall-time bar for keeping a change.
- [ ] Outputs byte-identical, and the snapshot untouched on a no-op.
