---
title: "Iteration 244 — index remaining deferrals: BUG-4 post-mutation BM25 drift, UX-3 dot-paths, UX-6 MDN case-insensitive, `new` link-graph upsert"
type: iteration
date: 2026-08-27
tags:
  - iteration
  - index
  - deferred
  - link-graph
status: completed
branch: iter-244/index-remaining-deferrals
---

# Iteration 244 — index remaining deferrals

## Goal

Clear the deferrals carried out of [[iterations/iteration-243-index-parity-bugfixes]]
and the still-parked items from
[[dogfood-results/dogfood-v0200-arch-refactors-and-agent-cli-followups]].

## Context

- BUG-4 was fixed in iter-243 for the fresh-index path (`on_raw_body_line`
  + `TOKENIZER_VERSION` 3); the **post-mutation** drift is the last
  index/disk-parity gap. The persisted inverted index strips per-entry
  tokens, so mutations cannot update corpus statistics incrementally today.
- UX-3 and UX-6 are the two UX findings from the v0.20.0 dogfood wave
  that no iteration has picked up yet (iter-241 fixed UX-1/2/4; UX-5's
  `--iteration` flag was removed in iter-242 / DEC-242, closing it).
- Carry-over from the iter-243 review: `hyalo new` records the created
  file via `MutationJournal::add_entry` (no link-graph registration), so
  outgoing wikilinks in a new file are invisible to `backlinks --index`
  until a full `create-index` — the last mutating write path without
  BUG-1's upsert-with-links guarantee.

## Tasks

- [x] `new` — journal records brand-new files with
      `insert_or_replace_entry_with_links` (rename `add_entry` usage or
      add a `add_entry_with_links` call) so a template's outgoing links
      enter the persisted graph; e2e test: `new` a file whose template
      links an indexed target, then `backlinks --index` must match disk
- [x] BUG-4 (carry-over) — post-mutation BM25 parity: design and
      implement incremental corpus-statistic maintenance (retain per-entry
      tokens in the snapshot, or maintain sufficient statistics — total
      doc length, df — in the inverted index header); e2e test: BM25
      scores byte-identical between `--index` and disk after a mutation
      wave
- [x] UX-3 — nested YAML dot-path property filters: either support
      `--property 'a.b=v'` traversal or reject dotted keys with a hint
      pointing at the `key~=serialized` workaround; e2e tests for both
      the supported and rejected forms
- [x] UX-6 — case-insensitive link resolution option
      (`[links.case_insensitive]` in `.hyalo.toml` or
      `links fix --case-insensitive`) that treats case-fold-resolving
      targets as resolved rather than fixable, so MDN-style vaults don't
      offer ~50k rewrite plans; e2e test with a case-folded directory
      layout
- [x] Hygiene — fix the stray `status: planned` on
      `iterations/done/iteration-13-read-command.md` (only false
      "planned" file left in the vault; set to `completed`)
- [x] Hygiene — clear the 8 pre-existing KB lint warnings (MD018 in
      decision-log via `hyalo lint --fix`, HYALO002s in old iteration
      files); `hyalo lint` reports zero warnings on this vault
- [x] Packaging — honor the DEC-101 version discipline for the root
      manifest change (PR #279): bump `pi-package/package.json` (and the
      root manifest) to 0.1.1 with a CHANGELOG entry

## Acceptance criteria

- [x] `backlinks <target> --index` sees outgoing links of files created
      by `hyalo new` without a rebuild
- [x] `find <query> --index` scores are byte-identical to the disk scan
      after a mutating wave, without an intervening `create-index`
- [x] `find --property 'a.b=v'` either returns correct results or exits
      with a clear hint (never a silent `No results`)
- [x] A vault with case-fold-resolving link targets under
      `[links.case_insensitive]` reports 0 case-mismatch fixes in
      `links fix --dry-run`
- [x] All quality gates green (`cargo fmt` / `cargo clippy --workspace
      --all-targets -- -D warnings` / `cargo test --workspace -q`, xtask
      `check-*` gates)
- [x] `hyalo find --property status=planned` returns only genuine plans
      (199/209), not iteration-13; `hyalo lint` reports 0 warnings
- [x] `pi-package/package.json` and the root `package.json` both read
      version 0.1.1 with a matching CHANGELOG entry

## Non-goals

- Rebuilding or re-versioning the snapshot format beyond what the
  incremental-statistics design requires
- UX-2/UX-4/UX-5 follow-ups — fixed (or moot) in iters 241/242
- Concurrency (two simultaneous mutating hyalo processes on one vault) —
  out of scope per owner verdict 2026-08-27: single-writer atomicity is
  the only guarantee; document nothing, add no locking
- No v0.21.0 release cut in this iteration (release decision is a
  separate owner call; see DEC-101's tag-pin advice waiting on it)

## Outcome

- **UX-3**: implemented full dot-path *traversal* (not rejection):
  `a.b=v` first tries the literal flat key, then walks nested mappings.
  Applies to `Scalar`, `Absent`, and `RegexMatch` filters alike.
- **UX-6**: implemented as `[links.case_insensitive] resolve = true`
  (sub-table — the scalar `[links] case_insensitive` string key already
  exists, so the table form is the only way to carry both) plus a
  `links fix --case-insensitive` run flag (OR-combined). All
  `case_mismatch` fixes are drained from the report; relocations keep
  their own bucket.
- **BUG-4**: snapshot format unchanged — per-entry tokens stay stripped at
  write time; incremental re-scans re-tokenize when a BM25 index is
  present, and the journal flush rebuilds the inverted index from
  re-scanned tokens ∪ tokens reconstructed from the old postings
  (`Bm25InvertedIndex::reconstruct_all_tokens`). BM25 ties now break by
  path so `--index`/disk ordering is deterministic.

## Links

- [[iterations/iteration-243-index-parity-bugfixes]]
- [[dogfood-results/dogfood-v0200-arch-refactors-and-agent-cli-followups]]
- [[iterations/iteration-241-stale-index-detection-and-ux-fixes]]
- [[iterations/iteration-242-remove-iteration-flag]]
