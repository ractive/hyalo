---
type: backlog
title: mv does not rewrite links to a note that .gitignore or .ignore excludes
date: 2026-11-08
status: planned
priority: medium
origin: PR 420 review follow-up (iteration 338), 2026-11-08
---

## Problem

`mv` moves a named note that `.gitignore` or `.ignore` keeps out of the scan, but it
rewrites none of the links to it, so every inbound link breaks. DEC-342 promises that a
named, excluded note keeps its backlinks, and since [[iterations/iteration-338-backlinks-unresolved-links]]
`backlinks` and `find --file --fields backlinks` both list them; `mv` does not use them.

Reproduced 2026-11-08 on the iteration 338 branch (and on main 85260414, unchanged):
a git repo with `.gitignore` = `drafts/`, `drafts/secret.md`, and `src.md` holding
`[[drafts/secret]] and [d](drafts/secret.md)`. `hyalo mv drafts/secret.md --to moved.md`
moves the file and reports `total_links_updated: 0`; `src.md` still points at
`drafts/secret`. `backlinks drafts/secret.md` lists both links before the move.

## Cause (from the iteration 338 audit)

`mv`'s catalog is a plain discovery walk, so the excluded note is not in it and its
inbound links never resolve to it. Every rewrite is gated on `catalog::resolve == old_rel`
(`link_rewrite.rs`), so nothing is rewritten. Before iteration 338 the graph's raw-spelling
key made `mv` open `src.md`, but it still rewrote nothing.

## Proposal

Fold the named source into `mv`'s catalog the way `find --file` and, since iteration 338,
`backlinks` do (`PreparedSelection::resolved` → `resolve_index_prepared`), so the gate sees
the excluded note. Test the DEC-342 example: `.gitignore` and `.ignore`, single and batch
`mv`, and `--dry-run` matching `--apply`.

## Acceptance criteria

- [ ] `mv` of a named, ignore-excluded note rewrites every link `backlinks` lists for it.
- [ ] An e2e test pins the example above for `.gitignore` and `.ignore`.
