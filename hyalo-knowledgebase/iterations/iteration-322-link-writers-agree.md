---
title: "Iteration 322: link writers agree with the reader"
type: iteration
date: 2026-11-05
tags: [iteration, links, mv, links-fix, dogfooding]
status: completed
branch: iter-322/link-writers-agree
priority: 1
related: ["[[dogfood-results/dogfood-v0251-rc-2026-11-05]]", "[[reviews/deep-review-2026-11-05]]", "[[iterations/iteration-321-content-safe-autofix]]", "[[decision-log]]"]
---

# Link writers agree with the reader

## Problem

Everything hyalo writes as a link has to resolve the way hyalo reads it, and the way Obsidian reads it. The deep review (C-13) found the root cause: a link target was normalised by three separate implementations (the catalog, the legacy `discovery` path, and the writers' own splits), so a spelling the reader accepted could be one a writer did not find or kept wrong. The dogfood findings below are all that one drift.

## Approach

One canonical split of an authored target into path, `?query`, `#fragment` and `|alias` (`link_target::TargetParts`, DEC-384), taken by the reader and by `mv`, `links fix` and `LinkWriter`; then each finding closed on top of it. No new flags or config keys. No snapshot change (a stored link is parsed as before), so the format stays 9.

## Tasks

- [x] Reproduce A-1, A-2, A-4, A-7, C-1, C-12, B-8, B-12, B-6, B-9, B-16, C-7, C-18 on the baseline build
- [x] C-13: `link_target::TargetParts` and `resolution_path`; reader and writers use it
- [x] A-4: `mv` finds and rewrites `?query`, `./` and frontmatter spellings it counted as backlinks
- [x] C-12: `./x` and `../x` wikilinks are source-relative in every reader
- [x] C-1: a wikilink matches a path suffix
- [x] A-7: `links fix` and `summary` agree with the reader on short forms and relocations
- [x] A-1, A-2, B-12: alias fix shape in body and frontmatter, `emitted_target` for frontmatter fixes
- [x] B-8, B-16: `links fix` text names the plain-apply fixes; markdown `suggested_fragment` is a slug
- [x] C-7, C-18: stale replacement offsets are skipped; exact pre-check on the fuzzy gate
- [x] B-6, B-9: decided and recorded without a code change (DEC-386)
- [x] Tests: unit, e2e (`iteration322_link_writers_agree`)
- [x] Round trip, read-side differential, idempotence and timing on MDN, GitHub Docs and the Obsidian Hub
- [x] Gates: fmt, clippy, tests, every xtask gate, `generate-npm-packages --check`, `cargo deny`, `hyalo lint`
- [x] Docs in sync: CHANGELOG, help text, `.claude/CLAUDE.md`, templates, DEC-384 to DEC-386

## Findings

| Id | Outcome |
| --- | --- |
| C-13 | Fixed: one split, DEC-384 |
| A-4 | Reproduced, fixed: all 5 links of the repro followed the move |
| C-12 | Reproduced, fixed: `[[../t]]` is in-vault, counted, rewritten |
| C-1 | Reproduced, fixed: path-suffix wikilinks resolve; shared suffix is ambiguous |
| A-7 | Reproduced, fixed for the over-count and the relocation under-count; the third disagreement (backlinks of an ambiguous link) is recorded, not changed (DEC-384) |
| A-1 | Reproduced, fixed (LF and CRLF share one path) |
| A-2 | Reproduced, fixed |
| B-12 | Reproduced, fixed (frontmatter fixes carry `emitted_target`) |
| B-8 | Reproduced, fixed in text only (DEC-385) |
| B-16 | Reproduced, fixed (markdown suggestion is a slug); a heading with an explicit `{#id}` still yields the slug of its whole text, see Follow-ups |
| C-7 | Fixed with a unit test |
| C-18 | Mitigated (exact pre-check), order of growth unchanged (DEC-386) |
| B-6 | Reproduced, not changed (DEC-386) |
| B-9 | Not verifiable: the Obsidian Help pages do not cover `&` in heading links, behaviour unchanged (DEC-386) |

## Follow-ups

- GitHub Docs headings written as `## Title {#custom-id}` are not recognised as anchors, so `[x](page#custom-id)` is reported as a broken anchor; found while checking B-16 (4 of the 5 changed suggestions). Not in this iteration.
- A root note's `backlinks` still lists an ambiguous `[[./note]]` (DEC-384, needs a separate ambiguous-link map in the graph).

## Proof

Baseline binary: `origin/main` at 39786c5e. New: this branch. Corpora: fresh copies of MDN (`files/en-us`, `--site-prefix en-US/docs`), GitHub Docs (`content`), Obsidian Hub.

**1. Round trip** (re-read after writing; a fix is resolved when a link on its line resolves to the planned `new_target`; an auto match when a link with its target resolves to that note or its directory index).

| Corpus | `links fix --apply` | `links fix --apply --apply-fuzzy` | `links auto --apply` | `find --broken-links` |
| --- | --- | --- | --- | --- |
| Hub | 58 written, 58 resolved, 0 mismatched (48 case, 2 relocation, 8 alias) | 59 / 59 / 0 | 6961 / 6961 / 0 | 162 -> 154 -> 153 (fuzzy) |
| GitHub Docs | 0 (nothing plain-applicable) | 2173 / 2173 / 0 (1998 basename fallback, 175 fuzzy) | 10979 / 10979 / 0 | 7406 -> 5233 (fuzzy) |
| MDN | 0 | 1 / 1 / 0 | 0 (no title source in this tree, same as baseline) | 522 -> 521 |

An earlier GitHub Docs `links auto` check counted 1998 matches as "resolved to another stem"; I read the sample: all are `[[billing|Billing]]`-style links resolving to `billing/index.md`, the directory-index note whose folder name is the title. The checker was wrong, the links are right.

`mv` samples: Hub 3 directory batches (101 files, 108 links updated), GitHub Docs 3 batches (77 files, 406 links), MDN 30 single moves of the most-linked pages (7044 links). Every moved file kept its backlink count (0 mismatches) and `summary.links.broken` did not move (162, 7406, 522).

**2. Read-side differential** against the baseline: `find --fields links` (Hub 73150 links, GitHub Docs 20975, MDN 75140), `find --fields backlinks` (byte-identical on all three), `summary.links` (identical), HYALO006 (7406, 162, 49784 without a prefix; identical) and `links fix` JSON (byte-identical). The only changes are `suggested_fragment` on 5 GitHub Docs and 58 MDN markdown links, all of them the heading text becoming the GFM slug (`Alternative text` -> `alternative-text`, `Using emojis` -> `using-emojis`). Four of the five GitHub Docs ones are headings written with a kramdown `{#id}` suffix, whose slug is wrong either way (Follow-ups). None of the corpora contains a `../` wikilink, a `?query` wikilink or a partial-path wikilink that changed a verdict, so the new resolver behaviour is exercised by the e2e tests and the hand repros only.

**3. Idempotence.** A second `links fix --apply` and a second `links auto --apply` changed 0 files on every corpus. `find --fields properties` is byte-identical before and after the fuzzy fix and auto runs on Hub and GitHub Docs.

**4. Timing**, MDN with prefix, same session, 6 alternating runs, median of the last 5: `links fix` 3.44 s base, 3.32 s new; `find --fields links` 2.04 s base, 2.13 s new (noise level). A synthetic vault of 2000 notes with 10000 distinct broken targets, where the gate dominates: `links fix` 11.6 s base, 4.9 s new, byte-identical output.

## Not verified

- Obsidian itself was not run. B-9 stays open (DEC-386); C-1 and C-12 follow Obsidian's `getFirstLinkpathDest` behaviour as I know it, which the Help pages do not document.
- Windows and Linux were not run locally; the code is path-separator neutral and CI covers both.
- A literal `?` in a note name (`What?.md`) moved with a wikilink to it was not re-tested end to end beyond the unit paths that keep the literal-stem lookup first.
