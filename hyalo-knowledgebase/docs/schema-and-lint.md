---
title: Schema & Lint — Document Type Validation
type: docs
date: 2026-04-14
status: active
tags:
  - docs
  - schema
  - lint
  - validation
---

# Schema & Lint — Document Type Validation

Hyalo supports optional schema validation for frontmatter properties. Define a schema in `.hyalo.toml` under `[schema.*]` sections, then run `hyalo lint` to validate all files.

## Configuring a Schema

```toml
# .hyalo.toml

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

### Property Types

| Type      | Validates |
|-----------|-----------|
| `string`  | Any string; optional `pattern` (regex) |
| `date`    | ISO 8601 date (YYYY-MM-DD) |
| `number`  | Integer or float |
| `boolean` | true/false |
| `list`    | YAML sequence |
| `enum`    | String matching one of `values` |

### Schema Merging

`schema.default` applies to every file regardless of type.

- `required`: type-specific list **extends** the default (additive, no duplicates)
- `properties`: type-specific constraints **override** defaults for the same property name; other defaults fill in gaps

Files without a `type` property are validated against `schema.default` only.

## Running `hyalo lint`

```sh
# Lint the whole vault
hyalo lint

# Lint a single file
hyalo lint iterations/iteration-101-bm25.md

# Lint with a glob
hyalo lint --glob "iterations/*.md"

# JSON output
hyalo lint --format json
```

**Exit codes:** `0` = clean, `1` = errors found, `2` = internal error.

### Output (text)

```
iterations/iteration-101-bm25.md:
  error  missing required property "foo" (type: iteration)
  warn   status "planed" not in [planned, in-progress, completed, ...] (did you mean "planned"?)

research/karpathy-llm-wiki.md:
  error  property "date" expected date (YYYY-MM-DD), got "April 9"
  warn   no tags defined

3 files checked, 2 with issues (2 errors, 2 warnings)
```

### Severity Levels

- **error** — schema violation (missing required property, wrong value type, invalid enum value, pattern mismatch)
- **warn** — soft issue (no `type` property, no `tags`, property not declared in schema)

## Summary Integration

When a schema is configured, `hyalo summary` includes a one-line lint count in the `schema` field of the JSON output:

```json
{
  "results": {
    "files": { "total": 42, ... },
    "schema": { "errors": 3, "warnings": 7, "files_with_issues": 5 },
    ...
  }
}
```

Run `hyalo lint` to see the full violation report.

## Backwards Compatibility

Vaults without a `[schema]` block in `.hyalo.toml` are fully supported: `hyalo lint` exits 0 with zero violations, and `hyalo summary` omits the `schema` field.
