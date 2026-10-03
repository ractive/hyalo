---
title: "Iteration 302: search query language"
type: iteration
date: 2026-10-03
tags: [iteration, search, bm25]
status: in-progress
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

- [ ] Replace the flat clause list with an AST (And/Or/Not/Term/Prefix/Phrase/Field) evaluated over postings
- [ ] `OR` binds tighter than implicit AND; parentheses group and nest; `-` negates terms, phrases and groups
- [ ] Unbalanced parenthesis, empty group and bare `*` exit 1 with the JSON error envelope
- [ ] Prefix terms expand against the stemmed dictionary, capped at 256 with a `-q`-proof warning
- [ ] Field terms `title:`, `heading:`, `tag:`, `path:` as per-document predicates from index metadata
- [ ] Compile the query once per corpus language; term leaves are the OR of their per-language stems; snippets share it
- [ ] Did-you-mean on zero results: notice text, `->` corrected-query hint, top-level `suggestions` key
- [ ] New `hyalo terms [PREFIX]` subcommand with typed output, help EXAMPLES, command reference entry
- [ ] DEC-333 and CHANGELOG `[Unreleased]` entries (behaviour change called out)
- [ ] Docs sync: `find --help` QUERY SYNTAX, skill templates, pi/codex packages, `.claude/CLAUDE.md`

## Acceptance criteria

- [ ] `a b OR c` returns documents with a AND (b OR c); unit and e2e tests prove precedence, nesting and negated groups
- [ ] `(a`, `a)`, `()` and `*` exit 1 with an `invalid search query` envelope
- [ ] `config*` finds "configuration"; a prefix with more than 256 terms warns under `-q`
- [ ] A field-only query returns files sorted by path with score 0 and no snippets
- [ ] A `language: de` note is found by `Häusern` with an English vault default, on disk and with `--index`
- [ ] A misspelled query reports candidates in text, hints and JSON `suggestions`
- [ ] `hyalo terms` honours PREFIX, `--limit`, `--count`, `--jq`, `--index`
- [ ] Snapshot format, `SNAPSHOT_FORMAT_VERSION` and `TOKENIZER_VERSION` unchanged
- [ ] fmt, clippy, tests, xtask quality gates and `hyalo lint --strict` pass

## Validation

Pending.
