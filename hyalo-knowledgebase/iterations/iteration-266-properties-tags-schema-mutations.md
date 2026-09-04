---
type: iteration
title: "Iteration 266 — properties/tags/schema: in-place rename, parent tags, --index parity"
date: 2026-09-03
status: completed
tags:
  - iteration
  - properties
  - tags
  - schema
  - dogfooding
branch: iter-266/properties-tags-schema-mutations
priority: 6
related:
  - "[[dogfood-results/dogfood-v0220-obsidian-vaults]]"
---

# Iteration 266 — properties/tags/schema: in-place rename, parent tags, --index parity

## Goal

The metadata commands are the ones an Obsidian user runs against a vault under
version control, and [[dogfood-results/dogfood-v0220-obsidian-vaults]] shows
their diffs are not what the help promises. `properties rename` moves the key
to the end of the frontmatter and turns an empty `rating:` into `score: null`
(BUG-12, 16 of 16 files on kepano-obsidian). `tags rename --from music --to
audio` does nothing although `music/genres` exists and Obsidian renames the
subtree (BUG-15). `properties` and `tags` reject `--index` while every other
reading command accepts it (BUG-11). Schema types cannot bind when `type:` is a
one-element list or a `[[Wikilink]]`, so `types set Authors` succeeds but never
applies and `--validate` passes a value that violates the schema (BUG-13).
`summary` text lists a mixed-type property once per type and counts pairs
(BUG-16), and `read --frontmatter` re-serialises YAML so indentation no longer
matches the file (UX-15).

Constraint: **no new CLI flags** from dogfood pressure (project rule).
`--index` on `properties`/`tags` is an existing flag reaching parity, not a new
one. Schema binding is a matching rule plus documentation, not a `--match`
flag; if per-type path matching is wanted it is a `[schema.types.X] match`
config key. Out of scope: `set` collapsing a list to a scalar (UX-12, in
[[iterations/iteration-262-frontmatter-wikilinks-first-class]]), frontmatter
wikilinks as graph edges (same place), and the `hyalo new` scaffold
placeholders (UX-17, in [[iterations/iteration-267-help-hints-text-polish]]).

## Tasks

### PROP-1: `properties rename` in place (BUG-12)

- [x] hyalo-core frontmatter editing: rename the key on its own line,
      preserving position, the exact value bytes (quoting, spacing, comments,
      block-list indentation) and an empty value as empty, never `null`.
      Handle the key appearing in a nested map only if the current
      implementation does (document the scope).
- [x] Unit tests: scalar, quoted scalar, empty value, block list, flow list,
      key with trailing comment, key at first/middle/last position; assert the
      frontmatter block is byte-identical except for the key token.
- [x] e2e in `crates/hyalo-cli/tests/e2e`: rename on a three-file fixture and
      compare against golden files; `--dry-run` writes nothing.
- [x] Docs: `properties rename --help` ("preserves position and value text"),
      changelog.

### TAG-1: `tags rename` on a parent tag (BUG-15)

- [x] DEC-282 (tentative): `tags rename --from music --to audio` renames
      `music` and every nested `music/…` tag (Obsidian semantics), in both the
      `tags:` frontmatter list and inline `#music/genres` body tags if hyalo
      rewrites those today; JSON reports `renamed_tags: [{from, to, files}]`
      so the expansion is visible. Text output lists each renamed tag. If the
      exact tag does not exist but children do, proceed rather than print
      `modified: (empty)`.
- [x] hyalo-core tag rename: prefix-aware match on the `/` boundary (`music`
      must not match `musical`).
- [x] Unit tests for boundary handling and the exact-tag-absent case; e2e on a
      fixture with `music`, `music/genres`, `musical`.
- [x] Docs: `tags rename --help`, skill file, changelog.

### IDX-1: `--index` on `properties` and `tags` (BUG-11)

- [x] hyalo-cli: add the shared `--index` / `--index-file` argument group to
      `properties` (summary, rename) and `tags` (summary, rename), reading
      frontmatter from the snapshot exactly as `find` does and refreshing the
      snapshot after a rename as `set --index` does.
- [x] Parity e2e: `properties --format json` with and without `--index` are
      identical on a fixture; same for `tags`; `tags rename --index` leaves
      the index consistent (`find --tag audio --index --count` matches disk).
