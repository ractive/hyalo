---
title: "Iteration 4 — Property Find Command"
type: iteration
date: 2026-03-21
tags:
  - iteration
  - properties
  - search
status: in-progress
branch: iter-4/property-find
---

# Iteration 4 — Property Find Command

## Goal

Add `hyalo property find --name <key> [--value <val>] [--file | --glob]` to find files that have a specific frontmatter property, optionally filtering by value. This mirrors `hyalo tag find` but operates on arbitrary frontmatter properties.

## Motivation

`tag find` lets you search by tag, but there's no equivalent for arbitrary properties like `status: draft` or `priority: 3`. Users need a way to query files by property existence or by property value. This partly overlaps with `hyalo properties` (which lists all properties) but serves a different use case: filtering files rather than listing metadata.

## Relationship to `search`

A general `search` command could subsume both `tag find` and `property find`. For now, `property find` fills the gap without over-engineering. If a `search` command is added later, it may cannibalize both `tag find` and `property find`.

## Tasks

- [x] Add `Find` variant to `PropertyAction` enum in `main.rs` with `--name`, `--value` (optional), `--file`, `--glob` flags
- [x] Implement `property_find()` in `src/commands/properties.rs`, reusing `collect_files()` and `read_frontmatter()`
- [x] Match logic: if `--value` given, compare property value (case-insensitive for strings); if omitted, match on property existence
- [x] Output JSON: `{"property": name, "value": value_or_null, "files": [...], "total": N}`
- [x] Wire up routing in `main.rs`
- [x] Add comprehensive help text following existing LLM-friendly pattern
- [x] Add unit tests in `properties.rs` (happy + unhappy paths) — 12 unit tests
- [x] Add e2e tests in `tests/e2e_property_find.rs` (happy + unhappy paths) — 19 e2e tests
- [x] Run quality gates: `cargo fmt`, `cargo clippy`, `cargo test`

## Design Notes

- Reuse `collect_files()` from `commands/mod.rs` (same as `tag find`)
- `property find` is read-only — no `--file`/`--glob` requirement (scan all by default)
- Value matching: use `yaml_to_json()` to normalize, then compare JSON string representations for non-string types; case-insensitive for strings
- Number matching: compare as numbers (e.g. `--value 3` matches `priority: 3`)
- Boolean matching: `--value true` matches `draft: true`
- List matching: `--value rust` matches if the list contains "rust"
