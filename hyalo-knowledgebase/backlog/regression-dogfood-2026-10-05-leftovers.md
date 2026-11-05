---
title: Regression dogfood 2026-10-05 leftovers
type: backlog
date: 2026-10-05
status: planned
priority: low
origin: regression dogfood 2026-10-05 (three explorers), deferred from iteration 318
---

# Regression dogfood 2026-10-05 leftovers

Everything else on this list was closed by
[[iterations/iteration-320-dogfood-leftovers]] (fixed, already fixed, or decided
against in DEC-376 to DEC-380). Two items remain.

## Backslash links

`backlinks` and `find` disagree on a literal `\` in a link target. Reproduced
2026-11-05 on macOS with a file named `back\slash.md` and `[[back\slash]]`:
`backlinks 'back\slash.md'` lists the link, `find --fields links` reports it
unresolved (`path: null`), because resolution replaces `\` with `/` in a link
target unconditionally (`discovery.rs`, DEC-363 only protects *selection* text).
`[[sub\t]]` (a Windows-authored path) resolves to `sub/t.md` in both, and
`[[sub%5Ct]]` / `[x](sub%5Ct.md)` resolve in neither, consistently. The fix means
deciding whether a link target may name a literal backslash off Windows and
touching the resolver shared by `find`, `backlinks`, `mv` and `links fix`: too
risky to bundle with a cleanup pass; needs its own DEC and a corpus run.

## No-op `create-index` on MDN

Measured 2026-11-05 (release build, 14 375 files, 141 MB snapshot, user CPU, not
wall): a no-op `create-index` takes **0.78 s user + 2.5 s sys, 550 MB RSS**;
`summary --index` takes 0.39 s user + 0.1 s sys. A BM25-decoding read
(`find zebra --index`) costs 0.56 s user, so the eager decode of the postings
(`take_bm25_index`, needed only when an entry is re-scanned) is about 0.35 s of
the difference and 175 MB of RSS. The rest, including nearly all of the sys time,
is not attributed: the sampled stack is dominated by `malloc` under
`reusable_entry`'s `entry.clone()` of every `IndexEntry` and the graph build in
`ScannedIndex::build_reusing`, but the release binary is stripped and no single
cause was confirmed. Candidate fixes: decode the postings only when something was
re-scanned; move entries out of the previous snapshot instead of cloning them.