- [x] Docs: both `-h` pages now show the flags; the `--index` availability
      list in `hyalo --help`; skill file; changelog.

### SCHEMA-1: type binding tolerance (BUG-13)

- [x] DEC-281 (tentative): schema type binding accepts `type:` as a string, a
      one-element list of strings, or a `[[Name]]` wikilink (bare or quoted),
      normalising to the string `Name`; a multi-element list does not bind and
      HYALO reports it as before. `types set '[[Authors]]'` is still an
      invalid type name. Also decide whether `types set --required K`
      auto-adds `K: type=string` when the vault's values are lists (proposal:
      infer the dominant type from the vault, as `properties` already knows
      it).
- [x] Fix `--validate`: `set … --property rating=high --validate` against
      `rating: number` must be refused with exit 1 even under `--dry-run`, and
      `validate_on_write = true` must apply whenever a schema binds.
- [x] hyalo-core schema binding + hyalo-cli `types set`; unit tests for the
      three accepted shapes and the rejected list; e2e on the kepano-style
      `type: ["[[Authors]]"]` fixture where `types set Authors --required
      categories` then makes `lint --strict` report the missing property on
      exactly the bound files.
- [x] Docs: `types set --help` binding paragraph, the schema reference page in
      `hyalo-knowledgebase/`, `.claude/skills/hyalo/SKILL.md`, changelog.

### OUT-1: `summary` mixed-type rows and raw `read --frontmatter` (BUG-16, UX-15)

- [x] `summary` text and JSON: key properties by name; a mixed-type property is
      one row with the breakdown, `published (103: 79 datetime, 24 date)`, and
      the count is the number of distinct names (7 on Obsidian Hub, not 13).
      JSON keeps one entry per name with a `types` map.
- [x] `read --frontmatter`: return the raw frontmatter text between the fence
      lines, byte for byte, in text mode; JSON keeps the parsed map and adds
      `frontmatter_raw`. No YAML re-serialisation on a read path.
- [x] e2e: `summary` on a fixture with a mixed-type property; `read
      --frontmatter` on a file with 2-space block-list indentation compared to
      `sed -n '/^---$/,/^---$/p'`.
- [x] Docs: `summary -h` result keys, `read -h`, changelog.

## Acceptance criteria

- [x] kepano-obsidian, cwd `../kepano-obsidian`, clean checkout: `hyalo
      properties rename --from rating --to score && git diff -U0 | grep '^[-+]'
      | grep -v '^[-+][-+]'` shows only paired `-rating: X` / `+score: X` lines
      with identical `X` (including the empty value in
      `Templates/App Template.md`), no `null`, and `git diff --stat` touches
      16 files; then `git checkout .`.
- [x] `../kepano-obsidian`: `hyalo tags rename --from music --to audio
      --format json --jq '.results.renamed_tags'` lists `music/genres →
      audio/genres` and `hyalo find --tag audio --count` equals the previous
      `hyalo find --tag music --count`; `git checkout .`.
- [x] `../kepano-obsidian`: `hyalo create-index && diff <(hyalo properties
      --format json) <(hyalo properties --index --format json)` → empty; same
      for `hyalo tags`.
- [x] `../kepano-obsidian`: `hyalo types set Authors --required categories &&
      hyalo lint --strict --rule HYALO001 --count` reports only files whose
      `type` normalises to `Authors` and lack `categories`; `hyalo lint
      --count` shows 0 `property "type" expected string, got […]` warnings (was
      15) and the 59 `no 'type' property` warnings are unchanged only for files
      truly without `type`; `git checkout . && git clean -fd .hyalo.toml`
      afterwards if it was created.
- [x] `../kepano-obsidian`: `hyalo set 'References/Kevin Kelly.md' --property
      rating=high --validate --dry-run; echo $?` → 1 with the schema error.
- [x] Obsidian Hub, cwd `../obsidian-hub`: `hyalo summary --format text | grep
      Properties` lists `published` once with a type breakdown and the count
      equals `hyalo properties --count`.
