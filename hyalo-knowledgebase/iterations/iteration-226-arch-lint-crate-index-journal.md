---
title: "Iteration 226 — architecture: lint crate boundary and unified index journal"
type: iteration
date: 2026-08-23
status: in-progress
branch: iter-226/arch-lint-crate-index-journal
tags: [iteration, architecture, refactor, lint, index, tech-debt]
related:
  - "[[reviews/deep-analysis-2-2026-08-23]]"
---

# Iteration 226 — architecture: lint crate boundary and unified index journal

## Goal

The two larger structural findings from
[[reviews/deep-analysis-2-2026-08-23]] that don't fit the typed-surface
theme of [[iterations/iteration-225-arch-thin-dispatch-typed-hints-core-facade]]:
ARCH-2 (the lint subsystem lives in hyalo-cli, not hyalo-mdlint) and ARCH-3
(snapshot-index maintenance scattered across three mechanisms that every
mutating command must opt into correctly).

**Not release-gating.** Largest and riskiest of the review follow-ups.
Sequence LAST, after 225, and only once the correctness work has settled —
ARCH-3 in particular touches every mutating command's write path.

## Context

- **ARCH-2** — `crates/hyalo-cli/src/commands/lint.rs` is 4,375 lines;
  `crates/hyalo-mdlint` holds only the engine wrapper + native rules
  HYALO001–004. Schema validation, the profile system (okf/madr/changelog/
  skills/github), and profile-lint orchestration
  (`changelog_lint.rs`/`madr_lint.rs`/`okf_lint.rs`/`skills_lint.rs`/
  `lint_github.rs` — five CLI modules) form a hidden subsystem in the CLI.
  Lint logic can't be reused by the planned library consumers of hyalo-core,
  and the e2e suite must spawn processes to drive it.
- **ARCH-3** — three index-refresh mechanisms coexist (VERIFIED by grep):
  `mutation.rs::save_index_if_dirty` (8 call sites across mv/set/remove/new/
  lint/append/dispatch), a *local* `patch_index` in `commands/tasks.rs:238,344`,
  and `index.rs:481-558` `refresh_entry`/`rename_entry` for graph-aware
  updates. Each mutating command picks its own; nothing enforces that a new
  mutating command refreshes the persisted index or its link graph. The
  mtime fallback (`patch_index_for_modified_files:427`) catches stale
  *entries* but not stale *link graph* — `index.rs:439`'s own doc comment
  records that this class of bug already occurred.

## Tasks

- [x] ARCH-2: move schema validation + profile linting into hyalo-mdlint
      (it already depends on hyalo-core for `util::is_iso8601_*`). The CLI
      keeps flag parsing and output formatting only. Expose an in-process
      lint API the e2e suite can drive without spawning. Do this
      profile-by-profile (changelog/madr/okf/skills/github) so each move is
      independently reviewable
- [x] ARCH-3: make index refresh a property of the write path, not the
      caller — a single `MutationJournal` (or equivalent) that every
      frontmatter/link write records dirty rel_paths + whether links changed
      into, flushed once at end of dispatch. Collapse `tasks.rs`'s local
      `patch_index` and the scattered `save_index_if_dirty` call sites into
      it; the graph-aware path is chosen by the journal based on whether
      links changed, so no command can forget it
- [x] Add a guard that a new mutating command cannot silently skip index
      refresh — e.g. the write goes through a type that owns the journal, so
      "forgot to refresh" is not expressible, or an xtask/test that fails if
      a `Commands::` arm mutates without journaling
- [x] Regression: the `index.rs:439` stale-link-graph scenario is covered by
      a test that mutates via each path and asserts the persisted graph is
      current
- [x] Docs: decision-log entries for the lint crate boundary and the
      journal; update the crate-layout notes

## Acceptance criteria

- [x] Schema + profile linting live in hyalo-mdlint with an in-process API;
      at least one lint behavior is unit-tested without spawning a process
- [x] All mutating commands refresh the persisted index (entries AND link
      graph) through one journal; the three old mechanisms are gone or
      funnel through it
- [x] A test/guard makes "a mutating command that forgets index refresh"
      fail to compile or fail CI
- [x] The stale-link-graph regression from `index.rs:439` is covered

## Non-goals

- ARCH-1/4/5 ([[iterations/iteration-225-arch-thin-dispatch-typed-hints-core-facade]])
- Changing lint rule semantics or the schema model — pure relocation +
  API extraction, output stays byte-identical
- Changing the on-disk snapshot format (the journal is an in-memory
  write-path concern; format changes are separate)
