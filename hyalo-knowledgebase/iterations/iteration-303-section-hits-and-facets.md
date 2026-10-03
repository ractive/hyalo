---
title: "Iteration 303: section hits and facets"
type: iteration
date: 2026-10-03
tags: [iteration, search, bm25, facets]
status: completed
branch: iter-303/section-hits-and-facets
---

# Section hits and facets

## Problem

`find PATTERN` ranks whole files. An agent looking for "the paragraph about
the snapshot index" gets a 400 KB decision log as its best hit and has to dig
the section out by hand. There is also no way to see how a match set is
distributed (by tag, status, type or directory) without paging through every
result with `--jq`. This iteration adds section-granular ranked hits and facet
counts to `find`, building on the query AST of
[[iterations/iteration-302-search-query-language]].

## Tasks

- [x] `--granularity file|section` on `find`; default `file` leaves output unchanged
- [x] Section mode requires a ranked PATTERN; otherwise exit 1 with the JSON error envelope
- [x] Section scoring: re-read each file-level match once, split by outline sections (preamble has heading null), tokenize with the file language, BM25 with corpus IDF and section-length normalisation
- [x] A section is a hit only when it satisfies the positive clauses on its own; negation stays file-level
- [x] Section results carry `file`, `section {heading, level, line_start, line_end, path}`, `score`, scoped `matches`
- [x] Sort by score, file, line_start; `--limit` counts sections; `--section` restricts eligible sections; `--sort` other than score is a user error
- [x] Section hits offer `-> hyalo read <file> --section '<heading>'` (or `--lines A:B` for the preamble)
- [x] Text mode prints `file#heading (lines A-B)  score`
- [x] Identical section output with and without `--index`
- [x] `--facet tags|property:K|type|dir` (repeatable) computed over the full match set before `--limit`
- [x] Top-level `facets` envelope key, buckets sorted by count desc then value, capped at 50 with `truncated: true`
- [x] Text mode facet block; drill-down hints for the top 3 buckets of each facet
- [x] Unknown facet spec exits 1; `--facet` combines with `--jq` and `--count`
- [x] Typed envelope keeps `suggestions` and `facets`; TS types and pi `hyalo-api.d.ts` regenerated
- [x] DEC-334 (section-hit semantics) and DEC-335 (facet contract)
- [x] Docs sync: `find --help`, skill templates, pi/codex packages, `.claude/CLAUDE.md`, CHANGELOG
- [x] Unit tests for section scoring and facet counting; e2e tests `search_section_hits.rs` and `find_facets.rs`

### Review round (PR #363)

- [x] `find -h` fits its 3072-byte ceiling under Windows CRLF (2949 bytes, 47 lines)
- [x] Section mode qualifies files on their whole body; `--section` only picks eligible sections
- [x] Negation pushed down with De Morgan; negated leaves and field terms are per-file constants (`-(-a)`, `kiwi OR title:x`)
- [x] A section's `line_end` counts oversized and non-UTF-8 lines
- [x] Empty `--files-from` still validates `--facet`/section arguments and reports empty facets
- [x] Property buckets resolve dot-paths and fold case like `--property K=V`; structured values get no drill-down; repeated specs counted once
- [x] Drill-down skip compares with the file count (also in section mode)
- [x] Zero section hits with file-level matches: explanatory notice, `--granularity file` hint, OR hint keeps `--granularity section`, no `terms` hint
- [x] `--filenames-only`/`--filenames0` list each file once

## Acceptance criteria

- [x] `find PATTERN` without `--granularity` produces byte-identical output to iteration 302
- [x] A section that lacks a Must term is not a hit even when the file matches
- [x] A negated term present in another section of the file excludes the whole file
- [x] `--limit 2` returns two sections; `--sort title` in section mode exits 1
- [x] Section output with `--index` equals output without it
- [x] Facet buckets count the full match set, honour the 50 cap and report missing properties as `null`
- [x] An unknown facet spec exits 1 with the envelope
- [x] Snapshot format and `TOKENIZER_VERSION` unchanged
- [x] fmt, clippy, tests, xtask quality gates and `hyalo lint --strict` pass

## Validation

