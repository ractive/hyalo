---
type: backlog
title: mv misses a frontmatter wikilink whose file has no other backlink to the target
date: 2026-09-04
status: planned
priority: medium
origin: "iter-262 review (PR #305), 2026-09-04"
---

## Problem

Iteration 262 (FM-2) taught `hyalo mv` to detect a frontmatter `[[…]]` whose brackets
span a line break — a folded (`>`) or literal (`|`) block scalar, or a wrapped quoted
string — and warn instead of silently leaving a dangling reference
(`frontmatter_links_skipped` in the JSON envelope, a stderr note in text mode). The
detection only runs inside `plan_inbound_rewrites`, which is only ever called for files
already in `by_source` — the set built from `LinkGraph::backlinks_ci(old_rel)`
(`crates/hyalo-core/src/link_rewrite.rs`, `plan_mv` step 2). `extract_frontmatter_links`
(the FM-1 graph scanner) never extracts a link whose target spans a line break as a graph
edge in the first place, so a file whose **only** reference to the moved target is such a
split wikilink is never added to `by_source` — `plan_inbound_rewrites` never runs on it,
and it gets neither a rewrite nor the warning. This is exactly the silent-dangling-reference
failure mode FM-2 exists to close, just for the one case where the split link is the file's
only connection to the target.

Confirmed by direct testing (not present in the merged test suite, which only exercises the
warning on a file that also carries an ordinary same-line frontmatter link to the same
target — that ordinary link is what puts the file in `by_source`, and the split-link warning
then piggybacks on it):

```text
Categories/Books.md                      # the file being moved
References/Folded.md:
  ---
  summary: >
    points at [[Categories/
    Books]] somehow
  ---
```

`hyalo backlinks Categories/Books.md` correctly reports 0 backlinks from `Folded.md` (the
split link is not a real graph edge — a reasonable call for `backlinks`/`summary`/orphans).
But `hyalo mv Categories/Books.md --to Categories/Library.md` also produces
`frontmatter_links_skipped: []` for that same vault — no rewrite, no warning, nothing. Only
adding an unrelated ordinary link to `Books` elsewhere in `Folded.md`'s frontmatter makes the
warning appear.

A related but distinct pre-existing gap surfaced by the same probing (present on `main`
before iter-262, not introduced by it, and not fixed here): `NEW-3`'s ambiguous-bare-link
detection (`skipped_ambiguous`) also depends on the file being in `by_source`, so two files
sharing a stem where **neither** sits at the vault root are never flagged as ambiguous by
`mv`, and which of two same-stemmed candidates gets flagged depends on which one is being
moved. Worth folding into whatever fixes the split-link gap, since both stem from the same
"`by_source` is the only files `mv` ever looks at" design.

## Proposal

Extend `plan_mv`'s single-file path (not necessarily batch `mv`, which already accepts a
higher per-call cost) to also scan files that are **not** in `by_source` for a frontmatter
value that, once whitespace is collapsed, contains the moved file's stem or vault-relative
path — the same substring test `split_frontmatter_wikilink` already applies, just run against
every file instead of only ones the graph already flagged. Gate this on the value actually
starting a `[[` without a same-line `]]` (already checked) so the extra scan is cheap: skip
straight past any file whose frontmatter contains no `[[` at all before running the
line-break-aware reconstruction. Needs a perf check against a large vault (Obsidian Hub,
6520 files) since this is, worst case, one extra frontmatter-text scan per vault file per
`mv` call — likely fine given `mv` is not a hot-path command, but should be measured, not
assumed.

Also worth a decision: should the same widened scan feed `skipped_ambiguous`, closing the
related NEW-3 gap in the same change, or should that be its own follow-up? They share a root
cause but are otherwise independent code paths (`split_frontmatter_wikilink` vs. the bare-link
ambiguity probe in `plan_inbound_rewrites`).

## Acceptance criteria

- [ ] A vault where a file's *only* reference to the moved target is a frontmatter wikilink
      spanning a line break: `mv` reports it under `frontmatter_links_skipped` (JSON) and
      prints the stderr warning (text), with no other link to the target present anywhere
      else in the vault.
- [ ] Existing behavior unchanged: `backlinks`/`summary`/`--orphan` still do not treat a
      split frontmatter link as a graph edge (FM-1's scope is deliberately narrower than
      FM-2's warning).
- [ ] Perf: `mv` on Obsidian Hub (6520 files) with no split links present stays within noise
      of the iter-262 baseline.
- [ ] Decide and record whether `skipped_ambiguous` gets the same widened-scan treatment in
      this change or a separate one.
