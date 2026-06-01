---
title: Iteration 154 — Incremental snapshot-index patch on `mv`
type: iteration
date: 2026-06-01
status: planned
branch: iter-154/mv-index-patch
tags:
  - iteration
  - mv
  - index
  - snapshot
related:
  - "[[dogfood-results/dogfood-v0160-iter-150-crazy]]"
  - "[[iterations/done/iteration-150-link-handling-refactor]]"
  - "[[iterations/done/iteration-149-new-updates-index]]"
---

## Goal

Finish the iter-150 deferred item: patch the persistent
`.hyalo-index` snapshot in place after a successful `hyalo mv`, the
same way iter-149 wired it for `hyalo new`.

The iter-150 plan listed this in scope; the shipped diff documented
it as deferred (commit `5013e37`: "*deferred:* mv-side index
incremental patching"). The iter-150 dogfood confirmed the gap:

```bash
$ hyalo mv b.md --to c.md
$ hyalo --index-file .hyalo-index find --format text
"a.md"  ...
"b.md"  ...        ← still there
```

Normal commands work because the fs walk picks up the rename, but
the snapshot is stale until the next `create-index`.

## Why now

- The plumbing already exists: `LinkGraph::rename_path` is defined at
  `link_graph.rs:269` and nothing wires it.
- iter-149 already shipped the parallel integration for `hyalo new`
  (commit `0b9fe45`). This is the same pattern: do the work, call
  `save_index_if_dirty`, done.
- Leaving the index stale undermines the iter-150 user contract that
  "mv is the only mutator you need to call." Users who rely on
  `--index-file` for scripting see ghost entries.
- Small, well-scoped, finishes a known loose end.

## Scope

### IN

**1. Wire `LinkGraph::rename_path` into `commands/mv.rs::run`.**

After the rename succeeds and after the link rewrites apply:

- Load the snapshot index if one exists in scope.
- Call `LinkGraph::rename_path(old, new)` on it.
- For each inbound file whose links were rewritten, update its
  `links` field in the index entry (same shape as iter-149's `new`
  integration patches `links` for the new file).
- Call `save_index_if_dirty`.

If no snapshot exists, do nothing (matches `new`'s behavior).

**2. Handle batch `mv`.**

The batch path (`mv --files-from`, or multi-arg) must patch the
index once per move, then save once at the end. Don't save in a
loop.

**3. Handle the rename-creates-directory case.**

`mv a.md sub/dir/a.md` creates intermediate dirs and writes the
file. Index entry path becomes `sub/dir/a.md`. Verify the path key
in the index is vault-relative and uses forward slashes (Windows
matters here — iter-137 lesson).

### OUT

- Index reconciliation for `set` / `append` / `remove` (already done
  in iter-149's wave or never relevant).
- Rebuild-on-corruption logic. If the patch fails to apply, fall
  back to invalidating the index (delete it, log it) and let the
  next read trigger a rebuild — same as iter-149.

## Tasks

- [ ] Call `LinkGraph::rename_path(old, new)` in `commands/mv.rs::run`
      after successful rename + rewrite
- [ ] Update each inbound entry's `links` field in the snapshot to
      reflect the rewritten target
- [ ] Call `save_index_if_dirty` once at end of mv (single or batch)
- [ ] Skip cleanly when no snapshot exists
- [ ] Ensure path keys are forward-slash + vault-relative on Windows
- [ ] Test: `mv b.md c.md` → `--index-file .hyalo-index find` shows
      `c.md`, not `b.md`
- [ ] Test: batch `mv --files-from list.txt` for 10 files patches
      all 10 entries and saves once
- [ ] Test: `mv a.md sub/dir/a.md` produces index entry with key
      `sub/dir/a.md` (forward slash even on Windows CI)
- [ ] Test: corrupt-snapshot path — if patch fails, snapshot is
      removed and a rebuild hint surfaces (mirror iter-149)
- [ ] Update `mv --help` to mention the snapshot is auto-patched
- [ ] `cargo fmt && cargo clippy --workspace --all-targets -- -D
      warnings && cargo test --workspace -q` clean
- [ ] Cross-platform CI green (Windows specifically — path keys)
- [ ] Mark `status=completed`, move to `iterations/done/`

## Acceptance Criteria

- [ ] After `hyalo mv` on a vault with a snapshot, the snapshot
      reflects the new state immediately. `hyalo --index-file
      .hyalo-index find` returns the new path and not the old.
- [ ] Inbound link rewrites are reflected in the snapshot's link
      tables for each affected file, not just the renamed file.
- [ ] Batch `mv` saves the snapshot once, not N times.
- [ ] Windows index keys are forward-slash and vault-relative.
- [ ] If the snapshot is corrupt/unreadable, mv still succeeds; the
      snapshot is invalidated with a clear stderr note.
- [ ] No performance regression on single `mv` (snapshot patch is
      O(inbound files), not O(vault)).

## Notes for the implementing agent

- Read [[iterations/done/iteration-149-new-updates-index]] for the
  exact pattern; copy its structure (`add_index_entry` becomes
  `rename_index_entry` + `update_index_links`).
- The dogfood repro is in
  [[dogfood-results/dogfood-v0160-iter-150-crazy]] under "Stale
  `.hyalo-index` after `mv`". Lock it down as a test.
- iter-137 retrospective applies: validate on Linux + Windows
  containers before review. Path separators in index keys are a
  common Windows regression.
