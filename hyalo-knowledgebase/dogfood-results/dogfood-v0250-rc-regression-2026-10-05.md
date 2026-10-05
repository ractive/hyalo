---
title: "Dogfood v0.25.0-rc — regression pass after iterations 311–317"
type: research
date: 2026-10-05
status: active
tags: [dogfooding, regression, search, index, links, release]
related:
  - "[[dogfood-results/dogfood-v0250-pre-search-roadmap-2026-10-04]]"
  - "[[iterations/iteration-311-link-repair-and-graph-parity]]"
  - "[[iterations/iteration-312-search-diagnostics-and-read-lines]]"
  - "[[iterations/iteration-313-index-honesty-and-result-caps]]"
  - "[[iterations/iteration-314-config-files-and-yaml-leftovers]]"
  - "[[iterations/iteration-315-remove-field-terms]]"
  - "[[iterations/iteration-316-search-discoverability-for-agents]]"
  - "[[iterations/iteration-317-backlog-leftovers-before-0250]]"
  - "[[iterations/iteration-318-regression-dogfood-fixes]]"
---

# Dogfood v0.25.0-rc — regression pass after iterations 311–317

Binary: `hyalo 0.24.1 (61600876f2d2 2026-10-05)`, built from `main` after iterations
311–317 and installed to PATH; the hash check passed before any command ran. Purpose:
confirm that the fixes driven by [[dogfood-results/dogfood-v0250-pre-search-roadmap-2026-10-04]]
hold on the combined tree, exercise what changed since (snapshot format v4 → v7, the
field-term removal, the new hints and FIND 101), and look for anything new before the
breaking 0.25.0 release.

Three parallel explorers plus the orchestrator's own verification:

| Explorer | Corpora | Scope |
| --- | --- | --- |
| search and help | own KB (527), GitHub Docs (3 710), kepano (103), scratch vaults | the 2026-10-04 search bugs, iterations 312/315/316, a help-only "new user" pass |
| index and links | MDN copy (14 375), GitHub Docs copy, Obsidian Hub copy (6 519) | snapshot v7 life cycle, DEC-368, disk/index parity, `links fix`/`mv` at scale, timings |
| correctness | 10 synthetic vaults, Hub and kepano read-only | iterations 311/314, the earlier review P1s, a 54-case exit-code sweep |

Every write ran in a scratch copy; the real corpora are untouched. Findings marked
**[verified]** were reproduced by the orchestrator on the same binary.

## Headline

- **0 regressions.** Every bug from the 2026-10-04 report that an iteration claimed is
  still fixed on the combined tree; the Hub agrees at 162 broken links on `summary`,
  `find --broken-links` and HYALO006; kepano at 51; 36 disk/`--index` command pairs are
  byte-identical on three corpora; 54 deliberately wrong invocations produced no panic
  and no exit code outside 0/1/2.
- **1 HIGH, 7 MEDIUM, 10 LOW new findings**, none introduced by iterations 311–317 as a
  regression of something that worked, but several in code those iterations touched.
  The HIGH one predates this cycle (iteration 295) and affects every `--jq` user.
- **Performance:** no regression. Indexed reads match the previous numbers, the own-KB
  indexed search is twice as fast, GitHub Docs `links fix` dropped from 9.35 s to
  4.87 s, a no-op `create-index` on MDN from 1.85 s to 1.09 s.
- All findings are filed as [[iterations/iteration-318-regression-dogfood-fixes]];
  what does not fit goes to one backlog note.

## Regression results

