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

# Auto-remediate fixable violations in place
hyalo lint --fix

# Preview fixes without writing any files
hyalo lint --fix --dry-run
```

**Exit codes:** `0` = clean (after fixes, if any), `1` = errors remain, `2` = internal error.

### Auto-Fix (`--fix`)

`hyalo lint --fix` attempts to repair the violations it can repair safely:

| Fixable | How |
|---------|-----|
| Missing property with a schema `default` | Insert the default (`$today` expands to the current ISO 8601 date) |
| Close enum typo (Levenshtein ≤ 2) | Replace with the nearest valid value (e.g. `planed` → `planned`) |
| Loose date format | Normalize to `YYYY-MM-DD` (e.g. `2026-4-9` → `2026-04-09`) |
| Missing `type` when the path matches a `filename-template` | Infer the type from the matching `[schema.types.*]` entry |

**Never fabricated.** Missing required properties without defaults are reported but never invented; a human or tool must supply the value. Fixes preserve the existing frontmatter key order and the document body byte-for-byte.

Pass `--dry-run` together with `--fix` to print the fixes that *would* be applied without modifying any files. The JSON output gains a top-level `fixes` array listing the actions per file:

```json
{
  "results": {
    "files": [...],
    "total": 3,
    "fixes": [
      {
        "file": "iterations/iteration-101-bm25.md",
        "actions": [
          { "kind": "fix-enum-typo", "property": "status", "old": "planed", "new": "planned" }
        ]
      }
    ],
    "dry_run": true
  }
}
```

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
