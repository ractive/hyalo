---
type: iteration
title: "Iteration 264 — find: sort direction, null-aware filters, projection edge cases"
date: 2026-09-03
status: completed
tags:
  - iteration
  - find
  - sort
  - filters
  - dogfooding
branch: iter-264/find-sort-filter-consistency
priority: 4
related:
  - "[[dogfood-results/dogfood-v0220-obsidian-vaults]]"
  - "[[dogfood-results/dogfood-v0220-help-efficiency-and-find-shape]]"
---

# Iteration 264 — find: sort direction, null-aware filters, projection edge cases

## Goal

A sweep over `hyalo find` edge cases from
[[dogfood-results/dogfood-v0220-obsidian-vaults]] and two leftovers from
[[dogfood-results/dogfood-v0220-help-efficiency-and-find-shape]]. Sort keys
disagree on direction: `backlinks_count` and `links_count` descend while every
other key ascends, so `--reverse` means the opposite thing per key (BUG-4).
Filters cannot express "is null" or "is an empty list" (BUG-17), compare
mixed-type values lexicographically so `last>=2023-09-01` matches the string
`"[[2022-04]]"` and nulls sort first (BUG-18), accept an empty regex `~=//`
that matches everything (BUG-23), an empty `--fields ''` (BUG-24), and the
undocumented `=~` operator as a regex (COH-13). Output shape issues: the
`--fields properties-typed` JSON key is `properties_typed` (BUG-20),
`--filenames-only` ends in a blank line (BUG-21), and `--files-from -` returns
a different envelope than `--file` (BUG-22). `--sort score` is still not in
`-h` (COH-12).

Constraint: **no new CLI flags** from dogfood pressure (project rule). Null and
empty-list filters are value syntax on the existing `--property`; sort direction
is a semantic fix plus documentation; everything else is validation or output
shape. Out of scope: title fallback to the file stem (UX-5, in
[[iterations/iteration-267-help-hints-text-polish]]), the second-positional
trap (UX-3, same place), and link-related `find` fields
([[iterations/iteration-261-link-resolution-obsidian-compat]]).

## Tasks

### SORT-1: one direction for every key (BUG-4, COH-12)

- [x] DEC-273 (tentative): every `--sort` key orders ascending and `--reverse`
      inverts it, including `backlinks_count` and `links_count`; the only
      exception is `score`, which orders by descending relevance because
      "best first" is the sole useful default, and `--reverse score` is
      allowed and documented. Record why the two count keys were descending
      (probably copied from `score`) and that this is a behaviour change for
      scripts using them.
- [x] hyalo-cli `find` sort: implement, and make the text output show a
      non-empty `backlinks:` field for the top results when sorting by
      `backlinks_count` (the report saw an empty field).
- [x] `find -h` and `--help`: list `score` among the sort keys, state the
      direction rule in one sentence, and add `--sort backlinks_count
      --reverse` to the cookbook as "most linked first".
- [x] Unit test on the comparator per key; e2e in
      `crates/hyalo-cli/tests/e2e` asserting `--sort backlinks_count` returns
      0-backlink files first and `--reverse` the most-linked first, and that
      `--sort score` without `--reverse` returns the best match first.
- [x] Changelog entry (flag it as a behaviour change).

### FILTER-1: null and empty-list values (BUG-17, BUG-18)

- [x] DEC-274 (tentative): `--property K=null` matches a property present with
      a YAML null (`~`, `null`, empty value); `K!=null` matches present and
      non-null; `K=[]` matches an empty list; the existing bare `K` / `!K`
      keep meaning present / absent. Comparisons `< <= > >=` apply numeric
      order when both sides parse as numbers, date order when both parse as
      dates, and text order only when both are plain strings; a value of
      another type never matches a comparison. `--sort property:K` puts nulls
      and missing values last regardless of `--reverse`.
- [x] hyalo-core property filter: implement the value syntax and the typed
      comparison; hyalo-cli sort: nulls-last comparator.
- [x] Unit tests: `aliases=null` on `aliases:` (empty), on `aliases: ~`, on
      `aliases: [null]` (list containing null, must not match `=null`), `K=[]`
      on `K: []` vs `K:` (empty scalar), `last>=2023-09-01` against a
      `"[[2022-04]]"` string (no match), `rating>=6` against `rating: "7"`
      (numeric parse, match), and the nulls-last sort in both directions.
- [x] e2e with a fixture vault; also assert `--fields properties-typed` shows
      `type: "null"` for the matched files so the two views agree.
- [x] Docs: `find --help` operator table (`=null`, `!=null`, `=[]`), COMMON
      MISTAKES block, skill file, changelog.

### FILTER-2: reject nonsense input (BUG-23, BUG-24, COH-13)

- [x] hyalo-cli argument validation: `--property 'K~=//'` → `error: empty
      regex in property filter`; `--fields ''` (and `--fields ,`) → the same
      error the unknown-field path produces, listing valid values;
      `--property 'K=~/pat/'` → `error: unknown operator '=~'; use 'K~=/pat/'
      for a regex match` (DEC-276 (tentative): `=~` was silently accepted as a
      regex because `=` matched then `~/pat/` was parsed as a YAML-tilde
      value; rejecting it is a breaking change for anyone who relied on the
      accident, which the help already called wrong).
- [x] Also audit `--where-property` on `set`/`remove`/`append` and `--property`
      on `mv`, which share the parser, so the same errors apply.
- [x] Unit tests for the three rejections; e2e asserting exit code 2 and the
      message.
- [x] Docs: COMMON MISTAKES block rewritten to match (it currently says `=~`
      is wrong while it worked, see also BUG-25 in
      [[iterations/iteration-267-help-hints-text-polish]]).

