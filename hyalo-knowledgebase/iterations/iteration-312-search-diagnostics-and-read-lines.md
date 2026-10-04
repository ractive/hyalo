---
title: "Iteration 312: search diagnostics and read lines"
type: iteration
date: 2026-10-04
tags: [iteration, search, find, read, hints, dogfooding]
status: completed
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

- [x] BUG-4: `read --lines A:B` uses the file-relative numbering that `find`, section hits, lint and `--fields sections` print; the section-hit hint stops translating; an out-of-range window is reported, not silently empty; `read` JSON `lines` and `read --help` agree on what is counted (DEC)
- [x] BUG-5: when `--section`/`--tag` received a value by short-flag concatenation and no PATTERN was given, the validation error and the multi-heading warning say "to search for a term starting with '-', write `hyalo find -- '-term'`" (DEC on why not `allow_hyphen_values`)
- [x] BUG-10: `prefix*` searches the union of the typed prefix's expansion and its stem's expansion, so `configuration*` finds what `config*` finds
- [x] UX-1: did-you-mean candidates are ordered by Damerau-Levenshtein distance then docs descending; the corrected-query hint corrects every misspelled term; the "try OR" hint is offered only when at least one term has documents; a misspelled word inside a phrase gets suggestions
- [x] UX-6: dangling `a OR` / `OR a`, an unterminated quote, `*foo` and `sn*p` warn (`-q`-proof) like bare `OR` does; `~N` above 64 warns that it was clamped; `"a b"~abc` exits 1 with `invalid search query`
- [x] BUG-18: `--facet property:a:b` exits 1 with the four accepted forms, like `property:`
- [x] BUG-6: `--sort property:a.b` resolves dot-paths exactly as `--property` and `--facet property:` do; the "no files have property" warning uses the same resolver
- [x] BUG-16: the zero-result `--property` diagnostic resolves dot-paths and lists the existing values with counts for `=`, `!=`, `~=` and the comparison operators alike (a comparison on string values says the values are strings)
- [x] Text polish: `title:(a OR b)` says a field term cannot take a group; a pure-negative query says it needs a positive term; section-mode snippet lines print in line order; the corrected-query hint keeps `--granularity section` and every other original flag
- [x] Docs in sync: `find --help`, `read --help`, `terms --help`, `.claude/CLAUDE.md` claims paragraph, `skill-hyalo.md`, `rule-knowledgebase.md`, CHANGELOG `[Unreleased]`, decision-log entries

## Acceptance criteria

- [x] `hyalo find snapshto` on the own KB hints `snapshot`; `hyalo find "ostrch kangroo"` hints `'ostrich kangaroo'` and the hinted command returns results
- [x] `hyalo read <file> --lines A:B` with the range a section hit printed returns that section's lines
- [x] `hyalo find '-snapshot'` either exits 1 naming `--` or warns with the `--` form; it never silently returns the `--section` result set
- [x] GitHub Docs: `--sort property:versions.ghes` orders and `--reverse` reverses; `--property 'versions.ghes=nothing'` lists existing values
- [x] Disk and `--index` byte-identical on every query touched; exit codes 0/1/2 (DEC-307)
- [ ] fmt, clippy `-D warnings`, `cargo test --workspace -q`, `cargo deny check`, `check-jq-recipes`, help-drift and `hyalo lint --strict` green; CI green on three platforms

## Outcome

All nine bug/UX items and the text-polish bullet are implemented and tested
(unit tests in `hyalo-core`/`hyalo-cli`, e2e tests under
`crates/hyalo-cli/tests/e2e/`). DEC-355 through DEC-358 recorded in
`decision-log.md`; `.claude/CLAUDE.md`'s claims paragraph and
`CHANGELOG.md`'s `[Unreleased]` → Fixed section updated to match.

**BUG-4** (`crates/hyalo-cli/src/commands/read.rs`): `--lines` is now
file-absolute via `translate_file_range`; an out-of-range window warns
(`warn_always`) instead of silently returning empty content. The
section-hit hint (`crates/hyalo-cli/src/commands/find/mod.rs`) stops
subtracting the frontmatter offset before building its `--lines A:B`
suggestion. Before: `read iterations/done/iteration-154-mv-index-patch.md
--lines 125:135` (a hint copied verbatim from `find --granularity section`)
returned the wrong 11 lines. After: the exact section text, and
`--lines 999:1000` now warns `requested lines 999:1000 have no overlap with
<file> (135 lines in the file)`.

