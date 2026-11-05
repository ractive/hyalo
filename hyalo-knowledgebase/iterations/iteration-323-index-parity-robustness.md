---
title: "Iteration 323: index parity and robustness"
type: iteration
date: 2026-11-05
tags: [iteration, index, robustness, mv, dogfooding]
status: completed
branch: iter-323/index-parity-robustness
priority: 1
related: ["[[dogfood-results/dogfood-v0251-rc-2026-11-05]]", "[[reviews/deep-review-2026-11-05]]", "[[iterations/iteration-322-link-writers-agree]]", "[[decision-log]]"]
---

# Index parity and robustness

## Problem

A command should give the same answer on disk and under `--index`, an incremental `create-index` should hold what `--force` holds, and hyalo should neither fall over nor lie on awkward input. The dogfood run (A-*) and the deep review (C-*, L-*) left 34 findings of those kinds; iterations 321 and 322 rewrote parts of the code they sit in, so each was reproduced on the current build first.

## Approach

Group by root cause (DEC-389 to DEC-392); no new flags or config keys. Snapshot format 10.

## Findings

| Root cause | Findings | Outcome |
| --- | --- | --- |
| Snapshot used only when it describes the vault and the settings | A-5, A-6, A-12, A-18, C-16 | fixed, DEC-389 |
| Exit codes, installers and generators | A-9, A-10, A-11, A-14, A-15, A-13 (init half), L-5, L-6, L-7, L-14, L-15, L-16, L-20, A-20 | fixed, DEC-390 |
| Comments in frontmatter | A-8, A-16, A-17 | fixed (splice keeps header comments, trailing comments and flow lists) |
| Relative wikilink form | M2 | fixed, DEC-391 |
| Hostile input | C-3, C-4, C-5, C-6, C-8a, C-10, C-11, C-14, C-17, L-19 | fixed, DEC-392 |
| Decided without a code change | A-13 (create-index), A-21, C-8b, C-9 | DEC-392 |

## Tasks

- [x] Baseline binary saved from origin/main; every finding reproduced on it
- [x] A-5, A-6, A-12, A-18, C-16: index parity (DEC-389), snapshot format 10
- [x] A-9, A-10, A-11, A-14, A-15, L-5, L-6, L-7, L-14, L-15, L-16, L-20, A-20: honest installers and generators (DEC-390)
- [x] A-8, A-16, A-17: frontmatter comments survive writes
- [x] M2: relative wikilinks stay relative; move matrix asserts the form (DEC-391)
- [x] C-3, C-4, C-5, C-6, C-8a, C-10, C-11, C-14, C-17, L-19: robustness (DEC-392)
- [x] Tests: `iteration323_index_parity_robustness` e2e and unit tests for each fix
- [x] Gates: fmt, clippy, tests, every xtask gate, npm packages check, cargo deny, hyalo lint
- [x] Case-sensitive volume run of the e2e suite
- [x] CHANGELOG, help text, templates and `.claude/CLAUDE.md` in sync

## Proof

Corpora: MDN (14 375 notes), GitHub Docs (3 710), Obsidian Hub (6 540), fresh copies.

- **Disk against `--index`**: `find` with links, properties, backlinks, `--broken-links`, `--orphan`, `--dead-end`; `summary`; `tags`; `properties`; `lint`; 25 `backlinks` per corpus. 0 differences on all three outside `results.source`.
- **Incremental against forced** after 20 edits, 5 deletes, 5 renames, 5 additions: header (bar `created_at`, `pid`), entries and graph identical; BM25 differs only in document numbering; 13 read commands plus 25 backlinks agree on all three corpora.
- **Baseline against new, on disk**: the same commands, 0 differences on all three. The corpora hold no relative wikilink, no `- [x]word` checkbox and no BOM, so none of the changed behaviours is reachable there; 40 single-note `mv --dry-run` per corpus and one batch were byte-identical to the baseline (bar the order of `updated_files`, which varies from run to run on the baseline too, a follow-up).
- **MDN timing**, median of 3, baseline against new: `create-index` cold 3.71 s / 3.84 s, no-op 1.45 s / 1.30 s, `find --index` 0.40 s / 0.40 s, `summary --index` 0.56 s / 0.55 s, `lint` 6.25 s / 6.25 s.
