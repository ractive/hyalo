---
title: "Iteration 315: remove field terms from the search grammar"
type: iteration
date: 2026-10-05
tags: [iteration, search, find, cleanup]
status: planned
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

- [ ] Remove field-term lexing from the query grammar: `title:`, `heading:`, `tag:`, `path:` (and any `field:"phrase"` / `field:prefix*` forms) are no longer recognised; `foo:bar`, `std::fs` and URLs stay plain words exactly as before
- [ ] Remove the field-only-query special case (score 0, path sort, no snippets) and the per-file field-term evaluation in `--granularity section` (DEC-334)
- [ ] Remove field-term handling from did-you-mean / corrected-query hints and from the malformed-query diagnostics (`title:(a OR b)` message)
- [ ] Delete or rewrite the unit and e2e tests that exercise field terms (iteration 302/303/312 suites); keep every test for words, phrases, slop, prefixes, OR, groups and negation
- [ ] Docs: `find --help` QUERY SYNTAX and EXAMPLES, `.claude/CLAUDE.md` claims paragraph (DEC-333 and DEC-334 sentences), `skill-hyalo.md`, `rule-knowledgebase.md`, README if it mentions them, CHANGELOG `[Unreleased]` (Removed), DEC-366 with the reasoning above, DEC-333/DEC-334 amended in place
- [ ] Generated artefacts refreshed if any output struct or help text they derive from changed (TS declarations, pi-package copies incl. the templates copy)

## Acceptance criteria

- [ ] `hyalo find 'title:dogfood'` treats `title:dogfood` as one plain word (0 or few hits, no field semantics); `hyalo find --title dogfood` is the documented way
- [ ] Every non-field query from the 2026-10-04 dogfood report's grammar table returns the same count as before (`snapshot incremental OR refresh` 52, `snapshot (incremental OR refresh) -mdn` 24, `"stale index"~3` 56, `config*` 228, `configuration*` 120)
- [ ] Disk and `--index` byte-identical on the queries above; `terms` unchanged
- [ ] No mention of `title:`/`heading:`/`tag:`/`path:` field terms remains in help, claims, templates or README (`grep -rn 'heading:' --include='*.md' --include='*.rs'` reviewed)
- [ ] fmt, clippy `-D warnings`, `cargo test --workspace -q`, `cargo deny check`, help-drift, jq-recipes, ts-types, pi-package-sync, pi-runtime and `hyalo lint --strict` green; CI green on three platforms
