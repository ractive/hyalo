---
title: Iteration 147 — `--files-from` on `task toggle` and `task set`
type: iteration
date: 2026-05-25
status: completed
branch: iter-147/task-files-from
tags:
  - iteration
  - cli
  - files-from
  - task
related:
  - "[[iterations/done/iteration-139-files-from-flag]]"
  - "[[iterations/done/iteration-143-hint-and-files-from-polish]]"
---

## Goal

Wire `--files-from` into `task toggle` and `task set`, closing the
iter-139 Step 1 deferred item. The CI / diff-aware-lint use case driving
iter-139 didn't need them — but bulk task mutation across a known file
list (e.g. "tick every task under ## Tasks in these 50 plans") is a real
pattern that's awkward without `--files-from`.

Tiny additive change: the resolution machinery already exists; this iter
plumbs it through two more dispatch arms and adds the right argument
combinations.

## Scope decisions

### Which selectors compose with `--files-from`?

`task toggle` and `task set` currently take one of: `--line` (1-based,
repeatable), `--section <heading>`, or `--all`. With `--files-from` (a
list of N files), the meaningful combinations are:

- **`--all --files-from -`**: toggle/set every task in every listed
  file. Coherent across all files.
- **`--section <heading> --files-from -`**: toggle/set every task under
  the named heading in every listed file. Coherent (section heading is
  the same across files; files that don't have the heading become no-ops
  or get a per-file warning).

The single-file selectors don't compose:

- **`--line <N> --files-from -`**: REJECT. Line numbers are
  file-specific; the same number doesn't reliably target a task in
  multiple files. Clap-level error: "`--line` requires a single `--file`;
  use `--all` or `--section` with `--files-from`".

`task read` is read-only and could in principle accept `--files-from`,
but the output shape (`{results: <task>}` for single-file) would need
extension. **Out of scope** — leave `task read` single-file for now;
revisit if a use case shows up.

### Conflict semantics with `--file` and `--glob`

Same as the other commands per iter-139: `--files-from` is mutually
exclusive with `--file` and `--glob` via clap `conflicts_with_all`.

### Index interaction

Same contract as iter-143: when `--index` is active, route the resolver
through `files_from::resolve_with_index`. No new resolver code; reuse
the existing one.

## Steps

### Argument shape

- [x] Add `--files-from <PATH>` to `TaskAction::Toggle` and
      `TaskAction::Set`. *(Via `InputSelection` flatten — landed in
      iter-145; closed out by iter-147.)*
- [x] Add clap `conflicts_with_all` against `--file` and `--glob`.
      *(Inherited from `InputSelection`.)*
- [x] Add clap `requires_all = ["all_or_section"]` or equivalent
      attribute group so `--files-from` cannot be combined with
      `--line`. Concretely: mark `--line` as conflicting with
      `--files-from`; when `--files-from` is set, at least one of
      `--all` / `--section` must also be set (clap `required_unless`
      or a manual post-parse validation). *(iter-147: clap
      `conflicts_with_all` for `--line` + post-parse dispatch
      validation requiring `--all`/`--section`.)*

### Dispatch

- [x] `run::resolve_files_from_for_command` gains two match arms for
      `TaskAction::Toggle` and `TaskAction::Set`. Pattern matches the
      existing Find/Lint/Set/etc. arms. *(Landed in iter-145.)*
- [x] Each task command consumes the resolved file list and loops the
      single-file path internally. Failure on one file (missing,
      malformed) is per-file: report in the result, continue with the
      rest. Aggregate results into the existing task output envelope.
      *(Landed in iter-145; iter-147 added post-parse guard so
      `--files-from` without `--all`/`--section` errors cleanly.)*

### Result envelope

- [x] Single-file shape today: `{results: {file, toggled: [...], ...}}`
      or similar. With `--files-from`, promote to a list:
      `{results: [{file, toggled: [...], ...}, ...]}`. *(Landed in
      iter-145.)*
- [x] When `--files-from` resolves to exactly one file, preserve the
      single-object shape for backwards compat — OR always promote to
      list. Decide on the cleaner UX during implementation; default is
      "always promote to list when `--files-from` was used" because
      callers can `jq .results[0]` cheaply. *(Chose "always promote to
      list when `--files-from` used"; single positional file keeps
      the object shape.)*
- [x] `files_missing` / `files_skipped_non_md` /
      `files_skipped_outside_vault` counters are already wired by
      `output_pipeline::inject_files_from_counters` — no new code.
      *(Verified via iter-147 e2e
      `task_toggle_files_from_counters_for_missing_and_outside_vault`.)*

### Hints

