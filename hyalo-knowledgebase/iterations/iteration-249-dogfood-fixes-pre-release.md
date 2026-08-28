---
type: iteration
title: Iteration 249 — dogfood fixes before v0.21.0
date: 2026-08-28
status: completed
tags:
  - iteration
  - dogfood
  - release
branch: iter-249/dogfood-fixes-pre-release
related:
  - "[[dogfood-results/dogfood-v0200-post-247-sweep]]"
  - "[[iterations/iteration-241-stale-index-detection-and-ux-fixes]]"
  - "[[iterations/iteration-244-index-remaining-deferrals]]"
---

# Iteration 249 — dogfood fixes before v0.21.0

## Goal

Close out the three findings from
[[dogfood-results/dogfood-v0200-post-247-sweep]] that block cutting v0.21.0:
the stale-index probe's blind spot on nested vaults (UX-1), the `task
toggle`/`task set --index` BM25 score drift left over from an earlier fix
wave (BUG-1, a partial regression of the [[iterations/iteration-244-index-remaining-deferrals]]
parity acceptance criterion), and a misleading text label on
`links fix --apply --apply-fuzzy` (UX-2). Scope held to exactly these three
findings plus their docs — no new CLI flags.

A previous run of this iteration was killed by a network outage mid-work;
its rescued WIP (`e8c814b`, `MutationJournal::update_task` re-scanning the
toggled file via `SnapshotIndex::refresh_links` instead of patching flags
in place) is the starting point for BUG-1 below.

## Tasks

- [x] UX-1: make the stale-index staleness probe
      (`hyalo_core::index::newest_dir_mtime`, renamed from
      `newest_shallow_dir_mtime`) walk directories recursively instead of
      only the vault root and its immediate children, so a file added or
      removed two or more directories deep (`iterations/done/*.md` here,
      nearly every page on MDN/GitHub Docs) trips the `index older than
      vault` warning. An unbounded walk was measured against MDN's 14,375
      files (`../mdn/files/en-us`, ~14,376 directories — one folder per
      page) and added ~65% to an already-indexed `find --limit 1 --index`
      query, well past the ~15% budget for a probe that exists to avoid a
      full scan; bounded the walk to depth 3 instead (no measurable
      overhead on the same benchmark) and documented the resulting blind
      spot (depth ≥ 4 file-only changes, and any in-place edit) in the
      function's doc comment, `create-index --help`, and the `--index` flag
      help.
- [x] BUG-1: finish routing `task toggle --all`/`task set` through a full
      re-scan of the mutated file when `--index` is active
      (`MutationJournal::update_task` → `SnapshotIndex::refresh_links`), so
      `find --index` BM25 scores stay byte-identical to a disk scan after a
      task mutation, matching the parity `set`/`append`/`mv`/`lint --fix`
      already hold. Audited `lint --fix --index`'s rescan-on-write path
      (`MutationJournal::rescan_modified` → `refresh_entry_and_links`) and
      confirmed it already re-tokenizes correctly, including when the fixed
      file had drifted from the index via an external edit beforehand.
- [x] UX-2: `links fix --apply --apply-fuzzy` text output no longer labels
      applied fuzzy matches as "excluded from plain --apply" — the summary
      line now reads "applied via --apply-fuzzy" when the flag was active.
      JSON keys unchanged.
- [x] Docs: CHANGELOG.md `[Unreleased]` entries for all three fixes; help
      texts and `create-index --help` updated in place; no README or skill
      doc referenced the old probe behaviour, so nothing else to sync.
- [x] Run `hyalo lint --strict` vault-wide and fix anything this iteration
      introduced.

## Acceptance criteria

- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, and every `cargo run -p xtask --
      check-*` gate pass.
- [x] Unit test: a directory created at depth 3 below the vault root moves
      `newest_dir_mtime`'s result; a directory below depth 3 does not
      (pins the documented boundary).
- [x] E2e test: a file added two directories deep after `create-index`
      trips the `index older than vault` warning on `find --index`, still
      exits 0, still serves the (stale) snapshot.
- [x] E2e parity test: `task toggle --all --index` on a multi-task file
      leaves `find --index` byte-identical to both a disk scan and a fresh
      `create-index` rebuild of the same post-toggle state.
- [x] E2e test: `links fix --apply --apply-fuzzy` text summary says
      "applied via --apply-fuzzy"; plain `--apply` (no `--apply-fuzzy`)
      keeps the original "excluded from plain --apply" wording.
- [x] `hyalo lint --strict` is clean vault-wide.