- [x] `../kepano-obsidian`: `diff <(hyalo read 'Clippings/Buy wisely.md'
      --frontmatter --format text --no-hints) <(sed -n '2,/^---$/p'
      'Clippings/Buy wisely.md' | sed '$d')` → empty (modulo the fence lines,
      adjust the sed to the chosen output shape).
- [x] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo test --workspace -q`, `hyalo lint --strict` on the KB,
      xtask help-drift check.
- [x] Changelog entries via `hyalo changelog add`; DEC-281 and DEC-282
      recorded in [[decision-log]]; `.claude/skills/hyalo/SKILL.md` and
      `.claude/CLAUDE.md` updated for `--index` on `properties`/`tags` and the
      binding rule.

## Links

- [[dogfood-results/dogfood-v0220-obsidian-vaults]]
- [[iterations/iteration-262-frontmatter-wikilinks-first-class]]
- [[iterations/iteration-267-help-hints-text-polish]]
- [[decision-log]]

## Outcome (2026-09-04)

All five task groups landed. Verified against the real testbeds:

- **PROP-1** — `../kepano-obsidian`, clean checkout: `hyalo properties rename
  --from rating --to score` touches **16 files, 16 insertions, 16 deletions**,
  every changed line a paired `-rating: X` / `+score: X` with identical `X`
  (including the five empty values), and **zero** `null` in the diff. The write
  is a new key-token splice in `hyalo-core`
  (`frontmatter::rename_key_in_place`), verified by re-parsing the result and
  comparing it against the original map with exactly one key renamed — the same
  safety gate `splice_frontmatter` uses. Unspliceable blocks (mixed line
  endings, non-UTF-8, YAML the segmenter cannot span-map) fall back to the
  props write, which now also preserves the key's position.
- **TAG-1** (DEC-282) — `tags rename --from music --to audio` on kepano
  reports `renamed_tags: [{music/genres → audio/genres, files: 2}]` and
  `find --tag audio --count` (2) equals the previous `--tag music --count` (2).
  The bare `music` tag does not occur in that vault at all, which is exactly
  the case that used to print `0 modified`.
- **IDX-1** — `diff <(hyalo properties --format json) <(hyalo properties
  --index --format json)` is empty on kepano, same for `tags`.
- **SCHEMA-1** (DEC-281) — `types set Authors --required categories` now binds
  the three `type: ["[[Authors]]"]` / `type: "[[Authors]]"` files
  (`Templates/Author Template.md`, `References/Steph Ango.md`,
  `References/Kevin Kelly.md`); the six multi-element `type:` lists are
  reported by `lint` with a message naming the shapes that work. With
  `rating: number` declared, `set 'References/Kevin Kelly.md' --property
  rating=high --validate --dry-run` exits **1**.
- **OUT-1** — Obsidian Hub: `Properties: 7 — … published (103: 79 datetime, 24
  date)`, equal to `hyalo properties --count`. `read 'Clippings/Buy wisely.md'
  --frontmatter` is byte-identical to the file's own block.

### Deviations from the plan

- **`summary` JSON keeps `mixed_types`, not a new `types` map.** The plan asked
  for "one entry per name with a `types` map"; `properties summary` has
  reported that breakdown as `mixed_types` since iteration 190, and forking two
  shapes for the same information would be a worse inconsistency than the one
  being fixed. Both commands now emit the identical `PropertySummaryEntry`.
- **`summary` now counts `tags` as a property.** The plan's AC demands the
  headline count equal `hyalo properties --count`; `summary` had been excluding
  `tags` on the grounds that tags get their own section, which made the two
  commands answer 6 and 7 on Obsidian Hub. The exclusion is gone.
- **`read --frontmatter` keeps its `---` fences in text mode** (the AC allowed
  adjusting the comparison to the chosen shape). The bytes *between* the fences
  are verbatim; the fences frame them.
- **`types set --required K` infers K's auto-declared type** rather than
  hardcoding `string` — the plan left this open, and the hardcoded `string` is
  what made `types set Authors --required categories` create the errors it then
  reported on a vault where `categories` is a list. Recorded in DEC-281.
- **Inline `#tag` body rewriting was not added.** hyalo does not rewrite inline
  tags today, so `tags rename` stays exactly as wide as the frontmatter data it
  already owns (noted in DEC-282).
