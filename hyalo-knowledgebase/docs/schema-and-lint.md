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
| `datetime` | ISO 8601 naive local datetime (YYYY-MM-DDThh:mm:ss); no `Z`/offset/fractional seconds |
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

```text
iterations/iteration-101-bm25.md:
  error  missing required property "foo" (type: iteration)
  error  property "status" value "planed" not in [planned, in-progress, completed, ...] (did you mean "planned"?)

research/karpathy-llm-wiki.md:
  error  property "date" expected date (YYYY-MM-DD), got "April 9"

3 files checked, 2 with issues (3 errors, 0 warnings)
```

### Severity Levels

- **error** — schema violation (missing required property, wrong value type, invalid enum value, pattern mismatch)
- **warn** — soft issue (no `type` property, property not declared in schema)

To require `tags` on a given document type, list it in that type's `required` array
(e.g. `required = ["title", "tags"]`) — a missing `tags` key then becomes an error.
A YAML null value (`tags: ~`) or an empty array (`tags: []`) also fails: vacuous
values are treated as semantically equivalent to absent for required properties.
Atomic-typed required properties (`string`, `date`, `number`, ...) only need to
be present — an empty string or zero still satisfies them.

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

## Lint --fix

`hyalo lint --fix` automatically repairs common frontmatter issues. Use `--dry-run` to preview changes without writing any files.

```bash
# Preview what --fix would change (no files written)
hyalo lint --fix --dry-run

# Apply auto-fixes
hyalo lint --fix

# Fix a single file
hyalo lint --fix iterations/iteration-101-bm25.md
```

**Note:** `--dry-run` requires `--fix` — it has no effect on a plain `hyalo lint` (which is already read-only).

### Fix guarantees (since iter-158)

- **Atomic writes.** Every fixed file is written via temp-file-plus-rename, and
  a modification-time guard aborts if another process changed the file between
  read and write — the same guarantees `set`/`remove`/`append` give. A crash
  mid-fix can never truncate a document.
- **Single-run convergence.** Body fixes are applied in internal passes until a
  fixpoint, so one `--fix` run finishes the job; a second run reports zero
  fixes and changes no bytes. `--dry-run` previews the fully converged result.
- **Severity wins conflicts.** When two rules' fixes overlap on the same byte
  range, the higher-severity fix (error over warn) is applied and the other is
  reported as a conflict.
- **Line endings are preserved.** Fixes on CRLF files emit CRLF; a fix never
  flips a file's line-ending style.
- **Size cap.** Files larger than 100 MiB are skipped with a warning (reported
  as a `FILE` group) instead of being read into memory.

### Fix categories

| Category | What it does |
|----------|-------------|
| **Insert defaults** | Adds missing required properties using their schema default values |
| **Fix enum typos** | Corrects near-matches to valid enum values (Levenshtein distance ≤ 2) |
| **Normalize dates** | Rewrites dates to ISO 8601 (YYYY-MM-DD) format |
| **Infer type** | Sets `type` from filename template matches when absent |

Each fix is reported in the output with the category, property name, and old/new values.

## Backwards Compatibility

Vaults without a `[schema]` block in `.hyalo.toml` are fully supported: `hyalo lint` exits 0 with zero violations, and `hyalo summary` omits the `schema` field.
