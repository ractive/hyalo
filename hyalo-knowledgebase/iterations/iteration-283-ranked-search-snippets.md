---
type: iteration
title: "Iteration 283 — Snippets for BM25 ranked search results"
date: 2026-09-06
status: planned
tags: [iteration, search, find]
branch: iter-283/ranked-search-snippets
priority: 2
related:
  - "[[research/npm-package-and-typed-typescript-api]]"
  - "[[decision-log]]"
---

# Iteration 283 — Snippets for BM25 ranked search results

## Goal

A ranked `hyalo find PATTERN` answers "which files" but not "where in them". `FileObject` has
both `score` and `matches` (`crates/hyalo-core/src/types.rs:556-558`, `ContentMatch { line,
section, text }`), but `find/mod.rs:899` fills `matches` only on the regex path
(`has_regex_search`); BM25 mode leaves it `None`. A consumer that wants to show a hit — the pi
tools, an MCP server, the planned TypeScript API in
[[research/npm-package-and-typed-typescript-api]] — must run a second `find -e` or `read` per
result to get a line to display, which turns one call into N+1.

This iteration fills `matches` in ranked mode with the lines that carry the query terms, using
the **same field and the same shape** regex mode already emits. No new CLI flag: the field
exists, `--section` scoping already applies to it, and text mode already knows how to print it.

## Tasks

- [ ] TASK-1: read the BM25 path end to end (`bm25_tokenize`, stemming, `OR`, quoted phrases,
      the CJK bigram tokenizer, `score_map`) and write down what "a line that matches" means for
      each query form: a line contains at least one stemmed query token; for a quoted phrase,
      the consecutive stemmed sequence; for `OR`, any side. Put the definition in the module doc
      before writing code.
- [ ] TASK-2: implement snippet extraction for the top results: for each ranked file, select
      the body lines that match per TASK-1, rank them by the number of *distinct* query tokens
      they carry (ties in document order), keep at most 3 per file, and emit them as
      `ContentMatch { line, section, text }` exactly as regex mode does. Honour `--section`
      scoping the way `find/mod.rs:920-923` does for regex. Lines inside frontmatter never
      qualify.
- [ ] TASK-3: `--index` path: confirm whether the snapshot carries enough to produce snippets
      without touching disk. If not, read only the files that made the result set (after
      `--limit`, if any), never the whole vault — the point of the index is that the scan stays
      cheap. Measure MDN `find <term> --index` before and after; the delta must stay well below
      the disk-scan time (DEC-322/323 numbers are the baseline).
- [ ] TASK-4: text mode prints ranked matches the same way it prints regex matches
      (`line N [section]: text`). JSON is unchanged in shape — `matches` simply stops being
      `null` in ranked mode. Update `find --help` (the "per-line 'matches' instead of 'score'"
      sentence is now wrong), `skill-hyalo.md`, `rule-knowledgebase.md`, the command reference,
      and the pi extension's `hyalo_find` description if it mentions the gap.
- [ ] TASK-5: e2e tests for a single term, a stemmed term (`running` finds a line with `run`),
      an `OR` query, a quoted phrase, `--section` scoping, the 3-per-file cap, a CJK query, and
      `--index` parity (same `matches` as the disk scan). One test asserts a match's `line`
      points at the line that actually contains the token.
- [ ] TASK-6: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, every xtask `check-*` gate, `hyalo lint --strict` on the
      knowledgebase. DEC entry for the snippet rule (what qualifies, the cap, the ranking).

## Acceptance criteria

- [ ] `hyalo find rust --format json` returns, for every result, a non-null `matches` array
      whose entries have the same `{line, section, text}` shape regex mode emits, each `text`
      containing at least one stemmed query token.
- [ ] `matches` is capped at 3 per file, ordered by distinct-token count then line number, and
      respects `--section`.
- [ ] `--index` and disk-scan output are byte-identical for the same query.
- [ ] No new CLI flag. `find --help` and the bundled skill/rule text describe the new behaviour.
- [ ] MDN `find <term> --index` wall time stays within 10% of `main`.
- [ ] Gates green.

## Links

- [[research/npm-package-and-typed-typescript-api]] — "Snippets in ranked mode" open question;
  this iteration is the "ranked mode grows a snippet field" option it prefers
- [[decision-log]] — DEC-322/323 (index-backed scan baseline)
