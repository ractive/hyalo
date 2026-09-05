---
type: iteration
title: Iteration 278 — Allocation-free site-prefix resolution
date: 2026-09-05
status: planned
tags:
  - iteration
  - links
  - performance
  - index
branch: iter-278/site-prefix-resolution-allocation
priority: 3
related:
  - "[[iterations/iteration-277-link-graph-parity-and-write-performance]]"
  - "[[decision-log]]"
---

# Iteration 278 — Allocation-free site-prefix resolution

## Goal

Carried over from [[iterations/iteration-277-link-graph-parity-and-write-performance]]
PREFIX-1: indexed site-prefix resolution on MDN roughly halved (4.47 s → 2.42 s for
`summary --index`, 4.75 s → 1.71 s for `find --broken-links --count`) but missed both
aggressive targets (≤ 0.8 s, ≤ 0.6 s). Iteration 277 eliminated the filesystem `stat` per
candidate by resolving from the in-memory case index (BUG-13); what remains is not I/O at
all — it is per-link **allocation** in `resolve_target`: `replace('\\', "/")`,
`strip_site_prefix`, `to_ascii_lowercase` and a `format!` for the `.md` / `/index.md`
candidates, run for every one of MDN's ~51 000 links across up to three probes each.

Rule: no behavior change — every result must stay byte-identical to iteration 277's (disk
scan and `--index` alike); this is purely a hot-path rewrite from owned `String`s to borrowed
`&str` / `Cow<str>` wherever the input is already normalized.

## Tasks

- [ ] ALLOC-1: profile `resolve_target` on MDN with `--site-prefix en-US/docs` (e.g.
      `cargo flamegraph` or `perf record` on the indexed path) to confirm allocation, not
      syscalls, dominates — record the top allocating call sites.
- [ ] ALLOC-2: replace the per-candidate `String` building (`replace`, `to_ascii_lowercase`,
      `format!("{target}.md")`, `format!("{target}/index.md")`) with a borrow-based path:
      skip `replace('\\', "/")` when the target has no backslash (the common case on every
      corpus but Windows-authored links), and build the `.md`/`/index.md` candidates without
      an intermediate lowercase copy when the index lookup itself folds case.
- [ ] ALLOC-3: re-measure MDN indexed `summary` and `find --broken-links --count`; target
      ≤ 0.8 s and ≤ 0.6 s respectively (iteration 277's unmet ACs). If the targets still are
      not met after removing the obvious allocations, record the new number and the remaining
      hot path in a DEC rather than continuing to chase the original target — this iteration's
      job is removing the allocation, not guaranteeing an arbitrary ceiling.
- [ ] ALLOC-4 (carried from iteration 277's own unverified AC): construct or locate a
      synthetic corpus with a single file carrying on the order of 2000 backlinks and measure
      `mv` on it directly, so DEC-317's write-phase win is verified at the fan-out iteration
      277's Hub copy did not happen to exercise (its most-linked note settled for ≤ 8 files in
      the sampled case, which stays on the durable per-file path by design and proves nothing
      about the parallel-phase path).
- [ ] ALLOC-5: `cargo test --workspace -q`, disk/`--index` parity check (byte-identical JSON
      on MDN, same as iteration 277), every xtask `check-*` gate.

## Acceptance criteria

- [ ] MDN with `--site-prefix en-US/docs`: indexed `summary` ≤ 0.8 s, `find --broken-links
      --count` ≤ 0.6 s (iteration 277's unmet targets) — or, if still unmet, a DEC recording
      the measured number and why the remaining cost is not allocation.
- [ ] Disk and `--index` results stay byte-identical to iteration 277's (no behavior change).
- [ ] A ~2000-backlink `mv` is measured directly and its number recorded, closing iteration
      277's unverified AC.
- [ ] Gates green.

## Links

- [[iterations/iteration-277-link-graph-parity-and-write-performance]] — PREFIX-1, PERF-1/2,
  the measured-but-unmet numbers this iteration inherits
- [[decision-log]] — DEC-317 (bulk write phase), DEC-098 (bench-scale)
