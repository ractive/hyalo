---
title: Iteration 135 — Batch `hyalo mv` with `--glob` and frontmatter filters
type: iteration
date: 2026-05-13
status: completed
branch: iter-135/batch-mv
tags:
  - iteration
  - feature
  - mv
  - bulk-ops
related:
  - "[[iterations/done/iteration-134-links-fix-short-form-wikilinks]]"
---

## Goal

Extend `hyalo mv` to move multiple files in a single invocation, selected
by `--glob` and/or the same frontmatter filters that `hyalo find` already
supports (`--property`, `--tag`, `--type`). Rewrites all affected
wikilinks and markdown links across the vault in **one** combined pass,
so batching N files costs one graph scan instead of N.

Example target UX:

```sh
hyalo mv --glob 'iterations/*.md' \
         --property status=completed \
         --property type=iteration \
         --to iterations/done/
hyalo mv --tag archive --to archive/
```

## Context

Current `hyalo mv` is strictly single-file (one source, one destination).
Real archival workflows (e.g. "move all completed iterations into
`iterations/done/`") today require either a shell loop calling `hyalo mv`
once per file — which means N full link-graph rebuilds and N vault
rewrites — or falling back to raw `mv` + `hyalo links fix`, which is
fragile and loses the per-file relative-link rewrite that `mv` does for
the moved file itself.

A batch mode that:

1. Resolves the source set up-front (glob ∩ filters),
2. Builds the link graph once,
3. Applies all renames + rewrites in a single pass,

closes that gap and is consistent with the `lint --fix` and
`links fix --apply` ergonomics introduced in earlier iterations.

## Scope

In scope:

- `hyalo mv` accepts `--glob` (repeatable, `!`-negatable, vault-relative)
  and `--property`/`--tag`/`--type` filters in addition to the existing
  positional `FILE`/`--file` forms.
- Source set is the intersection of all selectors. Empty selection is an
  error (exit non-zero with a clear message).
- `--to` semantics: in batch mode (>1 source resolved, or `--glob`
  present), `--to` must resolve to a **directory** (existing, or a path
  that would be a directory — trailing `/` accepted, `.md` rejected).
  In single-file mode, behavior is unchanged.
- Single link-graph build and single rewrite pass across the whole batch.
- `--dry-run` default for batch mode when any filter/glob is used;
  `--apply` to commit. Single-file mv keeps its current behavior to
  avoid breaking existing scripts.
- Conflict policy: if two sources would land on the same destination
  basename, error out before any write. Add `--on-conflict={error,skip}`
  (default `error`); no `overwrite` in v1.
- Destination collision with an existing file in the target dir: error
  unless `--on-conflict=skip`.
- Frontmatter-aware destination: when `--to` is a directory, the moved
  file's basename is preserved.

Out of scope:

- Renaming (changing basenames) as part of the batch — batch mv only
  relocates; basename-rewrite stays single-file.
- A `--rename-template` / pattern-rename feature (deferred; would need
  its own design).
- Cross-vault moves.
- Changing how relative links inside moved files are rewritten — the
  existing per-file logic is reused unchanged.

## Tasks

- [x] Refactor `hyalo mv` source resolution into a helper that returns
  `Vec<PathBuf>` given (positional, `--file`, `--glob`, `--property`,
  `--tag`, `--type`). Reuse the existing find/filter pipeline.
- [x] Decide batch vs single-file mode from the resolved source set
  (>1 file, or any selector flag present → batch).
- [x] Validate `--to`: in batch mode require directory shape; in
  single-file mode keep current behavior (file or directory).
- [x] Add `--on-conflict={error,skip}` flag (default `error`).
- [x] Add `--apply` flag; in batch mode default to dry-run unless
  `--apply` is given. Keep single-file behavior unchanged.
- [x] Build link graph once; compute the full rename map (old → new
  for every source); apply all on-disk renames; then run a single
  rewrite pass over every file containing any matching link.
- [x] Per-moved-file relative-link rewrite stays correct — reuse the
  existing helper, fed the full rename map so a link from moved file A
  to moved file B resolves to B's new path.
- [x] Atomicity: stage all destination writes; if any rename fails,
  roll back the renames already applied (best-effort, log clearly).
- [x] JSON output: `{moves: [{from, to}], updated_files: [{file,
  replacements: [...]}], totals: {moves, files_changed,
  replacements}, conflicts: [...]}`.
- [x] Text output: one line per move, summary at the end.
- [x] Update `hyalo mv --help` long-form text with batch examples and
  the `--apply` requirement.
- [x] Add e2e tests covering the matrix in the "Test plan" section
  below. Each test uses a fresh tempdir vault, asserts both the
  filesystem state (which files exist where) and the link state
  (which files contain which links) after the command runs.
- [x] Update README and `crates/hyalo-cli/templates/rule-knowledgebase.md`
  with batch-mv examples.

## Test plan

Each test below is named, lists its **fixture**, the **command** under
test, and the **assertions**. All are e2e tests under
`crates/hyalo-cli/tests/` driving the built `hyalo` binary against a
tempdir vault. Use a shared fixture-builder helper to keep tests
terse.

### T1 — Glob-only batch, dry-run default

**Fixture.** Vault root with:

- `iterations/iteration-10-a.md` (frontmatter `status: completed`)
- `iterations/iteration-11-b.md` (frontmatter `status: completed`)
- `iterations/iteration-12-c.md` (frontmatter `status: planned`)
- `notes/index.md` body: `See [[iterations/iteration-10-a]] and [[iterations/iteration-12-c]].`
- Destination `iterations/done/` does not exist yet.

**Command.** `hyalo mv --glob 'iterations/iteration-1*.md' --to iterations/done/`

**Assertions.**

- Exit 0; output lists 3 planned moves.
- No file on disk has changed (dry-run is default in batch mode).
- `iterations/done/` is **not** created.
- JSON envelope contains `totals.moves == 3` and `applied == false`.

### T2 — Glob ∩ property-filter intersection, `--apply`

**Fixture.** Same as T1.

**Command.**
`hyalo mv --glob 'iterations/iteration-1*.md' --property status=completed --to iterations/done/ --apply`

**Assertions.**

- `iterations/done/iteration-10-a.md` and `iterations/done/iteration-11-b.md` exist.
- `iterations/iteration-12-c.md` (status=planned) is **not** moved.
- `notes/index.md` body now contains
  `See [[iterations/done/iteration-10-a]] and [[iterations/iteration-12-c]].`
  (only the moved one is rewritten).
- `totals.moves == 2`, `totals.files_changed >= 1`.

### T3 — Property-filter batch (no glob)

**Fixture.** Same as T1 plus `archive/old-note.md`
(`status: completed`, `type: note`) and an existing
`iterations/done/` directory.

**Command.**
`hyalo mv --property status=completed --property type=iteration --to iterations/done/ --apply`

**Assertions.**

- Only the two `iteration-*` files move; `archive/old-note.md` stays
  put (it is `type: note`, not `type: iteration`).
- The two moved files end up in `iterations/done/`.

### T4 — Cross-batch link rewrite (A → B where both move)

**Fixture.**

- `iterations/iteration-20-foo.md` body: `Related: [[iterations/iteration-21-bar]]`
- `iterations/iteration-21-bar.md` body: `Back: [[iterations/iteration-20-foo]]`
- Both have `status: completed`, `type: iteration`.

**Command.**
`hyalo mv --glob 'iterations/iteration-2*.md' --to iterations/done/ --apply`

**Assertions.**

- Both files are in `iterations/done/`.
- `iterations/done/iteration-20-foo.md` contains
  `Related: [[iterations/done/iteration-21-bar]]` (the link to a sibling
  that *also* moved was rewritten to its new path, not left dangling
  at the old path).
- Symmetric assertion for `iteration-21-bar.md`.
- No file in the vault references the *old* paths anywhere.

### T5 — Destination basename collision errors

**Fixture.**

- `a/dup.md` (`status: completed`)
- `b/dup.md` (`status: completed`)
- Empty `archive/` directory.

**Command.**
`hyalo mv --property status=completed --to archive/ --apply`

**Assertions.**

- Exit non-zero before any rename happens.
- Stderr/JSON lists both source paths as colliding on `archive/dup.md`.
- Neither source file has moved; `archive/` is still empty.

### T6 — `--on-conflict=skip` skips colliders

**Fixture.** Same as T5.

**Command.**
`hyalo mv --property status=completed --to archive/ --on-conflict=skip --apply`

**Assertions.**

- Exit 0.
- Exactly **one** of `a/dup.md` / `b/dup.md` has moved to
  `archive/dup.md` (deterministic order: lexicographic on source
  path — document this); the other remains in place.
- Output includes a `skipped` list naming the unmoved file.

### T7 — Pre-existing file in destination errors

**Fixture.**

- `iterations/iteration-30-x.md` (`status: completed`)
- `iterations/done/iteration-30-x.md` already exists (any content).

**Command.**
`hyalo mv --glob 'iterations/iteration-30-x.md' --to iterations/done/ --apply`

**Assertions.**

- Exit non-zero before any rename.
- `iterations/iteration-30-x.md` still exists at its original path,
  with original content.
- The pre-existing `iterations/done/iteration-30-x.md` is unmodified.

### T8 — Empty selection errors out

**Fixture.** Vault contains only `notes/index.md` (`status: draft`).

**Command.**
`hyalo mv --property status=completed --to archive/ --apply`

**Assertions.**

- Exit non-zero.
- Stderr names the active filters and hints "no files matched".
- `archive/` is not created.

### T9 — `--to` must be a directory in batch mode

**Fixture.** Two completed iteration files (from T1's fixture).

**Command.**
`hyalo mv --property status=completed --to iterations/done.md --apply`

**Assertions.**

- Exit non-zero with a message that batch `--to` must be a directory
  (trailing slash accepted, `.md` rejected).
- No files moved.

### T10 — Single-file mv behavior unchanged

**Fixture.** `old.md`; `notes/index.md` body `[[old]]`.

**Command.** `hyalo mv old.md --to new.md` (no `--apply`, no glob, no filters).

**Assertions.**

- Behavior identical to pre-iteration-135 single-file mv: file moved,
  link rewritten, no `--apply` required. (Pin via golden snapshot.)

### T11 — `--glob` negation excludes paths

**Fixture.** T1 fixture plus `iterations/iteration-99-keep.md` (`status: completed`).

**Command.**
`hyalo mv --glob 'iterations/iteration-*.md' --glob '!iterations/iteration-99-*.md' --property status=completed --to iterations/done/ --apply`

**Assertions.**

- `iterations/done/` contains the completed files except `iteration-99-keep.md`.
- `iteration-99-keep.md` stays in `iterations/`.

### T12 — Frontmatter wikilink rewrite (regression for `related:` lists)

**Fixture.**

- `iterations/iteration-40-host.md` frontmatter:

  ```yaml
  related:
    - "[[iterations/iteration-41-dep]]"
  ```

- `iterations/iteration-41-dep.md` (`status: completed`).
- `iterations/iteration-40-host.md` itself is `status: planned`.

**Command.**
`hyalo mv --property status=completed --to iterations/done/ --apply`

**Assertions.**

- `iterations/iteration-40-host.md` frontmatter now reads
  `- "[[iterations/done/iteration-41-dep]]"`.
- The host iteration is **not** moved (it is `status: planned`).

### T13 — Rollback on mid-batch rename failure

**Fixture.** Two completed iteration files. Inject a failure for the
second rename (e.g. by pre-creating a directory at the second
destination path so `rename` fails on the OS).

**Command.**
`hyalo mv --property status=completed --to iterations/done/ --apply`

**Assertions.**

- Exit non-zero.
- The first file is rolled back to its original location.
- No link rewrites have been written to disk.
- Error message identifies the failing destination.

### T14 — Single-graph-build performance smoke test

**Fixture.** 50 completed iteration files + 200 unrelated markdown
files containing random `[[iteration-*]]` links pointing at the 50.

**Command.**
`hyalo mv --property status=completed --to iterations/done/ --apply`

**Assertions.**

- All 50 files moved; all 200 referencing files updated.
- Wall-clock time is within a generous bound (e.g. < 5× a single
  `hyalo mv`). Not a strict perf gate — guards against an accidental
  N-pass regression.

## Acceptance criteria

- [x] `hyalo mv --glob 'iterations/*.md' --property status=completed
  --property type=iteration --to iterations/done/` lists the matching
  files and does not write anything (dry-run default).
- [x] Same command with `--apply` moves all matching files and rewrites
  every wikilink/markdown-link pointing at any of them in one vault
  pass.
- [x] A link from one moved file to another moved file resolves to the
  other file's *new* path after the batch, with no manual second pass.
- [x] Two sources colliding on the same destination basename errors
  out before any write; `--on-conflict=skip` skips the colliding one.
- [x] Single-file `hyalo mv old.md --to new.md` behaves exactly as
  before (no `--apply` required, file-form `--to` allowed).
- [x] Empty source set exits non-zero with a hint about the filters.
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace -q` all pass.
- [x] Dogfood: archive all `status=completed` iteration files into
  `iterations/done/` in the hyalo KB with one command, and confirm
  cross-references (e.g. `related:` frontmatter wikilinks) are
  rewritten.
