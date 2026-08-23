---
title: Iteration 211 — links resolution correctness (anchors, offsets, trailing slash)
type: iteration
date: 2026-08-23
status: planned
branch: iter-211/links-resolution-correctness
tags:
  - iteration
  - links
  - lint
related:
  - "[[dogfood-results/dogfood-v0210-pre2-integrity-wave]]"
  - "[[iterations/iteration-203-index-md-resolution]]"
---

# Iteration 211 — links resolution correctness (anchors, offsets, trailing slash)

## Goal

Fix the resolution/reporting defects the 2026-08-23 dogfood found in the
links family: anchor checking that is inverted in practice, HYALO006
line numbers off by the frontmatter length, trailing-slash targets
counted inconsistently between commands, and four small spelling/
labeling defects on the rewrite path.

## Context

From [[dogfood-results/dogfood-v0210-pre2-integrity-wave]]:

- **BUG-8 (MEDIUM)**: `find --broken-links` compares anchors against raw
  heading text (case-insensitive, `%20`-decoded) instead of generated
  slugs: `#sub-section` against `### Sub Section` is reported broken
  while `#Sub Section` passes. 6 of 7 checkable anchors on the GitHub
  Docs copy were false positives. Same-file fragments (`[b](#nope)`) are
  never checked at all.
- **BUG-9 (MEDIUM)**: HYALO006 applies the frontmatter offset twice — 3
  frontmatter lines put a line-5 link at "line 8". Isolated to HYALO006;
  MD rules and `backlinks` report correct lines on the same file.
- **BUG-10 (MEDIUM)**: trailing-slash inconsistencies. With both
  `foo.md` and `foo/index.md` present, one relative `[b](foo/)` shows as
  a backlink of *both* files (slash normalized away before the index is
  keyed) while `links` reports `ambiguous: 0`. Conversely `[b](/baz/)`
  resolving to `baz.md` appears in `find --broken-links` resolution but
  is missing from `backlinks baz.md`. Root enabler: trailing-slash
  targets still fall back to `<target>.md`, which iteration 203
  documented as skipped ("unambiguously a directory reference").
- **BUG-12 (LOW cluster)**: query strings dropped on rewrite
  (`/deep/page?x=1` → `/deep/Page`; fragments survive); CommonMark link
  titles unparsed (`[a](p.md "Title")` → broken, missing from
  backlinks); `mv` appends `.md` to an extensionless spelling
  (`[f](foo/index)` → `[f](bar/index.md)`, violating iter-203's
  spelling AC on one of ten forms); a relative bare-stem relocation is
  labeled `link-case-mismatch` and applied by plain `--apply` at 0.95
  while the identical site-absolute guess is gated behind
  `--apply-fuzzy` — per iter-200's documented design, but indefensible
  to a user.

## Tasks

- [ ] Carry-over from iter-210 (PR #242 test plan): the counter-truth,
      hint-execution and bucket-sum fixes were only verified against
      fixture vaults — no GitHub Docs scratch copy was reachable in that
      review environment. Re-run a dogfood pass against the real corpus
      (`hyalo lint`, `hyalo links fix`, the executed-hint gate) to confirm
      BUG-6/BUG-13/UX-4 hold outside fixtures before this iteration's own
      GitHub-Docs-dependent tasks below build on top of them.
- [ ] BUG-8: slugify headings (GitHub-style: lowercase, spaces→`-`,
      strip punctuation, dedupe suffixes `-1`, `-2`) and check anchors
      against the slug set; keep accepting the raw-text forms for
      compatibility. Check same-file fragments too. Re-run on the GitHub
      Docs copy: the 6 false positives must clear, and a genuinely dead
      anchor must still be caught.
- [ ] BUG-9: fix the double frontmatter offset in HYALO006; e2e matrix
      over 0/3/5-line frontmatter asserting reported line == actual line
      (mirror the dogfood table).
- [ ] BUG-10: key the backlink index on the resolved file only after a
      single, shared resolution step so one link occurrence maps to
      exactly one target file; `foo/` with both candidates present
      either resolves by the documented precedence or surfaces in the
      `ambiguous` bucket — never double-counts. `backlinks` must agree
      with `find --broken-links` resolution for every spelling
      (regression test from the dogfood `/tmp/p1` + `/tmp/p3` repros).
- [ ] BUG-10 root: implement the documented trailing-slash rule (`foo/`
      does not fall back to `foo.md`) OR amend iteration 203 and the
      docs to the permissive behavior — pick one, record as a DEC, and
      make `links`, `backlinks`, and HYALO006 all follow it.
- [ ] BUG-12: preserve query strings through rewrites (treat `?…` like
      `#…`: strip before resolution, reattach on emit).
- [ ] BUG-12: parse CommonMark link titles — `[a](p.md "Title")`
      resolves `p.md` and appears in backlinks; title preserved on
      rewrite.
- [ ] BUG-12: `mv` preserves extensionless spelling
      (`[f](foo/index)` → `[f](bar/index)`).
- [ ] BUG-12: report relative bare-stem relocations under an honest
      strategy name (e.g. `ShortestPath`), not `link-case-mismatch`, and
      align its gating with the site-absolute form (one consistent rule;
      record as a DEC either way).

## Acceptance criteria

- [ ] `[c](t.md#sub-section)` against `### Sub Section` is not broken;
      `[c](t.md#nope)` is; `[b](#nope)` same-file is caught
- [ ] HYALO006 line numbers exact for 0/3/5-line frontmatter
- [ ] One `[b](foo/)` occurrence produces exactly one backlink entry,
      and `backlinks` agrees with `find --broken-links` on all eight
      dogfood spellings
- [ ] `/deep/page?x=1` rewrites to `/deep/Page?x=1`; titled links
      resolve and survive rewrites
- [ ] All ten `mv` spelling forms round-trip unchanged in style
- [ ] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean

## Non-goals

- Confidence scoring and `--apply-fuzzy` floors —
  [[iterations/iteration-212-fuzzy-confidence-trust]].
- Line numbers in `find --broken-links` output (dogfood UX-6) — goes
  with the JSON-shape work if that gets planned.
