---
title: Iteration 207 — inert-zone completion (release blocker)
type: iteration
date: 2026-08-23
status: planned
branch: iter-207/inert-zone-completion
tags:
  - iteration
  - links
  - integrity
related:
  - "[[dogfood-results/dogfood-v0210-pre2-integrity-wave]]"
  - "[[iterations/iteration-200-links-apply-integrity]]"
  - "[[iterations/iteration-202-boundary-completion]]"
---

# Iteration 207 — inert-zone completion (release blocker)

## Goal

Close the remaining `links --apply` corruption paths found by the
2026-08-23 dogfood (BUG-1 through BUG-4) and fix the one regression the
integrity wave introduced (BUG-7). All four corruption bugs are
pre-existing in released 0.20.0, but they are exactly the class v0.21.0
claims to fix — this iteration gates the release.

## Context

See [[dogfood-results/dogfood-v0210-pre2-integrity-wave]] "Bugs Found"
for full repros and corpus measurements. Summary:

- **BUG-1 (HIGH)**: one unmatched backtick (e.g. `` press <kbd>`</kbd> ``)
  flips inline-code parity for the rest of the file, so `links auto
  --apply` injects wikilinks into later code spans (`` `git blame` `` →
  `` `[[git]] blame` ``). 9 hits on the GitHub Docs copy, 8 on
  vscode-docs, 3 in this very knowledgebase. Under CommonMark an
  unmatched backtick is literal text and code spans require matching
  backtick-run lengths — the parity accumulator is the wrong model.
- **BUG-2 (MEDIUM)**: `links auto --apply` inserts wikilinks inside
  Liquid expressions — 3,328 of 11,141 insertions (30%) on the GitHub
  Docs copy landed inside `{% … %}` / `{{ … }}`.
- **BUG-3 (MEDIUM)**: `links auto --apply` inserts wikilinks inside raw
  HTML tags and attribute values (128 hits vscode-docs, 5 GitHub Docs) —
  breaks `src`/`href` paths, anchor names, and class hooks.
- **BUG-4 (MEDIUM-HIGH)**: `links fix --apply` treats
  `{% ifversion … %}/path{% endif %}/…` as literal path text and
  fuzzy-rewrites it, silently dropping the conditional. 25 offers at
  0.95 on the full GitHub Docs corpus. The round-trip guard cannot catch
  this — the rewritten target genuinely resolves; the corruption is
  semantic.
- **BUG-7 (MEDIUM, iter-202 regression)**: walker canonical dedup keeps
  the alphabetically-first path, so an in-vault symlink
  (`alias-target.md -> target.md`) shadows the real file: a link
  fixable at `[fuzzy 0.966]` becomes `Unfixable: 1`, and fixes are
  reported against the alias name.

## Tasks

- [ ] Replace the inline-code parity model in `inert_link_zones` with a
      CommonMark-correct code-span scanner: a code span opens only when a
      backtick run is later closed by a run of equal length; an unmatched
      run is literal text and must not flip state for the rest of the
      line/file. Add the BUG-1 minimal repro (kbd-backtick) and the three
      real-corpus shapes as fixtures.
- [ ] Add Liquid inert zones: `{% … %}` and `{{ … }}` spans are inert for
      `links auto` candidate matching. Unterminated markers should be
      conservative (treat rest of line as inert) rather than corrupting.
- [ ] Add raw-HTML inert zones: HTML tag spans (from `<` of a recognized
      tag/comment/autolink-lookalike to its closing `>`), covering
      attribute values. Text *between* tags stays linkable
      (`<div>prose</div>` — prose is fair game; the tag is not).
- [ ] `links fix`: skip (report as ignored/unfixable with a distinct
      reason, never rewrite) any link target containing `{%`, `{{`, or
      `${` — templated destinations are dynamic, not broken.
- [ ] BUG-7: make canonical dedup prefer the non-symlink path as the
      representative (fall back to first-seen only when all candidates
      are symlinks). Fix reporting so fixes cite the real file's path.
      Regression test from the dogfood repro (fuzzy 0.966 must survive
      adding `alias-target.md`).
- [ ] Re-run the dogfood corpus verification on scratch copies of GitHub
      Docs and vscode-docs: `links auto --apply` must produce **0**
      insertions inside code spans, Liquid expressions, or HTML tags
      (use the dogfood's grep patterns), and `links fix` must offer 0
      rewrites of templated targets. Record the numbers in this file.
- [ ] Update docs/help (`links --help`, knowledgebase docs) to name the
      complete inert-zone list and the templated-target skip.

## Acceptance criteria

- [ ] The BUG-1 minimal repro (unmatched backtick in `<kbd>`) produces 0
      insertions inside code spans
- [ ] GitHub Docs scratch copy: 0 wikilinks inserted inside Liquid
      expressions or HTML tags (was 3,328 / 5); vscode-docs: 0 (was 8 in
      code spans / 128 in HTML)
- [ ] `links fix` on the GitHub Docs corpus offers 0 rewrites for targets
      containing `{%`/`{{`/`${` (was 25), and they are reported under a
      named bucket rather than silently dropped
- [ ] The BUG-7 repro reports `fuzzy 0.966` with the symlink present, and
      the fix is attributed to the real filename
- [ ] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean

## Non-goals

- Full HTML/Liquid parsing — conservative span detection is enough; when
  in doubt, mark inert (a missed auto-link candidate is free, a corrupted
  file is not).
- Fuzzy confidence scoring (BUG-11) — that is
  [[iterations/iteration-212-fuzzy-confidence-trust]].
- Anchor/slug resolution (BUG-8) — that is
  [[iterations/iteration-211-links-resolution-correctness]].
