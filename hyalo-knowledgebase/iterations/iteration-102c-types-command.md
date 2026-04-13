---
title: Iteration 102c — `hyalo types` schema management CLI
type: iteration
date: 2026-04-13
status: completed
branch: iter-102c/types-command
tags:
  - iteration
  - schema
  - types
  - cli
depends-on: iterations/iteration-102a-schema-and-lint.md
---

# Iteration 102c — `hyalo types` command

## Goal

Add a `hyalo types` CLI for managing type schemas in `.hyalo.toml` without hand-editing TOML. Depends on **[[iteration-102a-schema-and-lint]]** for the schema data model. Independent of 102b (can ship in parallel).

## CLI Surface

```bash
hyalo types                            # alias for: hyalo types list
hyalo types list                       # list all defined types + required fields
hyalo types show iteration             # full schema for a type
hyalo types create <type>              # add a new type entry
hyalo types remove <type>              # remove a type entry
hyalo types set <type> --required <fields>
hyalo types set <type> --default <key=value>
hyalo types set <type> --filename-template <template>
hyalo types set <type> --property-type <key=type>
hyalo types set <type> --property-values <key=val1,val2,...>
hyalo types set <type> ... --dry-run   # preview file changes
```

## `types set` Side Effects

When `types set` modifies the schema, it immediately applies **safe** fixes to matching files.

**Defaults → auto-apply to files missing the property:**
```
hyalo types set iteration --default status=planned
→ Updates .hyalo.toml
→ Sets status=planned on all type:iteration files missing `status`
→ "Updated .hyalo.toml. Set status=planned on 3 files missing the property."
```

**Constraint changes → report violations only:**
```
hyalo types set iteration --property-values 'status=planned,active,completed'
→ Updates .hyalo.toml
→ "Found 2 iteration files with status values not in the new set. Run `hyalo lint` for details."
```

Rule: **defaults can be applied silently** (user just told us the value). **Constraint violations need judgment** — just report, let the user or LLM remediate via `lint --fix` (if 102b is merged) or manual edits.

`--dry-run` previews what would change without writing.

## Design Decisions

- [x] Should `types create` write directly to .hyalo.toml or output TOML to stdout for review? (default: write; offer `--print` for stdout)

## Tasks

### Commands
- [x] `hyalo types` / `hyalo types list` — list all defined types with required fields
- [x] `hyalo types show <type>` — show full schema for a type
- [x] `hyalo types create <type>` — create a new type entry in .hyalo.toml
- [x] `hyalo types remove <type>` — remove a type definition
- [x] `hyalo types set <type> --required <fields>`
- [x] `hyalo types set <type> --default <key=value>`
- [x] `hyalo types set <type> --filename-template <template>`
- [x] `hyalo types set <type> --property-type <key=type>`
- [x] `hyalo types set <type> --property-values <key=val1,val2,...>`

### Side-effect logic
- [x] `types set --default` auto-applies new defaults to files missing the property
- [x] `types set` constraint changes report violations without auto-fixing
- [x] `--dry-run` flag for `types set` to preview file changes
- [x] `.hyalo.toml` edits preserve formatting and comments

### Tests
- [x] E2E tests for `hyalo types list/show/create/set/remove`
- [x] E2E test: `types set --default` applies to files missing the property
- [x] E2E test: `types set` constraint change reports violations without fixing
- [x] E2E test: `types set --dry-run` previews without writing
- [x] E2E test: `.hyalo.toml` edits preserve formatting

### Docs & Surfaces (keep all four in sync)
- [x] CLI help text for `hyalo types` and all subcommands
- [x] Update README.md: add `hyalo types` section with examples
- [x] Update knowledgebase: user docs for type management
- [x] Update skills: mention `hyalo types` as preferred over hand-editing TOML

### Quality Gates
- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`
- [x] Dogfood: rebuild this repo's schemas via `hyalo types` instead of hand-edited TOML

## Acceptance Criteria

- [x] `hyalo types list` shows all defined types
- [x] `hyalo types create/set/remove` manage type schemas
- [x] `types set --default` applies defaults to files missing the property
- [x] `types set` constraint changes report violations without auto-fixing
- [x] `--dry-run` previews without writing
- [x] `.hyalo.toml` edits preserve formatting and comments
- [x] README, help texts, knowledgebase docs, and skills updated

## Future (Not This Iteration)

- `hyalo create --type iteration --title "BM25 search"` — create files from type templates with defaults
- Skill-driven migration when a type schema changes (bulk fix when schema evolves)
- Cross-file validation (e.g. unique titles, no duplicate branches)
- Lint checks beyond schema: orphan pages, broken links (currently in `summary` and `links fix`)
- Bulk renumber / shift of sequenced files (e.g. iterations) — `hyalo renumber iterations/iteration-4-*.md --shift +1` or similar:
  - Use case: iterations 4, 5, 6 are planned; a new ad-hoc iteration needs slot 4, so shift existing 4/5/6 → 5/6/7
  - Leverages filename templates (`iteration-{n}-{slug}.md`) + property types (`n: integer`) to know which field drives the number and how the filename is derived
  - Updates frontmatter AND renames the file, rewriting backlinks via `hyalo mv`
  - Could generalize beyond iterations to any type with an ordinal field + filename template (episodes, ADRs, RFCs)
- Markdown body linting (MD001–MD054-style rules) via an existing Rust crate:
  - [`rumdl`](https://crates.io/crates/rumdl) — actively maintained pure-Rust `markdownlint` drop-in with library API
  - [`mado`](https://crates.io/crates/mado) — alternative markdownlint-compatible Rust linter
  - [`comrak`](https://crates.io/crates/comrak) / [`pulldown-cmark`](https://crates.io/crates/pulldown-cmark) — parsers to build custom rules
