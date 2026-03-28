---
title: "Iteration 63: Mutation filter guard"
type: iteration
date: 2026-03-28
tags:
  - iteration
  - cli
  - safety
status: completed
branch: iter-63/mutation-filter-guard
---

# Iteration 63: Mutation filter guard

## Goal

Prevent `set`, `remove`, and `append` from silently accepting filter syntax (comparison operators) in `--property`, which creates garbage property keys instead of filtering.

## Motivation

During the [[dogfooding-legalize-es]] session, `hyalo set --property 'fecha_publicacion<=1900-01-01' --glob '*.md'` wrote a literal property `fecha_publicacion<: 1900-01-01` to all 8,642 files. The user meant `--where-property` but used `--property` out of habit from `find`. The CLI should reject this rather than silently corrupt files.

## Design

When `set`, `remove`, or `append` parses a `--property` value, check if the key portion ends with `<`, `>`, `!`, or `~` (the comparison operator prefixes from filter syntax). If so, reject with a helpful error:

```
error: '--property' in 'set' is for mutation, not filtering.
       'fecha<=1900' looks like a filter — did you mean --where-property?
```

Valid patterns that must still work:
- `--property key=value` (plain assignment)
- `--property key` (existence, in `remove`)
- `--property 'key=[a, b, c]'` (list assignment)

## Tasks

- [x] Add validation in the property-mutation parser to reject operator suffixes
- [x] Emit a clear error message suggesting `--where-property`
- [x] Apply to `set`, `remove`, and `append`
- [x] Add e2e tests for all rejected patterns (`<=`, `>=`, `!=`, `~=`, `<`, `>`)
- [x] Add e2e tests confirming valid patterns still work
- [x] Update help text to clarify `--property` vs `--where-property` distinction
