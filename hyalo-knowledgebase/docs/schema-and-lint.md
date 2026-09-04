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
| `string-list` | YAML sequence of strings; optional `item_pattern` (regex per item) |
| `object-list` | YAML sequence of maps; `required-keys`, `allowed-keys`, `key-patterns` |

#### `object-list` — lists of maps

`object-list` describes a list whose items are all YAML maps, so lint can enforce the
shape of records like a `sources:` list that pins each reference to a commit:

```toml
[schema.types.memory.properties.sources]
type = "object-list"
required-keys = ["ref"]
allowed-keys = ["ref", "commit", "version", "updated", "read"]

[schema.types.memory.properties.sources.key-patterns]
ref = "^(github|confluence|jira|slack|person|runtime|decision):|^https?://"
commit = "^[0-9a-f]{7,40}$"
read = "^\\d{4}-\\d{2}-\\d{2}$"
```

- `required-keys` must be present in **every** item.
- `allowed-keys` is the complete key set when given; a key outside it is an error.
  Omit the list to allow any extra key.
- `key-patterns` maps a key to a regex applied to that key's value when the key is
  present. Strings, numbers and booleans are matched against their YAML text; a
  list or map under a `key-patterns` key is an error. Pattern keys are optional
  unless also listed in `required-keys`.
- Every name in `required-keys` and `key-patterns` must appear in `allowed-keys`
  when that list is given, otherwise the config is rejected.
- Every `key-patterns` regex is compiled when `.hyalo.toml` is loaded, so a bad
  regex is a `schema/malformed` error rather than a per-file lint message (this is
  stricter than `item_pattern`, which reports an invalid regex at lint time — see
  DEC-287).
- An empty list is valid. A non-list value is one error.

Items are validated independently and every violation is reported. Each message
names the property, the 0-based item index and — where applicable — the key:

```text
memory/neon.md:
  error  property "sources" item 0: must be a map, not a string; did you mean `- ref: github:comparis/neon`?
  error  property "sources" item 1: unknown key "rev" (allowed: ref, commit, version, updated, read)
  error  property "sources" item 2: missing required key "ref"
  error  property "sources" item 3: key "commit" value "zzz" does not match pattern "^[0-9a-f]{7,40}$"
  error  property "sources" item 4: key "ref" must be a scalar, got a list
```

The plain-string item is the case worth catching: `find --property sources.ref=…`
walks dot-paths into list items but skips scalars, so a leftover string entry never
matches and is silently absent from every query — lint is what reports it.

`object-list` is **config-only**: `hyalo types set --property-type` does not accept it
(as with `string-list`), and `--fix` has no fixer for its violations, so they report
`autofixable: false`. Writing object *items* with `hyalo set` / `hyalo append` is not
supported and will not be (DEC-291): a write must say *which* item it means, which the
dot-path syntax `find` uses for reads cannot express, and `--property` on the write side
is a literal top-level key by design. Author items in an editor; `hyalo new` scaffolds
the property as `[]`, and `lint` plus `--validate` enforce the shape afterwards —
`--validate` already rejects a scalar or string-item value for an `object-list` property.

### When the schema itself cannot be loaded (DEC-290)

A `[schema]` block can be valid TOML and still be rejected — an uncompilable
`key-patterns` regex, `min-length` on a `number`, `values` outside an `enum`.
The rest of `.hyalo.toml` still applies; the schema is replaced by an empty one.

`hyalo lint` reports that as a `schema/malformed` violation (an error under
`--strict`), and `set` / `append` **refuse with exit 1 when validation was
asked for** — `--validate`, or `[schema] validate_on_write = true` — including
under `--dry-run`, because validating against an empty schema rejects nothing
and the promise would be silently vacuous:

```text
error: refusing to validate against an unusable [schema]: invalid [schema] in
.hyalo.toml: property 'sources': key-patterns.commit: invalid regex …; the
schema could not be loaded, so --validate would reject nothing
```

The gate is scoped to the promise, not to the command: a `set`/`append` without
`--validate` claims no validation and still writes (with the `-q`-proof
warning), and `mv`, `remove`, `task toggle` and every read are untouched — so a
vault whose schema is briefly broken mid-edit stays usable.

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

- **error** — schema violation (missing required property, wrong value type, invalid enum value, pattern mismatch, `object-list` shape violation)
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
  reported as a conflict. Text output explains each one —
  `conflict  MD047  line 5: range overlap with MD012` — capped at 20 lines per
  file, with `… and N more (use --detailed)` beyond that (iter-263).
- **Line endings are preserved.** Fixes on CRLF files emit CRLF; a fix never
  flips a file's line-ending style.
- **Size cap.** Files larger than 100 MiB are skipped with a warning (reported
  as a `FILE` group) instead of being read into memory.

### Obsidian grammar (autofix safety, iter-263)

The bundled `MD*` rules come from `mdbook-lint`, which targets mdBook — no `#tag`
grammar, and a line-based link scanner. On a real Obsidian vault that produced
destructive autofixes, so five rules are narrowed. Each deviation is stated in
`hyalo lint-rules show <ID>`.

| Rule | Narrowed to |
|------|-------------|
| **MD018** | A single `#` followed by a tag token — letters, digits, `_`, `-`, `/`, non-ASCII word characters, at least one non-digit — is an Obsidian tag, not a heading missing its space (DEC-271). `##Heading`, `#1`, `#!bang` and a capitalized word followed by prose (`#Heading typo`) still fire. |
| **MD034** | A URL already inside link markup — a markdown link or image destination, an autolink, a wikilink, a reference definition — is not bare. Only prose URLs are flagged. The autolink it proposes also stops before a following HTML tag (iter-269): `https://…/x<br>` fixes to `<https://…/x><br>`, never `<https://…/x<br>>`. |
| **MD047** | Skipped on a file with no body at all (iter-269): a frontmatter-only note ends with the newline that closes its `---` block, so there is no line left to terminate. A non-empty body genuinely missing its terminator still fires. |
| **MD042** | An image is valid link text: `[![](img.png)](https://…)` is neither an empty link nor a missing-alt warning. A standalone `![](img.png)` still warns, and `[](url)` / `[ ](url)` still error. |
| **MD001** | Reported, never autofixed (DEC-272): renumbering a deliberate `###### Caption` to `##` rewrites authored structure. `hyalo lint-rules list` shows `AUTOFIX no`; silence the warning with `hyalo lint-rules set MD001 --enabled false`. |

Measured on the Obsidian Hub vault (6,520 files): MD018 fix proposals 162 → 0,
MD034 209 → 116 (all remaining ones are prose URLs), MD001 17 → 0, and MD042's
"Found empty link" hits 55 → 0.

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
