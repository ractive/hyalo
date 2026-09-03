---
title: "Schema: object-list property type (typed list items with per-key constraints)"
type: backlog
date: 2026-09-03
status: planned
origin: mapl-memory sources-as-objects migration 2026-09-01 (PR comparis/mapl-memory#77)
priority: medium
tags:
  - schema
  - lint
---

## Problem

The schema language types lists as `list` (any items) or `string-list` (string items,
optional `item_pattern`). There is no way to validate a **list of maps**.

Concrete case: mapl-memory migrated `sources:` from strings to objects —

```yaml
sources:
  - ref: github:comparis/neon
    commit: 3c9e0f2
  - ref: https://example.org/post
    read: 2026-09-01
```

`find --property sources.ref=…` (0.21 dot-paths) makes this queryable, but lint cannot
enforce the shape: a leftover plain-string entry, a `rev:` typo, or an unknown pin key
all pass `--strict` — and a string entry silently drops out of every `sources.ref=`
query, which is the dangerous half. The vault currently documents the contract in a
comment next to `type = "list"`.

## Proposal

A new property constraint type, e.g.:

```toml
[schema.default.properties.sources]
type = "object-list"
required-keys = ["ref"]
allowed-keys = ["ref", "commit", "version", "updated", "read"]

[schema.default.properties.sources.key-patterns]
ref = "^(github|confluence|jira|slack|person|runtime|decision):|^https?://"
commit = "^[0-9a-f]{7,40}$"
read = "^\\d{4}-\\d{2}-\\d{2}$"
```

Semantics:

- Every list item must be a map (a plain string is a violation with a fix-it message
  suggesting `- ref: <value>`).
- `required-keys` must be present; keys outside `allowed-keys` are violations
  (omit `allowed-keys` = any extra keys allowed).
- `key-patterns` applies regex per key to scalar values; non-scalar values are
  violations.
- Items are validated independently; the first error names item index + key.

## Notes

- Keep it deliberately flat (string keys, scalar values, regex) — this is not JSON
  Schema; nesting deeper than one level stays out of scope.
- `hyalo set --property` support for appending object items is a separate concern;
  the type only needs lint + `types show` output.

## Acceptance criteria

- [ ] `object-list` parses from `.hyalo.toml` with `required-keys`, `allowed-keys`, `key-patterns`
- [ ] Lint flags: non-map item (with `- ref:` fix-it), missing required key, unknown key, pattern mismatch — each naming file, item index, key
- [ ] `hyalo types show` renders the constraint readably
- [ ] Existing `list` / `string-list` behavior unchanged
- [ ] Test matrix: valid objects pass; string item, typo key, bad pattern each fail with the right message
