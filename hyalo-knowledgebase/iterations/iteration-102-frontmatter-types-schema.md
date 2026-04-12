---
title: "Iteration 102 — Frontmatter Types, Schema & Lint"
type: iteration
date: 2026-04-09
status: planned
branch: iter-102/frontmatter-types-schema
tags: [iteration, schema, types, validation, lint, toml]
---

# Iteration 102 — Frontmatter Types, Schema & Lint

## Goal

Introduce first-class document types with schema definitions in `.hyalo.toml`. Types define required properties, allowed values, defaults, and filename templates. A new `hyalo lint` command validates files against their schema and optionally auto-fixes what it can. A new `hyalo types` command manages type schemas.

## Background

Inspired by the LLM Wiki pattern ([[research/karpathy-llm-wiki]]): different document types (iteration, research, backlog item, entity page) need different frontmatter structures. Currently conventions are enforced only by CLAUDE.md instructions — there's no machine-readable schema and no automated validation.

## Schema Format in .hyalo.toml

```toml
[schema.default]
required = ["title"]

[schema.types.iteration]
required = ["title", "date", "status", "branch", "tags"]
filename-template = "iterations/iteration-{n}-{slug}.md"

[schema.types.iteration.defaults]
status = "planned"
date = "$today"
type = "iteration"

[schema.types.iteration.properties.status]
type = "enum"
values = ["planned", "in-progress", "completed", "superseded", "shelved", "deferred"]

[schema.types.iteration.properties.branch]
type = "string"
pattern = "^iter-\\d+/"

[schema.types.iteration.properties.date]
type = "date"

[schema.types.iteration.properties.tags]
type = "list"

[schema.types.research]
required = ["title", "date", "status", "tags"]
filename-template = "research/{slug}.md"

[schema.types.research.defaults]
status = "active"
type = "research"

[schema.types.research.properties.status]
type = "enum"
values = ["active", "completed", "superseded"]
```

### Schema Merging

Type schemas merge with `schema.default`. Type-specific `required` extends (not replaces) default required. Type-specific property constraints override default ones for the same property name.

### Property Types

| Type | Validates |
|------|-----------|
| `string` | Any string; optional `pattern` (regex) |
| `date` | ISO 8601 date (YYYY-MM-DD) |
| `number` | Integer or float |
| `boolean` | true/false |
| `list` | YAML list |
| `enum` | String matching one of `values` |

### Filename Templates

| Placeholder | Meaning |
|---|---|
| `{slug}` | Kebab-cased title |
| `{n}` | Next sequential number (scanned from existing files matching the pattern) |
| `{date}` | Today's date (YYYY-MM-DD) |

## `hyalo lint` Command

The primary way to validate files against their schema. Separate from `summary` because lint needs detailed per-file, per-violation output that doesn't fit a stats overview.

```bash
# Lint the whole vault
hyalo lint

# Lint a single file (positional)
hyalo lint iterations/iteration-101-bm25-ranked-search.md

# Lint with glob
hyalo lint --glob "iterations/*.md"

# Auto-fix what's fixable
hyalo lint --fix
hyalo lint --fix iterations/iteration-101-bm25-ranked-search.md
```

### Output

```
iterations/iteration-101-bm25-ranked-search.md:
  error: missing required property "foo" (type: iteration)
  warn:  status "planed" not in [planned, in-progress, completed, ...] (did you mean "planned"?)

research/karpathy-llm-wiki.md:
  error: property "date" expected date, got "April 9"
  warn:  no tags defined

3 files checked, 2 with issues (2 errors, 2 warnings)
```

JSON output follows the standard envelope: `{"results": [{"file": "...", "violations": [...]}], "total": N}`.

### Severity Levels

- **error** — schema violation (missing required, wrong type, invalid enum value)
- **warn** — soft issue (no tags, no type property, property not in schema)

### `--fix` Auto-Fixes

| Fixable | How |
|---------|-----|
| Missing property with a default in schema | Insert the default |
| Close enum typo ("planed" → "planned") | Levenshtein via `strsim` (already a dep) |
| Missing `type` when file path matches a `filename-template` | Infer type from path |
| Normalizable date ("2026-4-9" → "2026-04-09") | Normalize format |

**Not fixable by `--fix`:** missing required properties without defaults — report only. The LLM/skill decides what value to use.

### Interaction with `summary`

`summary` gets a one-line lint count (e.g. `Schema: 5 errors, 3 warnings in 4 files`) with a hint to run `hyalo lint` for details. No `--validate` flag needed on summary — it always reports the count when a schema exists.

### `types set` Side Effects

When `types set` modifies the schema, it immediately applies safe fixes to matching files:

**Defaults → auto-apply to files missing the property:**
```bash
hyalo types set iteration --default status=planned
# → Updates .hyalo.toml
# → Sets status=planned on all type:iteration files missing `status`
# → "Updated .hyalo.toml. Set status=planned on 3 files missing the property."
```