### SHAPE-1: JSON key and stream shape fixes (BUG-20, BUG-21, BUG-22)

- [x] DEC-275 (tentative): the JSON key for the `--fields properties-typed`
      projection. Choose `properties_typed` (matches every other snake_case
      key in the envelope) and change the `--fields` value to accept both
      `properties-typed` and `properties_typed`, or rename the key to match
      the flag value. Recommend keeping the snake_case key and documenting the
      mapping in `find --help` next to the field list.
- [x] `--filenames-only`: emit exactly one `\n` after the last path; `wc -l`
      must equal `--count`. Check `views run --filenames-only` and every other
      command exposing the flag.
- [x] `--files-from -` and `--files-from <path>`: `results` is the same array
      as `--file`; the `files_missing`, `files_skipped_non_md`,
      `files_skipped_outside_vault` counters move to top-level envelope keys
      (present on every `find`, zero when unused) so `.results[0]` works for
      both. Apply the same shape to `lint --files-from` if it differs.
- [x] e2e: `echo decision-log.md | hyalo find --files-from - --format json`
      and `hyalo find --file decision-log.md --format json` produce identical
      `results`; `find --filenames-only | wc -l` equals `--count`; `--jq
      '.results[0].properties_typed'` is non-null under `--fields
      properties-typed`.
- [x] Docs: `find --help` result-shape section, skill file, changelog (shape
      change for `--files-from`).

## Acceptance criteria

- [x] Obsidian Hub, cwd `../obsidian-hub`: `hyalo find --sort backlinks_count
      --reverse --limit 1 --format json --jq '.results[0].backlinks | length'`
      is the maximum in the vault (report: 2190), and without `--reverse` the
      first result has 0 backlinks.
- [x] `../obsidian-hub`: `hyalo find --property aliases=null --count` → 2
      (matches `hyalo properties` reporting `aliases: 2 null`);
      `hyalo find --property 'aliases=~' --count` no longer matches 5623 files
      by accident (either 0 or the true count of the literal string `~`,
      documented).
- [x] kepano-obsidian, cwd `../kepano-obsidian`: `hyalo find --property
      'last>=2023-09-01' --format json --jq '.results[].properties.last'`
      contains no `[[…]]` string; `hyalo find --sort property:rating --reverse
      --format json --jq '[.results[].properties.rating]'` lists numbers first
      and nulls last.
- [x] `hyalo find --property 'title~=//'` → exit 2 with `empty regex`;
      `hyalo find --fields ''` → exit 2; `hyalo find --property
      'title=~/iter/'` → exit 2 naming `~=`.
- [x] `hyalo find --property status=completed --limit 0 --filenames-only |
      wc -l` equals `hyalo find --property status=completed --count`.
- [x] `diff <(echo decision-log.md | hyalo find --files-from - --format json
      --jq .results) <(hyalo find --file decision-log.md --format json --jq
      .results)` → empty.
- [x] `hyalo find -h | grep -c score` ≥ 1 and `find --help` states the sort
      direction rule and the null value syntax; the COMMON MISTAKES block no
      longer contradicts the parser.
- [x] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo test --workspace -q`, `hyalo lint --strict` on the KB,
      xtask help-drift check.
- [x] Changelog entry via `hyalo changelog add`; DEC-273 through DEC-276
      recorded in [[decision-log]]; `.claude/skills/hyalo/SKILL.md` and
      `.claude/CLAUDE.md` updated for the null syntax and the `--files-from`
      shape.

## Outcome

Decisions DEC-273 (uniform sort direction), DEC-274 (null / empty-list value
syntax and typed comparisons), DEC-275 (`properties_typed` stays snake_case,
both spellings accepted) and DEC-276 (`=~`, empty regex and empty `--fields`
rejected) are recorded in [[decision-log]] and were adopted as written, with
three deviations from the plan text:

- **Exit code 1, not 2, for the three rejections.** Exit 2 is this CLI's
  internal/system-error code (iter-181); every invalid-argument path — unknown
  field, unknown sort key, empty filter name — exits 1, and these belong to
  that class. Following the existing contract beat matching the number in the
  acceptance criteria. Recorded in DEC-276.
- **`--property 'aliases=~'` is an error, not a count.** The acceptance
  criterion left room for "0 or the true count of the literal string `~`";
  since `=~` is rejected wherever it appears in operator position, the whole
  expression is refused instead. On Obsidian Hub it went from silently matching
  5623 files to `unknown operator '=~' … use '~=' …`.
- **Ascending `--sort backlinks_count` starts at 1, not 0, on Obsidian Hub.**
  Every file in that vault has at least one backlink (`find --fields backlinks
  --limit 0 --jq '[.results[] | select((.backlinks|length)==0)] | length'` →
  0), so "the first result has 0 backlinks" was not reachable there. The
  `--reverse` half of the criterion holds exactly: 2190 backlinks, the vault
  maximum.

No separate text-renderer change was needed for the empty `backlinks:` field:
text mode omits an empty list, and under the corrected direction the top result
of `--sort backlinks_count --reverse` is the most-linked file, so the field is
populated.

Verified on the dogfood vaults: `../obsidian-hub` — max backlinks 2190 under
`--reverse`, `--property aliases=null --count` → 2 (matching `hyalo
properties`); `../kepano-obsidian` — `last>=2023-09-01` returns no `[[…]]`
string, `--sort property:rating --reverse` lists numbers first and nulls last.

## Links

- [[dogfood-results/dogfood-v0220-obsidian-vaults]]
- [[dogfood-results/dogfood-v0220-help-efficiency-and-find-shape]]
- [[iterations/iteration-267-help-hints-text-polish]]
- [[decision-log]]