- [x] `HintSource::TaskToggle` / `HintSource::TaskSetStatus` already
      exist. iter-143's counter-aware hints
      (`generate_hints_with_counters`) fire automatically — no new
      hint generator needed. Verify with a quick e2e. *(Verified via
      iter-147 counters e2e.)*
- [ ] If `--section` matched no tasks in any file: emit a "no
      matching tasks" hint (similar to the existing single-file
      handling). Same for `--all` on empty vaults. *(Not explicitly
      implemented as a multi-file aggregate hint. The empty-input
      case is covered by `task_toggle_files_from_empty_input_exits_zero`
      and per-file no-match behavior is unchanged; a dedicated
      aggregate hint can be added when a real use case surfaces.)*

### Docs + tests

- [x] `task toggle --help` + `task set --help`: add `--files-from`
      examples (probably `--all --files-from -` and
      `--section Tasks --files-from -`). *(`task toggle` already had
      them from iter-145; iter-147 added the matching examples to
      `task set`.)*
- [x] README: nothing new needed — the existing `--files-from` section
      already describes the general shape.
- [x] CHANGELOG `Unreleased` entry under Added.
- [x] Tick the iter-139 deferred box (Step 1 list of subcommands).
- [x] Unit tests in `commands/task.rs`: the per-file loop preserves
      ordering, accumulates results, surfaces per-file errors.
      *(Covered by the iter-145 e2e suite plus iter-147 hardening
      tests; no dedicated unit-test module was added since the e2e
      coverage already exercises ordering, accumulation, and
      per-file error paths.)*
- [x] E2E tests in `tests/e2e/files_from.rs` (or a sibling
      `tests/e2e/task_files_from.rs`):
  - Happy path: `--all --files-from list.txt` mutates tasks across
    multiple files.
  - Happy path: `--section Tasks --files-from -` (stdin).
  - Reject: `--line 5 --files-from -` → clap error mentioning the
    incompatibility.
  - Mixed inputs: `files_missing` and `files_skipped_outside_vault`
    counters fire and the counter-aware hint surfaces.
  - Empty input: exit 0, empty result.

## Tasks

- [x] Add `--files-from` arg + conflict groups to TaskToggle and TaskSet
- [x] Extend `run::resolve_files_from_for_command` with two arms
- [x] Refactor task command bodies to loop over `file` Vec when
      `--files-from` was used; preserve single-file shape otherwise
- [x] Verify counter-aware hints fire (e2e)
- [x] Update `--help` examples on both subcommands
- [x] CHANGELOG entry
- [x] Tick the iter-139 plan deferred box
- [x] Unit tests
- [x] E2E tests covering all five scenarios above
- [x] `cargo fmt`, `cargo clippy -D warnings`, `cargo test --workspace`
- [x] Cross-platform CI green

## Acceptance criteria

- [x] `hyalo task toggle --all --files-from list.txt` toggles every
      task in every listed file
- [x] `hyalo task set --section Tasks --status x --files-from -`
      sets every task under `## Tasks` to status `x` across stdin paths
- [x] `hyalo task toggle --line 5 --files-from -` fails at clap parse
      with a clear "use --all or --section" error
- [x] Result envelope is a list when `--files-from` was used; counters
      and counter-aware hints fire identically to other commands
- [x] Both `task toggle` and `task set` work with `--index --files-from`
      via the iter-143 snapshot-membership resolver (no disk fallback
      for paths absent from the index)
- [x] iter-139's Step 1 deferred checkbox can be ticked

## Design notes

- **Reuse, don't refactor.** The resolution machinery, mutual-exclusion
  groups, envelope-counter injection, and counter-aware hints all exist
  from iter-139 + iter-143. This iter is plumbing only.
- **Per-file loop, not parallel.** Task mutations write files atomically
  via `fs_util::atomic_write`. A multi-file run is just N single-file
  runs in sequence. No `rayon` involvement; ordering matters for
  reproducibility.
- **Line numbers don't compose**, so the iter explicitly rejects
  `--line` + `--files-from`. The error message points at the working
  combinations (`--all`, `--section`).

## Out of scope

- `task read --files-from` (read-only). Punt until someone asks.
- `task append` / `task remove`. They don't exist as separate
  subcommands today; nothing to extend.
- Per-file failure modes beyond "missing file" / "no matching tasks".
  The iter follows the iter-139 convention: warn-and-continue for
  resolution failures, hard error only on parse-time invalid args.
- Parallel task mutation. Atomic write semantics make per-file mutation
  fast enough; parallel would complicate aggregate error handling.

## References

- [[iterations/done/iteration-139-files-from-flag]] — the original
  `--files-from` work; Step 1 deferred this for `task *` subcommands
- [[iterations/done/iteration-143-hint-and-files-from-polish]] —
  counter-aware hints and snapshot-membership resolver this iter
  inherits unchanged
