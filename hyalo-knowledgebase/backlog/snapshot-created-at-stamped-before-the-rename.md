---
title: "Snapshot created_at is stamped before the serialize-and-rename tail"
type: backlog
date: 2026-10-05
status: completed
priority: low
origin: "iter-313 PR #379 review follow-up, 2026-10-05"
---

# Snapshot created_at is stamped before the serialize-and-rename tail

## Problem

`create-index` records the snapshot's `created_at` before it serializes the
snapshot (141 MB on MDN), fsyncs it and renames it into place. The rename bumps
the vault root directory's mtime. Iteration 313's no-op guard (DEC-361) and the
older `snapshot_drift` `tree_moved` probe both compare directory mtimes against
`created_at` with a one-second tolerance. If the serialize-fsync-rename tail
takes more than about two seconds (slow disk, Windows, CI), the root mtime lands
past `created_at + 1`:

- every later `create-index` sees a "moved tree", skips the no-op path and
  rewrites the full snapshot, indefinitely;
- every `--index` read trips `tree_moved` and re-walks the vault.

Not reproduced on the development Mac (the tail stays well inside the
tolerance even on MDN; runs 2–4 after a build were all `written: false` with
the root mtime equal to the index file's). The reviewer of PR #379 flagged it
as latent.

## Fix

Stamp `created_at` after the rename, or compare directory mtimes against
`max(created_at, mtime of the index file itself)`. Add a test that fakes a slow
tail (set the index file's mtime two seconds after `created_at`) and asserts
the next `create-index` is still a no-op and `tree_moved` stays false.

## Related

- [[iterations/iteration-313-index-honesty-and-result-caps]] (DEC-361, the no-op guard this interacts with)
- [[decision-log]] — DEC-339 (racily-clean rescan) and DEC-361

## Outcome

Fixed in [[iterations/iteration-317-backlog-leftovers-before-0250]] (DEC-368):
the directory-mtime probes compare against the later of `created_at` and the
snapshot file's own mtime; no format change.