**BUG-5** (`find/run.rs`, `find/mod.rs`): the `--tag` validation error and
the `--section`-matched-many-headings warning now append `to search for a
term starting with '-', write \`hyalo find -- '-term'\`` whenever no
PATTERN was present. Before: `hyalo find '-snapshot'` silently returned the
`--section` match set with only a generic warning. After: the warning
names the fix inline.

**BUG-10** (`hyalo-core/src/bm25/query.rs::effective_prefixes`): unions the
raw prefix's matches with the stem fallback's, instead of only trying the
stem when the raw prefix matched nothing at all. Before: `configuration*`
found 1 file (absorbed by a one-document typo stem sharing the prefix).
After: 120 files, matching `config*`'s 228 (own-KB numbers; the dogfood
report's MDN numbers were 1 → 224).

**UX-1** (`hyalo-core/src/bm25/query.rs::suggest`/`corrected_query`):
candidates rank by Damerau-Levenshtein distance then docs descending
(was normalised-Levenshtein/Jaro-Winkler with docs only a last-resort
tie-break); `corrected_query` splices every suggested term's fix in one
pass; a misspelled word inside a quoted phrase gets a suggestion via new
`phrase_raw_words` + span-tracking through `Lexeme`/`RawNode::Phrase`.
Before/after on a scratch two-file vault: `find snapshto` hinted
`-> hyalo find -- snapshot # Did you mean 'snapshot' instead of
'snapshto'?`; `find "ostrch kangroo"` hinted
`-> hyalo find -- 'ostrich kangaroo' # Did you mean 'ostrich' instead of
'ostrch'?`, and running the hinted command returns the matching file.

**BUG-6/BUG-16** (`find/sort.rs`, `find/mod.rs`, `find/run.rs`,
`hyalo-core/src/filter/match_props.rs::resolve_prop_in_object`): `--sort
property:K`, its two "no files have property"/"mixed types" warnings, and
the zero-result `--property` diagnostic (now covering `!=`/`~=`/comparison
operators, not just `=`) all resolve dot-paths through the shared
`resolve_prop`/`resolve_prop_in_object` helpers. Verified on GitHub Docs
(`/Users/james/devel/docs/content`, 3710 files): `--sort
property:versions.ghes` orders and `--reverse` reverses with no spurious
warning; `--property 'versions.ghes=nothing'` lists the 2222-file key's
real values with counts. Disk and `--index` byte-identical on both.

**BUG-18** (`find/facets.rs::FacetSpec::parse`): a second `:` in
`property:status:extra` is rejected with the same four-forms message an
unrecognized spec gets, instead of being accepted as a literal key.

**UX-6** (`hyalo-core/src/bm25/query.rs`: `QueryWarnings`, `lex`,
`Parser`, `Compiler::word`): a dangling `OR`, an unterminated quote, a
misplaced `*` (`*foo`, `sn*p`) and a slop clamped above 64 all warn
(`-q`-proof, via the new `CompiledQuery::warnings()`); `"a b"~abc` is
rejected pre-lex (`has_malformed_phrase_slop`) as `invalid search query`
rather than silently treated as `~0` plus a new word `abc`.

**Text polish**: `title:(a OR b)` now says "a field term cannot take a
group" (detected pre-parse in `fn parse`); a pure-negative query
(`-- '-snapshot'` alone) reports "a query needs at least one positive
term" instead of hinting `properties summary`; section-mode text output
(`output/text.rs::format_section_hit_text`) sorts snippet lines by line
number; the corrected-query hint re-appends `--granularity section` when
the original query ran in section mode.

All local gates ran clean: `cargo fmt --check`, `cargo clippy --workspace
--all-targets -- -D warnings`, `cargo test --workspace -q` (2433 e2e tests),
`cargo deny check` (advisories/bans/licenses/sources ok), `check-jq-recipes`
(47 recipes, 0 errors), `check-help-drift` (hung on the documented
nested-cargo-lock deadlock on the first attempt but completed clean, exit 0,
on retry — the workaround needs a few minutes, not a real failure), and
`hyalo lint --strict` on the whole vault. The last acceptance criterion's
"CI green on three platforms" half is left unticked for the orchestrator to
confirm on the PR.
