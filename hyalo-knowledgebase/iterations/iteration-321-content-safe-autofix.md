---
title: "Iteration 321: content-safe autofix"
type: iteration
date: 2026-11-05
tags: [iteration, lint, links-auto, autofix, dogfooding]
status: completed
branch: iter-321/content-safe-autofix
priority: 1
related: ["[[dogfood-results/dogfood-v0251-rc-2026-11-05]]", "[[reviews/deep-review-2026-11-05]]", "[[iterations/iteration-320-dogfood-leftovers]]", "[[decision-log]]"]
---

# Content-safe autofix

## Problem

The 0.25.1 release-candidate dogfood and the deep review of the same day found one
root cause behind a dozen reports ([[reviews/deep-review-2026-11-05]] L-3, L-13;
[[dogfood-results/dogfood-v0251-rc-2026-11-05]] B-1 to B-4): `BodySyntax` modelled
fences, indented code and HTML comments only, every other false positive of a stock
rule got its own `if rule_id ==` branch in `lint_body`, and `links auto` kept a
parallel scanner that knew none of HTML, math or block ids. `lint --fix` therefore
rewrote URLs in `href="…"`, `<code>` and `<pre>`, turned `getGamepads()[0]` into
`getGamepads[](0)`, treated `# comment` in a `<pre>` as a heading, and `links auto`
wrapped text inside `<a>…</a>`, `$$…$$` and `^block-id`s. On MDN 341 of 425 changed
edits were inside raw HTML before this iteration.

DEC numbers: **DEC-381 to DEC-383**.

## Design

`hyalo_core::protected` is the single model of protected regions
(`BodySyntax::with_protected`): block regions (raw HTML blocks per CommonMark §4.6,
`$$` blocks), inline regions (code spans, HTML tags with attribute values across
lines, the content of `<a>`/`<code>`/`<pre>`/`<kbd>`/`<samp>`/`<script>`/`<style>`/
`<textarea>`, `$…$`, `{{ }}`/`{% %}`, HTML comments, `^block-id`, a link label that
wraps lines) and link destinations (CommonMark §6.3). It rides on the existing line
loop, is linear (pending constructs retract at the block end) and is computed only
for lint and `links auto`. The lint engine drops a diagnostic that starts in
protected text and withholds a fix whose *changed* bytes reach it; `links auto`
skips a match that overlaps a region.

## Tasks

The model and its consumers:

- [x] `hyalo_core::protected`: block regions, inline regions, destinations, linear scan, unit tests (DEC-381)
- [x] `BodySyntax::with_protected`; container flags for block quote and list item; tab-indented fences recognised
- [x] `lint_body` reads the model for every stock rule except the code-block-aware ones and MD033; fixes are judged by the bytes they change
- [x] `links auto` reads the model (B-4 `^block-id`, L-13 `<a>…</a>` and `$$…$$`)

Findings:

- [x] B-1 MD034 flags prose URLs only (attributes, `<code>`, `<pre>`, macros, math, `"mailto:"`, two-line link text)
- [x] B-2 MD011 does not read `()[0]` or a call-and-index as a reversed link
- [x] B-3 MD022/MD023 stay out of `<pre>`, indented or not
- [x] B-4 `links auto --apply` leaves `^youtube` block ids alone
- [x] L-1 `apply_body_fixes`: an insertion at a replacement's start is a reported conflict, never stray text (DEC-383)
- [x] L-2 MD022/MD031 do not fix inside a block quote; MD023 quiet inside a container (DEC-383)
- [x] L-4 MD010 keeps the finding, never rewrites code (DEC-382)
- [x] L-13 `links auto --apply` leaves `<a>…</a>` and `$$…$$` alone
- [x] A-3 `links auto` and `links fix` write `\|` in a table row
- [x] B-14 a code span does not cross list items in `links auto` (DEC-381)
- [x] A-19 `lint --fix --fix-rule BOGUS` exits 1
- [x] B-13 `lint --fix` reports progress on stderr past 200 files
- [x] C-2 the code-span closer search is capped, so a huge block is linear (DEC-381)

