---
title: "Iteration 312: search diagnostics and read lines"
type: iteration
date: 2026-10-04
tags: [iteration, search, find, read, hints, dogfooding]
status: planned
branch: iter-312/search-diagnostics-and-read-lines
priority: 1
related:
  - "[[dogfood-results/dogfood-v0250-pre-search-roadmap-2026-10-04]]"
---

# Search diagnostics and read lines

## Problem

[[dogfood-results/dogfood-v0250-pre-search-roadmap-2026-10-04]] found that the did-you-mean hints are wrong more often than right (UX-1), that
`read --lines` is body-relative while every printed line number is file-relative
(BUG-4), that a leading-dash PATTERN is swallowed by a short flag with no runtime
hint (BUG-5), that the `prefix*` stem fallback never fires when a typo stem shares
the prefix (BUG-10), that `--sort property:` ignores dot-paths (BUG-6) and the
zero-result diagnostic is wrong for them (BUG-16), that `--facet property:a:b` is
accepted (BUG-18), and that malformed query input is silently accepted (UX-6). DEC
numbers reserved for this iteration: **DEC-355 to DEC-358**.

## Tasks

- [ ] BUG-4: `read --lines A:B` uses the file-relative numbering that `find`, section hits, lint and `--fields sections` print; the section-hit hint stops translating; an out-of-range window is reported, not silently empty; `read` JSON `lines` and `read --help` agree on what is counted (DEC)
- [ ] BUG-5: when `--section`/`--tag` received a value by short-flag concatenation and no PATTERN was given, the validation error and the multi-heading warning say "to search for a term starting with '-', write `hyalo find -- '-term'`" (DEC on why not `allow_hyphen_values`)
- [ ] BUG-10: `prefix*` searches the union of the typed prefix's expansion and its stem's expansion, so `configuration*` finds what `config*` finds
- [ ] UX-1: did-you-mean candidates are ordered by Damerau-Levenshtein distance then docs descending; the corrected-query hint corrects every misspelled term; the "try OR" hint is offered only when at least one term has documents; a misspelled word inside a phrase gets suggestions
- [ ] UX-6: dangling `a OR` / `OR a`, an unterminated quote, `*foo` and `sn*p` warn (`-q`-proof) like bare `OR` does; `~N` above 64 warns that it was clamped; `"a b"~abc` exits 1 with `invalid search query`
- [ ] BUG-18: `--facet property:a:b` exits 1 with the four accepted forms, like `property:`
- [ ] BUG-6: `--sort property:a.b` resolves dot-paths exactly as `--property` and `--facet property:` do; the "no files have property" warning uses the same resolver
- [ ] BUG-16: the zero-result `--property` diagnostic resolves dot-paths and lists the existing values with counts for `=`, `!=`, `~=` and the comparison operators alike (a comparison on string values says the values are strings)
- [ ] Text polish: `title:(a OR b)` says a field term cannot take a group; a pure-negative query says it needs a positive term; section-mode snippet lines print in line order; the corrected-query hint keeps `--granularity section` and every other original flag
- [ ] Docs in sync: `find --help`, `read --help`, `terms --help`, `.claude/CLAUDE.md` claims paragraph, `skill-hyalo.md`, `rule-knowledgebase.md`, CHANGELOG `[Unreleased]`, decision-log entries

## Acceptance criteria

- [ ] `hyalo find snapshto` on the own KB hints `snapshot`; `hyalo find "ostrch kangroo"` hints `'ostrich kangaroo'` and the hinted command returns results
- [ ] `hyalo read <file> --lines A:B` with the range a section hit printed returns that section's lines
- [ ] `hyalo find '-snapshot'` either exits 1 naming `--` or warns with the `--` form; it never silently returns the `--section` result set
- [ ] GitHub Docs: `--sort property:versions.ghes` orders and `--reverse` reverses; `--property 'versions.ghes=nothing'` lists existing values
- [ ] Disk and `--index` byte-identical on every query touched; exit codes 0/1/2 (DEC-307)
- [ ] fmt, clippy `-D warnings`, `cargo test --workspace -q`, `cargo deny check`, `check-jq-recipes`, help-drift and `hyalo lint --strict` green; CI green on three platforms
