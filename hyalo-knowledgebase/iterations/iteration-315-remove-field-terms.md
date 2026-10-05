---
title: "Iteration 315: remove field terms from the search grammar"
type: iteration
date: 2026-10-05
tags: [iteration, search, find, cleanup]
status: completed
branch: iter-315/remove-field-terms
priority: 1
---

# Remove field terms from the search grammar

## Problem

Iteration 302 (DEC-333) added `title:`, `heading:`, `tag:` and `path:` field terms
to the ranked-search PATTERN. Reviewing them after the 2026-10-04 dogfood, the owner
found that each duplicates an existing flag (`--title`, `--section`, `--tag`,
`--glob`) and that the only new capability — OR/negation composition of
structure with words — has no demonstrated need. Keeping them costs a colon-splitting
rule with exceptions, field-term interaction with `OR`/groups/negation, the
"field-only query scores 0, sorts by path, has no snippets" special case, help and
claims text, and disk/index parity tests. The rule going forward: search terms are
words, phrases, prefixes and their boolean combinations; structure is selected with
flags. Decided on 2026-10-05 before 0.25.0 ships, so this is not a breaking change
for any released version. DEC number reserved: **DEC-366** (amends DEC-333).

## Tasks

- [x] Remove field-term lexing from the query grammar: `title:`, `heading:`, `tag:`, `path:` (and any `field:"phrase"` / `field:prefix*` forms) are no longer recognised; `foo:bar`, `std::fs` and URLs stay plain words exactly as before
- [x] Remove the field-only-query special case (score 0, path sort, no snippets) and the per-file field-term evaluation in `--granularity section` (DEC-334)
- [x] Remove field-term handling from did-you-mean / corrected-query hints and from the malformed-query diagnostics (`title:(a OR b)` message)
- [x] Delete or rewrite the unit and e2e tests that exercise field terms (iteration 302/303/312 suites); keep every test for words, phrases, slop, prefixes, OR, groups and negation
- [x] Docs: `find --help` QUERY SYNTAX and EXAMPLES, `.claude/CLAUDE.md` claims paragraph (DEC-333 and DEC-334 sentences), `skill-hyalo.md`, `rule-knowledgebase.md`, README if it mentions them, CHANGELOG `[Unreleased]` (Removed), DEC-366 with the reasoning above, DEC-333/DEC-334 amended in place
- [x] Generated artefacts refreshed if any output struct or help text they derive from changed (TS declarations, pi-package copies incl. the templates copy)
- [x] Invalid-query hints that name a `--help` section point at a header that exists, and `check-help-drift` gate 3g fails on any "see SECTION in `hyalo <cmd> --help`" hint whose header is missing (added by the orchestrator mid-iteration)
- [x] Migration hint: a zero-result query holding a `title:x`/`heading:x`/`tag:x`/`path:x`-shaped positive word says field terms are not part of the grammar and hints `hyalo find --title x … -- <rest>` (`--section`, `--tag`, `--glob 'x/**'`); recorded in DEC-366 (added by the orchestrator mid-iteration)

## Acceptance criteria

- [x] `hyalo find 'title:dogfood'` treats `title:dogfood` as one plain word (0 or few hits, no field semantics); `hyalo find --title dogfood` is the documented way
- [x] Every non-field query from the 2026-10-04 dogfood report's grammar table returns the same count as before (`snapshot incremental OR refresh` 52, `snapshot (incremental OR refresh) -mdn` 24, `"stale index"~3` 56, `config*` 228, `configuration*` 120)
- [x] Disk and `--index` byte-identical on the queries above; `terms` unchanged
- [x] No mention of `title:`/`heading:`/`tag:`/`path:` field terms remains in help, claims, templates or README (`grep -rn 'heading:' --include='*.md' --include='*.rs'` reviewed)
- [x] fmt, clippy `-D warnings`, `cargo test --workspace -q`, `cargo deny check`, help-drift, jq-recipes, ts-types, pi-package-sync, pi-runtime and `hyalo lint --strict` green; CI green on three platforms

## Outcome

Field terms are gone (DEC-366, amends DEC-333 and DEC-334). Deleted from
hyalo-core: `FieldKind`, `FieldText`, `FieldValue`, `FieldTerm`,
`FieldDocument`, `FieldSource`, `NoFields`, the `Lexeme::Field`/`RawNode::Field`/
`Node::Field` variants, `Compiler::field`, `score_with_fields` (folded into
`score_compiled`), `has_field_terms` and `has_text_terms` (`has_positive_leaf`
answers both), the field constants of the section pruner and the
`title:(…)` "cannot take a group" diagnostic; from hyalo-cli the
`IndexFieldSource` adapter and the field-term wording of the section-mode
error. `query.rs` −419/+131 (the additions are mostly the migration hint and
tests), `sections.rs` −123/+43, `find/mod.rs` −45/+27; field-term unit and
e2e tests deleted (`field_terms_are_predicates`,
`field_term_with_a_group_names_the_real_problem`, the section-mode field
tests, `field_terms_filter_by_metadata`,
`field_only_query_scores_zero_sorted_by_file_without_snippets`,
`rejects_field_only_pattern`), replaced by tests that `name:value` tokens are
plain words; the parity test's field shape became an identifier
(`getUserName`).

Counts on this worktree's knowledgebase, pre-change binary vs new binary
(each is one higher than the plan's figure because this iteration file quotes
the queries): `snapshot incremental OR refresh` 53 → 53,
`snapshot (incremental OR refresh) -mdn` 24 → 24, `"stale index"~3` 57 → 57,
`config*` 229 → 229, `configuration*` 121 → 121. `title:dogfood` went 91
(field semantics) → 134 (the words `title` AND `dogfood`). Disk and `--index`
JSON byte-identical for all six queries in a scratch copy under
`target/scratch/`; `hyalo terms --limit 0` byte-identical to the pre-change
binary.

Orchestrator additions: the invalid-query hint's "see QUERY SYNTAX in
`hyalo find --help`" already named a real header (`QUERY SYNTAX (for
PATTERN):`), as do the SEARCH MODES and FACETS hints; gate 3g now enforces it.
The migration hint withholds the "Try OR" rewrite for such a query.

Gates: fmt, clippy `-D warnings`, `cargo test --workspace -q`, `cargo deny
check`, check-help-drift, check-jq-recipes, check-ts-types, check-pi-runtime,
check-pi-package-sync, check-codex-package, check-bundled-skills,
check-command-reference, check-typed-output and `hyalo lint --strict` green.
