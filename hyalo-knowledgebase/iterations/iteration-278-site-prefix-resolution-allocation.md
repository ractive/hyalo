---
type: iteration
title: Iteration 278 — Allocation-free site-prefix resolution
date: 2026-09-05
status: completed
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

## Tasks [5/5]

- [x] ALLOC-1: profile `resolve_target` on MDN with `--site-prefix en-US/docs` (e.g.
      `cargo flamegraph` or `perf record` on the indexed path) to confirm allocation, not
      syscalls, dominates — record the top allocating call sites.
- [x] ALLOC-2: replace the per-candidate `String` building (`replace`, `to_ascii_lowercase`,
      `format!("{target}.md")`, `format!("{target}/index.md")`) with a borrow-based path:
      skip `replace('\\', "/")` when the target has no backslash (the common case on every
      corpus but Windows-authored links), and build the `.md`/`/index.md` candidates without
      an intermediate lowercase copy when the index lookup itself folds case.
- [x] ALLOC-3: re-measure MDN indexed `summary` and `find --broken-links --count`; target
      ≤ 0.8 s and ≤ 0.6 s respectively (iteration 277's unmet ACs). If the targets still are
      not met after removing the obvious allocations, record the new number and the remaining
      hot path in a DEC rather than continuing to chase the original target — this iteration's
      job is removing the allocation, not guaranteeing an arbitrary ceiling.
- [x] ALLOC-4 (carried from iteration 277's own unverified AC): construct or locate a
      synthetic corpus with a single file carrying on the order of 2000 backlinks and measure
      `mv` on it directly, so DEC-317's write-phase win is verified at the fan-out iteration
      277's Hub copy did not happen to exercise (its most-linked note settled for ≤ 8 files in
      the sampled case, which stays on the durable per-file path by design and proves nothing
      about the parallel-phase path).
- [x] ALLOC-5: `cargo test --workspace -q`, disk/`--index` parity check (byte-identical JSON
      on MDN, same as iteration 277), every xtask `check-*` gate.

## Outcome

**The plan's premise was wrong, and the profile said so first.** ALLOC-1 was
supposed to confirm that allocation dominates. It confirmed the opposite:
`sample`, symbolicated, over the indexed `summary` on MDN put **930 of 979
samples (95 %)** in `classify_link`'s *literal* probe —
`resolve_target(.., case_index: None)` at `discovery.rs:1704` — of which 466
samples were `realpath` (`ensure_within_vault`) and 425 `stat`
(`Path::is_file`). The allocations the plan named (`replace('\\', "/")`,
`strip_site_prefix`, `to_ascii_lowercase`, two `format!`s) were a few dozen
samples between them.

Iteration 277 removed the syscalls from the *indexed* resolution path
(BUG-13) and left this one, because `None` was the only way to ask
`resolve_target` for the uncanonicalized answer and `None` also meant "no
index at all". `discovery::resolve_target_literal` (new) separates the two
meanings: the index it takes answers existence only — no stem, no alias, no
canonical spelling — so the verdict is still the author's own spelling.
See [[decision-log]] DEC-322 for the verdict table and why it matches what the
filesystem would have said on either kind of volume.

Measured (Apple Silicon, MDN 14 375 files / ~51 000 links,
`--site-prefix en-US/docs`, warm, median of 3):

| command | iter-277 binary | this iteration | target |
|---|---|---|---|
| `summary --index` | 2.01 s | **0.26 s** | ≤ 0.8 s |
| `find --broken-links --count --index` | 0.33 s | **0.30 s** | ≤ 0.6 s |
| `links fix` (14 k synthetic vault) | ~3.4 s (DEC-098) | **1.08 s** | — |

`find --broken-links --count` was already inside its target before this
iteration began — it resolves through the *indexed* path iteration 277 fixed —
so that AC was met by iteration 277 and is only confirmed here. `summary` is
the row this change moved, 7.7× on byte-identical output.

The ALLOC-2 allocation work was done as well and is kept: `Cow`-based
normalization through `resolve_target`, `strip_site_prefix_ref` (no `format!`
for the `"prefix/"` probe, no owned result), one case-fold per probe instead of
three (`fold_key` borrows a key that is already folded), and one shared buffer
for the `.md` / `/index.md` candidates. Real, small, and not what moved the
numbers.

ALLOC-4 is now a repeatable case in `xtask bench-scale` rather than a one-off
measurement: renaming a note with **2 000** backlinks takes 0.52 s end to end
(0.26 ms per rewritten file) against 5.8 ms/file on the 8-file durable path —
DEC-323.

## Acceptance criteria [4/4]

- [x] MDN with `--site-prefix en-US/docs`: indexed `summary` ≤ 0.8 s, `find --broken-links
      --count` ≤ 0.6 s (iteration 277's unmet targets) — or, if still unmet, a DEC recording
      the measured number and why the remaining cost is not allocation. *(0.26 s and 0.30 s;
      DEC-322 records the mechanism and the caveat on the second row.)*
- [x] Disk and `--index` results stay byte-identical to iteration 277's (no behavior change).
      *(`summary`, `find --broken-links`, `links fix --dry-run` on MDN, disk and `--index`,
      all `cmp`-identical against the iteration-277 binary.)*
- [x] A ~2000-backlink `mv` is measured directly and its number recorded, closing iteration
      277's unverified AC. *(0.52 s; DEC-323.)*
- [x] Gates green.

## Links

- [[iterations/iteration-277-link-graph-parity-and-write-performance]] — PREFIX-1, PERF-1/2,
  the measured-but-unmet numbers this iteration inherits
- [[decision-log]] — DEC-317 (bulk write phase), DEC-098 (bench-scale)
