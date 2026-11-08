---
title: Regression dogfood 2026-10-05 leftovers
type: backlog
date: 2026-10-05
status: completed
priority: low
origin: regression dogfood 2026-10-05 (three explorers), deferred from iteration 318
---

# Regression dogfood 2026-10-05 leftovers

Everything else on this list was closed by
[[iterations/iteration-320-dogfood-leftovers]] (fixed, already fixed, or decided
against in DEC-376 to DEC-380). The two items that remained are addressed:
backslash links by iteration 338, the no-op `create-index` by iteration 339. What
is left of the second, the cost of a rerun that changed something and of the
directory walks, is tracked in [[backlog/create-index-changed-rerun-and-walk-costs]].

## Backslash links

*Superseded diagnosis; see the Done note below the paragraph.* `backlinks` and `find` disagree on a literal `\` in a link target. Reproduced
2026-11-05 on macOS with a file named `back\slash.md` and `[[back\slash]]`:
`backlinks 'back\slash.md'` lists the link, `find --fields links` reports it
unresolved (`path: null`), because resolution replaces `\` with `/` in a link
target unconditionally (`discovery.rs`, DEC-363 only protects *selection* text).
`[[sub\t]]` (a Windows-authored path) resolves to `sub/t.md` in both, and
`[[sub%5Ct]]` / `[x](sub%5Ct.md)` resolve in neither, consistently. The fix means
deciding whether a link target may name a literal backslash off Windows and
touching the resolver shared by `find`, `backlinks`, `mv` and `links fix`: too
risky to bundle with a cleanup pass; needs its own DEC and a corpus run.

**Done, 2026-11-08 (iteration 338, DEC-413):** the diagnosis above was wrong.
Resolution never disagreed between the commands. `backlinks` listed a link that
did not resolve whenever its raw spelling equalled the queried path, and that held
for `my%20file.md` too, with no backslash involved. An unresolved link now goes
under a key no note path can equal, so `backlinks` never lists one. Resolution is
unchanged and a literal `\` in a link target is still read as `/`, because Obsidian
refuses `\` in file names. A `%5C` link resolves to a note whose file name contains
`\` (`[[back%5Cslash]]` to `back\slash.md`) and misses when no such file exists
(`[[sub%5Ct]]` above). The Obsidian Hub,
GitHub Docs and MDN gave byte-identical `summary` and per-note backlinks.

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

**Done for the no-op, 2026-11-08 (iteration 339, DEC-414):** the decision is made before any of
that work, so a rerun that changed nothing decodes no postings, clones no entry and builds no graph
(MDN: wall -23% to -28%, user -41%, 526 MB to 226 MB). About 80% of the remaining wall time
(0.6 to 0.9 s) is the two directory walks. Moving entries out of the previous snapshot instead of
cloning them only matters for a rerun that changed something. Both are tracked in
[[backlog/create-index-changed-rerun-and-walk-costs]].