**Constraint changes → report violations only:**
```bash
hyalo types set iteration --property-values 'status=planned,active,completed'
# → Updates .hyalo.toml
# → "Found 2 iteration files with status values not in the new set. Run `hyalo lint` for details."
```

The rule: **defaults can be applied silently** (the user just told us the value). **Constraint violations need judgment** (just report, let the user or LLM decide via `lint --fix` or manual edits).

Use `--dry-run` to preview what would change without writing anything.

## Design Decisions

- [ ] Should `schema.default.required` be additive with type-specific `required`, or should type override completely?
- [ ] Should unknown properties (not in schema) be a warning or silently allowed?
- [ ] How to handle files with no `type` property — validate against `schema.default` only?
- [ ] Should `types create` write directly to .hyalo.toml, or output TOML to stdout for review?
- [ ] Lint exit codes: 0 = clean, 1 = errors found, 2 = internal error?

## Tasks

### Schema Data Model
- [ ] Define Rust types for schema config (SchemaConfig, TypeSchema, PropertyConstraint)
- [ ] Parse `[schema.*]` sections from .hyalo.toml
- [ ] Implement schema merging (default + type-specific)
- [ ] Handle missing/empty schema gracefully (no schema = no validation)

### `hyalo lint` Command
- [ ] Implement validation: required properties check
- [ ] Implement validation: enum values check (with Levenshtein suggestion via strsim)
- [ ] Implement validation: date format check
- [ ] Implement validation: string pattern (regex) check
- [ ] Implement validation: type checks (list, number, boolean)
- [ ] Implement `--fix`: insert defaults for missing required properties
- [ ] Implement `--fix`: correct close enum typos
- [ ] Implement `--fix`: infer type from filename-template match
- [ ] Implement `--fix`: normalize date formats
- [ ] Support positional file arg, `--file`, and `--glob`
- [ ] JSON and text output formats
- [ ] Exit code 1 when errors found
- [ ] Add lint violation count to `hyalo summary` output

### `hyalo types` Command
- [ ] `hyalo types` / `hyalo types list` — list all defined types with their required fields
- [ ] `hyalo types show <type>` — show full schema for a type
- [ ] `hyalo types create <type>` — create a new type entry in .hyalo.toml
- [ ] `hyalo types set <type> --required <fields>` — set required fields
- [ ] `hyalo types set <type> --default <key=value>` — set default values
- [ ] `hyalo types set <type> --filename-template <template>` — set filename template
- [ ] `hyalo types set <type> --property-type <key=type>` — set property type constraint
- [ ] `hyalo types set <type> --property-values <key=val1,val2,...>` — set enum values
- [ ] `hyalo types remove <type>` — remove a type definition
- [ ] `types set --default` auto-applies new defaults to files missing the property
- [ ] `types set` constraint changes report violations without auto-fixing
- [ ] `--dry-run` flag for `types set` to preview file changes

### Tests
- [ ] Unit tests for schema parsing from TOML
- [ ] Unit tests for schema merging
- [ ] Unit tests for each validation type
- [ ] Unit tests for each `--fix` action
- [ ] E2E tests for `hyalo lint` (whole vault, single file, glob)
- [ ] E2E tests for `hyalo lint --fix`
- [ ] E2E tests for `hyalo types list/show/create/set/remove`
- [ ] E2E tests: lint summary count in `hyalo summary`
- [ ] E2E test: file with no type validates against default only
- [ ] E2E test: file with unknown type validates against default only
- [ ] E2E test: vault with no schema runs lint with zero violations
- [ ] E2E test: `types set --default` applies to files missing the property
- [ ] E2E test: `types set` constraint change reports violations without fixing
- [ ] E2E test: `types set --dry-run` previews without writing

### Quality Gates
- [ ] `cargo fmt`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] Dogfood: define schemas for hyalo-knowledgebase types (iteration, research, backlog)

## Acceptance Criteria

- [ ] Types defined in `.hyalo.toml` under `[schema.types.<name>]`
- [ ] `hyalo types list` shows all defined types
- [ ] `hyalo types create/set/remove` manage type schemas
- [ ] `hyalo lint` reports per-file violations with error/warn severity
- [ ] `hyalo lint --fix` auto-fixes: defaults, typos, date normalization, type inference
- [ ] `hyalo lint` supports positional file, `--file`, `--glob`
- [ ] `hyalo summary` shows one-line lint violation count
- [ ] Files without a `type` property validate against `schema.default` only
- [ ] No new external dependencies (TOML parsing + strsim already in tree)
- [ ] Backwards compatible: vaults without `[schema]` in .hyalo.toml work unchanged

## Future (Not This Iteration)

- `hyalo create --type iteration --title "BM25 search"` — create files from type templates with defaults
- Skill-driven migration when a type schema changes (bulk fix when schema evolves)
- Cross-file validation (e.g. unique titles, no duplicate branches)
- Lint checks beyond schema: orphan pages, broken links (currently in `summary` and `links fix`)
