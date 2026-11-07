---
type: backlog
title: A bulk rewrite can leave an empty note after power loss
date: 2026-11-07
status: completed
priority: low
origin: "PR #411 review (R4), 2026-11-07, DEC-404, DEC-317"
---

## Problem

A bulk rewrite (`lint --fix`, `set`/`append`/`remove --glob`, `properties rename`, `tags rename`,
`task`, `types set --default`, `mv` and `links fix --apply` rewrites) flushes the content of the first
eight notes it writes and none after that; `WriteSession::finish` then fsyncs the touched
directories only. The rename is atomic against a reader and against a crash of the process. It is
not ordered against the data: after power loss or a kernel panic before the OS has written the
content out, a rename can be on disk while the content is not, and on a filesystem that reorders
them (XFS, NTFS) the note is empty. Running the command again does not repair that, because the
original is gone ([[decision-log|DEC-404]]).

## Option

Write and flush every temporary file with one barrier, then rename them all, then fsync the
directories: one device flush for the phase instead of one per note, and the content is on disk
before any name points at it. On macOS `sync_all` is `F_FULLFSYNC`, a whole-device barrier that
costs about 5.5 ms per call and does not amortise across threads, which is where the 39 s of the
Obsidian Hub's `lint --fix` went; one barrier for the phase should keep the 5 s.

## Plan

- [x] Measure the Hub's `lint --fix` with all temp files written first, one flush, then the renames
- [x] If it stays within a second or two of today, make it the `BulkRewrite` behaviour and drop the sentence about power loss from DEC-404 and the docs

## Outcome

Done in [[iterations/iteration-331-bulk-write-durability]] (DEC-408): the Hub's `lint --fix` stays at 3.9 s against 4.0 s, and a note is whole after power loss, old or new.
