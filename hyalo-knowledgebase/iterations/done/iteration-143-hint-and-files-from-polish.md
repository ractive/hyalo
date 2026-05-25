---
title: Iteration 143 — Hint + `--files-from` polish (deferred from iter-138 / iter-139)
type: iteration
date: 2026-05-24
status: completed
branch: iter-143/hint-and-files-from-polish
tags:
  - iteration
  - hints
  - schema
  - files-from
related:
  - "[[iterations/iteration-138-schema-extensions-and-new-command]]"
  - "[[iterations/iteration-139-files-from-flag]]"
---

## Goal

Pick up the four hint-related items deferred from iter-138 and the two
items deferred from iter-139, plus a backlog cleanup. All small,
isolated changes in `hints.rs` and `commands/files_from.rs`. No new
schema vocabulary, no new commands.

## Steps

### Schema-violation hint (iter-138 deferred)

- [ ] When `hyalo lint` reports SCHEMA violations on a file that
      carries a `type:` frontmatter property, surface a hint
      `Show schema for type: <T>` → `hyalo types show <T>`. Generic
      across all SCHEMA failure modes (`required`, `pattern`,
      `item_pattern`, `required_sections`, type-mismatch) — replaces
      the iter-138-plan-time idea of per-subcategory hints, which
      would just duplicate the violation message.
- [ ] Cap: one such hint per distinct type per invocation, max 2 types
      surfaced (to avoid noise on multi-file lint runs).
- [ ] Skip when `--rule SCHEMA` or `--rule-prefix HYALO` is already
      active (the user is already focused on schema).

### `types show` → `hyalo new` cross-link (iter-138 deferred)

- [ ] In `hints_for_types` (`"show"` branch): when the inspected type
      has any `required` properties, surface a hint
      `Scaffold a new file of this type` →
      `hyalo new --type <T> --file <path/to/new.md>` (with a
      placeholder path the agent edits).
- [ ] Drop this hint when the type has no `required` properties — the
      scaffolder would produce just a `type:` stub, low value.

### `--files-from` envelope-aware hint (iter-139 deferred)

- [ ] New `HintSource::FilesFrom` (or extend existing source contexts)
      to fire when `--files-from` was used. Inspect the envelope's
      `files_missing`, `files_skipped_non_md`,
      `files_skipped_outside_vault` counters.
- [ ] When `files_missing > 0`: hint
      `<N> input paths did not exist on disk (likely deletions); use
      git diff --diff-filter=AMR upstream to filter them out`.
- [ ] When `files_skipped_outside_vault > 0`: hint
      `<N> input paths were outside the vault; check your --dir or
      upstream filter`.
- [ ] When `files_skipped_non_md > 0`: no hint by default — common
      with `git diff` output, not actionable. (Reconsider if dogfood
      reveals it's surprising.)
- [ ] Hint cap unchanged (`MAX_HINTS`).

### `--index --files-from` snapshot membership (iter-139 deferred)

- [ ] Thread a `&SnapshotIndex` (or analogous reference) into a sibling
      resolver `files_from::resolve_with_index` (keep existing
      `resolve` for the no-index path). The new resolver checks
      snapshot membership for each input path; missing-from-snapshot
      counts as `files_missing`.
- [ ] Dispatch sites: when `--index` (or `--index-file`) is set AND
      `--files-from` is set, call the index-aware resolver instead of
      the disk-based one.
- [ ] No disk fallback for paths absent from the snapshot. Matches the
      contract documented in iter-139 design notes: `--index` means
      "snapshot is the source of truth".
- [ ] E2E: `hyalo lint --index --files-from -` where the input
      includes a path that's on disk but absent from the snapshot
      reports it as `files_missing`, not as a lint of a fresh file.

### Docs + tests

- [ ] Help text: the existing `--files-from` help mentions the
      counters; verify it still reads correctly. No new flags.
- [ ] CHANGELOG `Unreleased` entry under Changed
      (hints) + Fixed (`--index --files-from` snapshot wiring).
- [ ] Tick the previously-deferred boxes in iter-138 and iter-139 plan
      files; reference this iter from each.
- [ ] Unit tests: `hints_for_lint` returns the SCHEMA-type hint when
      expected; `hints_for_types --show` returns the scaffolder hint
      when `required` is set; new `HintSource::FilesFrom` returns
      counter-aware hints.
- [ ] E2E: the `--index --files-from` case described above.

## Tasks

- [ ] Add SCHEMA → `types show <T>` hint generator
- [ ] Wire scaffolder-suggestion hint in `types show`
- [ ] Implement `HintSource::FilesFrom` (or per-source `--files-from`
      hint integration) with the three counter-aware messages
- [ ] Add `files_from::resolve_with_index` and wire dispatch sites
- [ ] Unit tests for each hint addition
- [ ] E2E test for `--index --files-from` snapshot membership
- [ ] Update iter-138 + iter-139 plan checkboxes
- [ ] CHANGELOG entry
- [ ] All three CI platforms green
- [ ] `cargo fmt`, `cargo clippy -D warnings`, `cargo test --workspace`

## Acceptance criteria

- [ ] `hyalo lint` against a file with SCHEMA violations and a `type:`
      property surfaces a `types show <T>` hint
- [ ] `hyalo types show <T>` (where T has `required` props) surfaces
      a `hyalo new --type <T> --file ...` hint
- [ ] `hyalo lint --files-from -` with mixed inputs surfaces hints
      about `files_missing` / `files_skipped_outside_vault` counters
- [ ] `hyalo lint --index --files-from -` reports paths absent from
      the snapshot as `files_missing` and does not fall back to disk
- [ ] iter-138 and iter-139 plan files updated with cross-references
      and ticks

## Design notes

- **No per-subcategory SCHEMA hints.** The original iter-138 plan
  proposed separate hints for `item_pattern` violations vs missing
  `required_sections`. In review those would have just paraphrased
  the violation message. A single hint linking to
  `hyalo types show <type>` is more actionable (the schema declaration
  is the source of truth) and works uniformly across all SCHEMA
  failures.
- **No "Write a new file" hints to replace.** The iter-138 deferred
  item assumed such hints existed; a grep confirms they don't. This
  item is dropped without action.
- **Snapshot-membership uses an index-aware resolver**, not a flag on
  the existing `resolve`. Keeps the disk path unchanged for the common
  case and makes the `--index` semantic explicit at the call site.
- **No new flags.** All changes are output-shape / hint-emission.

## Out of scope

- Index-suggestion hint (slow-query + large-vault) — covered by
  iter-144.
- `--quiet` flag work — covered by iter-144 (the slow-query hint is
  what needs it).
- Wiring `--files-from` into `task toggle` / `task set` (iter-139
  Step 1 leftover) — separate small ticket if anyone asks; the CI use
  case doesn't need it.

## References

- [[iterations/iteration-138-schema-extensions-and-new-command]]
  — deferred hint items (sections: Hints, status `[1/5]`)
- [[iterations/iteration-139-files-from-flag]] — deferred
  snapshot-membership + `HintSource::FilesFrom`
- [[backlog/index-suggestion-hint]] — explicitly NOT addressed here;
  iter-144 will pick it up
