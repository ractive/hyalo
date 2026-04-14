---
title: Iteration 108 — v0.11.0 Dogfood Fixes
type: iteration
date: 2026-04-14
tags:
  - iteration
  - dogfooding
  - lint
  - types
  - schema
  - hints
  - bug-fix
  - ux
status: in-progress
branch: iter-108/dogfood-v0110-fixes
related:
  - dogfood-results/dogfood-v0110-lint-types.md
---

# Iteration 108 — v0.11.0 Dogfood Fixes

## Goal

Fix the 3 bugs and 3 UX issues found during v0.11.0 dogfooding of lint, types, and hints across hyalo KB, MDN (14K files), GitHub Docs (3.5K files), and VS Code Docs (339 files).

## Background

See [[dogfood-results/dogfood-v0110-lint-types]] for full findings.

## Bugs

### Bug 1: `lint` JSON `total` counts files, not violations
- `"total": 228` when only 13 violations exist — 228 is the file count
- Consumers parsing JSON expect `total` to mean violation count
- **Fix:** rename to `total_files` and add `total_violations` (or keep `total` as violation count and add `files_checked`)

### Bug 2: `summary` hints don't mention `lint` when schema has warnings
- Summary reports `schema: {files_with_issues: 13, warnings: 13}` but hints only suggest `properties`, `tags`, `find`
- Should hint `hyalo lint` when `files_with_issues > 0`
- Was an AC of iter-107 that wasn't delivered

### Bug 3: `--dir` uses CWD's `.hyalo.toml` schema, not the target directory's
- `hyalo lint --dir ../mdn` from hyalo's directory applies hyalo's schema to MDN → 14,271 false errors
- `cd ../mdn && hyalo lint` → 0 issues (correct)
- Root cause: config file lookup walks up from CWD, not from `--dir` target
- **Fix:** when `--dir` is provided, resolve `.hyalo.toml` relative to the `--dir` path (or its ancestors), not CWD
- This is the most impactful bug — makes `--dir` unusable with lint/types for foreign repos

## UX Issues

### UX 1: `types show --format text` is unreadable
- Nested properties render flat: `properties: branch: pattern: ^iter-\d+[a-z]*/ type: string date: type: date`
- No indentation, no grouping, no visual structure
- JSON format is fine; only text format is affected
- **Fix:** use indented key-value layout with blank lines between property blocks

### UX 2: `types list --format text` same problem
- Required fields listed one-per-line without separators
- No visual grouping between types
- **Fix:** add type name as header, indent fields, blank line between types

### UX 3: `lint --fix --dry-run` hints miss "apply fixes"
- Normal `lint` correctly hints both `--fix --dry-run` (preview) and `--fix` (apply)
- `--fix --dry-run` output only hints `types list` — should also hint `hyalo lint --fix` to apply the previewed fixes
- **Fix:** when in dry-run mode and fixes were found, hint the non-dry-run `--fix` command

## Tasks

### Bug fixes
- [x] Fix `--dir` config lookup to resolve `.hyalo.toml` from target path, not CWD
- [x] Change lint JSON `total` to mean violation count; add `files_checked` for file count
- [x] Add `hyalo lint` hint to `summary` when `files_with_issues > 0`
- [x] Add `hyalo lint --fix` hint to `lint --fix --dry-run` output when fixes exist

### UX improvements
- [x] Redesign `types show --format text` with indented, grouped layout
- [x] Redesign `types list --format text` with type headers and visual separation

### Tests
- [x] Add e2e test: `lint --dir` uses target dir's config, not CWD's
- [x] Add e2e test: lint JSON `total` equals violation count, not file count
- [x] Add e2e test: summary hints include `lint` when schema violations exist
- [x] Add e2e test: `lint --fix --dry-run` hints include `lint --fix` when fixes found
- [x] Add e2e test: `types show --format text` output is properly indented
- [x] Add e2e test: `types list --format text` output has type headers

### Dogfood verification
- [x] Re-run `hyalo lint --dir ../mdn` from hyalo dir — should report 0 issues (no schema in MDN)
- [x] Re-run `hyalo lint --dir ../docs` from hyalo dir — should report 1 error (unclosed frontmatter only)
- [x] Verify `types show iteration --format text` is human-readable

### Quality Gates
- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`

## Acceptance Criteria
- [x] `hyalo lint --dir ../mdn` (from hyalo dir) uses MDN's config, not hyalo's
- [x] Lint JSON `total` counts violations, not files
- [x] `hyalo summary` hints include `lint` when schema has warnings
- [x] `lint --fix --dry-run` hints include `lint --fix` when fixes were proposed
- [x] `types show <type> --format text` output uses indentation for nested properties
- [x] `types list --format text` output visually separates types
- [x] All quality gates pass
