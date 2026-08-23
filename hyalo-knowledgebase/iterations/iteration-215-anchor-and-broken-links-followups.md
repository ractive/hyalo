---
title: Iteration 215 — anchor and broken-links follow-ups
type: iteration
date: 2026-08-23
status: planned
branch: iter-215/anchor-and-broken-links-followups
tags:
  - iteration
  - links
related:
  - "[[dogfood-results/dogfood-v0210-pre2-integrity-wave]]"
  - "[[iterations/iteration-211-links-resolution-correctness]]"
---

# Iteration 215 — anchor and broken-links follow-ups

## Goal

Pick up the two small findings iteration 211 deliberately left out of
scope: `find --broken-links` output carries no line numbers, and
Liquid-templated headings still cannot be slugified to what the
renderer emits.

## Context

Carried over from [[iterations/iteration-211-links-resolution-correctness]],
which fixed the anchor-matching false positives (DEC-075) but explicitly
deferred these two items rather than widen its own scope:

- **UX-6 (partial)**, from
  [[dogfood-results/dogfood-v0210-pre2-integrity-wave]]: `find
  --broken-links` lists every link of a matching file with no line
  numbers, so a user has to grep the file to find a reported broken
  link. (The other half of UX-6 — `.results` JSON shape varying by
  command — is a larger cross-cutting concern, not scoped here.)
  iter-211's own non-goals section: "Line numbers in `find
  --broken-links` output (dogfood UX-6) — goes with the JSON-shape work
  if that gets planned." Since no JSON-shape iteration is currently
  planned, this item is filed on its own rather than left to bit-rot.
- **Known limitation, not a regression**, from iter-211's Outcome:
  headings containing Liquid template expressions (`## {% data
  variables.product.prodname_pro %}`) cannot be slugified to what the
  renderer emits, so anchors into them stay reported broken on corpora
  that use Liquid (GitHub Docs). Same class as iter-207's
  `is_templated_target` zone-skip, applied to `anchor::github_slug`
  instead of the auto-linker.

## Tasks

- [ ] `find --broken-links` (and the underlying `LinkInfo` /
      `--fields links` JSON): add the source line number for each
      link, matching what `hyalo lint` (HYALO006) and `backlinks`
      already report. Decide JSON field name/shape as part of this
      task — do not silently reuse a name already used differently
      elsewhere in `.results`.
- [ ] `anchor::github_slug` (or its caller): recognize Liquid template
      markers (`{% … %}`, `{{ … }}`) inside heading text the same way
      `is_templated_target` recognizes them for links, and skip
      slug-matching (never report broken) for a heading/fragment pair
      where either side is templated. Reuse the iter-207 zone-detection
      helper if it is exposed for headings; otherwise add the minimal
      equivalent for heading text.
- [ ] Regression test for both on a fixture vault; if reachable in the
      review environment, spot-check against the GitHub Docs scratch
      copy (`~/devel/docs/content`) the way iter-211 did.

## Acceptance criteria

- [ ] `find --broken-links --fields links` (or equivalent) reports a
      line number for every link entry, matching the line `hyalo lint`
      reports for the same broken link on the same file
- [ ] A heading containing a Liquid expression is never reported as a
      dead anchor target, even when its slugified form does not match
      any written fragment
- [ ] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean

## Non-goals

- The broader UX-6 finding (`.results` JSON shape consistency across
  all commands) — out of scope here; file separately if it gets
  prioritized.
