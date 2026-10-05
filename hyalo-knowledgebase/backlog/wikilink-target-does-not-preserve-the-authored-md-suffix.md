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

## Audit (iteration 317, DEC-368) — not implemented

[[iterations/iteration-317-backlog-leftovers-before-0250]] audited every
reader of a wikilink's `target` against the contained design (keep
`Link.target` as the stripped stem for resolution, carry the authored `.md`
suffix alongside it, report the authored text). The resolution side is
indeed untouched by that design — but the *reporting* side is not small, so
by the iteration's stop rule (more than ~6 behavioural call sites, or
`links fix` matching would change) it was left open.

Reproduced on the release binary (0.24.1 + iter-317): `[[sub/note.md]]`,
`[[sub/note.md#H]]`, `[[sub/note.md|lbl]]`, `![[sub/note.md]]` all report
`target: "sub/note"`; HYALO006 says ``broken wikilink: `missing` `` for
`[[missing.md]]`; `links fix` prints `"sub/Note" → "sub/note"` for
`[[sub/Note.MD]]`; `find --broken-links --format text` prints `"missing"`.
`mv`'s text report already shows the authored text (it renders the source
span, not `Link.target`).

Internal consumers that assume stem form and would keep working unchanged
under a second field (about 130 reads): `discovery::resolve_target` /
`resolve_link_from_source`, `classify_short_form_wikilink`,
`resolves_via_alias`, `link_graph` edge building, `link_rewrite` self-link
and ambiguity guards (`ambiguous_bare_link_candidates`,
`ambiguous_self_link_candidates`, `classify_outbound_target`),
`link_resolve`, `anchor_fix` resolution, `auto_link`, `frontmatter_links`.

User-visible sites that would each need a behavioural change:

1. `Link` — new field (e.g. `md_suffix: Option<String>`, serde default,
   skipped when `None`) set by `links::parse_wikilink`; ~30 `Link { … }`
   literals (mostly tests in `link_fix.rs`, `links.rs`, `link_graph.rs`,
   `link_rewrite.rs`, `hyalo-mdlint` section scanner) gain the field.
2. Snapshot: `IndexEntry.links` stores `Link`, so an older snapshot would
   decode with the suffix missing and `--index` would disagree with disk —
   a format bump to **v8** (DEC-353 precedent).
3. `find --fields links` — `find/mod.rs` `LinkInfo { target: link.target… }`.
4. `find --broken-links --format text` — follows from 3.
5. HYALO006 / HYALO008 messages — `hyalo-mdlint/src/profiles/link.rs`
   (`check_broken_links`, `check_broken_anchors`) format the stem.
6. `links fix` `old_target` — five construction sites in `link_fix.rs`
   (unfixable, ambiguous, case-mismatch, fuzzy, anchor), and `old_target`
   is ALSO the apply matching key (`link_fix.rs` frontmatter matcher
   `f.old_target == link.target`, body matcher against
   `normalized_span_target`/`span.link.target`) — changing it means
   changing how `--apply` finds the span, the risk the stop rule names.
7. `links.rs` (CLI) — `find_column` needle and `BrokenLinkInfo.target`.
8. `backlinks` `written_target` (`backlinks.rs`) and the reconstructed
   `[[target|label]]` snippet in `index.rs` backlink rendering.
9. `mv` `skipped_ambiguous[].target` / `SkippedFrontmatterLink.target`
   (`link_rewrite.rs`).
10. `anchor_fix` report target.

Related, observed during the audit: `links fix --apply` rewrites a
case-mismatched `[[sub/Note.MD]]` to `[[sub/note]]`, dropping the authored
suffix (the documented "a wikilink always drops `.md`" rule) — a byte-output
change that a full fix would want to revisit at the same time.

### Recommended design

One iteration of its own, with the snapshot bump stated up front: add
`Link::authored_target() -> Cow<str>` (stem + stored suffix) and route
sites 3–10 through it; keep `old_target` as the authored text but match
plans on the stem (`strip_wikilink_md_suffix(old_target)`) so `--apply`
locates spans exactly as today; pin `mv`/`links fix --apply` byte output on
a fixture before touching anything. Tests: `[[sub/note.md]]`,
`[[sub/Note.MD]]`, `[[note.md#H]]`, `[[note.md|label]]`, `![[note.md]]`,
frontmatter `related: "[[note.md]]"`, disk and `--index` identical.
