---
title: "types set upsert, lint output cleanup"
type: iteration
date: 2026-04-14
status: in-progress
branch: iter-109/types-set-upsert
tags: [types, schema, ux, cli, dogfooding, lint]
---

# iter-109: types set upsert, lint output cleanup

Dogfooding found issues with the `types` and `lint` commands:

1. `types set` fails when the type doesn't exist — requires `types create` first, which only creates an empty entry with `required = []`. The two-step dance is unnecessary; `types set` should upsert.
2. Required fields without explicit property definitions cause a confusing display: "4 required, 2 properties". Required fields should auto-get a `string` property type if none is specified.
3. `hyalo lint` JSON output includes ALL files (even clean ones), wasting LLM context. Should only output files with violations.
4. `hyalo lint` has no `--limit` flag to cap output size.

## Tasks

### types set upsert
- [x] Make `types set` auto-create the type if it doesn't exist (upsert)
- [x] Remove the `types create` subcommand
- [x] When adding `--required` fields, auto-create `string` property entries for any that don't already have a constraint
- [x] Update help text, hints, args to remove `create` references
- [x] Update e2e tests for types

### lint output cleanup
- [x] Only include files with violations in lint JSON output
- [x] Add `--limit` flag to `hyalo lint` (same pattern as `find --limit`)
- [x] Update e2e tests for lint

### docs
- [x] Update CLAUDE.md skill and knowledgebase docs
