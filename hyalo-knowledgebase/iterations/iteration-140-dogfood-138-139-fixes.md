---
title: >-
  Iteration 140 — Dogfood fixes for iter-138 (schema extensions, `hyalo new`)
  and iter-139 (`--files-from`)
type: iteration
date: 2026-05-24
status: planned
branch: iter-140/dogfood-138-139-fixes
tags:
  - iteration
  - bugfix
  - schema
  - lint
  - cli
  - new-command
  - files-from
related:
  - "[[iteration-138-schema-extensions-and-new-command]]"
  - "[[iteration-139-files-from-flag]]"
---

## Goal

Fix seven concrete bugs surfaced by dogfooding iter-138 and iter-139
against a fresh vault and against this repo. Two are critical
(headline feature is dead code; canonical example doesn't work); the
rest are inconsistencies and friction that consumers will hit on day
one.

## Issues

### Critical

#### BUG-1 — `required-sections` lint enforcement is dead code

`validate_required_sections` is called from `lint_file_with_fix` but
the user-facing `hyalo lint` command goes through
`lint_file_with_fix_grouped`, which only invokes `validate_properties`.
A `note`-typed file missing a declared `## Details` section produces
zero violations.

- [ ] Call `validate_required_sections` from
      `lint_file_with_fix_grouped` (route violations into the `SCHEMA`
      rule group with `severity = error`, matching the unit-test
      expectations).
- [ ] Add an e2e test in `crates/hyalo-cli/tests/e2e/lint.rs`: vault
      with `required-sections = ["## Summary", "## Details"]`, file
      missing `## Details` → expect a `SCHEMA` error containing
      `"missing required section"`.
- [ ] Add an e2e test for the happy path (all sections present, in
      order → zero violations).
- [ ] Add an e2e test for order-significant detection (sections present
      but reversed → violation on the out-of-order entry).

#### BUG-2 — Canonical git pipeline broken when vault is a repo subdir

`git diff --name-only origin/main | hyalo lint --files-from -` emits
**repo-relative** paths. When the vault is configured as e.g.
`dir = "hyalo-knowledgebase"`, every path is silently reported as
`files_missing`. The README + cookbook example does not work in the
most common project layout (vault as a subdirectory of the repo).

Repro in this repo:

```sh
git diff --name-only HEAD~3 -- 'hyalo-knowledgebase/**/*.md' \
  | target/release/hyalo lint --files-from -
# → files_missing: 2, total: 0
```

- [ ] In `--files-from` path resolution: when an input path starts
      with the configured vault `dir` (e.g. `hyalo-knowledgebase/...`),
      strip that prefix before treating the remainder as
      vault-relative. Same auto-strip behaviour for both relative and
      absolute paths (the absolute case already works via
      `strip_absolute_vault_prefix`; this is the relative analogue).
- [ ] Emit a hint when **every** path in the input was skipped (e.g.
      "all 5 input paths were treated as missing — if these are
      repo-relative paths, the vault dir prefix may need stripping").
      Surface this even when `total > 0` overall is impossible.
- [ ] Update the canonical example in `README.md` and
      `crates/hyalo-cli/src/cli/help.rs` to confirm it works against a
      vault that is a subdir of the repo — add a comment to that
      effect.
- [ ] E2E test: vault at `kb/`, pipe `kb/notes/foo.md` (repo-relative)
      into `hyalo lint --files-from -` from the repo root → linted
      successfully.

### Inconsistencies and friction

#### BUG-3 — TOML key naming inconsistency (`required-sections` vs `item_pattern`)

iter-138 shipped `required-sections` in kebab-case (via
`#[serde(rename = "required-sections")]`) but `item_pattern` in
snake_case in the same iteration. The iter-138 plan specified both as
snake_case. The wider project mixes conventions
(`filename-template` kebab, `item_pattern` snake), but a *single
iteration* introducing two new keys should not split the convention.

- [ ] Decide: keep both snake (matches plan + `item_pattern`) or both
      kebab (matches `filename-template`). Recommend snake to match
      the iter-138 plan.
- [ ] Apply the chosen convention to `RawTypeSchema` and
      `RawPropertyConstraint` for the new keys.
- [ ] Update the iter-138 plan, README, and any decision-log entry
      that names the key.
- [ ] Decide whether to support the other form as a deprecation
      alias for one release (likely yes, since iter-138 is already
      cut). If yes: accept the alias, emit a deprecation warning at
      schema load, and document the cutover.

#### BUG-4 — `hyalo new` requires parent dir to exist

`hyalo new --type note --file notes/foo.md` fails with
`"parent directory does not exist; create it first"` when `notes/`
doesn't exist. Every other CLI scaffolder (`cargo new`, `git init`,
etc.) creates parent dirs.

- [ ] In `commands/new.rs`: `std::fs::create_dir_all(parent)?` before
      writing the file. Still reject `..` traversal.
- [ ] E2E test: `hyalo new --type note --file deep/nested/notes/foo.md`
      against a fresh vault → directories created, file written.

#### BUG-5 — `hyalo new` scaffold trips MD047

The scaffold body ends with two trailing newlines; the project's own
MD047 ("File has 2 trailing newlines, expected 1") fires on a
freshly-scaffolded file. Self-inconsistent — the tool writes output
that the tool then warns about.

- [ ] In `commands/new.rs`: ensure the rendered body ends with exactly
      one `\n`.
- [ ] E2E test: `hyalo new --type X --file Y.md` then
      `hyalo lint --file Y.md` reports zero MD047 violations.

#### BUG-6 — `--files-from` envelope counters at the wrong nesting level

`files_missing`, `files_skipped_non_md`, `files_skipped_outside_vault`
are emitted at the **top level** of the JSON envelope, while sibling
counters (`errors`, `warnings`, `files_checked`, `total`) live under
`.results`. Consumers parsing `.results` will never see the
files-from counters.

Repro:

```sh
$ ... | hyalo lint --files-from - --format json | jq '.results | keys'
[ "errors", "files", "files_checked", "files_truncated",
  "files_with_violations", "rules_fired", "total", "warnings" ]
$ ... | hyalo lint --files-from - --format json | jq 'keys'
[ "files_missing", "files_skipped_non_md", "files_skipped_outside_vault",
  "hints", "results", "total" ]
```

- [ ] Move `files_missing`, `files_skipped_non_md`,
      `files_skipped_outside_vault` from the envelope root into
      `.results.files_missing` etc. (Same shape as `files_checked`.)
- [ ] Update e2e snapshot tests accordingly.
- [ ] Update README/help-text envelope-shape examples if any.

#### BUG-7 — `find --files-from` exposes no envelope counters

`hyalo lint --files-from -` reports `files_missing` etc.;
`hyalo find --files-from -` returns a bare array with no counters.
iter-139 AC said counters should be consistent across every command
that accepts `--files-from`.

- [ ] Wrap `find` output in an envelope shape that includes the
      files-from counters, same as `lint` (after BUG-6 lands).
      Preserve the existing `results` array for backwards
      compatibility.
- [ ] Confirm `mv`, `set`, `remove`, `append`, `task toggle`,
      `task set` also emit the counters; add e2e coverage for at
      least one mutating command.

## Tasks

- [ ] BUG-1: route `validate_required_sections` through the grouped
      path
- [ ] BUG-1: three e2e tests (missing, happy, out-of-order)
- [ ] BUG-2: auto-strip vault-dir prefix for repo-relative paths
- [ ] BUG-2: hint when all inputs were treated as missing
- [ ] BUG-2: README + help-text example confirmed against subdir
      vault layout
- [ ] BUG-2: e2e test for repo-relative input with subdir vault
- [ ] BUG-3: decide snake-vs-kebab convention + apply
- [ ] BUG-3: decide on deprecation alias; if yes, wire warning
- [ ] BUG-3: update iter-138 plan + README + decision-log
- [ ] BUG-4: `create_dir_all` in `hyalo new`
- [ ] BUG-4: e2e test for nested target path
- [ ] BUG-5: trim trailing newline in scaffold output
- [ ] BUG-5: e2e test: scaffolded file lints clean
- [ ] BUG-6: relocate envelope counters under `.results`
- [ ] BUG-6: update e2e snapshots
- [ ] BUG-7: add envelope counters to `find` output
- [ ] BUG-7: e2e coverage for at least one mutating command
- [ ] CHANGELOG `Unreleased` entry under Fixed
- [ ] Cross-platform CI verification (macOS + Ubuntu + Windows)

## Acceptance criteria

- [ ] A vault with `required-sections` declared, plus a file missing
      one of those sections, surfaces a `SCHEMA` error from
      `hyalo lint` (BUG-1)
- [ ] `git diff --name-only origin/main | hyalo lint --files-from -`
      works against this repo's own subdir vault (BUG-2)
- [ ] When **every** input path was treated as missing, a hint
      explaining the likely cause is shown (BUG-2)
- [ ] `required-sections` and `item_pattern` use a consistent
      convention; iter-138 plan + README + decision-log all reflect
      it (BUG-3)
- [ ] `hyalo new --type X --file deep/nested/Y.md` succeeds against a
      fresh vault (BUG-4)
- [ ] `hyalo new --type X --file Y.md && hyalo lint --file Y.md` is
      clean of MD047 (BUG-5)
- [ ] `jq '.results | keys'` on `--files-from` output for both
      `find` and `lint` includes the files-from counters (BUG-6,
      BUG-7)
- [ ] CHANGELOG `Unreleased` updated under Fixed
- [ ] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace` all green on all three CI platforms

## Design notes

- **BUG-2 — why not require `--repo-relative`?** A flag would work
  but defeats the point: the canonical recipe in the help text
  doesn't show one. The whole value proposition of `--files-from` is
  "pipe whatever your VCS gives you". Make the common case work
  silently; surface a hint only when *every* input misses.
- **BUG-2 — collision risk.** A vault prefix of `notes/` would
  ambiguate `notes/notes/foo.md` (is that `notes/foo.md` relative to
  vault, or literally `notes/notes/foo.md`?). Resolution: try
  vault-relative first (current behaviour), then try
  repo-relative-with-prefix-stripped only if vault-relative misses.
  No information loss either way.
- **BUG-3 — why snake?** The iter-138 plan, `item_pattern`, and the
  general TOML "prefer snake_case for new keys" convention all align.
  `filename-template` is the outlier; not worth churning it now.
- **BUG-6/BUG-7 — envelope shape commitment.** The mixed top-level
  vs. `.results` shape is recent enough (iter-139) that consumers
  shouldn't depend on the broken layout. Move once, before more
  consumers form expectations against it. If we're worried, ship the
  new shape *additively* (both locations) for one release, then
  drop the top-level copy.

## Out of scope

- Adding `--files-from0` (NUL-separated): explicitly deferred by
  iter-139 design notes.
- Globs in `--files-from` line entries.
- Combining `--files-from` with `--glob` (intersection or union).
- New schema features beyond `required-sections` /`item_pattern`
  (e.g. `body_pattern`, body-level regex linting). Belongs in its
  own iteration if/when needed.
