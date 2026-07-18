---
title: "Iteration 184 — link resolver & writer unification (Phase C)"
type: iteration
date: 2026-07-18
status: planned
branch: iter-184/link-resolver-writer-unification
tags: [iteration, links, resolver, refactor]
depends-on: "[[iterations/iteration-183-link-scanner-unification]]"
related:
  - "[[reviews/link-handling-review-2026-07-18]]"
---

# Iteration 184 — link resolver & writer unification (Phase C)

## Goal

One resolution entry point and one write path. Collapses the 5
semi-independent resolvers and 2 write paths found by
[[reviews/link-handling-review-2026-07-18]], fixing the case-sensitivity
divergence (L-6) at the root and making apply-time failures honest
(L-11). Also lands the fuzzy-confidence and auto-link correctness items
that belong to this layer.

## Tasks

### 1. Single resolver (L-6 root fix)

- [ ] Extend iter-150's `LinkResolver` into
  `resolve_link(ctx, link, mode)` with `ResolveMode::Exists` (used by
  find --broken-links, backlinks, summary, orphan/dead-end) and
  `ResolveMode::Classify` (links fix's
  Broken/CaseMismatch/Ambiguous/ExactHit), sharing one lookup order
- [ ] Migrate the divergent call sites: find/mod.rs:735-765 inline
  block, its near-duplicate link_fix.rs:371-391,
  `resolve_and_classify_link`, backlinks.rs:41-45, find/mod.rs:824-826,
  summary.rs:293-302
- [ ] Case-insensitivity via a lowercased-key companion map inside
  `LinkGraph` (populated in `insert_file_links`,
  link_graph.rs:391-458) — O(1) lookups, NOT the O(vault)
  `backlinks_case_insensitive` helper per call
- [ ] e2e: `backlinks Foo.md` == `backlinks foo.md` on a
  case-insensitive vault; orphan/dead-end counts casing-independent;
  perf on MDN unchanged
- [ ] Merge the near-duplicate `detect_broken_links` /
  `detect_broken_links_from_index` (link_fix.rs:~440/~525) over the new
  resolver

### 2. Single write path + honest partial failure (L-11)

- [ ] `auto_link::apply_matches` (auto_link.rs:689-767) unified onto
  `execute_plans`/`RewritePlan`; keep the stronger content-comparison
  TOCTOU guard as the shared behavior
- [ ] Per-file failure handling: a failed write warns, is recorded in
  the envelope (applied/failed/skipped per file), and does not abort
  remaining files; exit code reflects partial failure
- [ ] Batch mv: completed writes are reported (not silently kept) when a
  mid-batch failure triggers rename rollback (mv.rs:586-599) — decide
  and document rollback vs report-only semantics
- [ ] L-25: `links fix` dry-run validates plans against on-disk text the
  same way apply does (single code path, parity guaranteed)

### 3. Fuzzy confidence tiers (L-10)

- [ ] `FuzzyMatch` fixes are reported in a separate bucket and excluded
  from `--apply` by default (like ambiguous short-form links); explicit
  `--apply-fuzzy` / `--min-confidence <f>` opts in
- [ ] Per-fix confidence shown in apply output, not only dry-run
- [ ] e2e: the live KB false positive (iteration-150's
  `[[iteration-132-mv-wikilinks]]` → `iteration-02-links.md` at 0.896)
  is no longer auto-applied

### 4. auto-link correctness (L-12, L-24)

- [ ] L-12: word-boundary detection is Unicode-aware (char-class based,
  not per-byte ASCII); tests with CJK adjacency and U+2011
- [ ] L-24: `--exclude-target-glob` matches case-insensitively like
  `--exclude-title` (GlobBuilder `.case_insensitive(true)`), or the
  asymmetry is documented deliberately

### 5. Small CLI edge (L-26)

- [ ] `create-index --index-file idx.bin` (bare relative filename)
  works; fix the `parent() == Some("")` canonicalization edge

### 6. Retrospective

- [ ] Update iteration 185 with anything learned; keep README/help/docs
  in sync with new flags

## Acceptance Criteria

- [ ] One resolver entry point: grep-audit shows no independent
  stem-matching outside `LinkResolver`/`LinkGraph`
- [ ] All apply paths emit a complete envelope even on partial failure
- [ ] `cargo fmt` / `clippy -D warnings` / `cargo test -q` clean
