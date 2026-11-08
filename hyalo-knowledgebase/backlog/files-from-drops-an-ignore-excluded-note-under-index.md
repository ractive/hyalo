---
type: backlog
title: find --files-from drops an ignore-excluded note under --index and calls it missing
date: 2026-11-08
status: planned
priority: low
origin: PR 420 review follow-up (iteration 338), 2026-11-08
---

## Problem

A `--files-from` list that names a note `.gitignore` or `.ignore` excludes returns it from
disk but not under `--index`, and under `--index` the warning says every entry was missing,
although the file exists.

Reproduced 2026-11-08 on the iteration 338 branch (and on main 85260414, unchanged), with
the vault from [[backlog/mv-does-not-rewrite-links-to-an-ignore-excluded-note]]:

- `printf 'drafts/secret.md\n' | hyalo find --files-from - --fields backlinks`:
  `[{"file":"drafts/secret.md", backlinks: 2}]`.
- The same with `--index` after `create-index`: `[]` plus
  `note: all --files-from entries were missing; …`.

## Cause (from the iteration 338 audit)

`refresh_index` passes `insert=false` for a `--files-from` provenance (DEC-301's batch
semantics), so a note the snapshot never held is dropped instead of read from disk, the way
`--file` reads it ("absent from the snapshot index — read from disk for this run").

## Proposal

Decide whether DEC-301's "a `--files-from` list counts a missing file and exits 0" should
treat an existing-but-unindexed file as missing. If not, insert it like `--file` does and
keep the missing count for files that do not exist. Either way, the warning must not say
"missing" for a file that exists.

## Acceptance criteria

- [ ] `--files-from` gives the same result on disk and under `--index` for an existing,
      ignore-excluded note, or a DEC records why it differs.
- [ ] The warning names only files that do not exist.
