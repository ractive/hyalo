---
title: "Iteration 4 — Property Find & List Operations"
type: iteration
date: 2026-03-21
tags:
  - iteration
  - properties
  - search
  - refactor
status: in-progress
branch: iter-4/property-find
---

# Iteration 4 — Property Find & List Operations

## Goal

1. Add `hyalo property find` to search files by frontmatter property (done).
2. Add generic list-property mutations: `property add-to-list` and `property remove-from-list`.
3. Refactor `tag add/remove` to delegate to the generic list operations, keeping tag-specific validation as a pre-step.

## Motivation

Tags are just a list property named "tags". By implementing generic list operations on properties, we eliminate duplication and enable the same operations on any list property (e.g. `aliases`, `authors`). The `tag` commands become thin wrappers that add tag-specific validation before delegating.

## Relationship to `search`

A general `search` command could subsume both `tag find` and `property find`. For now, `property find` fills the gap without over-engineering.

## Phase 1 — Property Find (done)

- [x] Add `Find` variant to `PropertyAction` enum in `main.rs`
- [x] Implement `property_find()` with type-aware value matching
- [x] Wire up routing, help text, unit tests (12), e2e tests (19)
- [x] Quality gates passed

## Phase 2 — Generic List Operations & Tag Refactor

### New commands
- [ ] `hyalo property add-to-list --name <key> --value <val>... --file <path> [--glob <pattern>]` — append values to a list property, create the list if absent, skip duplicates (case-insensitive)
- [ ] `hyalo property remove-from-list --name <key> --value <val>... --file <path> [--glob <pattern>]` — remove values from a list property, remove the key if list becomes empty

### Refactor tag commands
- [ ] Extract core list logic into shared helpers in `properties.rs`: `property_add_to_list()` and `property_remove_from_list()`
- [ ] Refactor `tag_add()` to: validate tag → delegate to `property_add_to_list(name="tags", ...)`
- [ ] Refactor `tag_remove()` to: delegate to `property_remove_from_list(name="tags", ...)`
- [ ] Keep `tag find` separate (nested tag matching is tag-domain logic, not generic list behavior)

### Tests
- [ ] Unit tests for `property_add_to_list` and `property_remove_from_list` (happy + unhappy paths)
- [ ] E2e tests for the new CLI commands (happy + unhappy paths)
- [ ] Verify all existing tag e2e tests still pass (no behavioral changes)
- [ ] Run quality gates: `cargo fmt`, `cargo clippy`, `cargo test`

## Design Notes

- `--value` accepts multiple values: `--value foo --value bar` (clap `Vec<String>`)
- Mutation commands require `--file` or `--glob` (same safety rule as `tag add/remove`)
- Duplicate detection is case-insensitive for strings (consistent with tag behavior)
- When a list becomes empty after removal, the property key is removed entirely
- `tag add/remove` keep their existing CLI interface unchanged — this is a pure internal refactor
- `tag add` still validates tag names before delegating (no spaces, non-numeric, etc.)
- Output format for list ops: `{"property": name, "values": [...], "modified": [...], "skipped": [...], "total": N}`