Gates on the branch (2026-10-03): `cargo fmt`, `cargo clippy --workspace
--all-targets -- -D warnings` clean, `cargo test --workspace -q` with 0
failures (34 new e2e tests in `search_section_hits.rs` and `find_facets.rs`,
14 core unit tests for the section scorer, 3 for facet counting), and every
xtask gate of the CI quality-gates job exits 0 (check-jev-assets,
check-codex-package, check-pi-package-sync, check-feature-fanout,
check-help-drift, check-command-reference, check-bundled-skills,
check-ts-types, check-pi-runtime, check-jq-recipes, check-mutation-journal,
check-typed-output). `hyalo find -h` is 2949 bytes over 47 lines, 123 bytes under its 3072 ceiling (Windows adds a CR per line).

Default output is unchanged: eight `find` invocations (ranked JSON and text,
negation, tag filter, `--broken-links`, a zero-result did-you-mean, `--section`,
`-e`) produce byte-identical output from this branch and from the iteration
302 merge commit `2f0c3934`. Section hits and facets are identical with and
without `--index-file` for three queries (`snapshot index`, `rust -tantivy`,
`(bm25 OR stemming) heading`, all with `--limit 0 --facet tags`).

Dogfood against this knowledgebase:

```text
$ hyalo find 'snapshot index' --granularity section --limit 5 --format text --no-hints
iterations/done/iteration-143-hint-and-files-from-polish.md#`--index --files-from` snapshot membership (iter-139 deferred) (lines 67-83)  3.26
  line 67: ### `--index --files-from` snapshot membership (iter-139 deferred)
  line 70: resolver `files_from::resolve_with_index` (keep existing
  line 71: `resolve` for the no-index path). The new resolver checks
iterations/done/iteration-125-review-fixes.md#4. Index not updated by `links fix`, `links auto --apply`, `lint --fix` (lines 53-68)  3.23
  line 55: **Bug:** Three mutating commands write to disk but don't patch the snapshot index. Subsequent `--index` queries see stale data.
  ...
iterations/done/iteration-154-mv-index-patch.md#Acceptance Criteria (lines 111-124)  3.21
iterations/iteration-157-lazy-stem-map.md#Part B — Seed from snapshot index when available (lines 124-149)  3.18
research/stale-index-check-matrix.md#What each path did (lines 22-30)  3.07
showing 5 of 341 matches

$ hyalo find 'snapshot index' --count                          # files
147
$ hyalo find 'snapshot index' --granularity section --count    # sections
341

$ hyalo find --tag iteration --facet property:status --facet dir --limit 0 --format text | tail -9
facet property:status:
  completed    255
  superseded   11
  in-progress  2

facet dir:
  iterations  268

  -> hyalo find --tag iteration --limit 0 --facet property:status --facet dir --property status=completed --format text  # Drill into status = completed (255 files)
  ...

$ hyalo find --tag iteration --facet property:status --facet dir --jq '.total, (.facets[] | "\(.facet): \([.buckets[] | "\(.value)=\(.count)"] | join(" "))")'
268
property:status: completed=255 superseded=11 in-progress=2
dir: iterations=268
```

The `dir` facet offers no drill-down here because its only bucket holds every
match. Each section hit's hint is a runnable read, e.g.
`hyalo read iterations/done/iteration-154-mv-index-patch.md --section 'Acceptance Criteria'`.

### Dogfood notes

- `--limit 0` means unlimited in hyalo, so `find --facet … --limit 0` prints
  every result before the facet block. There is no "facets only" projection;
  `--jq '.facets'` is the way to get just the counts.
- `--filenames-only` in section mode prints one line per section hit, so a
  file with several hits repeats. It is consistent with "one result per
  section" but rarely what a pipeline wants.
- Sub-agents again ended without delivering a report; their work had to be
  inspected on disk (`git status`, running their tests).

## Follow-ups

- Consider deduplicating `--filenames-only` in section mode, or rejecting the
  combination.
- `read --section '<heading>'` returns the heading with its nested
  subsections, so a section hit's read hint can show more than the flat
  section the hit scored. `line_start`/`line_end` give the exact range.