| Item (2026-10-04 report) | Status | Evidence |
| --- | --- | --- |
| BUG-1 slugs in wikilink anchors | STILL FIXED | Hub copy `links fix --apply`: `[[my note#Deploy Steps]]` → `[[my note#3. Deploy Steps]]` (embed and `\|alias` forms too); markdown links get slugs |
| BUG-2 `summary` vs alias links | STILL FIXED | Hub 162 = 162 = 162; after apply 155 on all three |
| BUG-3 `mv` and bare attachments | STILL FIXED | `(img.png)`, `![embed](img.png)`, `(.cfgfile)` rewritten to `../…`; `![[img.png]]` untouched and resolving |
| BUG-4 `read --lines` base | STILL FIXED | the range a section hit prints returns that section; past-EOF warns; `9:3`, `0`, `abc` exit 1 |
| BUG-5 leading-dash pattern | PARTIAL | `find '-snapshot'`, `'-sqlite'` get the `--` tip; `find -snapshot index` (a PATTERN follows) does not — NEW-4 |
| BUG-6 / BUG-16 dot-paths | STILL FIXED | `--sort property:versions.ghes` orders both ways; zero-result lists values with counts; one stray warning — NEW-13 |
| BUG-7 / BUG-8 refused snapshots | STILL FIXED | v2 snapshot on the real MDN tree refused under `-q` naming v2 and v7; `code_blocks` mismatch refused under `-q`; `summary --index` reports format, `code_blocks`, `source` |
| BUG-9 HTML anchors | STILL FIXED | `<a id>`, `<a name>`, `<h2 id>` resolve; not inside a comment, code span or fence; GitHub Docs broken anchors 374 → 370 |
| BUG-10 `prefix*` stem fallback | STILL FIXED | `configuration*` 122 = `configur*` |
| BUG-11 backslash filenames | STILL FIXED | 12 commands on `notes/back\slash.md`, an emoji+backslash name and a backslash directory; one link edge — NEW-11 |
| BUG-12 `init --codex` CRLF | STILL FIXED | both host files stay pure CRLF through init ×2 and deinit |
| BUG-13 NFC/NFD | STILL FIXED | both directions resolve; one `mv` edge — NEW-10 |
| BUG-14 YAML 1.1 forms | STILL FIXED | 49 scalars typed identically alone and in one block |
| BUG-15 `[x](/)` | STILL FIXED | all 8 GitHub Docs links resolve to `index.md` |
| BUG-17 plan target | STILL FIXED | text and `emitted_target` show what is written |
| BUG-18 `--facet property:a:b` | STILL FIXED | exit 1 with a runnable `hyalo find --help  # FACETS` pointer |
| BUG-19 non-UTF-8 notes | PARTIAL | body bytes consistent on disk and index; invalid bytes in FRONTMATTER are not — NEW-2 |
| UX-1 did-you-mean | STILL FIXED | `snapshot` ranked first, every typo corrected in one hint, phrase typos corrected |
| UX-2 schema error as a gate | STILL FIXED | one rule across four kinds of broken config and 23 commands |
| UX-3, UX-5, UX-7 | FIXED / FIXED / PARTIAL | `--apply-fuzzy` wording, `truncated`; `find` still suggests `#Intro-duction` where `links fix` plans `#1. Intro` |
| Review P1s (F2, F3, F4, F5), DEC-348, HYALO008 | STILL FIXED | emoji batch `mv`, dotted stems, lint refusal, gitignored named file, code-span anchors |
| Snapshot v7 life cycle, DEC-368 | WORKING | three no-op reruns `written: false` with identical md5; incremental = `--force` on 6 queries; a new note is found after `touch .hyalo-index` in a subdirectory and at the root |
| Field-term removal (315), FIND 101 and hints (316) | WORKING | five grammar counts identical on disk and index; FIND 101 byte-identical in help, skill and rule; every hinted command exits 0 |

## New findings

### NEW-1: every `--jq` fails when SIGINT is ignored (HIGH) [verified]

```text
$ sh -c 'hyalo find snapshot --jq ".total" & wait $!'
{"error": "cannot install jq cancellation handler: Ctrl-C error: Ctrl-C signal handler already registered"}   exit 1
$ nohup hyalo find snapshot --jq '.total'                          same
$ (trap '' INT; hyalo summary --jq '.results.files.total')         same
```

Foreground runs work; `--count` and plain `--format json` are unaffected. A
non-interactive shell starts `&` jobs with SIGINT ignored, as does `nohup`. Introduced
by iteration 295 (2eebf90a), so it is in the released 0.24 line. A `set … --jq` fails
before writing, so there is no data risk, but every documented `--jq` recipe breaks in
background jobs and under any runner that ignores SIGINT.

### NEW-2: invalid UTF-8 in one note's frontmatter breaks whole-vault listings (MEDIUM) [verified]

`printf -- '---\ntitle: Bad \xff\xfe\ntags: [b]\n---\nbody\n' > bad.md`: `properties`,
`tags` and `find --fields file` exit 2 with `frontmatter is not valid UTF-8` and no file
named; `find`, `summary`, `lint` skip the file; with `--index` all three listings exit 0.
`read bad.md` exits 2. Expected: skip and count like every other scan shape, and exit 1
naming the file when it is named.

### NEW-3: `prefix*` and `terms PREFIX` are not accent-folded (MEDIUM)

`find 'résumé'` 12, `find 'resume*'` 13, `find 'résumé*'` 0, `find 'Résum*'` 0; the
zero-result hint `terms 'rés'` returns nothing. Bare words fold accents (DEC-336).

### NEW-4: `find -snapshot index` runs `--section napshot` silently (MEDIUM)

26 results instead of 156, no `--` tip; the tip only fires when no PATTERN follows, and
`-q` silences it where the other query warnings are `-q`-proof.

### NEW-5: `links fix --apply` drops an anchor fix in a file that also gets a target fix (MEDIUM) [verified]

`c.md` with `[[sub/NOTE]]` and `[[note2#Intro]]`: one run reports `case_mismatches: 1,
anchor_fixable: 1, anchors_applied: 0, unapplied: 0`, exit 0, and leaves the anchor
unchanged; a second run applies it. With the two fixes in different files both apply.

