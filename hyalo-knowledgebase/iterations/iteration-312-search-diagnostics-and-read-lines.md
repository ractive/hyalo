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

## Review follow-up (2 MUST-FIX, 5 SHOULD-FIX)

All seven findings from the independent review fixed on the same branch,
with new tests for each:

1. **MUST-FIX — malformed-slop false positive.** The pre-lex scan for `"` +
   `~` + a letter matched any `"` in the query, including an *opening* one,
   so `hyalo find '"~home dir"'` (an ordinary phrase whose content starts
   with `~`) wrongly exited 1. Moved the check into `lex()` itself, right
   after `read_phrase` confirms it found a *closing* quote
   (`peek_malformed_slop`, a 2-char lookahead on the still-unconsumed
   stream) — `lex()` is now fallible (`Result<(Vec<Lexeme>, QueryWarnings),
   QuerySyntaxError>`), `classify_word` too. New unit test
   `phrase_content_starting_with_tilde_letter_is_not_malformed_slop`.
2. **MUST-FIX — `--section` + `--lines` stayed section-local.** `read a.md
   --section Sub --lines 8:9` (file lines 8-9 ARE that section) printed
   nothing with no warning, while the help text and claims paragraph
   promised file-absolute everywhere. `extract_sections` now returns each
   matched section's body-relative start line; `--lines` intersects the
   requested file-absolute range with every matched section's own
   file-absolute span (`section_range_overlap`) and joins the overlapping
   parts, warning only when no section overlaps at all. New unit tests in
   `read.rs` plus e2e coverage via the dash-hint/frontmatter tests below.
3. **SHOULD-FIX — dash hint over/under-fired.** Was keyed on "no PATTERN",
   which also caught a deliberate `--section Task` with no PATTERN, and
   missed the common single-heading-match case entirely (`-sqlite` ->
   `--section qlite`, one clean match, no warning at all). Now keyed on the
   real argv shape: `argv_has_concatenated_short_flag` (pure, unit-tested)
   detects a `-s…`/`-t…` token with a value concatenated onto it, computed
   once from `std::env::args_os()` and threaded through `FindExtras`. Fires
   on both the ambiguous-heading warning and a new standalone
   single-match warning; the `--tag` validation-error hint uses the same
   signal. Four new e2e tests in `search_query_language.rs` (ambiguous
   `-s`, single-match `-s`, `-t`, and the legit long-form negative case).
4. **SHOULD-FIX — `corrected_query` missed repeated terms.** `suggest()`
   dedups by raw term, so `ostrch ostrch kangroo` had one `TermSuggestion`
   for `ostrch`, and `corrected_query`'s `.find()` (singular) fixed only the
   first occurrence. Now replaces every `QueryWord` whose raw matches a
   suggestion's term; the hint description also names every corrected word,
   not just the first. New unit test `corrected_query_fixes_a_repeated_misspelled_word`
   plus an e2e test for the hint description.
5. **SHOULD-FIX — "Try OR" still offered when no word had any postings.**
   `ctx.search_suggestions` undercounts: `suggest()` drops a word silently
   when it has *no* close dictionary candidate either, so `qqqzzz` was
   absent from it and `any_word_has_docs` read that absence as "has
   matches". Added `Bm25InvertedIndex::words_without_postings` (independent
   of `suggest()`'s candidate search) threaded through a new
   `SearchReport`/`HintContext` field `zero_posting_terms`; `any_word_has_docs`
   now checks that instead. New unit test
   `words_without_postings_reports_a_hopeless_word_suggest_drops` plus an
   e2e test for both directions (withheld / still offered).
6. **SHOULD-FIX — `--frontmatter --lines` regressed, and the large-file hint
   broke on any frontmatter.** Decided the sensible behaviour the reviewer
   asked for: a frontmatter line *is* a file line, so `--lines 1:2` now
   returns those two lines (the raw fence/key bytes, via a new
   `read_body_lines` return value `frontmatter_raw_lines`) rather than
   warning "no overlap" — that warning is now reserved for a request past
   the end of the file. `translate_file_range` (the old body-relative
   translator) is gone, replaced by a direct slice over
   `frontmatter_raw_lines ++ body_lines`. Separately, `hints/mutation.rs`'s
   "read the first 80 lines" hint now offsets by the frontmatter's own line
   count (derived from `lines - content.lines().count()`, no extra read) so
   it still previews body text instead of YAML. New unit tests in
   `read.rs` (`lines_wholly_inside_frontmatter_returns_those_lines`,
   `lines_spanning_frontmatter_and_body_returns_both_parts`,
   `lines_past_end_of_file_still_warns`,
   `frontmatter_flag_and_lines_combine_the_frontmatter_block_with_a_content_slice`)
   and two e2e tests for the large-file hint (with and without frontmatter).
7. **SHOULD-FIX — CHANGELOG placement, duplicate warning, undocumented
   recall widening.** Moved the `read --lines` renumbering from `Fixed` to
   `Changed` as a `**Breaking:**` bullet (it changes what a previously
   well-formed command returns). Collapsed `find OR`'s two overlapping
   warnings (the generic dangling-operator one and the more specific
   "interpreted as a boolean operator" one) to one: `find/mod.rs` skips the
   dangling-operator warning when `query_is_operator_only(pattern)`. Added a
   short addendum under DEC-358 (no new DEC number) documenting that BUG-10's
   `prefix*` union is an intentional widening of recall. New e2e test
   `operator_only_query_warns_exactly_once`.

Gates rerun clean after all seven fixes: `cargo fmt`, `cargo clippy
--workspace --all-targets -- -D warnings`, `cargo test --workspace -q`
(2443 e2e tests), `check-jq-recipes`, `check-help-drift`, and
`hyalo lint --strict` (exit 0; the only violation is the same
expected-and-unticked CI checkbox note on this file itself).
