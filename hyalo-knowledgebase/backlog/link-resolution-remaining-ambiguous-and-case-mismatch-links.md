---
type: backlog
title: >-
  Hub's ambiguous short-form links and case-mismatch links are untouched by
  iter-272
date: 2026-09-05
status: planned
priority: low
origin: "iter-272 (PR #320) carry-over sweep, 2026-09-05"
---

## Problem

[[iterations/iteration-272-resolution-completeness]]'s Outcome flagged, but did not act on, a
follow-up observation on the Obsidian Hub corpus (6393 notes): after Part B's alias resolution
landed, the Hub still carries **100 `ambiguous`** short-form wikilinks (a bare `[[stem]]`
matching two or more files, or in some cases now two or more alias declarations) and **48
`case_mismatches`** (a link whose casing differs from the on-disk file). Neither category was
touched by iter-272 — they were pre-existing before the alias work and remain exactly as
before.

This is not a bug: `ambiguous` links are correctly *not* auto-resolved (resolving one
arbitrarily would be worse than reporting it), and `case_mismatches` already has a fix path
(`links fix` applies `LinkCaseMismatch` plans routinely). The open question is whether the
*volume* on a corpus this size (100 + 48 out of ~6400 notes) indicates a pattern worth a
targeted iteration — e.g. a cluster of ambiguous stems that share a root cause (two notes with
very similar names that a rename or merge would resolve), or whether it is simply background
noise for a personal-vault-scale corpus and does not warrant more automation.

## Proposal

Not yet designed — this is a placeholder to decide whether there is an iteration here at all.
Whoever picks this up should:

- Re-run `hyalo find --broken-links` (or the ambiguous/case-mismatch specific views) against a
  current Obsidian Hub checkout and confirm the 100/48 figures still hold (iter-272 measured
  them on 2026-09-05; both counts could have shifted with upstream Hub edits).
- Sample a handful of the 100 ambiguous entries: are they genuinely unrelated notes sharing a
  stem (nothing to fix — this is Obsidian's own ambiguity, correctly reported), or is there a
  systematic cause (e.g. a common template producing near-duplicate filenames) that a *tool*
  feature could address, versus something only the vault's own author can fix by renaming?
- If nothing systematic turns up: close this backlog item `wont-do`, with the sample findings
  recorded as the reasoning — "reported correctly, no tooling gap" is a legitimate outcome, not
  a deferral.
- If something systematic does turn up: scope it as its own iteration plan (not folded into
  this backlog item) with its own DEC and acceptance criteria.

## Acceptance criteria

- [ ] The 100 ambiguous / 48 case-mismatch figures are re-confirmed or updated on a current
      Hub checkout.
- [ ] A sample of the ambiguous entries is reviewed and characterized (unrelated notes vs.
      systematic pattern).
- [ ] Either: closed `wont-do` with the sample findings recorded, or a new iteration plan is
      filed for whatever systematic gap the sample revealed.