### NEW-6: `find --index --file <deleted>` serves the stale entry (MEDIUM) [verified]

Exit 0 with the snapshot's entry; the disk scan exits 1 "file not found". `read` and
`backlinks` under `--index` already refuse. DEC-301: a named path is a promise.

### NEW-7: `find --index PATTERN <deleted file>` exits 2 (MEDIUM) [verified]

`{"error": "reading ranked snippets for note2.md", …}`. Same root cause as NEW-6.

### NEW-8: the field-term migration notice appears only on zero results (MEDIUM, UX)

`title:dogfood` returns 134 files (91 under the old field semantics) with no notice;
`tag:iteration` 215, `path:iterations` 328. The one breaking change in search is
invisible exactly when it produces plausible results.

### LOW

- NEW-9: the migration hint carries a literal `*` into `--title 'birds*'` (a substring flag).
- NEW-10: `mv` rewrites a bare `[[Café]]` whose only difference from the target is Unicode composition (DEC-354 says no rewrite).
- NEW-11: a link written with a backslash to a backslash-named file is broken for `find`/`summary`/lint but listed by `backlinks`; `links fix` writes `%5C` into a wikilink.
- NEW-12: under a TOML syntax error `set --dry-run` exits 0 where the real run refuses.
- NEW-13: a zero-result dot-path filter prints a false "no files matched --property map.b; did you mean: map?" beside the correct hint.
- NEW-14: `set --property p=9223372036854775808` writes a lossy float.
- NEW-15: HYALO006 counts three out-of-vault links on GitHub Docs that `find` and `summary` exclude (7 409 vs 7 406).
- NEW-16: help drift — `read --help` calls `size`/`lines` body numbers (they are whole-file), `find --help` says "snapshot format 4", `terms --help` omits tag-only terms, `lint-rules -h`/`types -h` footers list `--count`.
- NEW-17: an operator-only query (`OR`, `-`) says "negating every word"; `--tag X --facet tags` prints one hint twice; `new --dry-run` still has no hint.
- NEW-18: teaching hints that mislead — the PATTERN-less `--section Tasks` hint widens 299 files to 1 007 sections; `"release tokenizer"~5` returns 0 for a 55-file AND and the follow-up hint is `terms rel`.

## What worked well

- Disk and `--index` are byte-identical on 36 command pairs (section mode, facets, slop,
  prefixes, negation, nested-key filters and sort, broken links, orphans, backlinks,
  properties, tags, terms, HYALO lint), including after an index-patching `mv` and `set`.
- The DEC-368 rule held in every touch scenario; the stale note names files, survives
  `-q` and never writes the snapshot.
- Hub `links fix --apply`: 53 files, +59/−59, every hunk one line, alias rewrites in
  Obsidian's own form.
- The schema-gate rule is uniform; `read --lines` is correct on every edge tried.
- The help-only pass: three self-chosen questions were each answered by the first
  command, from FIND 101 and the hints alone.

## Performance

Explorer timings ran at load average 22–45 (parallel agents); the orchestrator
re-measured the one outlier at load 12.

| Operation | 2026-10-04 | Now | Note |
| --- | ---: | ---: | --- |
| Own KB ranked disk / `--index` | 0.36 / 0.165 s | 0.353 / 0.082 s | |
| Own KB section query / `summary` / `lint --strict` | 0.38 / 0.079 / 0.25 s | 0.396 / 0.079 / 0.268 s | |
| MDN ranked disk / `--index` | 4.81 / 0.74 s | 5.19 / 0.71 s | disk under load |
| MDN `summary` disk / `--index` | 1.15 / 0.47 s | 1.28–1.39 / 0.44 s | disk re-measured at load 12 |
| MDN fresh `create-index` / `--force` / no-op | 3.12 / 4.07 / 1.85 s | 3.09 / 3.63 / 1.09 s | no-op writes nothing |
| MDN `terms` disk / `--index` | 2.17 / 0.73 s | 2.49 / 0.73 s | |
| Hub `summary` / `links fix` / strict HYALO lint | 0.38 / 0.52 / 1.04 s | 0.33 / 0.54 / 0.97 s | |
| Hub ranked disk / indexed | 1.49 / 0.30 s | 1.27–1.35 / 0.29 s | explorer saw 2.47 s at load > 34 |
| GitHub Docs `summary` / `links fix` dry run | 0.30 / 9.35 s | 0.42 / 4.87 s | |

Nothing exceeds 1.5× once load is accounted for.

## Outcome

[[iterations/iteration-318-regression-dogfood-fixes]] takes NEW-1 to NEW-8 and the
quick LOW items; the rest goes to `backlog/regression-dogfood-2026-10-05-leftovers.md`.
The release follows that iteration's merge and a short re-check of NEW-1 to NEW-7 on the
final binary.
