---
title: "Iteration 317: backlog leftovers before 0.25.0"
type: iteration
date: 2026-10-05
tags: [iteration, index, snapshot, links, backlog]
status: in-progress
branch: iter-317/backlog-leftovers-before-0250
priority: 2
---

# Backlog leftovers before 0.25.0

## Problem

Two backlog notes came out of the reviews of iterations 311 and 313. Neither blocks
the release, but both are cheap to close before a breaking version ships:

- [[backlog/snapshot-created-at-stamped-before-the-rename]] — `create-index` stamps
  the snapshot's `created_at` before the serialize-fsync-rename tail. On a slow disk
  the rename can push the vault root's mtime past `created_at + 1 s`, after which
  iteration 313's no-op guard (DEC-361) rewrites the snapshot on every run and the
  `tree_moved` drift probe re-walks the vault on every `--index` read.
- [[backlog/wikilink-target-does-not-preserve-the-authored-md-suffix]] — `[[sub/note.md]]`
  reports `target: "sub/note"` although DEC-310 says `target` keeps what was written;
  the `.md` suffix is stripped at parse time and every resolver call site reads the
  stripped form.

DEC number reserved: **DEC-368**.

## Tasks

- [x] `created_at`: stamp it after the rename, or compare directory mtimes against `max(created_at, the index file's own mtime)`; a test that fakes a slow tail (index file mtime two seconds after `created_at`) asserts the next `create-index` is `written: false` and `tree_moved` stays false; no snapshot format change unless unavoidable (if the header changes, bump and say so)
- [x] Authored `.md` suffix: audit every reader of a wikilink's `target` (`mv` self-link matching, `discovery::resolve_target`, `classify_short_form_wikilink`, alias matching, `links fix`, the snapshot); implement ONLY if the audit shows a contained change — the reported `target` in `--fields links` carries the authored text (`sub/note.md`) while every internal consumer keeps the stripped stem — with tests for `[[sub/note.md]]`, `[[sub/Note.MD]]`, `[[note.md#H]]`, `[[note.md|label]]`, disk and `--index` identical, and `mv`/`links fix` output unchanged; otherwise leave the backlog note open and record the audit's findings in it
- [x] Both backlog notes updated: `status=completed` with a pointer to this iteration for what was fixed; an unfixed one stays `planned` with the audit appended
- [x] Docs in sync: claims paragraph, CHANGELOG `[Unreleased]` Fixed, DEC-368 (covers both decisions; amends DEC-361 and, if the suffix is fixed, DEC-310)

## Acceptance criteria

- [x] On an MDN scratch copy, `create-index` followed by three reruns reports `written: false` each time; with the index file's mtime pushed two seconds past `created_at`, the next rerun is still `written: false`
- [x] `hyalo find --file <f> --fields links` reports `target: "sub/note.md"` for `[[sub/note.md]]` with `path: "sub/note.md"`, OR the backlog note carries the audit explaining why not
- [x] No behaviour change for links without an authored `.md`; Obsidian Hub counts unchanged (`summary.links.broken` 162, orphans 0, dead ends 193)
- [ ] fmt, clippy `-D warnings`, `cargo test --workspace -q`, `cargo deny check`, the xtask gates and `hyalo lint --strict` green; CI green on three platforms

## Outcome

- `created_at` (DEC-368, amended in review): fixed. The publish re-stamps the
  snapshot file's mtime after the rename; `index::tree_moved` (one helper for
  the no-op guard, `snapshot_drift` and the stale warning) compares every
  directory against `created_at` and excuses only the index's own directory
  when its bump coincides with the file's mtime. The first version
  (`max(created_at, file mtime)` for every directory) hid new notes after a
  `touch .hyalo-index` and was replaced. No format change (v7).
  MDN scratch copy: build + three reruns → `true, false, false, false`; slow
  tail faked → `false`; a later root bump → `true`.
- Authored `.md` suffix: audit only, not implemented. More than ten reporting
  sites (`find --fields links`, HYALO006/008, `links fix` `old_target` — also
  the `--apply` matching key — `backlinks`, `mv` skip reports, `anchor_fix`)
  plus a v8 snapshot bump. The audit and a recommended design are in
  [[backlog/wikilink-target-does-not-preserve-the-authored-md-suffix]], which
  stays `planned`.
- Obsidian Hub unchanged: `summary.links.broken` 162, orphans 0, dead ends 193.
- Gates: fmt, clippy, `cargo test --workspace -q` (5679 passed), `cargo deny
  check`, `hyalo lint --strict`, and the nine xtask gates all green locally.
