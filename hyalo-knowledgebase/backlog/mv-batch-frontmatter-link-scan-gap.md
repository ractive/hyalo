---
type: backlog
title: batch mv has no split-frontmatter-link scan or skipped-links channel
date: 2026-09-04
status: planned
priority: low
origin: "iter-269 (PR #313) carry-over sweep, 2026-09-04"
---

## Problem

Iteration 269 (DEC-288) gave single-file `hyalo mv` two fixes for the same root cause —
`plan_mv` only ever looking at files the backlinks graph already flagged: a split
frontmatter wikilink (`[[…]]` spanning a line break) is now found even in a file with no
other link to the moved target, and an ambiguous bare `[[stem]]` is now flagged
regardless of which same-stemmed candidate is being moved.

Batch `mv` (`plan_batch_mv`, driven by `--glob`/`--property` + `--to`) gets neither fix.
It was excluded deliberately, not by oversight: `plan_batch_mv` returns bare
`RewritePlan`s and has never had a `frontmatter_links_skipped` channel at all (nor,
independently of iter-269, does it appear to surface `skipped_ambiguous` per-move — that
needs confirming, see below). Extending it means changing its return type and the CLI's
batch-output JSON shape, which iter-269 judged a larger change than the three contained
fixes it was bundling, with no reported real-world failure behind it yet. `mv --help`
was updated to state the asymmetry rather than leave it to be discovered.

## Proposal

Not yet designed — this is a placeholder for the follow-up iter-269 deferred, not a
committed shape. Whoever picks this up should:

- Confirm the current gap precisely: run a batch `mv` over a fixture that would trigger
  the split-frontmatter-link case in single-file mode, and separately one that would
  trigger the nested-ambiguous-stem case, and record exactly what `plan_batch_mv`
  reports today (probably nothing, silently).
- Decide whether `plan_batch_mv`'s return type changes to carry
  `frontmatter_links_skipped`/`skipped_ambiguous` per move, or whether batch mode instead
  gets a cheaper vault-wide pass (batch already accepts a higher per-call cost than
  single-file `mv`, so the perf argument that shaped DEC-288's option (b) may not apply
  the same way here — worth re-measuring rather than assuming).
- If the return type changes, update every caller of `plan_batch_mv` and the batch `mv`
  CLI output (text and JSON) accordingly, plus its `--help` text and
  `hyalo-knowledgebase/docs/` wherever batch `mv`'s guarantees are described.
- Update `mv --help`'s asymmetry note (added in iter-269) once batch mode's behavior
  changes.

## Acceptance criteria

- [ ] Batch `mv` either reports split-frontmatter-link and ambiguous-bare-link cases the
      same way single-file `mv` does, or a DEC records why it stays out of scope for
      batch mode specifically (distinct from iter-269's "not this iteration" note).
- [ ] If implemented: unit/e2e tests mirroring iter-269's single-file fixtures, run
      through `plan_batch_mv` instead of `plan_mv`.
- [ ] Perf: batch `mv` over the Obsidian Hub vault (`../obsidian-hub`) with no split
      links or ambiguous stems present stays within noise of its pre-change baseline.
- [ ] `mv --help` reflects whatever the resolved behavior is.
