---
title: "Iteration 324: search, hints and gates"
type: iteration
date: 2026-11-05
tags: [iteration, search, hints, gates, dogfooding]
status: completed
branch: iter-324/search-hints-gates
priority: 1
related: ["[[dogfood-results/dogfood-v0251-rc-2026-11-05]]", "[[reviews/deep-review-2026-11-05]]", "[[iterations/iteration-323-index-parity-robustness]]", "[[decision-log]]"]
---

# Search, hints and gates

## Problem

A search suggestion has to lead to results, the canonical page has to rank where a reader expects it, every hint hyalo prints has to be runnable and worth running, and the repository's gates have to fail when the thing they guard breaks. No new flags or config keys. The snapshot format stays 10: every change is query-time.

## Findings

| Root cause | Findings | Outcome |
| --- | --- | --- |
| A stem is not a fixed point of the stemmer | B-5 | fixed, DEC-393 |
| Title and prose shared one saturating curve | B-7 | fixed, DEC-394 |
| Hints that lead nowhere, print as commands, or drop the flags of the command that printed them | B-10, B-11, B-15, B-17, B-18, B-19, L-12 | fixed, DEC-395 |
| Gates that fail open | L-8, L-9, L-10, L-17, L-18 | fixed, DEC-396 |
| Dead public API and `expect` in non-test code | C-15, L-11 | fixed, DEC-397 |
| Leftovers of #404 | N1, N2, N3 | fixed, DEC-397 |

## Tasks

- [x] Reproduce every finding on the current build and save a baseline binary
- [x] B-5: a plain word matches the dictionary term spelled like it
- [x] B-7: find out why `title = 10` changes nothing, then score the title apart
- [x] B-7: judge it on a written query list and on exact-title searches, before and after
- [x] B-10, B-19: shipped recipe and doc drift
- [x] B-11, B-17, B-18, L-12: hint wording, flags and dead ends
- [x] B-15: a refused snapshot is hinted as stale
- [x] L-8, L-9, L-10, L-17, L-18: tighten the gates and show each fail on its condition
- [x] C-15, L-11: dead public API and `expect`
- [x] N1, N2, N3
- [x] Hint audit on the three corpora, plain and with `--index`, `--format` and `--dir`
- [x] Baseline differential on disk and disk against `--index`
- [x] MDN timing against the baseline
- [x] Gates, npm gate, case-sensitive volume, `lint-kb`
- [x] CHANGELOG, help, skills and templates in sync

## Results

Measured on fresh copies of MDN `files/en-us`, GitHub Docs `content` and the Obsidian Hub, against a binary built from `2b41adcd`.

- **B-7 ranking** (DEC-394). Fixed list of 29 API, element and topic queries: 19 improved, 9 unchanged, 1 worse (`flexbox` 3 to 9, where pages titled Flexbox now lead). `addEventListener` 43 to 1, `content security policy` 46 to 3, `service worker cache` 27 to 12, `workflow syntax` 28 to 1, `personal access tokens` 9 to 1, `fetch` 30 to 8, `video element` 12 to 1. Searching the exact title of 120 random pages per corpus: MDN top-1 69 to 106 (MRR 0.710 to 0.937), GitHub Docs 81 to 103 (0.762 to 0.887), Obsidian Hub 113 to 117 (0.966 to 0.988). A first variant that saturated headings and tags separately was worse on GitHub Docs (MRR 0.762 to 0.734) and was dropped. The title weight now scales: `getElementById` scores 17, 28, 65 for `title` 1, 3, 10 (12 at all three before).
- **B-5** (DEC-393), with `--index-file` and 4 probes in parallel. Hub, GitHub Docs and MDN: 396, 421 and 455 suggested candidates over 149, 152 and 155 real misspellings (swap, drop, substitute of frequent terms) with no result; every candidate returns documents, plain and inside quotes (`"cand"`), and so does every `Did you mean` corrected query (149, 152, 155, plain and quoted): 0 failures. 100 listed terms per corpus: `find` returns the document count `terms` lists for 298 of 300; the two others (GitHub Docs `occasion` 26 vs 2, `disagre` 3 vs 2) are the stated limit, where the typed word's own stem also occurs and only that stem is searched.
- **Hint audit**: 735 commands (34 commands over three corpora in seven modes: plain, `--format json`, `--format text`, `--index-file`, both, a refused v7 snapshot and a missing one; `--glob`-scoped `find`, `lint`, `tags summary` and `properties summary`, and `read --section` included), 1,524 hints. 1,306 read-only hints run: 0 non-zero exits apart from 13 `lint` hints that exit 1 because the corpus has findings, 0 dropped `--format`, `--index-file` or `--dir`, 0 loops, 0 hints carrying a refused or missing index path. 186 write hints were not run. 32 hints have an empty `cmd`: all the site-absolute note, printed as `note:`.
- **Baseline differential** on disk, `find` with `--fields links`, `--fields properties`, `--broken-links`, `--orphan`, `--dead-end`, plus `summary`, `tags`, `properties`, `lint`, three corpora: 27 of 27 byte-identical to the baseline with hints off.
- **Disk against `--index`** on the same 27 commands: byte-identical, bar the three index-provenance keys `summary --index` adds (`source`, `index_format_version`, `code_blocks`). 38 ranked queries, suggestions included, are byte-identical on disk and under `--index`.
- **MDN timing** (best of 3, baseline against new, same session): `create-index` cold 3.24 s and 3.24 s, `find --index` on a ranked query 0.71-0.76 s and 0.71-0.72, ranked `find` on disk 4.90-5.04 and 4.73-5.17 (rerun after the review round; the other operations are unchanged from the first round: `create-index` cold 3.24 and 3.24, `summary --index` 0.79 and 0.80, `lint` 5.25 and 5.11).
- **Gates**: fmt, clippy `-D warnings`, `cargo test --workspace -q` 5,943 passed over all binaries, every xtask gate, `generate-npm-packages --check`, `cargo deny check`, the npm typecheck, build and 34 tests, and the e2e suite on a case-sensitive volume (only `wikilink_resolution_folds_unicode_normalization` fails, as on the baseline).
- **Gates shown to fail**: a wrong working directory and a bogus `CARGO_MANIFEST_DIR` (`workspace_root`), a symlinked and a stale `node_modules` (`check-pi-runtime`), a module that stops naming the journal, one that only mentions it in a comment, and an unclassified module (`check-mutation-journal`), `hyalo --dir kb tags rename ... --jq` in a shipped document (`check-jq-recipes`), a build failure, a nested `links auto` without examples and a truncated listing (`check-help-drift`, `check-command-reference`; the recursive walk found `drop-index` without examples), and a journal reference in a `#[cfg(test)]` use only (`check-mutation-journal`).
