---
title: "Iteration 5 — Summary + List Subcommand Refactor"
type: iteration
date: 2026-03-21
tags:
  - iteration
  - refactor
  - cli
  - properties
  - tags
status: in-progress
branch: iter-5/summary-list-refactor
---

# Iteration 5 — Summary + List Subcommand Refactor

## Goal

Refactor `properties` and `tags` commands into consistent `summary` + `list` subcommands, extract shared helpers, and clean up clippy pedantic warnings.

## Motivation

Before this iteration, `properties` dumped an aggregate summary and `tags` did the same, but there was no way to get per-file detail (which file has which properties/tags). Adding `--file`/`--glob` to the top-level commands was overloading a single output format. Splitting into `summary` (aggregate) and `list` (per-file detail) gives each subcommand a clear purpose while keeping `summary` as the default for backward compatibility.

The same pattern applies to both `properties` and `tags`, so a consistent CLI model reduces cognitive load.

## Relationship to [[iteration-04-property-find]]

Iteration 4 added `property find` and generic list operations (`add-to-list`, `remove-from-list`). This iteration restructures the read-only listing commands (`properties`, `tags`) without changing the mutation commands from iteration 4.

## Tasks

### Subcommand refactor
- [x] Split `properties` into `properties summary` (default) and `properties list`
- [x] Split `tags` into `tags summary` (default) and `tags list`
- [x] Move `--file`/`--glob` flags to the subcommand level
- [x] Ensure `summary` is the default subcommand (running `hyalo properties` still works)

### Code quality
- [x] Extract shared helpers to reduce duplication between properties and tags
- [x] Fix clippy pedantic warnings across the workspace
- [x] Run quality gates: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`

### Documentation
- [x] Update README.md to reflect new CLI structure
- [x] Add iteration file for iteration 5
- [x] Add decision log entry for the summary/list split
- [x] Verify all e2e tests cover the new subcommands

## Commits

- `84b7a00` Add `property find` command for searching files by frontmatter properties
- `42a866c` Add generic list-property operations and refactor tag commands to delegate
- `1372993` Address PR review: fix clippy warnings, harden list ops, update iteration doc
- `7f91cda` Refactor properties/tags into summary + list subcommands for consistent CLI
- `0ad43d2` Fix clippy pedantic warnings and extract shared helpers for code reuse

## Design Notes

- `summary` is the default subcommand for both `properties` and `tags` — no breaking change for existing callers that omit the subcommand name
- `list` provides per-file detail: each file with its property key/value pairs (for `properties list`) or tags array (for `tags list`)
- Both subcommands accept `--file` and `--glob` for scoping; omitting both scans all `.md` files under `--dir`
- Shared helpers extracted to avoid duplicating file-discovery and frontmatter-reading logic between the two command groups
