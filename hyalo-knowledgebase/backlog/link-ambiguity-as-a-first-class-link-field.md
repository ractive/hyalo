---
type: backlog
title: "ambiguous + candidates as a first-class field on every link record"
date: 2026-09-05
status: planned
priority: medium
origin: "iter-277 GRAPH-4 (G3 of the post-batch-271-274 dogfood report), deferred 2026-09-05"
---

## Problem

A `[[stem]]` that matches two files and a `[[stem]]` that matches none look identical in
`find --fields links`: both report `path: null`, and the record carries nothing to tell the
two apart. Today the only place ambiguity is visible is `links fix`, whose `ambiguous_links`
bucket lists the candidates — so answering "is this link broken or merely ambiguous?" means
running the fixer, a command whose whole purpose is to propose writes.

This matters because the two have opposite remedies. A broken link needs a target created or
a typo corrected; an ambiguous one needs the *author* to disambiguate (`[[sub/stem]]`), and
no amount of fuzzy matching will help. `mv` already refuses to rewrite an ambiguous link and
reports it under `skipped_ambiguous`; HYALO006 already names it ("ambiguous wikilink … matches
2 candidates"). Three commands compute the same fact three ways and only two of them say so.

## Repro

```sh
mkdir -p /tmp/amb/a /tmp/amb/b
printf 'x\n' > /tmp/amb/a/note.md
printf 'x\n' > /tmp/amb/b/note.md
printf 'see [[note]] and [[nowhere]]\n' > /tmp/amb/src.md
hyalo --dir /tmp/amb find --fields links --format json \
  --jq '.results[] | select(.file == "src.md") | .links'
```

Both links come back with `"path": null` and nothing else. `hyalo --dir /tmp/amb links fix
--dry-run` is the only command that distinguishes them, listing `note` under
`ambiguous_links` with its two candidates and `nowhere` under `unfixable`.

## Proposal

Add two keys to the link record in `hyalo_core::types::LinkInfo`, both skipped from JSON when
absent so no existing shape changes:

- `ambiguous: true` — the target resolved to more than one vault file.
- `candidates: ["a/note.md", "b/note.md"]` — the files it matched, sorted.

Then make the three existing consumers read that one field rather than recomputing it:
`mv`'s `skipped_ambiguous` guard, `backlinks`, and HYALO006's message.

## Why it was deferred out of iteration 277

The *reporting* half is small — `discovery::classify_link_from_source` already returns
`LinkResolution::ShortFormAmbiguous`, and `case_index::lookup_stem_all` already has the
candidate list. The expensive half is the second sentence: `mv` and HYALO006 reach ambiguity
through their own code paths (`link_rewrite::plan_inbound_rewrites` and the lint link context),
and converging them onto one field is a refactor across three modules with its own parity
tests — the same shape of work DEC-318 turned out to be for the edge predicate. Iteration 277
already carried five DECs and a write-path change; adding a fourth cross-module convergence to
it would have made the diff unreviewable.

Doing only the reporting half would have shipped a *fourth* place that computes ambiguity,
which is the problem this item exists to remove.

## Acceptance criteria

- [ ] `find --fields links` reports `ambiguous` and `candidates` on an ambiguous target, and
      neither key on any other link.
- [ ] `mv`'s `skipped_ambiguous` entries and HYALO006's "matches N candidates" message are
      derived from the same computation, with a test that pins all three against one vault.
- [ ] `--index` and disk scans report identical `candidates` (sorted, vault-relative).
- [ ] Text mode prints `(ambiguous: a/note.md, b/note.md)` in place of `(unresolved)`.

## Plan reconciliation — iteration 287 (2026-09-07)

`LinkInfo` now supplies the generated TypeScript API contract. When adding the
optional ambiguity fields, preserve their omission semantics in its test-only
`ts-rs` derives, regenerate with `cargo run -p xtask -- generate-ts-types`, and
extend the same-binary API contract fixtures. Rebuild and sync the generated Pi
companions and run both freshness gates. The shared-computation, text-output,
and index-parity acceptance criteria above remain required.
