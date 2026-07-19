---
title: Iteration 187 — link writer & resolver unification completion
type: iteration
date: 2026-07-19
status: in-progress
branch: iter-187/link-writer-unification
tags:
  - iteration
  - links
  - resolver
  - refactor
depends-on: "[[iterations/iteration-185-link-semantics]]"
related:
  - "[[reviews/link-handling-review-2026-07-18]]"
  - "[[iterations/iteration-184-link-resolver-writer-unification]]"
---

# Iteration 187 — link writer & resolver unification completion

## Goal

Finish the two large mechanical refactors that iter-184 (Phase C)
deliberately carried forward to keep its PR reviewable: (a) one shared
`resolve_link` entry point collapsing the remaining independent
resolution loops, and (b) one write path with honest partial-failure
envelopes (L-11) and dry-run/apply parity (L-25). These are the last
unchecked items of
[[iterations/iteration-184-link-resolver-writer-unification]] and the
"iter-184 carried refactors (a–d)" block of
[[iterations/iteration-185-link-semantics]].

**Do NOT release; release is a separate user-gated step.**

**Scope delivered (iter-187 PR):** item (b) — the single write path (task 2),
L-11 honest partial-failure envelopes across all three apply paths (task 3),
and L-25 dry-run/apply parity (task 4) are done, with unit + Unix read-only
e2e coverage and docs/CHANGELOG/decision-log (DEC-056) in sync. **Deferred to a
follow-up:** item (a) — the single `resolve_link` entry point and the
`find`/`summary` migration + `detect_broken_links` merge (task 1), and the perf
A/B benchmark (task 5). These are independent mechanical refactors of the
*read/resolve* side and are safest as their own reviewable PR; they carry
forward to [[iterations/iteration-188-link-semantics-completion]].

