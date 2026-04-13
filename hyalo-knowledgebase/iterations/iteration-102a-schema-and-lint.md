---
title: Iteration 102a — Schema Data Model & `hyalo lint` (read-only)
type: iteration
date: 2026-04-13
status: planned
branch: iter-102a/schema-and-lint
tags: [iteration, schema, types, validation, lint, toml]
supersedes: iterations/iteration-102-frontmatter-types-schema.md
---

# Iteration 102a — Schema Data Model & `hyalo lint` (read-only)

## Goal

Introduce first-class document types with schema definitions in `.hyalo.toml`, and a read-only `hyalo lint` command that validates files against their schema.

This is the **foundation** of the original iteration 102. It ships validation without `--fix` and without a `hyalo types` CLI — users author schemas by hand in `.hyalo.toml`. Follow-up iterations 102b (`lint --fix`) and 102c (`hyalo types` CLI) build on this.

## Background

Inspired by the LLM Wiki pattern ([[research/karpathy-llm-wiki]]): different document types (iteration, research, backlog, entity) need different frontmatter structures. Conventions are currently enforced only by CLAUDE.md instructions — no machine-readable schema, no validation.

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
```

### Schema Merging

Type schemas merge with `schema.default`. Type-specific `required` **extends** (does not replace) default required. Type-specific property constraints override default ones for the same property name.

### Property Types

| Type | Validates |
|------|-----------|
| `string` | Any string; optional `pattern` (regex) |
| `date` | ISO 8601 date (YYYY-MM-DD) |
| `number` | Integer or float |
| `boolean` | true/false |
| `list` | YAML list |
| `enum` | String matching one of `values` |

## `hyalo lint` Command (read-only)

```bash
hyalo lint                                     # whole vault
hyalo lint iterations/iteration-101-bm25.md    # single file
hyalo lint --glob "iterations/*.md"            # glob
hyalo lint --format json                       # JSON envelope
```

### Output

```
iterations/iteration-101-bm25-ranked-search.md:
  error: missing required property "foo" (type: iteration)
  warn:  status "planed" not in [planned, in-progress, completed, ...]

research/karpathy-llm-wiki.md:
  error: property "date" expected date, got "April 9"
  warn:  no tags defined

3 files checked, 2 with issues (2 errors, 2 warnings)
```

JSON output follows the standard envelope: `{"results": [{"file": "...", "violations": [...]}], "total": N}`.

### Severity Levels

- **error** — schema violation (missing required, wrong type, invalid enum value)
- **warn** — soft issue (no tags, no type property, property not in schema)

### Interaction with `summary`

`summary` gets a one-line lint count (e.g. `Schema: 5 errors, 3 warnings in 4 files`) with a hint to run `hyalo lint` for details. No flag needed — always reports when a schema exists.

## Design Decisions

- [ ] `schema.default.required` is additive with type-specific `required` (decided: additive)
- [ ] Unknown properties (not in schema) → warning or silently allowed? (default: warning)
- [ ] Files with no `type` → validate against `schema.default` only
- [ ] Lint exit codes: 0 = clean, 1 = errors found, 2 = internal error

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
- [ ] Support positional file arg, `--file`, and `--glob`
- [ ] JSON and text output formats
- [ ] Exit code 1 when errors found
- [ ] Add lint violation count to `hyalo summary` output

### Tests
- [ ] Unit tests for schema parsing from TOML
- [ ] Unit tests for schema merging
- [ ] Unit tests for each validation type
- [ ] E2E tests for `hyalo lint` (whole vault, single file, glob)
- [ ] E2E tests: lint summary count in `hyalo summary`
- [ ] E2E test: file with no type validates against default only
- [ ] E2E test: file with unknown type validates against default only
- [ ] E2E test: vault with no schema runs lint with zero violations

### Docs & Surfaces (keep all four in sync)
- [ ] CLI help text for `hyalo lint` (`--help`)
- [ ] Update README.md: add `hyalo lint` section + `[schema]` config example
- [ ] Update knowledgebase: document schema format + lint in user docs
- [ ] Update skills: `.claude/skills/hyalo` and symlinked `crates/*/templates/` — mention lint + schema authoring
- [ ] Dogfood: hand-author `[schema]` in this repo's `.hyalo.toml` for iteration/research/backlog types

### Quality Gates
- [ ] `cargo fmt`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`

## Acceptance Criteria

- [ ] Types defined in `.hyalo.toml` under `[schema.types.<name>]`
- [ ] `hyalo lint` reports per-file violations with error/warn severity
- [ ] `hyalo lint` supports positional file, `--file`, `--glob`
- [ ] `hyalo lint` returns exit 1 when errors found
- [ ] `hyalo summary` shows one-line lint violation count
- [ ] Files without a `type` property validate against `schema.default` only
- [ ] No new external dependencies (TOML + strsim already in tree)
- [ ] Backwards compatible: vaults without `[schema]` work unchanged
- [ ] README, help texts, knowledgebase docs, and skills updated

## Follow-up iterations

- **[[iteration-102b-lint-fix]]** — `hyalo lint --fix` auto-remediation
- **[[iteration-102c-types-command]]** — `hyalo types` CLI for schema management
