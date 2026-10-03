---
title: "Iteration 302: search query language"
type: iteration
date: 2026-10-03
tags: [iteration, search, bm25]
status: completed
branch: iter-302/search-query-language
---

# Search query language

## Problem

`find PATTERN` is a BM25 ranked search whose query language stopped at flat
clauses. A single `OR` anywhere made every positive term an alternative
(`rust async OR tokio` meant any of the three). There was no grouping, no prefix
matching and no way to scope a term to a title, heading, tag or path. The query
was stemmed in one language while documents use their own `language`. A
zero-result query gave no lead. See [[decision-log#DEC-333: Ranked search grammar: OR binds tighter than AND, groups, prefixes, field terms (2026-10-03)]].

## Tasks

- [x] Replace the flat clause list with an AST (And/Or/Not/Term/Prefix/Phrase/Field) evaluated over postings
- [x] `OR` binds tighter than implicit AND; parentheses group and nest; `-` negates terms, phrases and groups
- [x] Unbalanced parenthesis, empty group and bare `*` exit 1 with the JSON error envelope
- [x] Prefix terms expand against the stemmed dictionary, capped at 256 with a `-q`-proof warning
- [x] Field terms `title:`, `heading:`, `tag:`, `path:` as per-document predicates from index metadata
- [x] Compile the query once per corpus language; term leaves are the OR of their per-language stems; snippets share it
- [x] Did-you-mean on zero results: notice text, `->` corrected-query hint, top-level `suggestions` key
- [x] New `hyalo terms [PREFIX]` subcommand with typed output, help EXAMPLES, command reference entry
- [x] DEC-333 and CHANGELOG `[Unreleased]` entries (behaviour change called out)
- [x] Docs sync: `find --help` QUERY SYNTAX, skill templates, pi/codex packages, `.claude/CLAUDE.md`

## Acceptance criteria

- [x] `a b OR c` returns documents with a AND (b OR c); unit and e2e tests prove precedence, nesting and negated groups
- [x] `(a`, `a)`, `()` and `*` exit 1 with an `invalid search query` envelope
- [x] `config*` finds "configuration"; a prefix with more than 256 terms warns under `-q`
- [x] A field-only query returns files sorted by path with score 0 and no snippets
- [x] A `language: de` note is found by `Häusern` with an English vault default, on disk and with `--index`
- [x] A misspelled query reports candidates in text, hints and JSON `suggestions`
- [x] `hyalo terms` honours PREFIX, `--limit`, `--count`, `--jq`, `--index`
- [x] Snapshot format, `SNAPSHOT_FORMAT_VERSION` and `TOKENIZER_VERSION` unchanged
- [x] fmt, clippy, tests, xtask quality gates and `hyalo lint --strict` pass

## Validation

Gates on the merged tree (after `git merge origin/main`, 2026-10-03): `cargo fmt`,
`cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo test
--workspace -q` 5280 passed / 0 failed, and every xtask gate of the CI
quality-gates job exits 0 (check-jev-assets, check-codex-package,
check-pi-package-sync, check-feature-fanout, check-help-drift,
check-command-reference, check-bundled-skills, check-ts-types, check-pi-runtime,
check-jq-recipes, check-mutation-journal, check-typed-output). `hyalo lint
--strict` reports no finding in files this iteration added; the remaining
findings (three `#DEC-` anchors in older decision-log entries and older
iteration files) predate it.

Dogfood against this knowledgebase:

```text
$ hyalo find "(bm25 OR stemming) -tantivy" --jq ".total, (.results[:3][] | .file)"
138
iterations/done/iteration-101-bm25-ranked-search.md
research/benchmark-iter101-bm25.md
iterations/done/iteration-101b-bm25-serializable-index.md

$ hyalo find "title:iteration tag:iteration link*" --jq ".total, (.results[:3][] | \"\(.file) \(.score)\")"
178
iterations/done/iteration-150-link-handling-refactor.md 36.49361266039518
iterations/done/iteration-151-link-mv-followups.md 22.087774365531917
iterations/iteration-224-test-quality-hardening.md 15.459096756103463

$ hyalo find zzzz --format text --fields title
"dogfood-results/dogfood-v0220-obsidian-vaults.md"
  title: Dogfood v0.22.0 — Obsidian vaults: frontmatter wikilinks, obsidian:// URIs, attachments, sort asymmetry
  matches:
    line 361 (### MDN — disk, no `--index` (single run unless noted)): | `title~=/zzzz/` probe | — | 0.58 s | — |

$ hyalo find stemmng --format text
No results for 'stemmng'
'stemmng' occurs in no document; did you mean: stem (80 docs), stemmer (16 docs), stemindex (3 docs)?
  -> hyalo find stem --format text  # Did you mean 'stem' instead of 'stemmng'?
  -> hyalo terms ste --format text  # List indexed terms starting with 'ste'

$ hyalo find stemmng --jq .suggestions
[{"candidates":[{"docs":80,"term":"stem"},{"docs":16,"term":"stemmer"},{"docs":3,"term":"stemindex"}],"term":"stemmng"}]

$ hyalo terms link --format text --limit 8
link              322 docs
linkgraph         23 docs
linker            15 docs
linkinfo          11 docs
linkmatch         10 docs
linkcasemismatch  7 docs
linkresolv        6 docs
linkgraphvisitor  5 docs
showing 8 of 24 matches

  -> hyalo find link --format text  # Search for 'link'

$ hyalo find "a (b"
error: invalid search query: unbalanced parenthesis: a '(' is never closed
  hint: quote text to search it literally (e.g. '"a)"'); see QUERY SYNTAX in `hyalo find --help`
exit 1

$ hyalo find "rust async OR tokio" --count  (old meaning: any of the three)
5
117
```

The last two counts show the precedence change: `rust async OR tokio` now
returns 5 files (rust AND (async OR tokio)); the old meaning, any of the three
terms, is what `rust OR async OR tokio` returns (117 files).

### Dogfood notes

- `hyalo changelog add --wrap` demands a column count with no default, and a
  new `### Changed` subsection lands after `### Fixed` instead of in Keep a
  Changelog order.
- Every piped `find --format json` carries `files_missing`/`files_skipped_*`
  counters even without `--files-from` (stdin is not a terminal); this predates
  the iteration.
- The `terms` listing hint searches the most frequent term, which on an
  unfiltered listing is a stop word (`the`).
- Running xtask gates through `./target/debug/xtask` needs an absolute
  `CARGO_MANIFEST_DIR` and `CARGO` set, or child `cargo` calls fail with
  "No such file or directory".

## Follow-ups

- No typed `terms()` wrapper exists in the npm/pi API yet. This matches iteration 287's scope, where `tags` and `backlinks` are also raw-only.
- Per-language stems of one word each add a score unit (DEC-333 "Known approximation"); revisit if mixed-language ranking proves skewed.
