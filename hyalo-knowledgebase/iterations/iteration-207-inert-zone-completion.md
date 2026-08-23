---
title: Iteration 207 — inert-zone completion (release blocker)
type: iteration
date: 2026-08-23
status: completed
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

## Tasks [6/6]

- [x] Replace the inline-code parity model in `inert_link_zones` with a
      CommonMark-correct code-span scanner: a code span opens only when a
      backtick run is later closed by a run of equal length; an unmatched
      run is literal text and must not flip state for the rest of the
      line/file. Add the BUG-1 minimal repro (kbd-backtick) and the three
      real-corpus shapes as fixtures.
- [x] Add Liquid inert zones: `{% … %}` and `{{ … }}` spans are inert for
      `links auto` candidate matching. Unterminated markers should be
      conservative (treat rest of line as inert) rather than corrupting.
- [x] Add raw-HTML inert zones: HTML tag spans (from `<` of a recognized
      tag/comment/autolink-lookalike to its closing `>`), covering
      attribute values. Text *between* tags stays linkable
      (`<div>prose</div>` — prose is fair game; the tag is not).
- [x] `links fix`: skip (report as ignored/unfixable with a distinct
      reason, never rewrite) any link target containing `{%`, `{{`, or
      `${` — templated destinations are dynamic, not broken.
- [x] BUG-7: make canonical dedup prefer the non-symlink path as the
      representative (fall back to first-seen only when all candidates
      are symlinks). Fix reporting so fixes cite the real file's path.
      Regression test from the dogfood repro (fuzzy 0.966 must survive
      adding `alias-target.md`).
- [x] Re-run the dogfood corpus verification on scratch copies of GitHub
      Docs and vscode-docs: `links auto --apply` must produce **0**
      insertions inside code spans, Liquid expressions, or HTML tags
      (use the dogfood's grep patterns), and `links fix` must offer 0
      rewrites of templated targets. Record the numbers in this file.
- [x] Update docs/help (`links --help`, knowledgebase docs) to name the
      complete inert-zone list and the templated-target skip.

## Verification (2026-08-23, scratch corpus copies)

`links auto --apply` on fresh copies of GitHub Docs `content/` (3,710 files,
35,860 insertions) and vscode-docs (780 files, 32,924 insertions), then
checked with a CommonMark-correct code-span scanner plus Liquid/HTML span
matching (`check_zones.py`; the dogfood's raw greps over-report, since
`` `a` [[x]] `b` `` matches a naive backtick pattern):

| Corpus | inside code spans | inside Liquid | inside HTML tags |
|---|---|---|---|
| GitHub Docs (was 9 / 3,328 / 5) | **0** | **0** | **0** |
| vscode-docs (was 8 / — / 128) | **0** | **0** | **0** |

The three `[[…]]` occurrences the scanner still reports inside GitHub Docs
code spans (`` `[[source]]` ``, `` `[[tool.poetry.source]]` ``,
`` `[[Nameofwikipage|Link Text]]` ``) and the one on vscode-docs
(`` `src/routes/post/[[]id[]]/**` ``) are pre-existing corpus content —
TOML tables and wiki-syntax documentation — present byte-for-byte in the
pristine checkouts, not insertions.

`links fix --dry-run` on the full GitHub Docs corpus: 6,099 broken,
0 fixable, 4,658 fuzzy, 1,378 unfixable, **63 templated** — and **0** of the
6,099 rewrite offers targets a destination containing `{%` / `{{` / `${`
(was 25).

BUG-7 repro: with `alias-target.md -> target.md` present, `links fix
--dry-run` still reports `source.md line 5: "targt" → "target.md"
[fuzzy 0.966]` and `Unfixable: 0`; the fix is attributed to `target.md`,
not the alias.

## Acceptance criteria [5/5]

- [x] The BUG-1 minimal repro (unmatched backtick in `<kbd>`) produces 0
      insertions inside code spans
- [x] GitHub Docs scratch copy: 0 wikilinks inserted inside Liquid
      expressions or HTML tags (was 3,328 / 5); vscode-docs: 0 (was 8 in
      code spans / 128 in HTML)
- [x] `links fix` on the GitHub Docs corpus offers 0 rewrites for targets
      containing `{%`/`{{`/`${` (was 25), and they are reported under a
      named bucket rather than silently dropped
- [x] The BUG-7 repro reports `fuzzy 0.966` with the symlink present, and
      the fix is attributed to the real filename
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean

## Non-goals

- Full HTML/Liquid parsing — conservative span detection is enough; when
  in doubt, mark inert (a missed auto-link candidate is free, a corrupted
  file is not).
- Fuzzy confidence scoring (BUG-11) — that is
  [[iterations/iteration-212-fuzzy-confidence-trust]].
- Anchor/slug resolution (BUG-8) — that is
  [[iterations/iteration-211-links-resolution-correctness]].