Proof and documentation:

- [x] Unit and e2e tests for every finding (`iteration321_content_safe_autofix.rs`)
- [x] Corpus proof on MDN, GitHub Docs and the Obsidian Hub: changed edits inside a protected region, idempotence, no new finding, properties unchanged, rule counts explained, CPU time
- [x] CHANGELOG, DEC-381 to DEC-383, `docs/schema-and-lint.md`, lint and `links auto` help, rule template, `.claude/CLAUDE.md`
- [x] Gates: fmt, clippy, tests, every xtask gate, `generate-npm-packages --check`, `cargo deny`, `hyalo lint`

## Results

Corpora copied to scratch (MDN 14 375 files, GitHub Docs 3 710, Obsidian Hub 6 520), `lint --fix` and `links auto --apply` run with the 0.25.1 binary and this branch, every changed edit classified by an independent checker (markdown-it for block structure, per-paragraph regexes for inline constructs).

| Corpus | edits | in a protected region before | after |
|---|---|---|---|
| MDN `lint --fix` | 425 → 76 | 341 | 0 |
| GitHub Docs `lint --fix` | 1 368 → 658 | 709 | 0 |
| Hub `lint --fix` | 24 127 → 23 986 | 88 | 4 |
| GitHub Docs `links auto --apply` | 22 036 → 21 958 | 55 | 4 |
| Hub `links auto --apply` | 14 031 → 13 953 | 50 | 2 |

The 10 remaining are checker artefacts, read by hand: four `$…$` hits are `$env:NAME` inside backticks (the checker's regex ignores code spans), two are a URL after `](` that lost its closing parenthesis (no link by CommonMark §6.3), one is a diff-granularity artefact on a 2 KB line, one a fence closed at a different indent than it was opened.

- Idempotence: a second `lint --fix` changed 0 files on all three corpora; no file/rule pair has more findings after the fix than before.
- `find --fields properties` output is byte-identical before and after `lint --fix` on all three.
- Lint counts by rule, before → after: MDN MD034 191 → 18 (all 18 read: prose URLs) and MD011 1 → 0; GitHub Docs MD034 126 → 120 (link text over two lines, `<code>https://<em>…`) and MD023 3 → 0 (`<pre>`); Hub MD034 116 → 113 (`"https://"` as a word). Nothing else moved; MD022/MD031 findings in block quotes are still reported, only their fix is withheld (Hub MD031 fixes 62 → 60).
- User CPU, MDN `lint`: 18.7 s before, 18.9 s after (+1%); `summary` 1.49 s both. An intermediate version was +18% because the protected checks ran after the fix conversion; they now run before it.
- `lint --fix` second pass conflicts: Hub 11 → 37 reported conflicts, the insert-plus-replace pairs that used to be applied together and corrupt text; the following pass applies them.
- Review round on PR #402: the block tracker is an overlay (0 lines disagree on code classification between `BodySyntax::new` and `with_protected` over 1 889 000 lines of the three corpora); a `$$` block needs a real closer within 128 lines and no fence before it; snapshot format 9.
- `%%` comment blocks (DEC-381 addendum): a line starting `%%` with no second `%%` opens, a line ending `%%` closes, an opener without a later closer stays prose; one definition in `BodySyntax`, the scanner's test-only second copy is gone. Against the 0.25.1 build, Hub lint loses only findings inside real comments (MD009 -43, MD022 -2, MD034 -3) and gains 45 inventoried links (73 085 → 73 129 in `find --fields links`) that sat in prose between two comments (mostly URLs; `Alfred`, `PARA`, `SlRvb`), and loses one (`🗂️ Vaults`, inside a comment). GitHub Docs and MDN links are unchanged.
