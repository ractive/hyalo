---
type: backlog
title: A wikilink target does not preserve the authored .md suffix
date: 2026-10-05
status: planned
priority: low
origin: "iter-311 BUG-17, PR #377 review, 2026-10-05"
---

## Problem

[[dogfood-results/dogfood-v0250-pre-search-roadmap-2026-10-04]] BUG-17 has two
halves. Iteration 311 (PR #377) fixed the `links fix` half: the case-mismatch
(and relocation/fuzzy/certain-fix) text rendering now shows the byte-truthful
`emitted_target` instead of the vault-relative `new_target` a wikilink write
never puts on disk verbatim.

The second half is untouched: `[[sub/note.md]]`'s reported `target` is
`"sub/note"`, not `"sub/note.md"`, although DEC-310 says `target` keeps
exactly what the author wrote. `links::parse_wikilink` calls
`strip_wikilink_md_suffix` at **parse time** — before the link ever reaches
anything that reports `target` — so the `.md` suffix is gone before any
caller (`find --fields links`, `mv`, `lint`, alias/stem matching) ever sees
it.

## Why it was not fixed in iteration 311

`Link.target` for a wikilink is not just a display field: `mv`'s
self-link matching, `discovery::resolve_target`'s stem lookup,
`classify_short_form_wikilink`, and alias matching all read `target` on the
assumption it is already stem-form (no `.md`). Changing `parse_wikilink` to
preserve the authored suffix would mean every one of those call sites needs
its own `.md`-stripping before it can keep working — either duplicated at
each site, or by introducing a second field (`target` = authored,
`stem` = normalised) threaded through `Link`, `LinkInfo`, the index
snapshot, and every function that currently reads `target` expecting the
stripped form. That is a resolver/parsing-layer change, not a text-rendering
fix, and iteration 311's time box did not allow auditing every call site
safely.

## Proposal

Not yet designed. Whoever picks this up should:

- Decide where the authored `.md` suffix should live: a second field on
  `Link`/`LinkInfo` (e.g. `target_as_written`) alongside the existing
  stripped `target`, or change `target` itself and push the stripping down
  into each of `mv`, `discovery::resolve_target`,
  `classify_short_form_wikilink`, and alias matching.
- Audit every caller of `Link.target` for a wikilink (`grep -rn
  "\.target" crates/hyalo-core/src/discovery.rs crates/hyalo-core/src/link_fix.rs
  crates/hyalo-core/src/link_rewrite.rs`) to confirm which ones assume
  stem-form and which would break if `target` suddenly carried `.md`.
- Add the DEC-310 fixture from the dogfood report as a regression test
  first (`[[sub/note.md]]` → `target: "sub/note.md"` in `find --fields
  links`), confirm it fails today, then fix forward.

## Acceptance criteria

- [ ] `find --fields links` on `[[sub/note.md]]` reports `target:
      "sub/note.md"`, matching DEC-310's "target keeps what was written".
- [ ] Every existing wikilink-resolution test (`mv`, alias matching,
      short-form stem resolution, `links fix`) still passes unmodified or
      with changes that are themselves reviewed — no silent behaviour
      change to resolution, only to the reported `target` field.
- [ ] A new regression test pins the exact DEC-310 fixture from the dogfood
      report.