**Line-reference note:** all file:line citations below were re-derived
against main at `045f6cb` (post iter-183/184/185/186 + PR #220). The old
citations in the 184/185 plans are stale — re-grep before editing if
this plan itself ages.

**What is already in place (do not redo):**

- `links fix --apply` already goes through
  `RewritePlan`/`execute_plans` (`link_fix.rs::apply_fixes`,
  link_fix.rs:883-945) and already tracks stale plans honestly via
  `unapplied`/`unapplied_fixes` (commands/links.rs:158-243). Only
  `auto_link::apply_matches` remains on a hand-rolled writer.
- `backlinks` already resolves through `LinkGraph::backlinks_ci` +
  `resolve_file_user_ci` (commands/backlinks.rs:50-62) — the
  "backlinks.rs:41-45" migration item from iter-184's list is done.
- `detect_broken_links` (link_fix.rs:414) is already demoted to
  test-only (`#[allow(dead_code)]` at :413); the CLI uses only
  `detect_broken_links_from_index` (link_fix.rs:526). The merge task
  below is about deleting the duplication, not migrating callers.
- iter-184 lesson (MUST honor): any new `LinkGraph` key-set mutation
  keeps `lower_index` incrementally maintained (see
  `lower_index_stays_consistent_across_incremental_mutations` in
  link_graph.rs); no O(vault) rebuild inside a per-file loop.

## Tasks

### 1. Single resolver entry point (finishes iter-184 item (a)) [0/5]

- [ ] Extend `link_resolve.rs` (currently only the mv-oriented
  `LinkResolver`, link_resolve.rs:62-180) with a public
  `resolve_link(ctx, link, mode)` entry point: `ResolveMode::Exists`
  (find --broken-links, backlinks, summary, orphan/dead-end) and
  `ResolveMode::Classify` (links fix's
  Broken/CaseMismatch/Ambiguous/ExactHit). A `ResolveCtx` bundles
  `canonical_dir`, `site_prefix`, `Option<&CaseInsensitiveIndex>`, and
  the stem index. Move (or delegate) the policy currently living in
  `link_fix.rs::classify_link` (:256) and
  `link_fix.rs::resolve_and_classify_link` (:311) so `link_fix.rs` no
  longer owns resolution order
- [ ] Migrate `find/mod.rs`'s inline per-link resolution block
  (find/mod.rs:723-780: kind-dependent source-relative normalization +
  direct `discovery::resolve_target` calls at :752 and :767) onto
  `resolve_link(.., ResolveMode::Exists)`; the broken-links filter
  (:871-879) and orphan/dead-end filters (:884-899) keep identical
  observable behavior (lock with e2e before migrating)
- [ ] Merge the near-duplicates `detect_broken_links` (link_fix.rs:414,
  test-only) and `detect_broken_links_from_index` (link_fix.rs:526)
  into one implementation over `resolve_link`; port the ~10 unit tests
  (link_fix.rs:1431+) onto the surviving entry point
- [ ] Finish the L-6 tail in `summary.rs`: orphan/dead-end counting
  (summary.rs:303-326) does manual case-SENSITIVE
  `targets.contains(rel)/contains(without_md)` membership checks
  against `all_targets()`/`all_sources()` — route through
  `LinkGraph`'s `lower_index`-aware lookups so orphan/dead-end counts
  agree with `backlinks_ci` on case-insensitive vaults; e2e proving
  `[[foo]]` → `Foo.md` is not counted as orphan
- [ ] Grep-audit AC (from iter-184): no independent stem-matching or
  direct `discovery::resolve_target` calls in `hyalo-cli/src/commands/`
  outside the shared entry point; document the audit command + result
  in the PR description

### 2. Single write path: auto-link onto RewritePlan (iter-184 item (b)) [3/3]

- [x] Rewrite `auto_link::apply_matches` (auto_link.rs:628-706, writes
  via hand-rolled `split_lines_preserving_endings` + `atomic_write` at
  :701) to build `Replacement`s/`RewritePlan`s
  (link_rewrite.rs:49) from the scan-cache content and execute through
  the shared machinery; delete `split_lines_preserving_endings`
  (auto_link.rs:712-725) when unused
- [x] Keep the stronger content-comparison TOCTOU guard
  (auto_link.rs:664-676: full `disk_content != content` compare, not
  just the mtime+size pair on `RewritePlan.mtime`) as the shared
  behavior — either extend the plan-execution machinery with an
  optional full-content guard or verify content immediately before
  handing plans to `execute_plans`; record which in the decision log
- [x] `links auto --apply` envelope (commands/links.rs:296-302
  currently reports only `scanned/total/matches/applied`) gains
  per-file outcome reporting (applied/skipped/failed with reason) —
  the skip-warnings currently go to stderr only (auto_link.rs:659,
  :668, :675)

### 3. L-11: honest partial-failure envelopes [5/5]

- [x] `execute_plans` (link_rewrite.rs:430-459) aborts the whole batch
  with `?` at the first mtime-check or write failure — files written
  before the failure stay on disk and the caller gets a bare `Err`
  with no envelope. Add a partial-failure variant (e.g.
  `execute_plans_partial` returning per-plan
  `applied`/`failed(reason)` records) that warns, records, and
  continues with the remaining files; keep the abort-semantics wrapper
  only where all-or-nothing is genuinely wanted
- [x] `links fix --apply`: `apply_fixes` (link_fix.rs:883-945,
  `execute_plans` call at :942) and the CLI envelope
  (commands/links.rs:219-243) gain a `failed`/`failed_fixes` bucket
  (per-file failure records with the error string); `applied_fixes`
  excludes files whose write failed; exit code reflects partial
  failure
- [x] `links auto --apply`: same per-file failure records in its
  envelope (builds on task 2)
- [x] Batch mv: `execute_batch_mv` (mv.rs:518-610) rolls back *renames*
  when `execute_plans` fails mid-way (:587-600) but completed
  link-rewrite `atomic_write`s are silently kept and never reported.
  Decide rollback-vs-report semantics for completed writes, record a
  DEC entry in [[decision-log]], and implement at minimum: the error
  path reports which files were durably rewritten before the abort
- [x] e2e: induce a mid-batch write failure (Unix: read-only target
  file) for `links fix --apply` and batch `mv --apply`; assert the
  JSON envelope lists both applied and failed records and the exit
  code is non-zero; assert `links auto --apply` skip/fail records
  surface in the envelope

### 4. L-25: dry-run validates plans against on-disk text [2/2]

- [x] `links_fix` dry-run (commands/links.rs:163-181) currently skips
  `apply_fixes` entirely, so `unapplied` is always empty and dry-run
  can promise fixes that apply would refuse (stale index / concurrent
  edit). Split `apply_fixes` into a plan-building phase
  (`build_replacements_for_file`, link_fix.rs:960) and an execute
  phase; dry-run runs the plan-building phase against on-disk text and
  reports would-be-stale fixes in the same `unapplied`/
  `unapplied_fixes` fields — one code path, parity guaranteed
- [x] e2e: with a stale `.hyalo-index` (disk edited after `create-index`
  so detection sees text that no longer matches), dry-run's
  `unapplied_fixes` equals what a subsequent `--apply` reports; the
  dry-run "Apply N fixes" hint count matches what `--apply` actually
  writes (iter-184 fuzzy-bucket lesson)

### 5. Perf guard [0/1]

- [ ] A/B benchmark main vs branch with `bench-e2e.sh`
  (HYALO_BENCH_VAULT corpus): `links fix` dry-run scan path,
  `find --broken-links`, and a synthetic 2000-file batch `mv --apply`
  (the iter-184 regression scenario). Record before/after numbers in
  this file before ticking — no unmeasured perf claims (iter-184
  lesson); budget: within noise on scan paths, no regression >5% on
  apply paths

### 6. Retrospective [1/2]

- [ ] Update [[iterations/iteration-188-link-semantics-completion]]
  with anything learned (especially the final `resolve_link` signature
  it depends on)
- [x] README/help/CHANGELOG in sync with the new envelope fields
  (failed buckets, auto per-file records); no release — release is a
  separate user-gated step

## Acceptance Criteria

- [ ] One resolver entry point: grep-audit shows no independent
  stem-matching or direct `resolve_target` resolution loops in command
  code outside `resolve_link`/`LinkGraph` (closes iter-184 AC 1)
- [x] All apply paths (`links fix --apply`, `links auto --apply`, batch
  `mv --apply`) emit a complete JSON envelope even on partial failure,
  with per-file applied/failed/skipped records and honest exit codes
  (closes iter-184 AC 2)
- [x] `links fix` dry-run and apply share one plan-validation code path;
  parity e2e green
- [ ] Perf numbers recorded; scan paths within noise, apply paths not
  regressed
- [x] `cargo fmt` / `clippy --workspace --all-targets -- -D warnings` /
  `cargo test --workspace -q` clean
